use crate::models::search_filter::normalize_name;
use crate::models::{
    filter_search_results, JackettResult, JackettSearchResponse, ResourceSearchRequest,
    ResourceSearchResponse, SearchEpisodeParams, SearchEpisodeResult, SearchType,
};
use crate::{CLIENT, CONFIG, DATA_JSON};
use actix_web::{web, HttpResponse};
use futures_util::future::join_all;
use log::{debug, error};
use serde_json;
use std::collections::HashSet;

/// 将 FilterTerm 转换为用于搜索的字符串
/// 这个函数处理单个词或多个词的组合，统一用空格连接后用于搜索
fn filter_term_to_string(term: &crate::models::media::FilterTerm) -> String {
    match term {
        crate::models::media::FilterTerm::Single(s) => s.clone(),
        crate::models::media::FilterTerm::Multiple(v) => v.join(" "),
    }
}

/// 构建搜索查询字符串
/// 将多个搜索部件用 "+" 连接，用于构建Jackett的搜索URL
fn build_search_query(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("+")
}

/// 纯粹的搜索行为，不进行结果过滤
/// 直接向Jackett发送搜索请求并返回原始结果
pub async fn search_jackett(
    client: &reqwest::Client,
    jackett_address: &str,
    jackett_port: u16,
    jackett_api_key: &str,
    query: &str,
) -> Option<Vec<JackettResult>> {
    let url = format!(
        "http://{}:{}/api/v2.0/indexers/all/results?apikey={}&Query={}",
        jackett_address, jackett_port, jackett_api_key, query
    );

    debug!("[Jackett] 请求搜索: {}", url);

    let resp = match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("[Jackett] 请求失败: {}", e);
            return None;
        }
    };
    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            error!("[Jackett] 读取响应失败: {}", e);
            return None;
        }
    };

    let search_resp = match serde_json::from_str::<JackettSearchResponse>(&text) {
        Ok(r) => r,
        Err(e) => {
            error!("[Jackett] 解析响应失败: {}", e);
            return None;
        }
    };

    debug!("[Jackett] 请求成功");

    search_resp.results
}

/// 单个搜索任务信息
/// 包含搜索查询、目标集数和搜索类型
struct SearchTask {
    query: String,
    target_ep: i32,
    search_type: SearchType,
}

/// 对单个名称进行搜索（并发执行多个查询，先去重）
/// 这个函数会尝试不同的搜索类型（Episode、TvdbEpisode、Absolute）
/// 同时避免重复的查询，提高搜索效率
async fn search_single_name(
    name: String,
    params: &SearchEpisodeParams,
    client: &reqwest::Client,
    filter_terms: Option<&[crate::models::media::FilterTerm]>,
) -> Option<Vec<JackettResult>> {
    let mut tasks = Vec::new();
    let mut queries = HashSet::new();

    // 1. 使用标准季集编号进行搜索
    let task1_query = build_search_query(&[&name, &params.episode_number.to_string()]);
    if queries.insert(task1_query.clone()) {
        tasks.push(SearchTask {
            query: task1_query,
            target_ep: params.episode_number,
            search_type: SearchType::Episode,
        });
    }

    // 2. 如果有TVDB编号，使用TVDB编号进行搜索
    if let Some(tvdb_e) = params.tvdb_episode_number {
        let task2_query = build_search_query(&[&name, &tvdb_e.to_string()]);
        if queries.insert(task2_query.clone()) {
            tasks.push(SearchTask {
                query: task2_query,
                target_ep: tvdb_e,
                search_type: SearchType::TvdbEpisode,
            });
        }
    }

    // 3. 使用绝对集数编号进行搜索
    let task3_query = build_search_query(&[&name, &params.absolute_number.to_string()]);
    if queries.insert(task3_query.clone()) {
        tasks.push(SearchTask {
            query: task3_query,
            target_ep: params.absolute_number,
            search_type: SearchType::Absolute,
        });
    }

    // 并发执行所有搜索任务
    let mut futures = Vec::new();
    for task in tasks {
        let client_clone = client.clone();
        let addr = params.jackett_address.clone();
        let port = params.jackett_port;
        let api_key = params.jackett_api_key.clone();
        let sn = params.season_number;
        let tsn = params.tvdb_season_number;
        let sen = params.season_name.clone();
        let filter_terms = filter_terms.map(|terms| terms.to_vec());
        futures.push(tokio::spawn(async move {
            if let Some(results) =
                search_jackett(&client_clone, &addr, port, &api_key, &task.query).await
            {
                let filtered = filter_search_results(
                    results,
                    task.target_ep,
                    task.search_type,
                    sn,
                    tsn,
                    sen.as_deref(),
                    filter_terms.as_deref(),
                );
                if !filtered.is_empty() {
                    debug!("[Jackett] 找到 {} 个结果", filtered.len());
                    let mapped: Vec<_> = filtered
                        .into_iter()
                        .map(|fr| {
                            let mut result = fr.result;
                            result.is_multi_episode = fr.is_multi_episode;
                            result
                        })
                        .collect();
                    return Some(mapped);
                }
            }
            None
        }));
    }

    let results = join_all(futures).await;

    // 合并所有搜索结果
    let mut combined_results = Vec::new();
    for result in results {
        if let Ok(Some(r)) = result {
            combined_results.extend(r);
        }
    }

    if !combined_results.is_empty() {
        Some(combined_results)
    } else {
        None
    }
}

