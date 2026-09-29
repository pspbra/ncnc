//! 后端服务主文件
//!
//! 本文件实现了一个基于Actix Web框架的后端服务，主要提供以下功能：
//! 1. 用户登录认证
//! 2. 静态文件服务（前端WebUI）
//! 3. 路由重定向
//! 4. 配置文件管理

use actix_web::{App, HttpServer};
use chrono::Timelike;
use dashmap::DashSet;
use log::{error, info};
use once_cell::sync::Lazy;
use reqwest::Client;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::sync::{RwLock, Semaphore};

mod aria2_client;
mod auto_download;
mod auto_rss;
mod data_store;
mod download_cache;
mod handlers;
mod io_util;
mod logging;
mod media_processor;
mod media_tools;
mod middleware;
mod models;
mod monitor;
mod openlist_uploader;
mod rate_limiter;
mod runtime_state;
mod skyhook;

use models::{Config, DataJson, DownloadData, UploadData};

// 全局配置管理器
pub static CONFIG: Lazy<Arc<RwLock<Config>>> = Lazy::new(|| {
    let config = load_or_create_config();
    Arc::new(RwLock::new(config))
});

// 全局HTTP客户端管理器
pub static CLIENT: Lazy<Arc<RwLock<Option<Arc<Client>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

// 正在处理的 TMDB ID 集合
pub static PROCESSING_TMDBIDS: Lazy<DashSet<u32>> = Lazy::new(|| DashSet::new());

// 全局 data.json 内存缓存
pub static DATA_JSON: Lazy<Arc<data_store::DataStore>> =
    Lazy::new(|| Arc::new(data_store::DataStore::new(load_data_json())));

pub static RUNTIME_STATE: Lazy<Arc<runtime_state::RuntimeStateStore>> =
    Lazy::new(|| Arc::new(runtime_state::RuntimeStateStore::new(runtime_state::load())));

/// 加载 data.json
fn load_data_json() -> DataJson {
    let data_json_path = "data.json";

    if Path::new(data_json_path).exists() {
        match fs::read_to_string(data_json_path) {
            Ok(content) => {
                if let Ok(d) = serde_json::from_str::<DataJson>(&content) {
                    return d;
                }
            }
            Err(e) => {
                error!("[加载] 读取 data.json 失败: {}", e);
            }
        }
    }

    DataJson::default()
}

/// 保存 data.json 到文件
pub async fn save_data_json() {
    if let Err(e) = DATA_JSON.save_to("data.json".into()).await {
        error!("[数据] 保存 data.json 失败: {}", e);
    }
}

pub static SHUTDOWN_REQUEST: Lazy<Notify> = Lazy::new(Notify::new);
pub static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

// 全局下载状态缓存
pub static DOWNLOAD_STATUS_CACHE: Lazy<Arc<RwLock<Vec<DownloadData>>>> =
    Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

// 全局上传状态缓存
pub static UPLOAD_STATUS_CACHE: Lazy<Arc<RwLock<Vec<UploadData>>>> =
    Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

// 全局系统信息缓存
pub static SYSTEM_INFO_CACHE: Lazy<Arc<RwLock<Option<models::SystemInfo>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

// 媒体处理并发限制器，最大并发数为3
pub static MEDIA_PROCESS_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(3)));

// 自动下载扫描互斥锁，防止 TOCTOU 竞态
pub static AUTODOWNLOAD_LOCK: Lazy<Arc<tokio::sync::Mutex<()>>> =
    Lazy::new(|| Arc::new(tokio::sync::Mutex::new(())));

