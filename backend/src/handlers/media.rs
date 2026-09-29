use crate::models::{
    DeleteMediaRequest, DeleteMediaResponse, MediaSettingsRequest, MediaSettingsResponse,
};
use crate::DATA_JSON;
use actix_web::web::Bytes;
use actix_web::{web, HttpResponse};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

static MEDIA_RESPONSE: Lazy<Mutex<Option<(u64, Bytes)>>> = Lazy::new(|| Mutex::new(None));

/// 获取所有媒体信息
pub async fn get_all_media() -> HttpResponse {
    match media_json(&DATA_JSON, &MEDIA_RESPONSE).await {
        Ok(body) => HttpResponse::Ok()
            .content_type("application/json")
            .body(body),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

async fn media_json(
    store: &crate::data_store::DataStore,
    response: &Mutex<Option<(u64, Bytes)>>,
) -> std::io::Result<Bytes> {
    let mut cached = response.lock().await;
    let (version, media_list) = {
        let data = store.read().await;
        let version = store.version();
        if let Some((cached_version, body)) = cached.as_ref() {
            if *cached_version == version {
                return Ok(body.clone());
            }
        }
        (version, data.media.values().cloned().collect::<Vec<_>>())
    };
    let body = crate::io_util::blocking(move || serde_json::to_vec(&media_list))
        .await
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    let body = Bytes::from(body);
    *cached = Some((version, body.clone()));
    Ok(body)
}

/// 获取单个媒体信息
pub async fn get_media_by_id(path: web::Path<u32>) -> HttpResponse {
    let tmdbid = path.into_inner();
    let media = DATA_JSON.read().await.media.get(&tmdbid).cloned();
    if let Some(media) = media {
        HttpResponse::Ok()
            .content_type("application/json")
            .json(media)
    } else {
        HttpResponse::NotFound()
            .content_type("application/json")
            .json(serde_json::json!({"success": false, "message": "未找到该媒体"}))
    }
}

/// 删除媒体
pub async fn delete_media(req: web::Json<DeleteMediaRequest>) -> HttpResponse {
    let DeleteMediaRequest { tmdbid } = req.into_inner();

    let mut data = DATA_JSON.write().await;

    if data.media.remove(&tmdbid).is_none() {
        let response = DeleteMediaResponse {
            success: false,
            message: Some("未找到该媒体".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    }

    let response = DeleteMediaResponse {
        success: true,
        message: Some("删除成功".to_string()),
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}

/// 更新媒体设置
pub async fn update_media_settings(req: web::Json<MediaSettingsRequest>) -> HttpResponse {
    let MediaSettingsRequest {
        tmdbid,
        optional_names,
        filter_names,
        seasons_tracked,
    } = req.into_inner();

    let mut data = DATA_JSON.write().await;

    let Some(media) = data.media.get_mut(&tmdbid) else {
        let response = MediaSettingsResponse {
            success: false,
            message: Some("未找到该媒体".to_string()),
        };
        return HttpResponse::Ok()
            .content_type("application/json")
            .json(response);
    };

    let media = std::sync::Arc::make_mut(media);
    if let Some(names) = optional_names {
        media.optional_names = names;
    }

    if let Some(names) = filter_names {
        media.filter_names = names;
    }

    if let Some(tracked) = seasons_tracked {
        for (i, &is_tracked) in tracked.iter().enumerate() {
            if i < media.seasons.len() {
                media.seasons[i].is_tracked = is_tracked;
            }
        }
    }

    let response = MediaSettingsResponse {
        success: true,
        message: Some("设置已更新".to_string()),
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}
