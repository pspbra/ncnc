use crate::handlers::tvdb::process_tvdb_match;
use crate::handlers::update::update_single_media;
use crate::models::utils::get_webui_images_dir;
use crate::models::{MediaInfo, SubscribeRequest, SubscribeResponse};
use crate::{CLIENT, CONFIG, DATA_JSON, PROCESSING_TMDBIDS};
use actix_web::{web, HttpResponse};
use log::{debug, error};

/// 处理订阅请求
pub async fn subscribe(req: web::Json<SubscribeRequest>) -> HttpResponse {
    let SubscribeRequest {
        tmdbid,
        name,
        original_name,
        poster_path,
        optional_names,
        filter_names,
        media_path,
        display_name,
        ..
    } = req.into_inner();

    if !PROCESSING_TMDBIDS.insert(tmdbid) {
        let response = SubscribeResponse {
            success: false,
            message: Some("正在添加中，请稍候".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    }

    {
        let data = DATA_JSON.read().await;
        if data.media.contains_key(&tmdbid) {
            PROCESSING_TMDBIDS.remove(&tmdbid);
            let response = SubscribeResponse {
                success: false,
                message: Some("已经添加完毕".to_string()),
            };
            return HttpResponse::Ok()
                .content_type("application/json")
                .json(response);
        }
    }

    let media_info = MediaInfo {
        tmdbid,
        tvdb_series_id: None,
        name: name.clone(),
        original_name: original_name.clone(),
        name_tw: name.clone(),
        poster_path: poster_path.clone(),
        optional_names: optional_names
            .into_iter()
            .map(|s| crate::models::media::FilterTerm::Single(s))
            .collect(),
        filter_names: filter_names
            .into_iter()
            .map(|s| crate::models::media::FilterTerm::Single(s))
            .collect(),
        media_path: media_path.clone(),
        number_of_seasons: 0,
        seasons: vec![],
        display_name,
    };

    {
        let mut data = DATA_JSON.write().await;
        data.media
            .insert(tmdbid, std::sync::Arc::new(media_info.clone()));
    }

    let config = CONFIG.read().await;
    let tvdb_api_key = config.tvdb_api_key.clone();
    drop(config);

    let client_clone = {
        let client_guard = CLIENT.read().await;
        client_guard.as_ref().cloned()
    };

    if let Some(poster) = poster_path.clone() {
        let client_guard = CLIENT.read().await;
        if let Some(client) = client_guard.as_ref() {
            let client = client.clone();
            tokio::spawn(async move {
                let image_url = format!("https://image.tmdb.org/t/p/w500{}", poster);
                debug!("[订阅] 开始下载海报: {}", image_url);

                let image_dir = get_webui_images_dir();
                debug!("[订阅] 图片保存目录: {:?}", image_dir);

                if let Err(e) = tokio::fs::create_dir_all(&image_dir).await {
                    error!("[订阅] {}", e);
                } else if let Some(file_name) = poster.split('/').last() {
                    let image_path = image_dir.join(file_name);
                    debug!("[订阅] 图片保存路径: {:?}", image_path);

                    match client.get(&image_url).send().await {
                        Ok(img_resp) => {
                            if img_resp.status().is_success() {
                                match img_resp.bytes().await {
                                    Ok(bytes) => match tokio::fs::write(&image_path, bytes).await {
                                        Ok(_) => debug!("[订阅] 海报下载成功: {:?}", image_path),
                                        Err(e) => error!("[订阅] 海报保存失败: {}", e),
                                    },
                                    Err(e) => error!("[订阅] 读取海报数据失败: {}", e),
                                }
                            } else {
                                error!("[订阅] 下载海报失败，HTTP状态码: {}", img_resp.status());
                            }
                        }
                        Err(e) => error!("[订阅] 发送海报下载请求失败: {}", e),
                    }
                }
            });
        }
    }

    tokio::spawn(async move {
        let _ = async {
            if let Ok(updated_media) =
                update_single_media(tmdbid, Some(std::sync::Arc::new(media_info))).await
            {
                let mut data = DATA_JSON.write().await;
                data.media
                    .insert(tmdbid, std::sync::Arc::new(updated_media));
            }

            if let Some(client) = client_clone {
                process_tvdb_match(tmdbid, original_name, tvdb_api_key, client).await;
            }
        }
        .await;

        PROCESSING_TMDBIDS.remove(&tmdbid);
    });

    HttpResponse::Ok()
        .content_type("application/json")
        .json(SubscribeResponse {
            success: true,
            message: Some("订阅成功，正在加载详细信息...".to_string()),
        })
}
