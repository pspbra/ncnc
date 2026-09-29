use crate::aria2_client::build_download_path;
use crate::aria2_client::Aria2Client;
use crate::handlers::jackett::search_jackett;
use crate::models::config::FilterTerm;
use crate::models::search_filter::{
    calculate_priority_score, filter_search_results, normalize_name, SearchType,
};
use crate::models::{EpisodeInfo, JackettResult, MediaInfo, SearchEpisodeParams, SeasonInfo};
use crate::{CLIENT, CONFIG, DATA_JSON};
use chrono::{Local, Timelike};
use futures_util::future::join_all;
use log::{debug, error};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

const CHANNEL_BUFFER: usize = 100;
const CONCURRENCY: usize = 3;
const SEARCH_QPS: u64 = 2;

static SEARCH_LIMITER: once_cell::sync::Lazy<crate::rate_limiter::RateLimiter> =
    once_cell::sync::Lazy::new(|| crate::rate_limiter::RateLimiter::new(SEARCH_QPS, 1));
static EXECUTION_LOCK: once_cell::sync::Lazy<Mutex<()>> =
    once_cell::sync::Lazy::new(|| Mutex::new(()));

#[derive(Clone)]
struct DownloadTask {
    media: Arc<MediaInfo>,
    season_index: usize,
    episode_index: usize,
}

fn filter_term_to_string(term: &FilterTerm) -> String {
    match term {
        FilterTerm::Single(s) => s.clone(),
        FilterTerm::Multiple(v) => v.join(" "),
    }
}

fn build_search_query(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("+")
}

struct SearchTask {
    query: String,
    target_ep: i32,
    search_type: SearchType,
}

async fn search_single_name(
    name: String,
    params: &SearchEpisodeParams,
    client: &reqwest::Client,
    filter_terms: Option<&[FilterTerm]>,
) -> Option<Vec<(JackettResult, bool)>> {
    let mut tasks = Vec::new();
    let mut queries = HashSet::new();
    let mut eps_seen = HashSet::new();

    let task1_query = build_search_query(&[&name, &params.episode_number.to_string()]);
    if queries.insert(task1_query.clone()) {
        eps_seen.insert(params.episode_number);
        tasks.push(SearchTask {
            query: task1_query,
            target_ep: params.episode_number,
            search_type: SearchType::Episode,
        });
    }

    if let Some(tvdb_e) = params.tvdb_episode_number {
        if !eps_seen.contains(&tvdb_e) {
            let task2_query = build_search_query(&[&name, &tvdb_e.to_string()]);
            if queries.insert(task2_query.clone()) {
                eps_seen.insert(tvdb_e);
                tasks.push(SearchTask {
                    query: task2_query,
                    target_ep: tvdb_e,
                    search_type: SearchType::TvdbEpisode,
                });
            }
        }
    }

    if !eps_seen.contains(&params.absolute_number) {
        let task3_query = build_search_query(&[&name, &params.absolute_number.to_string()]);
        if queries.insert(task3_query.clone()) {
            tasks.push(SearchTask {
                query: task3_query,
                target_ep: params.absolute_number,
                search_type: SearchType::Absolute,
            });
        }
    }

    let mut futures = Vec::new();
    for task in tasks {
        futures.push(async move {
            SEARCH_LIMITER.acquire().await;
            if let Some(results) = search_jackett(
                client,
                &params.jackett_address,
                params.jackett_port,
                &params.jackett_api_key,
                &task.query,
            )
            .await
            {
                let filtered = filter_search_results(
                    results,
                    task.target_ep,
                    task.search_type,
                    params.season_number,
                    params.tvdb_season_number,
                    params.season_name.as_deref(),
                    filter_terms,
                );
                if !filtered.is_empty() {
                    debug!("[自动下载] 找到 {} 个结果", filtered.len());
                    let mapped: Vec<_> = filtered
                        .into_iter()
                        .map(|fr| (fr.result, fr.is_multi_episode))
                        .collect();
                    return Some(mapped);
                }
            }
            None
        });
    }

    let results = join_all(futures).await;

    let mut combined_results = Vec::new();
    for result in results {
        if let Some(r) = result {
            combined_results.extend(r);
        }
    }

    if !combined_results.is_empty() {
        Some(combined_results)
    } else {
        None
    }
}

