use crate::handlers::tvdb::process_tvdb_match;
use crate::models::{
    EpisodeInfo, MediaInfo, SeasonInfo, TmdbSeasonDetail, TmdbTvDetail, COMMON_MEDIA_EXTS,
};
use crate::rate_limiter::RateLimiter;
use crate::{CLIENT, CONFIG, DATA_JSON};
use actix_web::{web, HttpResponse};
use futures_util::{stream, StreamExt};
use log::{debug, error};
use once_cell::sync::OnceCell;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

const TMDB_API_QPS: u64 = 3;
const TMDB_API_MAX_PERMITS: usize = 3;

static SEASON_EPISODE_RE: OnceCell<Regex> = OnceCell::new();
static TMDB_LIMITER: OnceCell<Arc<RateLimiter>> = OnceCell::new();

fn get_season_episode_re() -> &'static Regex {
    SEASON_EPISODE_RE.get_or_init(|| Regex::new(r"[Ss](\d+)[Ee](\d+)").unwrap())
}

fn get_tmdb_limiter() -> Arc<RateLimiter> {
    TMDB_LIMITER
        .get_or_init(|| RateLimiter::new(TMDB_API_QPS, TMDB_API_MAX_PERMITS).into())
        .clone()
}

fn collect_media_files(dir: &Path) -> Result<HashSet<(i32, i32)>, String> {
    let mut keys = HashSet::new();
    match std::fs::metadata(dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err("Media path is not a directory".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(keys),
        Err(error) => return Err(error.to_string()),
    }
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter() {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| COMMON_MEDIA_EXTS.contains(&ext.to_lowercase().as_str()))
        {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if let Some(caps) = get_season_episode_re().captures(name) {
                if let (Ok(season), Ok(episode)) = (caps[1].parse(), caps[2].parse()) {
                    keys.insert((season, episode));
                }
            }
        }
    }
    Ok(keys)
}

