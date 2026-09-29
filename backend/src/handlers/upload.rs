use crate::media_processor;
use crate::models::constants::{COMMON_CAPTION_EXTS, COMMON_MEDIA_EXTS};
use crate::models::media::MediaInfo;
use crate::models::utils::parse_season_episode;
use crate::DATA_JSON;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::task::JoinSet;

#[derive(Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: String,
    pub file_count: Option<usize>,
}

async fn save_upload<S, E>(path: &str, chunks: &mut S) -> std::io::Result<usize>
where
    S: futures_util::Stream<Item = Result<web::Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut file = BufWriter::with_capacity(64 * 1024, fs::File::create(path).await?);
    let result = async {
        let mut written = 0;
        while let Some(chunk) = chunks.next().await {
            let data = chunk.map_err(|error| std::io::Error::other(error.to_string()))?;
            file.write_all(&data).await?;
            written += data.len();
        }
        file.flush().await?;
        Ok(written)
    }
    .await;
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path).await;
    }
    result
}

pub async fn add_season(mut payload: Multipart) -> impl Responder {
    debug!("[上传] 开始处理季度上传请求");

    let upload_cache_dir = "upload_cache";
    if !Path::new(upload_cache_dir).exists() {
        if let Err(e) = fs::create_dir_all(upload_cache_dir).await {
            error!("[上传] 创建upload_cache目录失败: {}", e);
            return HttpResponse::InternalServerError().json(UploadResponse {
                success: false,
                message: format!("创建上传目录失败: {}", e),
                file_count: None,
            });
        }
    }

    let mut tmdbid: Option<u32> = None;
    let mut season_number: Option<i32> = None;
    let mut files: Vec<(String, String)> = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(e) => {
                error!("[上传] 读取字段失败: {}", e);
                continue;
            }
        };

        let content_disposition = field.content_disposition();
        let field_name = match content_disposition.get_name() {
            Some(name) => name,
            None => continue,
        };

        if field_name == "tmdbid" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取tmdbid失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(id) = std::str::from_utf8(&bytes) {
                tmdbid = id.parse::<u32>().ok();
            }
        } else if field_name == "season_number" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取season_number失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(num) = std::str::from_utf8(&bytes) {
                season_number = num.parse::<i32>().ok();
            }
        } else if field_name == "file" {
            let filename = match content_disposition.get_filename() {
                Some(name) => name.to_string(),
                None => continue,
            };

            let safe_name = Path::new(&filename)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload");
            let file_path = format!(
                "{}/{:032x}-{}",
                upload_cache_dir,
                rand::random::<u128>(),
                safe_name
            );
            let bytes_written = match save_upload(&file_path, &mut field).await {
                Ok(written) => written,
                Err(error) => {
                    error!("[上传] 文件保存失败: {}", error);
                    return HttpResponse::InternalServerError().json(UploadResponse {
                        success: false,
                        message: "上传未完成，请重试".to_string(),
                        file_count: None,
                    });
                }
            };

            debug!(
                "[上传] 文件保存成功: {} ({} bytes)",
                file_path, bytes_written
            );
            files.push((filename, file_path));
        }
    }

    if tmdbid.is_none() || season_number.is_none() {
        return HttpResponse::BadRequest().json(UploadResponse {
            success: false,
            message: "缺少必要参数: tmdbid 或 season_number".to_string(),
            file_count: None,
        });
    }

    let tmdbid = tmdbid.unwrap();
    let season_number = season_number.unwrap();
    let file_count = files.len();

    debug!(
        "[上传] 文件接收完成，共 {} 个文件，立即返回前端",
        file_count
    );

    let media_info = {
        let data = DATA_JSON.read().await;
        match data.media.get(&tmdbid) {
            Some(media) => media.clone(),
            None => {
                return HttpResponse::BadRequest().json(UploadResponse {
                    success: false,
                    message: format!("未找到 tmdbid 为 {} 的媒体", tmdbid),
                    file_count: None,
                });
            }
        }
    };

    tokio::spawn(async move {
        debug!("[上传] 开始异步处理文件");
        let mut handles = JoinSet::new();

        for (file_name, file_path) in files {
            if handles.len() >= 3 {
                if let Some(Err(e)) = handles.join_next().await {
                    error!("[上传] 媒体处理任务失败: {}", e);
                }
            }
            let media_info_clone = media_info.clone();
            let season_str = season_number.to_string();

            handles.spawn(async move {
                debug!("[上传] 处理文件: {}", file_name);

                let parse_result = parse_season_episode(&file_name);
                if let Some(ep) = parse_result.ep {
                    debug!("[上传] 识别到集数: {}", ep);
                    process_file(
                        &file_path,
                        &file_name,
                        &media_info_clone,
                        &season_str,
                        &ep,
                        tmdbid,
                    )
                    .await;
                } else {
                    debug!(
                        "[上传] 未能识别集数，文件保留在 upload_cache: {}",
                        file_name
                    );
                }
            });
        }

        while let Some(result) = handles.join_next().await {
            if let Err(e) = result {
                error!("[上传] 媒体处理任务失败: {}", e);
            }
        }
        debug!("[上传] 本批后台任务已结束，处理结果见逐文件日志");
    });

    HttpResponse::Ok().json(UploadResponse {
        success: true,
        message: format!("已成功接收 {} 个文件", file_count),
        file_count: Some(file_count),
    })
}