async fn search_episode_with_filters(
    params: SearchEpisodeParams,
    client: &reqwest::Client,
    filter_terms: Option<&[FilterTerm]>,
) -> Option<Vec<(JackettResult, bool)>> {
    if params.season_number == 0 {
        debug!("[自动下载] season_number为0，返回无搜索结果");
        return None;
    }

    let mut all_names = Vec::new();
    all_names.push(normalize_name(&params.name));

    if let Some(name_tw) = &params.name_tw {
        all_names.push(normalize_name(name_tw));
    }

    for opt_name in &params.optional_names {
        all_names.push(filter_term_to_string(opt_name));
    }

    let mut futures = Vec::new();
    for name in all_names {
        let params = &params;
        futures.push(async move { search_single_name(name, params, client, filter_terms).await });
    }

    let results = join_all(futures).await;

    let mut all_results = Vec::new();
    for result in results {
        if let Some(r) = result {
            all_results.extend(r);
        }
    }

    if !all_results.is_empty() {
        let mut seen: HashSet<String> = HashSet::new();
        let mut unique_results = Vec::new();
        for (result, is_multi_episode) in all_results {
            if let Some(link) = &result.link {
                if seen.insert(link.clone()) {
                    unique_results.push((result, is_multi_episode));
                }
            } else {
                unique_results.push((result, is_multi_episode));
            }
        }

        debug!(
            "[自动下载] 所有名称搜索完成，找到 {} 个结果（去重后）",
            unique_results.len()
        );
        return Some(unique_results);
    }

    debug!("[自动下载] 未找到任何结果");
    None
}

pub struct AutoDownloadManager;

