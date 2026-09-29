use crate::models::{
    TvdbEpisode, TvdbLoginRequest, TvdbLoginResponse, TvdbRemoteIdSearchResponse,
    TvdbSearchResponse, TvdbSeriesEpisodesDefaultResponse,
};
use crate::rate_limiter::RateLimiter;
use crate::{save_current_config, CONFIG, DATA_JSON};
use log::{debug, error, info};
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use serde_json;
use std::collections::HashMap;
use urlencoding;

static RE_YMD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d{4})-(\d{1,2})-(\d{1,2})$").unwrap());
static RE_MDY: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d{1,2})-(\d{1,2})-(\d{4})$").unwrap());

// TVDB API 限速（用户 key 官方限额 50 次/10 秒，取 2 QPS 留足余量），
// 避免 update-all 时每部剧并发匹配导致请求频率过高
const TVDB_API_QPS: u64 = 2;
const TVDB_API_MAX_PERMITS: usize = 2;

static TVDB_LIMITER: OnceCell<std::sync::Arc<RateLimiter>> = OnceCell::new();

fn get_tvdb_limiter() -> std::sync::Arc<RateLimiter> {
    TVDB_LIMITER
        .get_or_init(|| std::sync::Arc::new(RateLimiter::new(TVDB_API_QPS, TVDB_API_MAX_PERMITS)))
        .clone()
}

async fn tvdb_login(
    tvdb_api_key: String,
    client: std::sync::Arc<reqwest::Client>,
) -> Option<String> {
    let login_url = "https://api4.thetvdb.com/v4/login";
    debug!("[TVDB] 请求 TVDB 登录: {}", login_url);

    let login_request = TvdbLoginRequest {
        apikey: tvdb_api_key.clone(),
    };

    get_tvdb_limiter().acquire().await;
    let login_resp = match client
        .post(login_url)
        .json(&login_request)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("[TVDB] 登录失败: {}", e);
            return None;
        }
    };

    let login_text = match login_resp.text().await {
        Ok(t) => t,
        Err(e) => {
            error!("[TVDB] 读取登录响应失败: {}", e);
            return None;
        }
    };

    let token = match serde_json::from_str::<TvdbLoginResponse>(&login_text) {
        Ok(resp) => {
            if let Some(data) = resp.data {
                data.token
            } else {
                error!("[TVDB] 登录响应中无 token");
                return None;
            }
        }
        Err(e) => {
            error!("[TVDB] 解析登录响应失败: {}", e);
            return None;
        }
    };

    token
}

async fn get_or_refresh_token(
    tvdb_api_key: String,
    client: std::sync::Arc<reqwest::Client>,
) -> Option<String> {
    let config = CONFIG.read().await;
    let mut token = config.tvdb_token.clone();
    drop(config);

    if token.is_none() {
        info!("[TVDB] Config 中无 token，尝试登录获取");
        if let Some(new_token) = tvdb_login(tvdb_api_key.clone(), client.clone()).await {
            token = Some(new_token.clone());
            let mut config = CONFIG.write().await;
            config.tvdb_token = Some(new_token);
            drop(config);
            save_current_config().await;
            info!("[TVDB] Token 已保存到 config");
        }
    }

    token
}

async fn refresh_token(
    tvdb_api_key: String,
    client: std::sync::Arc<reqwest::Client>,
) -> Option<String> {
    if let Some(new_token) = tvdb_login(tvdb_api_key.clone(), client.clone()).await {
        let mut config = CONFIG.write().await;
        config.tvdb_token = Some(new_token.clone());
        drop(config);
        save_current_config().await;
        info!("[TVDB] Token 已刷新并保存");
        return Some(new_token);
    }
    None
}

enum ApiResponse<T> {
    Success(T),
    Unauthorized,
    Failed,
}