pub async fn add_episode(mut payload: Multipart) -> impl Responder {
    debug!("[上传] 开始处理单集上传请求");

    let upload_cache_dir = "upload_cache";
    if !Path::new(upload_cache_dir).exists() {
        if let Err(e) = fs::create_dir_all(upload_cache_dir).await {
            error!("[上传] 创建upload_cache目录失败: {}", e);
            return HttpResponse::InternalServerError().json(UploadResponse {
                success: false,
                message: format!("创建上传目录失败: {}", e),
                file_count: None,
            });
        }
    }

    let mut tmdbid: Option<u32> = None;
    let mut season_number: Option<i32> = None;
    let mut episode_number: Option<i32> = None;
    let mut files: Vec<(String, String)> = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(e) => {
                error!("[上传] 读取字段失败: {}", e);
                continue;
            }
        };

        let content_disposition = field.content_disposition();
        let field_name = match content_disposition.get_name() {
            Some(name) => name,
            None => continue,
        };

        if field_name == "tmdbid" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取tmdbid失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(id) = std::str::from_utf8(&bytes) {
                tmdbid = id.parse::<u32>().ok();
            }
        } else if field_name == "season_number" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取season_number失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(num) = std::str::from_utf8(&bytes) {
                season_number = num.parse::<i32>().ok();
            }
        } else if field_name == "episode_number" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取episode_number失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(num) = std::str::from_utf8(&bytes) {
                episode_number = num.parse::<i32>().ok();
            }
        } else if field_name == "file" {
            let filename = match content_disposition.get_filename() {
                Some(name) => name.to_string(),
                None => continue,
            };

            let safe_name = Path::new(&filename)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload");
            let file_path = format!(
                "{}/{:032x}-{}",
                upload_cache_dir,
                rand::random::<u128>(),
                safe_name
            );
            let bytes_written = match save_upload(&file_path, &mut field).await {
                Ok(written) => written,
                Err(error) => {
                    error!("[上传] 文件保存失败: {}", error);
                    return HttpResponse::InternalServerError().json(UploadResponse {
                        success: false,
                        message: "上传未完成，请重试".to_string(),
                        file_count: None,
                    });
                }
            };

            debug!(
                "[上传] 文件保存成功: {} ({} bytes)",
                file_path, bytes_written
            );
            files.push((filename, file_path));
        }
    }

    if tmdbid.is_none() || season_number.is_none() || episode_number.is_none() {
        return HttpResponse::BadRequest().json(UploadResponse {
            success: false,
            message: "缺少必要参数: tmdbid、season_number 或 episode_number".to_string(),
            file_count: None,
        });
    }

    let tmdbid = tmdbid.unwrap();
    let season_number = season_number.unwrap();
    let episode_number = episode_number.unwrap();
    let file_count = files.len();

    debug!(
        "[上传] 文件接收完成，共 {} 个文件，立即返回前端",
        file_count
    );

    let media_info = {
        let data = DATA_JSON.read().await;
        match data.media.get(&tmdbid) {
            Some(media) => media.clone(),
            None => {
                return HttpResponse::BadRequest().json(UploadResponse {
                    success: false,
                    message: format!("未找到 tmdbid 为 {} 的媒体", tmdbid),
                    file_count: None,
                });
            }
        }
    };

    tokio::spawn(async move {
        debug!("[上传] 开始异步处理文件");
        let mut handles = JoinSet::new();

        for (file_name, file_path) in files {
            if handles.len() >= 3 {
                if let Some(Err(e)) = handles.join_next().await {
                    error!("[上传] 媒体处理任务失败: {}", e);
                }
            }
            let media_info_clone = media_info.clone();
            let season_str = season_number.to_string();
            let ep_str = episode_number.to_string();

            handles.spawn(async move {
                info!("[上传] 处理文件: {}", file_name);
                process_file(
                    &file_path,
                    &file_name,
                    &media_info_clone,
                    &season_str,
                    &ep_str,
                    tmdbid,
                )
                .await;
            });
        }

        while let Some(result) = handles.join_next().await {
            if let Err(e) = result {
                error!("[上传] 媒体处理任务失败: {}", e);
            }
        }
        debug!("[上传] 本批后台任务已结束，处理结果见逐文件日志");
    });

    HttpResponse::Ok().json(UploadResponse {
        success: true,
        message: format!("已成功接收 {} 个文件", file_count),
        file_count: Some(file_count),
    })
}

