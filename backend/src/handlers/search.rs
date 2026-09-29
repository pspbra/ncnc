use crate::models::{MovieSearchResponse, SearchRequest, SearchResponse};
use crate::{CLIENT, CONFIG, DATA_JSON};
use actix_web::{web, HttpResponse};
use log::{debug, error};
use serde_json;

/// 处理电视剧搜索请求
pub async fn search(req: web::Json<SearchRequest>) -> HttpResponse {
    let SearchRequest { query } = req.into_inner();
    debug!("收到搜索请求: {}", query);

    let client_guard = CLIENT.read().await;
    let client = match client_guard.as_ref() {
        Some(c) => c.clone(),
        None => {
            error!("错误: HTTP客户端未初始化");
            return HttpResponse::InternalServerError()
                .content_type("application/json")
                .json(serde_json::json!({
                    "error": "HTTP client not initialized"
                }));
        }
    };
    drop(client_guard);

    let config = CONFIG.read().await;
    let api_key = config.tmdb_api_key.clone();
    drop(config);

    let existing_tmdbids: Vec<u32> = {
        let data = DATA_JSON.read().await;
        data.media.keys().copied().collect()
    };

    let url = format!(
        "https://api.themoviedb.org/3/search/tv?api_key={}&language=zh-CN&page=1&include_adult=false&query={}",
        api_key, query
    );
    debug!("请求TMDB API: {}", url);

    match client.get(&url).send().await {
        Ok(response) => {
            debug!("TMDB API响应状态: {}", response.status());
            let text = response.text().await.unwrap_or_default();
            debug!("TMDB API响应长度: {}", text.len());
            match serde_json::from_str::<SearchResponse>(&text) {
                Ok(mut search_response) => {
                    for result in &mut search_response.results {
                        result.is_in_library = existing_tmdbids.contains(&result.id);
                    }
                    debug!("搜索成功，返回 {} 个结果", search_response.results.len());
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .json(search_response)
                }
                Err(e) => {
                    error!("解析TMDB响应失败: {}", e);
                    HttpResponse::InternalServerError()
                        .content_type("application/json")
                        .json(serde_json::json!({
                            "error": format!("Failed to parse TMDB API response: {}", e)
                        }))
                }
            }
        }
        Err(e) => {
            error!("请求TMDB API失败: {}", e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .json(serde_json::json!({
                    "error": format!("Failed to send request to TMDB API: {}", e)
                }))
        }
    }
}

/// 处理电影搜索请求
pub async fn search_movie(req: web::Json<SearchRequest>) -> HttpResponse {
    let SearchRequest { query } = req.into_inner();
    debug!("收到电影搜索请求: {}", query);

    let client_guard = CLIENT.read().await;
    let client = match client_guard.as_ref() {
        Some(c) => c.clone(),
        None => {
            error!("错误: HTTP客户端未初始化");
            return HttpResponse::InternalServerError()
                .content_type("application/json")
                .json(serde_json::json!({
                    "error": "HTTP client not initialized"
                }));
        }
    };
    drop(client_guard);

    let config = CONFIG.read().await;
    let api_key = config.tmdb_api_key.clone();
    drop(config);

    let url = format!(
        "https://api.themoviedb.org/3/search/movie?api_key={}&language=zh-CN&page=1&include_adult=false&query={}",
        api_key, query
    );
    debug!("请求TMDB电影API: {}", url);

    match client.get(&url).send().await {
        Ok(response) => {
            debug!("TMDB电影API响应状态: {}", response.status());
            let text = response.text().await.unwrap_or_default();
            debug!("TMDB电影API响应长度: {}", text.len());
            match serde_json::from_str::<MovieSearchResponse>(&text) {
                Ok(search_response) => {
                    debug!(
                        "电影搜索成功，返回 {} 个结果",
                        search_response.results.len()
                    );
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .json(search_response)
                }
                Err(e) => {
                    error!("解析TMDB电影响应失败: {}", e);
                    HttpResponse::InternalServerError()
                        .content_type("application/json")
                        .json(serde_json::json!({
                            "error": format!("Failed to parse TMDB movie API response: {}", e)
                        }))
                }
            }
        }
        Err(e) => {
            error!("请求TMDB电影API失败: {}", e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .json(serde_json::json!({
                    "error": format!("Failed to send request to TMDB movie API: {}", e)
                }))
        }
    }
}