/// 加载或创建配置文件
fn load_or_create_config() -> Config {
    let config_path = "config.json";

    if Path::new(config_path).exists() {
        match fs::read_to_string(config_path) {
            Ok(content) => match serde_json::from_str::<Config>(&content) {
                Ok(mut config) => {
                    let mut need_save = false;

                    if !config.password.starts_with("$2b$") {
                        if let Ok(hashed) = crate::models::hash_password(&config.password) {
                            config.password = hashed;
                            need_save = true;
                            info!("[配置] 密码已自动升级为bcrypt加密存储");
                        }
                    }

                    if config.jwt_secret.is_empty() {
                        config.jwt_secret = crate::models::generate_random_jwt_secret();
                        need_save = true;
                        info!("[配置] 已自动生成JWT密钥");
                    }

                    if need_save {
                        save_config(&config);
                    }

                    config
                }
                Err(_) => {
                    let default_config = Config::default();
                    save_config(&default_config);
                    default_config
                }
            },
            Err(_) => {
                let default_config = Config::default();
                save_config(&default_config);
                default_config
            }
        }
    } else {
        let default_config = Config::default();
        save_config(&default_config);
        default_config
    }
}

/// 保存配置文件
pub fn save_config(config: &Config) {
    if let Err(e) = io_util::atomic_json(Path::new("config.json"), config) {
        error!("[配置] 保存失败: {}", e);
    }
}

pub async fn save_current_config() {
    static WRITER: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));
    let _writer = WRITER.lock().await;
    let snapshot = CONFIG.read().await.clone();
    match io_util::blocking(move || io_util::atomic_json(Path::new("config.json"), &snapshot)).await
    {
        Ok(Ok(())) => {}
        result => error!("[配置] 保存失败: {:?}", result),
    }
}

/// 根据配置创建HTTP客户端
pub async fn create_client_from_config() {
    let config = CONFIG.read().await;
    let client = if config.proxy_enabled {
        let proxy_url = format!("http://{}:{}", config.proxy_address, config.proxy_port);
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            Client::builder()
                .proxy(proxy)
                .build()
                .unwrap_or_else(|_| Client::new())
        } else {
            Client::new()
        }
    } else {
        Client::new()
    };

    let mut client_guard = CLIENT.write().await;
    *client_guard = Some(Arc::new(client));
}