async fn make_tvdb_request<T: serde::de::DeserializeOwned>(
    client: &std::sync::Arc<reqwest::Client>,
    url: &str,
    bearer_token: &str,
) -> ApiResponse<T> {
    debug!("[TVDB] 请求: {}", url);

    let limiter = get_tvdb_limiter();
    limiter.acquire().await;

    match client
        .get(url)
        .header("Authorization", format!("Bearer {}", bearer_token))
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status() == 401 {
                return ApiResponse::Unauthorized;
            }
            match resp.text().await {
                Ok(text) => match serde_json::from_str::<T>(&text) {
                    Ok(data) => ApiResponse::Success(data),
                    Err(e) => {
                        error!("[TVDB] 解析响应失败: {}", e);
                        ApiResponse::Failed
                    }
                },
                Err(e) => {
                    error!("[TVDB] 读取响应失败: {}", e);
                    ApiResponse::Failed
                }
            }
        }
        Err(e) => {
            error!("[TVDB] 请求失败: {}", e);
            ApiResponse::Failed
        }
    }
}

async fn find_tvdb_series_id(
    tmdbid: u32,
    original_name: &str,
    client: &std::sync::Arc<reqwest::Client>,
    bearer_token: &str,
) -> Option<String> {
    let remoteid_url = format!("https://api4.thetvdb.com/v4/search/remoteid/{}", tmdbid);

    match make_tvdb_request::<TvdbRemoteIdSearchResponse>(client, &remoteid_url, bearer_token).await
    {
        ApiResponse::Success(search_resp) => {
            if let Some(data) = search_resp.data {
                for item in data {
                    if let Some(series) = item.series {
                        debug!("[TVDB] 通过 Remote ID 找到 TVDB 剧集 ID: {:?}", series.id);
                        return series.id.map(|id| id.to_string());
                    }
                }
            }
        }
        ApiResponse::Unauthorized => return None,
        ApiResponse::Failed => {}
    }

    let encoded_name = urlencoding::encode(original_name);
    let search_by_name_url = format!(
        "https://api4.thetvdb.com/v4/search?query={}&type=series",
        encoded_name
    );

    match make_tvdb_request::<TvdbSearchResponse>(client, &search_by_name_url, bearer_token).await {
        ApiResponse::Success(search_resp) => {
            if let Some(data) = search_resp.data {
                if let Some(result) = data.first() {
                    debug!("[TVDB] 通过名称找到 TVDB 剧集 ID: {:?}", result.tvdb_id);
                    return result.tvdb_id.map(|id| id.to_string());
                }
            }
        }
        ApiResponse::Unauthorized => return None,
        ApiResponse::Failed => {}
    }

    None
}

async fn fetch_all_episodes(
    series_id: &str,
    client: &std::sync::Arc<reqwest::Client>,
    bearer_token: &mut String,
    tvdb_api_key: &str,
) -> Option<Vec<TvdbEpisode>> {
    let mut all_tvdb_episodes = Vec::new();
    let mut current_url = format!(
        "https://api4.thetvdb.com/v4/series/{}/episodes/default",
        series_id
    );

    loop {
        match make_tvdb_request::<TvdbSeriesEpisodesDefaultResponse>(
            client,
            &current_url,
            bearer_token,
        )
        .await
        {
            ApiResponse::Success(episodes_resp) => {
                if let Some(data) = episodes_resp.data {
                    if let Some(episodes) = data.episodes {
                        debug!("[TVDB] 当前页找到 {} 集", episodes.len());
                        all_tvdb_episodes.extend(episodes);
                    }
                }

                let next_url = episodes_resp.links.and_then(|l| l.next);
                if let Some(next) = next_url {
                    if next.starts_with("http") {
                        current_url = next;
                    } else {
                        current_url = format!("https://api4.thetvdb.com{}", next);
                    }
                    debug!("[TVDB] 继续请求下一页: {}", current_url);
                } else {
                    break;
                }
            }
            ApiResponse::Unauthorized => {
                info!("[TVDB] Token 无效，重新登录");
                if let Some(new_token) =
                    refresh_token(tvdb_api_key.to_string(), client.clone()).await
                {
                    *bearer_token = new_token;
                    continue;
                }
                break;
            }
            ApiResponse::Failed => break,
        }
    }

    if all_tvdb_episodes.is_empty() {
        None
    } else {
        Some(all_tvdb_episodes)
    }
}