impl AutoDownloadManager {
    pub async fn start() {
        debug!("[自动下载] 自动下载服务已启动");

        tokio::spawn(async {
            let mut last_execution_date: Option<chrono::NaiveDate> = None;

            loop {
                let config = CONFIG.read().await;
                if config.jackett_auto_download {
                    drop(config);

                    let now = Local::now();
                    let current_hour = now.hour();
                    let current_minute = now.minute();
                    let current_date = now.date_naive();

                    if last_execution_date != Some(current_date)
                        && current_hour == 12
                        && current_minute == 0
                    {
                        debug!("[自动下载] 到达中午12点，开始执行自动下载");
                        last_execution_date = Some(current_date);

                        if let Err(e) = Self::execute_auto_download().await {
                            error!("[自动下载] 执行自动下载失败: {}", e);
                        }
                    }
                } else {
                    drop(config);
                }

                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    pub async fn execute_now() {
        debug!("[自动下载] 立即执行一次自动下载");

        if let Err(e) = Self::execute_auto_download().await {
            error!("[自动下载] 执行自动下载失败: {}", e);
        }
    }

    async fn process_task(
        task: DownloadTask,
        client: &reqwest::Client,
        jackett_address: &str,
        jackett_port: u16,
        jackett_api_key: &str,
        global_filters: &[FilterTerm],
        processed_count: Arc<AtomicUsize>,
        success_count: Arc<AtomicUsize>,
        failed_count: Arc<AtomicUsize>,
        skipped_count: Arc<AtomicUsize>,
        cache_set: Arc<Mutex<HashSet<(u32, u32, u32)>>>,
    ) {
        let season = &task.media.seasons[task.season_index];
        let episode = &season.episodes[task.episode_index];
        let tmdbid = task.media.tmdbid;
        let media_name = task.media.name.clone();
        let season_number = season.season_number;
        let episode_number = episode.episode_number;
        let cache_key = (tmdbid, season_number as u32, episode_number as u32);

        processed_count.fetch_add(1, Ordering::Relaxed);

        {
            let cache = cache_set.lock().await;
            if cache.contains(&cache_key) {
                debug!(
                    "[自动下载] {} S{}E{} 已被其他任务处理，跳过",
                    media_name, season_number, episode_number
                );
                skipped_count.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        if let Some((search_result, is_multi_episode)) = Self::search_and_process_episode(
            &task.media,
            season,
            episode,
            client,
            jackett_address,
            jackett_port,
            jackett_api_key,
            global_filters,
        )
        .await
        {
            if let Some(magnet_uri) = search_result.magnet_uri {
                let media_path = task.media.media_path.clone();
                let season_number_u32 = season_number as u32;
                let episode_number_u32 = episode_number as u32;
                let download_dir = build_download_path(&media_path, Some(season_number_u32));

                {
                    let mut cache = cache_set.lock().await;
                    cache.insert(cache_key);
                }

                match Aria2Client::add_uri_with_cache(
                    &magnet_uri,
                    &download_dir,
                    tmdbid,
                    season_number_u32,
                    episode_number_u32,
                    is_multi_episode,
                )
                .await
                {
                    Ok(gid) => {
                        success_count.fetch_add(1, Ordering::Relaxed);
                        debug!(
                            "[自动下载] ✅ 已添加下载: {} S{}E{}，GID: {}",
                            media_name, season_number, episode_number, gid
                        );
                    }
                    Err(e) => {
                        failed_count.fetch_add(1, Ordering::Relaxed);
                        error!(
                            "[自动下载] ❌ 添加下载失败: {} S{}E{}，错误: {}",
                            media_name, season_number, episode_number, e
                        );
                    }
                }
            } else {
                skipped_count.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            skipped_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    async fn execute_auto_download() -> Result<(), Box<dyn std::error::Error>> {
        let _execution = EXECUTION_LOCK.lock().await;
        let data_json = DATA_JSON.read().await.clone();

        let config = Arc::new(CONFIG.read().await.clone());
        let client_guard = CLIENT.read().await;
        let client = match client_guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                debug!("[自动下载] HTTP客户端未初始化");
                return Ok(());
            }
        };
        drop(client_guard);

        let initial_cache = crate::download_cache::episode_keys().await?;
        let cache_set = Arc::new(Mutex::new(initial_cache));

        let processed_count = Arc::new(AtomicUsize::new(0));
        let success_count = Arc::new(AtomicUsize::new(0));
        let failed_count = Arc::new(AtomicUsize::new(0));
        let skipped_count = Arc::new(AtomicUsize::new(0));

        let concurrency = Arc::new(Semaphore::new(CONCURRENCY));

        let (tx, mut rx) = mpsc::channel::<DownloadTask>(CHANNEL_BUFFER);

        let scan_task = tokio::spawn({
            let tx = tx.clone();
            let skipped_count = skipped_count.clone();
            let cache_set = cache_set.clone();

            async move {
                for media in data_json.media.into_values() {
                    let tmdbid = media.tmdbid;

                    for (season_index, season) in media.seasons.iter().enumerate() {
                        if !season.is_tracked {
                            continue;
                        }

                        for (episode_index, episode) in season.episodes.iter().enumerate() {
                            if episode.exists {
                                continue;
                            }

                            if !crate::skyhook::is_episode_aired(
                                episode,
                                chrono::Utc::now(),
                                Local::now().date_naive(),
                            ) {
                                debug!(
                                    "[自动下载] {} S{}E{} 尚未播出，跳过",
                                    media.name, season.season_number, episode.episode_number
                                );
                                skipped_count.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            let cache_key = (
                                tmdbid,
                                season.season_number as u32,
                                episode.episode_number as u32,
                            );
                            {
                                let cache = cache_set.lock().await;
                                if cache.contains(&cache_key) {
                                    debug!(
                                        "[自动下载] {} S{}E{} 已有下载缓存，跳过",
                                        media.name, season.season_number, episode.episode_number
                                    );
                                    skipped_count.fetch_add(1, Ordering::Relaxed);
                                    continue;
                                }
                            }

                            let task = DownloadTask {
                                media: media.clone(),
                                season_index,
                                episode_index,
                            };

                            debug!(
                                "[自动下载] 准备处理: {} S{}E{}",
                                media.name, season.season_number, episode.episode_number
                            );

                            if tx.send(task).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });

        drop(tx);

        let mut join_set = JoinSet::new();

        while let Some(task) = rx.recv().await {
            if join_set.len() >= CONCURRENCY {
                if let Some(Err(e)) = join_set.join_next().await {
                    error!("[自动下载] 下载任务执行失败: {}", e);
                }
            }
            let permit = concurrency.clone().acquire_owned().await.unwrap();
            let client = client.clone();
            let config = config.clone();
            let processed_count = processed_count.clone();
            let success_count = success_count.clone();
            let failed_count = failed_count.clone();
            let skipped_count = skipped_count.clone();
            let cache_set = cache_set.clone();

            join_set.spawn(async move {
                let _permit = permit;

                Self::process_task(
                    task,
                    &client,
                    &config.jackett_address,
                    config.jackett_port,
                    &config.jackett_api_key,
                    &config.global_filter_terms,
                    processed_count,
                    success_count,
                    failed_count,
                    skipped_count,
                    cache_set,
                )
                .await;
            });
        }

        let _ = scan_task.await;

        while let Some(result) = join_set.join_next().await {
            if let Err(error) = result {
                error!("[自动下载] 下载任务执行失败: {}", error);
            }
        }

        let processed = processed_count.load(Ordering::Relaxed);
        let success = success_count.load(Ordering::Relaxed);
        let failed = failed_count.load(Ordering::Relaxed);
        let skipped = skipped_count.load(Ordering::Relaxed);

        debug!(
            "[自动下载] 执行完成，处理 {} 个任务，成功 {} 个，失败 {} 个，跳过 {} 个",
            processed, success, failed, skipped
        );

        Ok(())
    }

    async fn search_and_process_episode(
        media: &MediaInfo,
        season: &SeasonInfo,
        episode: &EpisodeInfo,
        client: &reqwest::Client,
        jackett_address: &str,
        jackett_port: u16,
        jackett_api_key: &str,
        global_filters: &[FilterTerm],
    ) -> Option<(JackettResult, bool)> {
        let search_params = SearchEpisodeParams {
            name: media.name.clone(),
            name_tw: Some(media.name_tw.clone()),
            optional_names: media.optional_names.clone(),
            filter_names: media.filter_names.clone(),
            season_number: season.season_number,
            season_name: Some(season.name.clone()),
            episode_number: episode.episode_number,
            absolute_number: episode.absolute_number,
            tvdb_season_number: episode.tvdb_season_number,
            tvdb_episode_number: episode.tvdb_episode_number,
            jackett_address: jackett_address.to_string(),
            jackett_port,
            jackett_api_key: jackett_api_key.to_string(),
        };

        let mut combined_filters = Vec::new();
        combined_filters.extend_from_slice(&media.filter_names);
        combined_filters.extend_from_slice(global_filters);

        if let Some(results) =
            search_episode_with_filters(search_params, client, Some(&combined_filters)).await
        {
            if results.is_empty() {
                debug!(
                    "[自动下载] {} S{}E{} 无结果",
                    media.name, season.season_number, episode.episode_number
                );
                return None;
            }

            let (best_result, is_multi_episode) = results
                .into_iter()
                .max_by_key(|(r, _)| r.title.as_ref().map_or(0, |t| calculate_priority_score(t)))
                .unwrap();

            debug!("[自动下载] 选择最佳结果: {:?}", best_result.title);
            return Some((best_result, is_multi_episode));
        }

        None
    }
}