/// 主函数
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    logging::init()?;
    media_tools::init().await;

    create_client_from_config().await;
    monitor::Monitor::start_aria2();

    tokio::spawn(async {
        monitor::Monitor::start_monitor().await;
    });

    tokio::spawn(async {
        auto_download::AutoDownloadManager::start().await;
    });

    tokio::spawn(async {
        handlers::system::start_system_monitor().await;
    });

    tokio::spawn(async {
        openlist_uploader::OpenListUploadManager::start().await;
    });

    tokio::spawn(async {
        auto_rss::AutoRssManager::start().await;
    });

    let save_stop = Arc::new(Notify::new());
    let save_stop_worker = save_stop.clone();
    let save_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = save_stop_worker.notified() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => save_data_json().await,
            }
        }
    });

    let runtime_save_stop = save_stop.clone();
    let runtime_save_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = runtime_save_stop.notified() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                    if let Err(error) = RUNTIME_STATE.save_to(runtime_state::RUNTIME_STATE_PATH.into()).await {
                        error!("[运行状态] 保存失败: {}", error);
                    }
                },
            }
        }
    });

    tokio::spawn(async {
        let mut last_hour: Option<u32> = None;
        loop {
            let now = chrono::Local::now();
            let current_hour = now.hour();

            if last_hour != Some(current_hour) {
                last_hour = Some(current_hour);

                // update-all 每 4 小时执行一次（0/4/8/12/16/20 点）
                if current_hour % 4 == 2 {
                    info!("[定时任务] 到达整点（每4小时），执行 update-all");
                    let _ = handlers::update_all().await;
                }

                info!("[定时任务] 检查是否需要扫描 autodownload 目录");
                monitor::Monitor::hourly_autodownload_scan().await;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });

    info!("[系统] 服务器正在启动");

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::auth::AuthMiddleware)
            .wrap(actix_web::middleware::Compress::default())
            .route("/v1/user/login", actix_web::web::post().to(handlers::login))
            .route("/v1/search", actix_web::web::post().to(handlers::search))
            .route(
                "/v1/resourcesearch",
                actix_web::web::post().to(handlers::resource_search),
            )
            .route(
                "/v1/subscribe",
                actix_web::web::post().to(handlers::subscribe),
            )
            .route(
                "/v1/media/all",
                actix_web::web::get().to(handlers::get_all_media),
            )
            .route(
                "/v1/media/{tmdbid}",
                actix_web::web::get().to(handlers::get_media_by_id),
            )
            .route(
                "/v1/media/delete",
                actix_web::web::post().to(handlers::delete_media),
            )
            .route(
                "/v1/media/settings",
                actix_web::web::post().to(handlers::update_media_settings),
            )
            .route(
                "/v1/settings",
                actix_web::web::get().to(handlers::get_settings),
            )
            .route(
                "/v1/settings",
                actix_web::web::post().to(handlers::update_settings),
            )
            .route(
                "/v1/update-all",
                actix_web::web::get().to(handlers::update_all),
            )
            .route(
                "/v1/update-single/{tmdbid}",
                actix_web::web::get().to(handlers::update_single),
            )
            .route(
                "/v1/download/add",
                actix_web::web::post().to(handlers::add_download),
            )
            .route(
                "/v1/download/all",
                actix_web::web::get().to(handlers::get_downloads),
            )
            .route(
                "/v1/download/clear",
                actix_web::web::post().to(handlers::clear_downloads),
            )
            .route(
                "/v1/log/dates",
                actix_web::web::get().to(handlers::get_log_dates),
            )
            .route(
                "/v1/log/content",
                actix_web::web::get().to(handlers::get_log_content),
            )
            .route(
                "/v1/upload/all",
                actix_web::web::get().to(handlers::get_uploads),
            )
            .route(
                "/v1/upload/addseason",
                actix_web::web::post().to(handlers::add_season),
            )
            .route(
                "/v1/upload/addepisode",
                actix_web::web::post().to(handlers::add_episode),
            )
            .route(
                "/v1/upload/addmovie",
                actix_web::web::post().to(handlers::add_movie),
            )
            .route(
                "/v1/download/addmovie",
                actix_web::web::post().to(handlers::add_movie_download),
            )
            .route(
                "/v1/movie/search",
                actix_web::web::post().to(handlers::search_movie),
            )
            .route(
                "/v1/upload/moviesearch",
                actix_web::web::post().to(handlers::search_movie),
            )
            .route(
                "/v1/systems/info",
                actix_web::web::get().to(handlers::get_system_info),
            )
            .route(
                "/v1/app/restart",
                actix_web::web::post().to(handlers::restart_app),
            )
            .route("/", actix_web::web::get().to(handlers::redirect_to_webui))
            .route(
                "/webui{_:.*}",
                actix_web::web::get().to(handlers::webui_handler),
            )
    })
    .disable_signals()
    .bind("0.0.0.0:3000")?
    .run();
    let handle = server.handle();
    tokio::spawn(async move {
        tokio::select! {
            _ = shutdown_signal() => {},
            _ = SHUTDOWN_REQUEST.notified() => {},
        }
        handle.stop(true).await;
    });
    let server_result = server.await;
    save_stop.notify_waiters();
    let _ = save_task.await;
    let _ = runtime_save_task.await;

    io_util::blocking(monitor::Monitor::stop_aria2_sync)
        .await
        .map_err(std::io::Error::other)?;
    // Hold the write lock through the final save and process shutdown.
    let final_data = DATA_JSON.write().await;
    let snapshot = final_data.clone();
    io_util::blocking(move || io_util::atomic_json(Path::new("data.json"), &snapshot))
        .await
        .map_err(std::io::Error::other)??;
    RUNTIME_STATE
        .save_to(runtime_state::RUNTIME_STATE_PATH.into())
        .await?;
    if RESTART_REQUESTED.load(Ordering::Acquire) {
        std::process::Command::new(std::env::current_exe()?)
            .args(std::env::args().skip(1))
            .spawn()?;
    }
    server_result
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
        tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
