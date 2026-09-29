// 引入必要的模块
use crate::aria2_client::build_download_path;
use crate::aria2_client::Aria2Client;
use crate::models::search_filter::{
    calculate_priority_score, matches_optional_name, normalize_name, normalize_season_name,
    PreparedTitle,
};
use crate::models::{MediaInfo, SearchResult, SearchType};
use crate::{CLIENT, CONFIG, DATA_JSON};
use dashmap::DashSet;
use log::{debug, error, info};
use once_cell::sync::Lazy;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

// RSS检查间隔，单位：秒（10分钟）
const RSS_INTERVAL: u64 = 600;
// 并发处理数量
const CONCURRENCY: usize = 10;

// 已处理的RSS条目GUID集合，用于避免重复处理
// 使用DashSet是一个线程安全的集合，可以在多个任务中安全地读写
static EXECUTION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static PROCESSED_GUIDS: Lazy<DashSet<String>> = Lazy::new(|| DashSet::new());

// RSS条目结构体，存储从RSS源获取的单个条目信息
#[derive(Debug, Clone)]
struct RssItem {
    title: String,        // 标题
    guid: String,         // 唯一标识符
    link: String,         // 下载链接
    size: Option<u64>,    // 文件大小
    seeders: Option<i32>, // 做种数
    score: i32,           // 优先级评分
}