async fn process_file(
    file_path: &str,
    file_name: &str,
    media_info: &MediaInfo,
    season: &str,
    ep: &str,
    tmdbid: u32,
) {
    let path = Path::new(file_path);
    let ext = match path.extension() {
        Some(ext) => ext.to_string_lossy().to_lowercase().to_string(),
        None => return,
    };

    let is_media = COMMON_MEDIA_EXTS.contains(&ext.as_str());
    let is_caption = COMMON_CAPTION_EXTS.contains(&ext.as_str());

    if !is_media && !is_caption {
        debug!("[上传] 文件不是媒体或字幕文件，跳过: {}", file_name);
        return;
    }

    let display_name = &media_info.display_name;
    let is_jp = false;
    let media_path = &media_info.media_path;

    debug!("[上传] 开始媒体处理: {}", file_name);

    let outcome = media_processor::rename_and_move(
        media_path,
        display_name,
        is_jp,
        season,
        ep,
        file_name,
        file_path,
        &ext,
        tmdbid,
    )
    .await;
    if !outcome.is_success() {
        error!(
            "[上传] 媒体未完成，保留上传缓存: {} ({:?})",
            file_path, outcome
        );
    }
}

pub async fn add_movie(mut payload: Multipart) -> impl Responder {
    debug!("[上传] 开始处理电影上传请求");

    let upload_cache_dir = "upload_cache";
    if !Path::new(upload_cache_dir).exists() {
        if let Err(e) = fs::create_dir_all(upload_cache_dir).await {
            error!("[上传] 创建upload_cache目录失败: {}", e);
            return HttpResponse::InternalServerError().json(UploadResponse {
                success: false,
                message: format!("创建上传目录失败: {}", e),
                file_count: None,
            });
        }
    }

    let mut display_name: Option<String> = None;
    let mut media_path: Option<String> = None;
    let mut files: Vec<(String, String)> = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(e) => {
                error!("[上传] 读取字段失败: {}", e);
                continue;
            }
        };

        let content_disposition = field.content_disposition();
        let field_name = match content_disposition.get_name() {
            Some(name) => name,
            None => continue,
        };

        if field_name == "display_name" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取display_name失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(name) = std::str::from_utf8(&bytes) {
                display_name = Some(name.to_string());
            }
        } else if field_name == "media_path" {
            let mut bytes = web::BytesMut::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[上传] 读取media_path失败: {}", e);
                        break;
                    }
                };
                bytes.extend_from_slice(&data);
            }
            if let Ok(path) = std::str::from_utf8(&bytes) {
                media_path = Some(path.to_string());
            }
        } else if field_name == "file" {
            let filename = match content_disposition.get_filename() {
                Some(name) => name.to_string(),
                None => continue,
            };

            let safe_name = Path::new(&filename)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload");
            let file_path = format!(
                "{}/{:032x}-{}",
                upload_cache_dir,
                rand::random::<u128>(),
                safe_name
            );
            let bytes_written = match save_upload(&file_path, &mut field).await {
                Ok(written) => written,
                Err(error) => {
                    error!("[上传] 文件保存失败: {}", error);
                    return HttpResponse::InternalServerError().json(UploadResponse {
                        success: false,
                        message: "上传未完成，请重试".to_string(),
                        file_count: None,
                    });
                }
            };

            debug!(
                "[上传] 文件保存成功: {} ({} bytes)",
                file_path, bytes_written
            );
            files.push((filename, file_path));
        }
    }

    if display_name.is_none() || media_path.is_none() {
        return HttpResponse::BadRequest().json(UploadResponse {
            success: false,
            message: "缺少必要参数: display_name 或 media_path".to_string(),
            file_count: None,
        });
    }

    let display_name = std::sync::Arc::new(display_name.unwrap());
    let media_path = std::sync::Arc::new(media_path.unwrap());
    let file_count = files.len();

    info!(
        "[上传] 电影文件接收完成，共 {} 个文件，立即返回前端",
        file_count
    );

    tokio::spawn(async move {
        debug!("[上传] 开始异步处理电影文件");
        let mut handles = JoinSet::new();

        for (file_name, file_path) in files {
            if handles.len() >= 3 {
                if let Some(Err(e)) = handles.join_next().await {
                    error!("[上传] 媒体处理任务失败: {}", e);
                }
            }
            let display_name_clone = display_name.clone();
            let media_path_clone = media_path.clone();

            handles.spawn(async move {
                debug!("[上传] 处理电影文件: {}", file_name);

                let path = Path::new(&file_path);
                let ext = match path.extension() {
                    Some(ext) => ext.to_string_lossy().to_lowercase().to_string(),
                    None => return,
                };

                let is_media = COMMON_MEDIA_EXTS.contains(&ext.as_str());
                let is_caption = COMMON_CAPTION_EXTS.contains(&ext.as_str());

                if !is_media && !is_caption {
                    debug!("[上传] 文件不是媒体或字幕文件，跳过: {}", file_name);
                    return;
                }

                debug!("[上传] 开始电影媒体处理: {}", file_name);

                let outcome = media_processor::rename_and_move_movie(
                    &file_path,
                    &display_name_clone,
                    &media_path_clone,
                    &file_name,
                    &ext,
                )
                .await;
                if !outcome.is_success() {
                    error!(
                        "[上传] 电影未完成，保留上传缓存: {} ({:?})",
                        file_path, outcome
                    );
                }
            });
        }

        while let Some(result) = handles.join_next().await {
            if let Err(e) = result {
                error!("[上传] 媒体处理任务失败: {}", e);
            }
        }
        debug!("[上传] 本批电影后台任务已结束，处理结果见逐文件日志");
    });

    HttpResponse::Ok().json(UploadResponse {
        success: true,
        message: format!("已成功接收 {} 个文件", file_count),
        file_count: Some(file_count),
    })
}