fn normalize_date(date_str: &str) -> String {
    let s = date_str.replace('/', "-");

    if let Some(caps) = RE_YMD.captures(&s) {
        let year = &caps[1];
        let month = format!("{:02}", caps[2].parse::<u32>().unwrap_or(1));
        let day = format!("{:02}", caps[3].parse::<u32>().unwrap_or(1));
        return format!("{}-{}-{}", year, month, day);
    }

    if let Some(caps) = RE_MDY.captures(&s) {
        let year = &caps[3];
        let month = format!("{:02}", caps[1].parse::<u32>().unwrap_or(1));
        let day = format!("{:02}", caps[2].parse::<u32>().unwrap_or(1));
        return format!("{}-{}-{}", year, month, day);
    }

    date_str.to_string()
}

pub async fn process_tvdb_match(
    tmdbid: u32,
    original_name: String,
    tvdb_api_key: String,
    client: std::sync::Arc<reqwest::Client>,
) {
    match_tvdb(tmdbid, original_name, tvdb_api_key, client.clone()).await;
    crate::skyhook::refresh_media(tmdbid, &client).await;
}

async fn match_tvdb(
    tmdbid: u32,
    original_name: String,
    tvdb_api_key: String,
    client: std::sync::Arc<reqwest::Client>,
) {
    debug!(
        "[TVDB] 开始处理 TMDB ID: {}, 原名: {}",
        tmdbid, original_name
    );

    if tvdb_api_key.is_empty() {
        info!("[TVDB] TVDB API key 为空，跳过处理");
        return;
    }

    let mut bearer_token = match get_or_refresh_token(tvdb_api_key.clone(), client.clone()).await {
        Some(t) => t,
        None => {
            error!("[TVDB] 无法获取 token");
            return;
        }
    };

    let mut tvdb_series_id = DATA_JSON
        .read()
        .await
        .media
        .get(&tmdbid)
        .and_then(|m| m.tvdb_series_id)
        .map(|id| id.to_string());
    let mut need_refresh = false;

    if tvdb_series_id.is_some() {
        // Reuse the established series identity instead of searching by title again.
    } else if let Some(id) =
        find_tvdb_series_id(tmdbid, &original_name, &client, &bearer_token).await
    {
        tvdb_series_id = Some(id);
    } else {
        need_refresh = true;
    }

    if need_refresh {
        if let Some(new_token) = refresh_token(tvdb_api_key.clone(), client.clone()).await {
            bearer_token = new_token;
            tvdb_series_id =
                find_tvdb_series_id(tmdbid, &original_name, &client, &bearer_token).await;
        }
    }

    let Some(series_id) = tvdb_series_id else {
        return;
    };

    if let Some(media) = DATA_JSON.write().await.media.get_mut(&tmdbid) {
        std::sync::Arc::make_mut(media).tvdb_series_id = series_id.parse().ok();
    }

    let Some(all_tvdb_episodes) =
        fetch_all_episodes(&series_id, &client, &mut bearer_token, &tvdb_api_key).await
    else {
        return;
    };

    let mut local_episodes = {
        let data = DATA_JSON.read().await;
        let Some(media) = data.media.get(&tmdbid) else {
            return;
        };
        let mut episodes = Vec::new();
        for season in &media.seasons {
            for episode in &season.episodes {
                if episode.absolute_number > 0 {
                    episodes.push(episode.clone());
                }
            }
        }
        episodes
    };

    let mut use_abs_match = false;
    if let Some(last_tvdb) = all_tvdb_episodes.last() {
        if let Some(last_local) = local_episodes.last() {
            let last_tvdb_abs = last_tvdb.absolute_number.unwrap_or(0);
            if last_tvdb_abs == last_local.absolute_number {
                use_abs_match = true;
            } else {
            }
        }
    }

    let mut _matched_count = 0;

    if use_abs_match {
        let abs_map: HashMap<i32, _> = all_tvdb_episodes
            .iter()
            .filter_map(|ep| ep.absolute_number.map(|abs| (abs, ep)))
            .collect();

        for local_ep in &mut local_episodes {
            if let Some(tvdb_ep) = abs_map.get(&local_ep.absolute_number) {
                local_ep.tvdb_season_number = tvdb_ep.season_number;
                local_ep.tvdb_episode_number = tvdb_ep.number;
                local_ep.tvdb_episode_id = tvdb_ep.id;
                _matched_count += 1;
            }
        }
    } else {
        let abs_map: HashMap<i32, _> = all_tvdb_episodes
            .iter()
            .filter_map(|ep| ep.absolute_number.map(|abs| (abs, ep)))
            .collect();

        let mut ep_map: HashMap<i32, Vec<_>> = HashMap::new();
        let mut date_map: HashMap<String, Vec<_>> = HashMap::new();

        for tvdb_ep in &all_tvdb_episodes {
            if let Some(ep_num) = tvdb_ep.number {
                ep_map.entry(ep_num).or_default().push(tvdb_ep);
            }
            if let Some(date) = &tvdb_ep.aired {
                let norm_date = normalize_date(date);
                date_map.entry(norm_date).or_default().push(tvdb_ep);
            }
        }

        for local_ep in &mut local_episodes {
            let mut candidates = Vec::new();

            if let Some(tvdb_ep) = abs_map.get(&local_ep.absolute_number) {
                candidates.push(*tvdb_ep);
            }

            if let Some(ep_candidates) = ep_map.get(&local_ep.episode_number) {
                candidates.extend(ep_candidates);
            }

            if let Some(local_date) = &local_ep.air_date {
                let norm_local_date = normalize_date(local_date);
                if let Some(date_candidates) = date_map.get(&norm_local_date) {
                    candidates.extend(date_candidates);
                }
            }

            candidates.sort_by_key(|ep| ep.absolute_number);
            candidates.dedup_by_key(|ep| ep.absolute_number);

            for tvdb_ep in candidates {
                let tvdb_abs = tvdb_ep.absolute_number.unwrap_or(0);
                let mut match_count = 0;
                let mut match_reasons = Vec::new();

                if Some(local_ep.episode_number) == tvdb_ep.number {
                    match_count += 1;
                    match_reasons.push("episode_number");
                }

                if local_ep.absolute_number == tvdb_abs {
                    match_count += 1;
                    match_reasons.push("absolute_number");
                }

                if let (Some(local_date), Some(tvdb_date)) = (&local_ep.air_date, &tvdb_ep.aired) {
                    if normalize_date(local_date) == normalize_date(&tvdb_date) {
                        match_count += 1;
                        match_reasons.push("air_date");
                    }
                }

                if match_count >= 2 {
                    local_ep.tvdb_season_number = tvdb_ep.season_number;
                    local_ep.tvdb_episode_number = tvdb_ep.number;
                    local_ep.tvdb_episode_id = tvdb_ep.id;
                    _matched_count += 1;

                    break;
                }
            }
        }
    }

    let _local_episodes_count = local_episodes.len();

    {
        let mut data = DATA_JSON.write().await;
        let Some(media) = data.media.get_mut(&tmdbid) else {
            return;
        };
        let media = std::sync::Arc::make_mut(media);
        media.tvdb_series_id = series_id.parse().ok();

        let local_map: HashMap<_, _> = local_episodes
            .into_iter()
            .map(|ep| (ep.absolute_number, ep))
            .collect();

        for season in &mut media.seasons {
            for episode in &mut season.episodes {
                if episode.absolute_number > 0 {
                    if let Some(updated_ep) = local_map.get(&episode.absolute_number) {
                        if (
                            episode.tvdb_season_number,
                            episode.tvdb_episode_number,
                            episode.tvdb_episode_id,
                        ) != (
                            updated_ep.tvdb_season_number,
                            updated_ep.tvdb_episode_number,
                            updated_ep.tvdb_episode_id,
                        ) {
                            episode.airtime = Default::default();
                        }
                        episode.tvdb_season_number = updated_ep.tvdb_season_number;
                        episode.tvdb_episode_number = updated_ep.tvdb_episode_number;
                        episode.tvdb_episode_id = updated_ep.tvdb_episode_id;
                    }
                }
            }
        }
    }
}