// 为RssItem实现SearchResult trait，使其可以与搜索过滤系统兼容
// 这样我们就可以复用filter_search_results函数
impl SearchResult for RssItem {
    fn title(&self) -> Option<&str> {
        Some(&self.title)
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn category_desc(&self) -> Option<&str> {
        None
    }
}

// 匹配到的剧集结构体，关联媒体、季、集和对应的RSS条目
#[derive(Debug, Clone)]
struct MatchedEpisode {
    media: Arc<MediaInfo>,
    season_index: usize,
    episode_index: usize,
    rss_item: Arc<RssItem>,
    is_multi_episode: bool,
}

fn match_rss(
    data: crate::models::DataJson,
    items: Vec<RssItem>,
    global_filters: &[crate::models::config::FilterTerm],
    cache: &HashSet<(u32, u32, u32)>,
) -> HashMap<(u32, u32, u32), MatchedEpisode> {
    // 名称、过滤词和季名称按媒体预处理，集数只保存索引。
    let mut media_index = Vec::new();
    for media in data.media.into_values() {
        let mut names = vec![normalize_name(&media.name), normalize_name(&media.name_tw)];
        for optional in &media.optional_names {
            match optional {
                crate::models::config::FilterTerm::Single(name) => names.push(name.clone()),
                crate::models::config::FilterTerm::Multiple(names_in_term) => {
                    names.extend(names_in_term.iter().cloned())
                }
            }
        }
        names.sort();
        names.dedup();
        let filters: Vec<_> = media
            .filter_names
            .iter()
            .chain(global_filters)
            .cloned()
            .collect();
        let season_names: Vec<_> = media
            .seasons
            .iter()
            .map(|season| normalize_season_name(&season.name))
            .collect();
        let mut pending = Vec::new();
        for (season_index, season) in media.seasons.iter().enumerate() {
            if !season.is_tracked {
                continue;
            }
            for (episode_index, episode) in season.episodes.iter().enumerate() {
                if !episode.exists
                    && !cache.contains(&(
                        media.tmdbid,
                        season.season_number as u32,
                        episode.episode_number as u32,
                    ))
                {
                    pending.push((season_index, episode_index));
                }
            }
        }
        if !pending.is_empty() {
            media_index.push((media, names, filters, season_names, pending));
        }
    }
    let mut matches: HashMap<(u32, u32, u32), MatchedEpisode> = HashMap::new();
    for item in items {
        let prepared = PreparedTitle::new(&item.title);
        let item = Arc::new(item);
        for (media, names, filters, season_names, pending) in &media_index {
            let name_match = names
                .iter()
                .any(|name| prepared.normalized.contains(name) || item.title.contains(name))
                || matches_optional_name(&item.title, &media.optional_names);
            if !name_match {
                continue;
            }
            for &(season_index, episode_index) in pending {
                let season = &media.seasons[season_index];
                let episode = &season.episodes[episode_index];
                let season_name = Some(season_names[season_index].as_str());
                let is_multi = prepared
                    .matches(
                        item.as_ref(),
                        episode.episode_number,
                        SearchType::Episode,
                        season.season_number,
                        episode.tvdb_season_number,
                        season_name,
                        Some(filters),
                    )
                    .or_else(|| {
                        episode.tvdb_episode_number.and_then(|number| {
                            prepared.matches(
                                item.as_ref(),
                                number,
                                SearchType::TvdbEpisode,
                                season.season_number,
                                episode.tvdb_season_number,
                                season_name,
                                Some(filters),
                            )
                        })
                    })
                    .or_else(|| {
                        prepared.matches(
                            item.as_ref(),
                            episode.absolute_number,
                            SearchType::Absolute,
                            season.season_number,
                            episode.tvdb_season_number,
                            season_name,
                            Some(filters),
                        )
                    });
                if let Some(is_multi_episode) = is_multi {
                    let key = (
                        media.tmdbid,
                        season.season_number as u32,
                        episode.episode_number as u32,
                    );
                    if matches
                        .get(&key)
                        .is_none_or(|previous| item.score > previous.rss_item.score)
                    {
                        matches.insert(
                            key,
                            MatchedEpisode {
                                media: media.clone(),
                                season_index,
                                episode_index,
                                rss_item: item.clone(),
                                is_multi_episode,
                            },
                        );
                    }
                }
            }
        }
    }
    matches
}

// 解析RSS XML内容，提取RSS条目信息
// 使用quick_xml库进行快速XML解析
fn parse_rss(xml: &str) -> Vec<RssItem> {
    let mut items = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_item: Option<RssItem> = None;
    let mut current_tag = String::new();
    let mut current_title = String::new();
    let mut current_guid = String::new();
    let mut current_link = String::new();
    let mut current_size: Option<u64> = None;
    let mut current_seeders: Option<i32> = None;

    // 使用quick_xml解析XML
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match e.name().as_ref() {
                    b"item" => {
                        // 开始新的item标签，初始化RSS条目
                        current_item = Some(RssItem {
                            title: String::new(),
                            guid: String::new(),
                            link: String::new(),
                            size: None,
                            seeders: None,
                            score: 0,
                        });
                        current_title.clear();
                        current_guid.clear();
                        current_link.clear();
                        current_size = None;
                        current_seeders = None;
                    }
                    b"torznab:attr" => {
                        // 解析torznab属性，提取做种数等信息
                        if current_item.is_some() {
                            let mut name = None;
                            let mut value = None;
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    match a.key.as_ref() {
                                        b"name" => {
                                            name =
                                                Some(String::from_utf8_lossy(&a.value).to_string())
                                        }
                                        b"value" => {
                                            value =
                                                Some(String::from_utf8_lossy(&a.value).to_string())
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            if let (Some(n), Some(v)) = (name, value) {
                                if n == "seeders" {
                                    if let Ok(s) = v.parse::<i32>() {
                                        current_seeders = Some(s);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                // 处理文本内容（标签内部的文本）
                if current_item.is_some() {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "title" => current_title = text,
                        "guid" => current_guid = text,
                        "link" => current_link = text,
                        "size" => {
                            if let Ok(s) = text.parse::<u64>() {
                                current_size = Some(s);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                // 结束标签，完成当前item的解析
                if e.name().as_ref() == b"item" {
                    if let Some(mut item) = current_item.take() {
                        item.title = current_title.clone();
                        item.guid = current_guid.clone();
                        item.link = current_link.clone();
                        item.size = current_size;
                        item.seeders = current_seeders;
                        // 计算优先级评分（基于字幕组等关键词）
                        item.score = calculate_priority_score(&item.title);
                        // 预处理标准化标题（移除特殊字符，统一格式）
                        items.push(item);
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    items
}

// 自动RSS管理器
// 负责定期检查Jackett RSS源，自动下载匹配的剧集
pub struct AutoRssManager;

impl AutoRssManager {
    // 启动自动RSS服务，定期执行RSS检查
    // 这个函数会在后台持续运行
    pub async fn start() {
        info!("[自动RSS] 自动RSS服务已启动");

        tokio::spawn(async {
            loop {
                let config = CONFIG.read().await;
                // 检查是否启用了自动RSS
                if config.jackett_auto_rss {
                    drop(config);

                    if let Err(e) = Self::execute_rss_check().await {
                        error!("[自动RSS] 执行RSS检查失败: {}", e);
                    }
                } else {
                    drop(config);
                }

                // 等待指定间隔后再次检查（10分钟）
                sleep(Duration::from_secs(RSS_INTERVAL)).await;
            }
        });
    }

    // 立即执行一次RSS检查（手动触发）
    pub async fn execute_now() {
        debug!("[自动RSS] 立即执行一次RSS检查");

        if let Err(e) = Self::execute_rss_check().await {
            error!("[自动RSS] 执行RSS检查失败: {}", e);
        }
    }

    // 从Jackett获取RSS内容
    // 使用Torznab API获取TV类别的RSS源
    async fn fetch_rss(client: &reqwest::Client, config: &crate::models::Config) -> Option<String> {
        // 构建Jackett RSS查询URL（TV搜索）
        let url = format!(
            "http://{}:{}/api/v2.0/indexers/all/results/torznab?t=tvsearch&apikey={}",
            config.jackett_address, config.jackett_port, config.jackett_api_key
        );

        debug!("[自动RSS] 请求RSS: {}", url);

        // 发送HTTP请求获取RSS
        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
        {
            Ok(resp) => match resp.text().await {
                Ok(text) => Some(text),
                Err(e) => {
                    error!("[自动RSS] 读取RSS响应失败: {}", e);
                    None
                }
            },
            Err(e) => {
                error!("[自动RSS] 请求RSS失败: {}", e);
                None
            }
        }
    }

    // 处理单个匹配的剧集，添加下载任务
    async fn process_match(
        matched: MatchedEpisode,
        processed_count: Arc<AtomicUsize>,
        success_count: Arc<AtomicUsize>,
    ) {
        let media = &matched.media;
        let season = &media.seasons[matched.season_index];
        let episode = &season.episodes[matched.episode_index];
        let rss_item = &matched.rss_item;
        let is_multi_episode = matched.is_multi_episode;

        processed_count.fetch_add(1, Ordering::Relaxed);

        debug!(
            "[自动RSS] 找到匹配: {} S{}E{} - {}",
            media.name, season.season_number, episode.episode_number, rss_item.title
        );

        // 构建下载路径
        let media_path = media.media_path.clone();
        let tmdbid = media.tmdbid;
        let season_number_u32 = season.season_number as u32;
        let episode_number_u32 = episode.episode_number as u32;
        let download_dir = build_download_path(&media_path, Some(season_number_u32));

        // 通过Aria2添加下载任务，并记录缓存
        match Aria2Client::add_uri_with_cache(
            &rss_item.link,
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
                info!(
                    "[自动RSS] ✅ 已添加下载: {} S{}E{}，GID: {}",
                    media.name, season.season_number, episode.episode_number, gid
                );

                // 记录已处理的GUID，避免重复处理
                PROCESSED_GUIDS.insert(rss_item.guid.clone());
            }
            Err(e) => {
                error!("[自动RSS] ❌ 添加下载失败: {}", e);
            }
        }
    }

    // 执行RSS检查的主逻辑
    // 这个函数包含完整的RSS检查流程：
    // 1. 获取并解析RSS
    // 2. 过滤已处理的条目
    // 3. 加载下载缓存
    // 4. 构建剧集索引和倒排索引
    // 5. 匹配RSS条目和剧集
    // 6. 并发处理匹配结果
    async fn execute_rss_check() -> Result<(), Box<dyn std::error::Error>> {
        let _execution = EXECUTION_LOCK.lock().await;
        // 读取媒体数据
        let data_json = DATA_JSON.read().await.clone();

        let config = CONFIG.read().await.clone();
        let client_guard = CLIENT.read().await;
        let client = match client_guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                debug!("[自动RSS] HTTP客户端未初始化");
                return Ok(());
            }
        };
        drop(client_guard);

        // 1. 获取并解析RSS
        let rss_content = match Self::fetch_rss(&client, &config).await {
            Some(c) => c,
            None => return Ok(()),
        };

        let mut rss_items = parse_rss(&rss_content);
        debug!("[自动RSS] 解析到 {} 个RSS条目", rss_items.len());

        // 2. 过滤掉已处理的RSS条目
        rss_items.retain(|item| !PROCESSED_GUIDS.contains(&item.guid));
        debug!("[自动RSS] 找到 {} 个新RSS条目", rss_items.len());

        // 3. 加载下载缓存
        let cache_set = crate::download_cache::episode_keys().await?;

        // 获取全局过滤词
        let global_filter_terms = config.global_filter_terms.clone();
        let episode_matches = crate::io_util::blocking(move || {
            match_rss(data_json, rss_items, &global_filter_terms, &cache_set)
        })
        .await?;

        let all_matches = episode_matches.into_values();

        // 6. 并发处理匹配结果
        let processed_count = Arc::new(AtomicUsize::new(0));
        let success_count = Arc::new(AtomicUsize::new(0));

        let concurrency = Arc::new(Semaphore::new(CONCURRENCY));
        let mut join_set = JoinSet::new();

        for matched in all_matches {
            if join_set.len() >= CONCURRENCY {
                if let Some(Err(e)) = join_set.join_next().await {
                    error!("[自动RSS] RSS任务执行失败: {}", e);
                }
            }
            let permit = concurrency.clone().acquire_owned().await.unwrap();
            let processed_count = processed_count.clone();
            let success_count = success_count.clone();

            join_set.spawn(async move {
                let _permit = permit;
                Self::process_match(matched, processed_count, success_count).await;
            });
        }

        // 等待所有任务完成
        while let Some(result) = join_set.join_next().await {
            if let Err(error) = result {
                error!("[自动RSS] RSS任务执行失败: {}", error);
            }
        }

        let processed = processed_count.load(Ordering::Relaxed);
        let success = success_count.load(Ordering::Relaxed);
        debug!(
            "[自动RSS] RSS处理完成：已处理 {} 个，成功 {} 个",
            processed, success
        );
        Ok(())
    }
}