/// 搜索剧集
/// 这是主要的搜索入口函数，会：
/// 1. 收集所有需要搜索的名称（原名、台湾名、可选搜索名）
/// 2. 对每个名称并发执行搜索
/// 3. 合并结果并去重
/// 4. 返回搜索结果
pub async fn search_episode(
    params: SearchEpisodeParams,
    client: &reqwest::Client,
    global_filter_terms: Option<&[crate::models::media::FilterTerm]>,
) -> Option<SearchEpisodeResult> {
    if params.season_number == 0 {
        debug!("[Jackett] season_number为0，返回无搜索结果");
        return None;
    }

    // 合并过滤词：媒体级别 + 全局
    let mut combined_filters = Vec::new();
    combined_filters.extend_from_slice(&params.filter_names);
    if let Some(global) = global_filter_terms {
        combined_filters.extend_from_slice(global);
    }
    let filter_terms = if combined_filters.is_empty() {
        None
    } else {
        Some(&combined_filters[..])
    };

    // 收集所有需要搜索的名称
    let mut all_names = Vec::new();
    // 添加标准化后的原名
    all_names.push(normalize_name(&params.name));

    // 添加标准化后的台湾名（如果存在）
    if let Some(name_tw) = &params.name_tw {
        all_names.push(normalize_name(name_tw));
    }

    // 添加可选搜索名（转换为字符串）
    // 可选搜索名支持单个词或多个词的组合（FilterTerm类型）
    for opt_name in &params.optional_names {
        all_names.push(filter_term_to_string(opt_name));
    }

    // 对每个名称并发执行搜索
    let mut tasks = Vec::new();
    for name in all_names {
        let params_clone = params.clone();
        let client_clone = client.clone();
        let filter_terms = filter_terms.map(|terms| terms.to_vec());
        tasks.push(tokio::spawn(async move {
            search_single_name(name, &params_clone, &client_clone, filter_terms.as_deref()).await
        }));
    }

    let results = join_all(tasks).await;

    // 合并所有搜索结果
    let mut all_results = Vec::new();
    for result in results {
        if let Ok(Some(r)) = result {
            all_results.extend(r);
        }
    }

    // 去重（通过链接判断是否重复）
    if !all_results.is_empty() {
        let mut seen: HashSet<String> = HashSet::new();
        let mut unique_results = Vec::new();
        for result in all_results {
            if let Some(link) = &result.link {
                if seen.insert(link.clone()) {
                    unique_results.push(result);
                }
            } else {
                unique_results.push(result);
            }
        }

        debug!(
            "[Jackett] 所有名称搜索完成，找到 {} 个结果（去重后）",
            unique_results.len()
        );
        return Some(SearchEpisodeResult {
            results: unique_results,
        });
    }

    debug!("[Jackett] 未找到任何结果");
    None
}

/// 处理资源搜索请求
/// 这是HTTP API的处理函数，负责：
/// 1. 验证请求参数和权限
/// 2. 从数据库中获取媒体信息
/// 3. 构建搜索参数并调用search_episode
/// 4. 返回搜索结果给前端
pub async fn resource_search(req: web::Json<ResourceSearchRequest>) -> HttpResponse {
    let ResourceSearchRequest {
        tmdbid,
        season_number,
        episode_number,
    } = req.into_inner();
    debug!(
        "[ResourceSearch] 收到搜索请求: TMDBID={}, S={}, E={}",
        tmdbid, season_number, episode_number
    );

    // 读取配置和HTTP客户端
    let config = CONFIG.read().await.clone();
    let client_guard = CLIENT.read().await;
    let client = match client_guard.as_ref() {
        Some(c) => c.clone(),
        None => {
            let response = ResourceSearchResponse {
                success: false,
                results: None,
                message: Some("HTTP客户端未初始化".to_string()),
            };
            return HttpResponse::Ok()
                .content_type("application/json")
                .json(response);
        }
    };

    // 从数据中获取媒体信息
    drop(client_guard);
    let data = DATA_JSON.read().await;

    let Some(media) = data.media.get(&tmdbid) else {
        let response = ResourceSearchResponse {
            success: false,
            results: None,
            message: Some("未找到该媒体".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    };

    // 获取季信息
    let Some(season) = media
        .seasons
        .iter()
        .find(|s| s.season_number == season_number)
    else {
        let response = ResourceSearchResponse {
            success: false,
            results: None,
            message: Some("未找到该季度".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    };

    // 获取集信息
    let Some(episode) = season
        .episodes
        .iter()
        .find(|e| e.episode_number == episode_number)
    else {
        let response = ResourceSearchResponse {
            success: false,
            results: None,
            message: Some("未找到该集".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    };

    // 构建搜索参数
    let search_params = SearchEpisodeParams {
        name: media.name.clone(),
        name_tw: Some(media.name_tw.clone()),
        optional_names: media.optional_names.clone(),
        filter_names: media.filter_names.clone(),
        season_number,
        season_name: Some(season.name.clone()),
        episode_number,
        absolute_number: episode.absolute_number,
        tvdb_season_number: episode.tvdb_season_number,
        tvdb_episode_number: episode.tvdb_episode_number,
        jackett_address: config.jackett_address.clone(),
        jackett_port: config.jackett_port,
        jackett_api_key: config.jackett_api_key.clone(),
    };

    // 执行搜索并返回结果
    drop(data);
    if let Some(result) =
        search_episode(search_params, &client, Some(&config.global_filter_terms)).await
    {
        let response = ResourceSearchResponse {
            success: true,
            results: Some(result.results),
            message: None,
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    }

    let response = ResourceSearchResponse {
        success: false,
        results: None,
        message: Some("未找到搜索结果".to_string()),
    };
    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}
