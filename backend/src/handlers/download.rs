use crate::aria2_client::{build_download_path, Aria2Client};
use crate::models::{
    AddDownloadRequest, AddDownloadResponse, AddMovieDownloadRequest, DownloadResponse,
    UploadResponse,
};
use crate::{DOWNLOAD_STATUS_CACHE, UPLOAD_STATUS_CACHE};
use actix_web::{web, HttpRequest, HttpResponse};
use log::{error, info};

pub async fn add_download(req: web::Json<AddDownloadRequest>) -> HttpResponse {
    let AddDownloadRequest {
        tmdbid,
        season_number,
        episode_number,
        magnet_uri,
        media_path,
        is_multi_episode,
    } = req.into_inner();

    let download_dir = build_download_path(&media_path, Some(season_number));

    match Aria2Client::add_uri_with_cache(
        &magnet_uri,
        &download_dir,
        tmdbid,
        season_number,
        episode_number,
        is_multi_episode,
    )
    .await
    {
        Ok(gid) => {
            let response = AddDownloadResponse {
                success: true,
                message: "下载已添加到队列".to_string(),
                gid: Some(gid),
            };

            HttpResponse::Ok()
                .content_type("application/json")
                .json(response)
        }
        Err(e) => {
            let response = AddDownloadResponse {
                success: false,
                message: format!("添加下载失败: {}", e),
                gid: None,
            };

            HttpResponse::Ok()
                .content_type("application/json")
                .json(response)
        }
    }
}

pub async fn add_movie_download(req: web::Json<AddMovieDownloadRequest>) -> HttpResponse {
    let AddMovieDownloadRequest {
        display_name,
        media_path,
        magnet_uri,
    } = req.into_inner();

    let download_dir = build_download_path(&media_path, None);

    match Aria2Client::add_uri_with_cache_movie(
        &magnet_uri,
        &download_dir,
        display_name,
        media_path,
    )
    .await
    {
        Ok(gid) => {
            let response = AddDownloadResponse {
                success: true,
                message: "电影下载已添加到队列".to_string(),
                gid: Some(gid),
            };

            HttpResponse::Ok()
                .content_type("application/json")
                .json(response)
        }
        Err(e) => {
            let response = AddDownloadResponse {
                success: false,
                message: format!("添加电影下载失败: {}", e),
                gid: None,
            };

            HttpResponse::Ok()
                .content_type("application/json")
                .json(response)
        }
    }
}

pub async fn get_downloads(_req: HttpRequest) -> HttpResponse {
    let cache = DOWNLOAD_STATUS_CACHE.read().await;
    let downloads = cache.clone();

    let response = DownloadResponse {
        success: true,
        message: None,
        data: Some(downloads),
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}

pub async fn get_uploads(_req: HttpRequest) -> HttpResponse {
    let cache = UPLOAD_STATUS_CACHE.read().await;
    let uploads = cache.clone();

    let response = UploadResponse {
        success: true,
        message: None,
        data: Some(uploads),
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}

pub async fn clear_downloads(_req: HttpRequest) -> HttpResponse {
    let mut cleared_count = 0;
    let mut processed_gids = std::collections::HashSet::new();
    let mut gids_to_process = std::collections::VecDeque::new();
    // 清理之前获取完整列表，连接失败或分页失败时保留全部下载缓存。
    let (active, waiting, stopped) = match (
        Aria2Client::tell_active().await,
        Aria2Client::tell_all_waiting().await,
        Aria2Client::tell_all_stopped().await,
    ) {
        (Ok(active), Ok(waiting), Ok(stopped)) => (active, waiting, stopped),
        _ => {
            return HttpResponse::Ok().json(AddDownloadResponse {
                success: false,
                message: "获取下载任务列表失败，未执行清理".to_string(),
                gid: None,
            });
        }
    };
    let mut removal_failed = false;

    for status in active.into_iter().chain(waiting) {
        gids_to_process.push_back(status.gid);
    }

    while let Some(gid) = gids_to_process.pop_front() {
        if processed_gids.contains(&gid) {
            continue;
        }
        processed_gids.insert(gid.clone());

        if let Ok(status) = Aria2Client::tell_status(&gid).await {
            for followed_gid in status.followed_by {
                gids_to_process.push_back(followed_gid);
            }
        }

        if let Err(e) = Aria2Client::force_remove(&gid).await {
            removal_failed = true;
            error!("[下载] 强制移除任务失败 {}: {}", gid, e);
        } else {
            cleared_count += 1;
            info!("[下载] 已移除任务: {}", gid);
        }
    }

    for status in stopped {
        if let Err(e) = Aria2Client::remove_download_result(&status.gid).await {
            removal_failed = true;
            error!("[下载] 移除已停止任务结果失败 {}: {}", status.gid, e);
        } else {
            cleared_count += 1;
        }
    }

    if let Err(e) = Aria2Client::purge_download_result().await {
        removal_failed = true;
        error!("[下载] 清理下载结果失败: {}", e);
    }

    if !removal_failed {
        crate::download_cache::clear().await;
        info!("[下载] 已清空统一运行状态中的下载任务");
    }

    let response = AddDownloadResponse {
        success: !removal_failed,
        message: if removal_failed {
            format!(
                "已清除 {} 个任务，部分清理失败，剩余下载缓存已保留",
                cleared_count
            )
        } else {
            format!("已清除 {} 个正在下载的任务并清空下载状态", cleared_count)
        },
        gid: None,
    };

    if !removal_failed {
        DOWNLOAD_STATUS_CACHE.write().await.clear();
    }

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}