async fn scan_media_files(path: String) -> Result<HashSet<(i32, i32)>, String> {
    crate::io_util::blocking(move || collect_media_files(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

pub async fn mark_episode_exists(tmdbid: u32, season_number: i32, episode_number: i32, path: &str) {
    if !Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| COMMON_MEDIA_EXTS.contains(&ext.to_lowercase().as_str()))
    {
        return;
    }
    if !tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return;
    }
    let mut data = DATA_JSON.write().await;
    let Some(media) = data.media.get(&tmdbid) else {
        return;
    };
    if !Path::new(path).starts_with(&media.media_path) {
        return;
    }
    let Some(season_index) = media
        .seasons
        .iter()
        .position(|season| season.season_number == season_number)
    else {
        return;
    };
    let Some(episode_index) = media.seasons[season_index]
        .episodes
        .iter()
        .position(|episode| episode.episode_number == episode_number)
    else {
        return;
    };
    if media.seasons[season_index].episodes[episode_index].exists {
        return;
    }
    let media = Arc::make_mut(data.media.get_mut(&tmdbid).unwrap());
    media.seasons[season_index].episodes[episode_index].exists = true;
}

pub async fn update_single_media(
    tmdbid: u32,
    original_media: Option<Arc<MediaInfo>>,
) -> Result<MediaInfo, String> {
    let client_guard = CLIENT.read().await;
    let client = match client_guard.as_ref() {
        Some(c) => c.clone(),
        None => {
            return Err("HTTP client not initialized".to_string());
        }
    };
    drop(client_guard);

    let config = CONFIG.read().await;
    let api_key = config.tmdb_api_key.clone();
    drop(config);

    let rate_limiter = get_tmdb_limiter();

    let url_zh = format!(
        "https://api.themoviedb.org/3/tv/{}?api_key={}&language=zh-CN",
        tmdbid, api_key
    );
    debug!("[Update] 请求剧集详情 (zh-CN): {}", url_zh);

    rate_limiter.acquire().await;
    let tv_detail_resp = match client.get(&url_zh).send().await {
        Ok(r) => r,
        Err(e) => {
            return Err(format!("Failed to fetch TV detail: {}", e));
        }
    };

    let tv_detail_text = tv_detail_resp.text().await.unwrap_or_default();
    let tv_detail = match serde_json::from_str::<TmdbTvDetail>(&tv_detail_text) {
        Ok(d) => d,
        Err(e) => {
            return Err(format!("Failed to parse TV detail: {}", e));
        }
    };

    let url_tw = format!(
        "https://api.themoviedb.org/3/tv/{}?api_key={}&language=zh-TW",
        tmdbid, api_key
    );
    debug!("[Update] 请求剧集详情 (zh-TW): {}", url_tw);

    rate_limiter.acquire().await;
    let tv_detail_tw_name = match client.get(&url_tw).send().await {
        Ok(r) => {
            let text = r.text().await.unwrap_or_default();
            match serde_json::from_str::<TmdbTvDetail>(&text) {
                Ok(d) => d.name,
                Err(_) => tv_detail.name.clone(),
            }
        }
        Err(_) => tv_detail.name.clone(),
    };

    let number_of_seasons = tv_detail.number_of_seasons;

    let media_path = original_media
        .as_ref()
        .map(|m| m.media_path.clone())
        .unwrap_or_default();
    let media_files_map = scan_media_files(media_path.clone()).await?;

    let mut sorted_seasons: Vec<_> = tv_detail
        .seasons
        .iter()
        .map(|season| season.season_number)
        .collect();
    sorted_seasons.sort_unstable();
    let max_season_number = sorted_seasons.iter().copied().max().unwrap_or(0);

    let tvdb_series_id = original_media.as_ref().and_then(|m| m.tvdb_series_id);
    let mut tvdb_map: HashMap<(i32, i32), EpisodeInfo> = HashMap::new();
    let mut is_tracked_map: HashMap<i32, bool> = HashMap::new();
    if let Some(om) = &original_media {
        for s in &om.seasons {
            is_tracked_map.insert(s.season_number, s.is_tracked);
            for e in &s.episodes {
                tvdb_map.insert((s.season_number, e.episode_number), e.clone());
            }
        }
    }

    let media_files_map_arc = Arc::new(media_files_map);
    let tvdb_map_arc = Arc::new(tvdb_map);
    let is_tracked_map_arc = Arc::new(is_tracked_map);

    let mut season_futures = stream::iter(sorted_seasons)
        .map(|season_number| {
            let client = client.clone();
            let api_key = api_key.clone();
            let media_files_map = media_files_map_arc.clone();
            let tvdb_map = tvdb_map_arc.clone();
            let is_tracked_map = is_tracked_map_arc.clone();
            let rate_limiter = rate_limiter.clone();

            async move {
                let season_url = format!(
                    "https://api.themoviedb.org/3/tv/{}/season/{}?api_key={}&language=zh-CN",
                    tmdbid, season_number, api_key
                );
                debug!("[Update] 请求季度 {} 详情: {}", season_number, season_url);

                rate_limiter.acquire().await;

                if let Ok(season_resp) = client.get(&season_url).send().await {
                    let season_text = season_resp.text().await.unwrap_or_default();
                    if let Ok(season_detail) =
                        serde_json::from_str::<TmdbSeasonDetail>(&season_text)
                    {
                        let mut episodes: Vec<EpisodeInfo> = vec![];

                        for e in &season_detail.episodes {
                            let exists =
                                media_files_map.contains(&(season_number, e.episode_number));

                            let previous = tvdb_map.get(&(season_number, e.episode_number));
                            let tvdb_season_number = previous.and_then(|e| e.tvdb_season_number);
                            let tvdb_episode_number = previous.and_then(|e| e.tvdb_episode_number);

                            episodes.push(EpisodeInfo {
                                air_date: e.air_date.clone(),
                                airtime: previous
                                    .filter(|old| old.air_date == e.air_date)
                                    .map(|old| old.airtime.clone())
                                    .unwrap_or_default(),
                                tvdb_episode_id: previous.and_then(|e| e.tvdb_episode_id),
                                name: e.name.clone(),
                                exists,
                                episode_number: e.episode_number,
                                absolute_number: 0,
                                season_number: season_number,
                                tvdb_season_number,
                                tvdb_episode_number,
                            });
                        }

                        let is_tracked = is_tracked_map
                            .get(&season_number)
                            .copied()
                            .unwrap_or(season_number == max_season_number);

                        Some((
                            season_number,
                            SeasonInfo {
                                name: season_detail.name,
                                episode_count: season_detail.episodes.len() as i32,
                                episodes,
                                is_tracked,
                                season_number,
                            },
                            season_detail.episodes.len() as i32,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        })
        .buffer_unordered(3);

    let mut season_results = Vec::new();
    while let Some(result) = season_futures.next().await {
        if let Some(result) = result {
            season_results.push(result);
        }
    }

    season_results.sort_by_key(|(sn, _, _)| *sn);

    let mut seasons = Vec::new();
    let mut absolute_counter = 0;

    for (season_number, mut season_info, episode_count) in season_results {
        for e in &mut season_info.episodes {
            if season_number != 0 {
                e.absolute_number = absolute_counter + e.episode_number;
            }
        }
        seasons.push(season_info);
        if season_number != 0 {
            absolute_counter += episode_count;
        }
    }

    let (poster_path, optional_names, filter_names, display_name) = original_media
        .map(|m| {
            (
                m.poster_path.clone(),
                m.optional_names.clone(),
                m.filter_names.clone(),
                m.display_name.clone(),
            )
        })
        .unwrap_or((None, vec![], vec![], String::new()));

    Ok(MediaInfo {
        tmdbid,
        tvdb_series_id,
        name: tv_detail.name,
        original_name: tv_detail.original_name,
        name_tw: tv_detail_tw_name,
        poster_path,
        optional_names,
        filter_names,
        media_path,
        number_of_seasons,
        seasons,
        display_name,
    })
}

pub async fn update_all() -> HttpResponse {
    debug!("[UpdateAll] 开始更新所有媒体信息");

    let media_list: Vec<Arc<MediaInfo>> = {
        let data = DATA_JSON.read().await;
        data.media.values().cloned().collect()
    };

    if media_list.is_empty() {
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(serde_json::json!({
                "success": true,
                "message": "No media to update"
            }));
    }

    debug!("[UpdateAll] 找到 {} 个媒体需要更新", media_list.len());

    let config = CONFIG.read().await;
    let tvdb_api_key = config.tvdb_api_key.clone();
    drop(config);

    let client_clone = {
        let client_guard = CLIENT.read().await;
        client_guard.as_ref().cloned()
    };

    let mut update_futures = stream::iter(media_list)
        .map(|media| {
            let tmdbid = media.tmdbid;
            let original_name = media.original_name.clone();
            let tvdb_api_key = tvdb_api_key.clone();
            let client = client_clone.clone();
            async move {
                debug!("[UpdateAll] 正在更新媒体 TMDBID: {}", tmdbid);
                let result = update_single_media(tmdbid, Some(media)).await;
                // 先把 TMDB 最新数据写入 DATA_JSON，再执行 TVDB 匹配，
                // 保证匹配结果写回后不会被旧数据覆盖
                if let Ok(updated_media) = &result {
                    let mut data = DATA_JSON.write().await;
                    data.media.insert(tmdbid, Arc::new(updated_media.clone()));
                }
                if result.is_ok() {
                    if let Some(client) = client {
                        process_tvdb_match(tmdbid, original_name, tvdb_api_key, client).await;
                    }
                }
                (tmdbid, result)
            }
        })
        .buffer_unordered(3);

    while let Some(result) = update_futures.next().await {
        match result {
            (tmdbid, Ok(_)) => {
                debug!("[UpdateAll] 媒体 TMDBID: {} 更新完成", tmdbid);
            }
            (tmdbid, Err(e)) => {
                error!("[UpdateAll] 媒体 TMDBID: {} 更新失败: {}", tmdbid, e);
            }
        }
    }

    HttpResponse::Ok()
        .content_type("application/json")
        .json(serde_json::json!({
            "success": true,
            "message": "更新完成"
        }))
}

pub async fn update_single(path: web::Path<u32>) -> HttpResponse {
    let tmdbid = path.into_inner();
    debug!("[UpdateSingle] 开始更新媒体 TMDBID: {}", tmdbid);

    let original_media = {
        let data = DATA_JSON.read().await;
        data.media.get(&tmdbid).cloned()
    };

    let Some(media) = original_media else {
        return HttpResponse::NotFound()
            .content_type("application/json")
            .json(serde_json::json!({
                "success": false,
                "message": "Media not found"
            }));
    };

    let original_name = media.original_name.clone();

    let config = CONFIG.read().await;
    let tvdb_api_key = config.tvdb_api_key.clone();
    drop(config);

    let client_clone = {
        let client_guard = CLIENT.read().await;
        client_guard.as_ref().cloned()
    };

    match update_single_media(tmdbid, Some(media)).await {
        Ok(updated_media) => {
            let mut data = DATA_JSON.write().await;
            data.media.insert(tmdbid, Arc::new(updated_media));
            debug!("[UpdateSingle] 媒体 TMDBID: {} 更新完成", tmdbid);

            if let Some(client) = client_clone {
                tokio::spawn(async move {
                    process_tvdb_match(tmdbid, original_name, tvdb_api_key, client).await;
                });
            }

            HttpResponse::Ok()
                .content_type("application/json")
                .json(serde_json::json!({
                    "success": true,
                    "message": "更新完成"
                }))
        }
        Err(e) => {
            error!("[UpdateSingle] 媒体 TMDBID: {} 更新失败: {}", tmdbid, e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .json(serde_json::json!({
                    "success": false,
                    "message": format!("更新失败: {}", e)
                }))
        }
    }
}
