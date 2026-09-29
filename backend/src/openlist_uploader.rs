use crate::models::constants::{COMMON_CAPTION_EXTS_SET, COMMON_MEDIA_EXTS_SET};
use crate::models::Config;
use crate::rate_limiter::RateLimiter;
use crate::{CLIENT, CONFIG};
use chrono::{Local, Timelike, Utc};
use log::{debug, error, info};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::fs::File;
use tokio::sync::{mpsc, Mutex, OnceCell, RwLock, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use tokio_util::io::ReaderStream;

const CACHE_TTL: i64 = 600;
const CACHE_FILE_PATH: &str = "openlist_cache.json";
const CHANNEL_BUFFER: usize = 100;
const CONCURRENCY: usize = 5;
const UPLOAD_QPS: u64 = 5;
const LIST_QPS: u64 = 3;

static CACHE: OnceLock<Arc<RwLock<HashMap<String, CacheEntry>>>> = OnceLock::new();
static CACHE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
static CACHE_LOADED: OnceCell<()> = OnceCell::const_new();
static SINGLE_UPLOAD_QUEUE: OnceCell<mpsc::Sender<String>> = OnceCell::const_new();
static EXECUTION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static UPLOAD_CONCURRENCY: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(CONCURRENCY));
static UPLOAD_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new(UPLOAD_QPS, 1));
static LIST_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new(LIST_QPS, 1));
static REMOVE_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new(LIST_QPS, 1));

pub async fn enqueue_upload(path: String) {
    let sender = SINGLE_UPLOAD_QUEUE
        .get_or_init(|| async {
            let (sender, mut receiver) = mpsc::channel::<String>(CHANNEL_BUFFER);
            tokio::spawn(async move {
                let mut tasks = JoinSet::new();
                while let Some(path) = receiver.recv().await {
                    if tasks.len() >= CONCURRENCY {
                        if let Some(Err(error)) = tasks.join_next().await {
                            error!("[OpenList上传] 上传任务执行失败: {}", error);
                        }
                    }
                    tasks.spawn(async move {
                        upload_single_file(&path).await;
                    });
                }
                while let Some(result) = tasks.join_next().await {
                    if let Err(error) = result {
                        error!("[OpenList上传] 上传任务执行失败: {}", error);
                    }
                }
            });
            sender
        })
        .await;
    if let Err(error) = sender.send(path).await {
        error!("[OpenList上传] 上传队列已关闭: {}", error);
    }
}

fn get_cache() -> &'static Arc<RwLock<HashMap<String, CacheEntry>>> {
    CACHE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

fn get_cache_mutex() -> &'static Mutex<()> {
    CACHE_MUTEX.get_or_init(|| Mutex::new(()))
}

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    timestamp: i64,
    files: Arc<HashMap<String, u64>>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Clone)]
struct UploadTask {
    local_file_path: String,
    filename: String,
    remote_dir_path: String,
    src_size: u64,
    remote_file_map: Arc<HashMap<String, u64>>,
}

#[derive(Serialize)]
struct ListRequest {
    path: String,
    password: String,
    page: i32,
    per_page: i32,
    refresh: bool,
}

#[derive(Deserialize)]
struct ListResponseItem {
    name: String,
    size: u64,
    is_dir: bool,
}

#[derive(Deserialize)]
struct ListResponseData {
    content: Vec<ListResponseItem>,
}

#[derive(Deserialize)]
struct ListResponse {
    message: String,
    data: Option<ListResponseData>,
}

#[derive(Serialize)]
struct FsRemoveRequest {
    dir: String,
    names: Vec<String>,
}

#[derive(Deserialize)]
struct FsRemoveResponse {
    code: i32,
    message: String,
}

async fn load_cache() {
    CACHE_LOADED.get_or_init(load_cache_once).await;
}

async fn load_cache_once() {
    let _lock = get_cache_mutex().lock().await;

    if !Path::new(CACHE_FILE_PATH).exists() {
        return;
    }

    match tokio::fs::read_to_string(CACHE_FILE_PATH).await {
        Ok(content) => match serde_json::from_str::<CacheFile>(&content) {
            Ok(cache_file) => {
                let mut cache = get_cache().write().await;
                *cache = cache_file.entries;
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
}

async fn save_cache() {
    let _lock = get_cache_mutex().lock().await;

    let mut cache = get_cache().write().await;
    let now = Utc::now().timestamp();
    cache.retain(|_, entry| now - entry.timestamp < CACHE_TTL);
    let cache_file = CacheFile {
        entries: cache.clone(),
    };
    drop(cache);

    if let Err(error) = crate::io_util::blocking(move || {
        crate::io_util::atomic_json(Path::new(CACHE_FILE_PATH), &cache_file)
    })
    .await
    .unwrap_or_else(|error| Err(std::io::Error::other(error)))
    {
        error!("[OpenList上传] 保存缓存失败: {}", error);
    }
}

fn is_media_or_subtitle_file(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    COMMON_MEDIA_EXTS_SET.contains(ext.as_str()) || COMMON_CAPTION_EXTS_SET.contains(ext.as_str())
}

fn get_remote_root_path(media_path: &str) -> String {
    let path = Path::new(media_path);
    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("media");
    format!("/{}", dir_name)
}

async fn update_cache_after_upload(remote_dir_path: &str, filename: &str, file_size: u64) {
    let now = Utc::now().timestamp();
    let mut cache = get_cache().write().await;

    if let Some(entry) = cache.get_mut(remote_dir_path) {
        Arc::make_mut(&mut entry.files).insert(filename.to_string(), file_size);
        entry.timestamp = now;
    }
}

async fn update_cache_after_remove(remote_dir_path: &str, names: &[String]) {
    let now = Utc::now().timestamp();
    let mut cache = get_cache().write().await;

    if let Some(entry) = cache.get_mut(remote_dir_path) {
        for name in names {
            Arc::make_mut(&mut entry.files).remove(name);
        }
        entry.timestamp = now;
    }
}

async fn get_remote_file_dict(
    remote_path: &str,
    config: &Config,
    client: &Arc<Client>,
) -> Option<Arc<HashMap<String, u64>>> {
    let now = Utc::now().timestamp();

    {
        let cache = get_cache().read().await;
        if let Some(entry) = cache.get(remote_path) {
            if now - entry.timestamp < CACHE_TTL {
                return Some(entry.files.clone());
            }
        }
    }

    LIST_LIMITER.acquire().await;
    let api_list_url = format!(
        "http://{}:{}/api/fs/list",
        config.openlist_address, config.openlist_port
    );

    let request = ListRequest {
        path: remote_path.to_string(),
        password: "".to_string(),
        page: 1,
        per_page: 0,
        refresh: false,
    };

    let response = client
        .post(&api_list_url)
        .header("Authorization", &config.openlist_apikey)
        .json(&request)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await;

    match response {
        Ok(res) => {
            let status = res.status();
            match res.json::<ListResponse>().await {
                Ok(json_res) => {
                    let msg = json_res.message.to_lowercase();

                    if msg.contains("success") {
                        if let Some(data) = json_res.data {
                            let file_map: HashMap<String, u64> = data
                                .content
                                .into_iter()
                                .filter(|item| !item.is_dir)
                                .map(|item| (item.name, item.size))
                                .collect();

                            let file_map = Arc::new(file_map);
                            let entry = CacheEntry {
                                timestamp: now,
                                files: file_map.clone(),
                            };

                            let mut cache = get_cache().write().await;
                            cache.insert(remote_path.to_string(), entry);

                            return Some(file_map);
                        }
                        return Some(Arc::new(HashMap::new()));
                    } else if msg.contains("object not found") {
                        return Some(Arc::new(HashMap::new()));
                    } else {
                        debug!(
                            "[OpenList上传] 跳过目录：{}, 状态码: {}, message: {}",
                            remote_path, status, json_res.message
                        );
                        return None;
                    }
                }
                Err(e) => {
                    debug!(
                        "[OpenList上传] 解析响应失败: {}, 错误: {:?}",
                        remote_path, e
                    );
                    None
                }
            }
        }
        Err(e) => {
            debug!(
                "[OpenList上传] 列出云端目录失败: {}, 错误: {:?}",
                remote_path, e
            );
            None
        }
    }
}

async fn upload_single_file_internal(
    local_file_path: &str,
    remote_file_path: &str,
    config: &Config,
    client: &Arc<Client>,
) -> bool {
    let _permit = UPLOAD_CONCURRENCY
        .acquire()
        .await
        .expect("upload limiter closed");
    let upload_path = urlencoding::encode(remote_file_path);
    let api_upload_url = format!(
        "http://{}:{}/api/fs/put",
        config.openlist_address, config.openlist_port
    );

    let file = match File::open(local_file_path).await {
        Ok(f) => f,
        Err(e) => {
            debug!(
                "[OpenList上传] 打开文件失败: {}, 错误: {:?}",
                local_file_path, e
            );
            return false;
        }
    };

    let file_size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(_) => return false,
    };
    let stream = ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);

    UPLOAD_LIMITER.acquire().await;
    let response = client
        .put(&api_upload_url)
        .header("Authorization", &config.openlist_apikey)
        .header("File-Path", upload_path.to_string())
        .header("As-Task", "true")
        .header("Content-Length", file_size.to_string())
        .header("Content-Type", "application/octet-stream")
        .body(body)
        .send()
        .await;

    match response {
        Ok(res) => res.status().is_success(),
        Err(e) => {
            debug!(
                "[OpenList上传] 上传失败: {}, 错误: {:?}",
                local_file_path, e
            );
            false
        }
    }
}

async fn remove_remote_files(
    remote_dir_path: &str,
    names: &[String],
    config: &Config,
    client: &Arc<Client>,
) -> usize {
    if names.is_empty() {
        return 0;
    }

    REMOVE_LIMITER.acquire().await;
    let api_remove_url = format!(
        "http://{}:{}/api/fs/remove",
        config.openlist_address, config.openlist_port
    );

    let request = FsRemoveRequest {
        dir: remote_dir_path.to_string(),
        names: names.to_vec(),
    };

    let response = client
        .post(&api_remove_url)
        .header("Authorization", &config.openlist_apikey)
        .json(&request)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    match response {
        Ok(res) => {
            if res.status().is_success() {
                match res.json::<FsRemoveResponse>().await {
                    Ok(json_res) if json_res.code == 200 => {
                        let count = names.len();
                        info!(
                            "[OpenList删除] 删除成功: {} 个文件 (目录: {})",
                            count, remote_dir_path
                        );
                        update_cache_after_remove(remote_dir_path, names).await;
                        return count;
                    }
                    Ok(json_res) => {
                        info!(
                            "[OpenList删除] 删除失败: 目录={}, code={}, message={}",
                            remote_dir_path, json_res.code, json_res.message
                        );
                    }
                    Err(e) => {
                        info!(
                            "[OpenList删除] 解析响应失败: 目录={}, 错误={:?}",
                            remote_dir_path, e
                        );
                    }
                }
            } else {
                info!(
                    "[OpenList删除] HTTP错误: 目录={}, 状态码={}",
                    remote_dir_path,
                    res.status()
                );
            }
        }
        Err(e) => {
            info!(
                "[OpenList删除] 请求失败: 目录={}, 错误={:?}",
                remote_dir_path, e
            );
        }
    }
    0
}

pub async fn upload_single_file(local_file_path: &str) -> bool {
    let config = Arc::new(CONFIG.read().await.clone());

    if !config.openlist_auto_upload {
        return false;
    }

    if !Path::new(local_file_path).exists() {
        debug!("[OpenList上传] 文件不存在: {}", local_file_path);
        return false;
    }

    if !is_media_or_subtitle_file(local_file_path) {
        return false;
    }

    let client_guard = CLIENT.read().await;
    let client = match client_guard.as_ref() {
        Some(c) => c.clone(),
        None => {
            debug!("[OpenList上传] HTTP客户端未初始化");
            return false;
        }
    };
    drop(client_guard);

    let media_path = &config.media_library_path;
    let remote_root_path = get_remote_root_path(media_path);

    let local_path = Path::new(local_file_path);
    let media_path_p = Path::new(media_path);

    let relative_path = match local_path.strip_prefix(media_path_p) {
        Ok(p) => p,
        Err(_) => {
            debug!("[OpenList上传] 文件不在媒体库目录中: {}", local_file_path);
            return false;
        }
    };

    let remote_file_path = format!(
        "{}/{}",
        remote_root_path,
        relative_path.to_str().unwrap_or("").replace("\\", "/")
    );

    debug!(
        "[OpenList上传] 正在上传文件: {} -> {}",
        local_file_path, remote_file_path
    );

    upload_single_file_internal(local_file_path, &remote_file_path, &config, &client).await
}

async fn scan_dirs(
    root: String,
    tx: mpsc::Sender<UploadTask>,
    config: Arc<Config>,
    client: Arc<Client>,
    compared_count: Arc<AtomicUsize>,
    failed_count: Arc<AtomicUsize>,
    deleted_count: Arc<AtomicUsize>,
) {
    let remote_root_path = get_remote_root_path(&root);
    let mut stack = vec![root.clone()];

    while let Some(dir) = stack.pop() {
        let path = Path::new(&dir);

        if !path.is_dir() {
            continue;
        }

        let relative_path = match path.strip_prefix(&root) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let remote_dir_path = format!(
            "{}/{}",
            remote_root_path,
            relative_path.to_str().unwrap_or("").replace("\\", "/")
        );

        let remote_map = match get_remote_file_dict(&remote_dir_path, &config, &client).await {
            Some(m) => m,
            None => continue,
        };

        let mut rd = match tokio::fs::read_dir(path).await {
            Ok(r) => r,
            Err(e) => {
                error!("[OpenList上传] 读取目录失败 {}: {}", dir, e);
                continue;
            }
        };

        let mut subdirs: Vec<String> = Vec::new();
        let mut local_filenames: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut scan_complete = true;

        loop {
            match rd.next_entry().await {
                Ok(Some(entry)) => {
                    let p = entry.path();
                    let metadata = match tokio::fs::metadata(&p).await {
                        Ok(metadata) => metadata,
                        Err(_) => {
                            scan_complete = false;
                            failed_count.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    };

                    if metadata.is_dir() {
                        if let Some(path_str) = p.to_str() {
                            subdirs.push(path_str.to_string());
                        } else {
                            scan_complete = false;
                        }
                        continue;
                    }

                    let file = p.to_string_lossy().to_string();

                    if !is_media_or_subtitle_file(&file) {
                        continue;
                    }

                    let filename = match p.file_name().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => {
                            scan_complete = false;
                            continue;
                        }
                    };

                    let size = metadata.len();

                    local_filenames.insert(filename.clone());

                    let task = UploadTask {
                        local_file_path: file,
                        filename,
                        remote_dir_path: remote_dir_path.clone(),
                        src_size: size,
                        remote_file_map: remote_map.clone(),
                    };

                    compared_count.fetch_add(1, Ordering::Relaxed);

                    if tx.send(task).await.is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    scan_complete = false;
                    failed_count.fetch_add(1, Ordering::Relaxed);
                    error!("[OpenList上传] 读取目录条目失败: {}", e);
                    break;
                }
            }
        }

        // 删除云端存在但本地不存在的媒体/字幕文件
        let remote_only: Vec<String> = remote_map
            .keys()
            .filter(|name| !local_filenames.contains(*name) && is_media_or_subtitle_file(name))
            .cloned()
            .collect();

        if !scan_complete {
            error!("[OpenList删除] 本地目录扫描不完整，跳过删除: {}", dir);
        } else if !remote_only.is_empty() {
            debug!(
                "[OpenList删除] 目录 {} 中云端多余文件: {:?}",
                remote_dir_path, remote_only
            );
            let removed =
                remove_remote_files(&remote_dir_path, &remote_only, &config, &client).await;
            deleted_count.fetch_add(removed, Ordering::Relaxed);
        }

        for subdir in subdirs.into_iter().rev() {
            stack.push(subdir);
        }
    }
}

async fn process_upload(
    task: UploadTask,
    config: &Config,
    client: &Arc<Client>,
    uploaded_count: Arc<AtomicUsize>,
    failed_count: Arc<AtomicUsize>,
    skipped_count: Arc<AtomicUsize>,
) {
    let dst_size = task.remote_file_map.get(&task.filename);

    let should_upload;
    let reason: String;

    if dst_size.is_none() {
        should_upload = true;
        reason = "云端文件不存在".to_string();
    } else if *dst_size.unwrap() != task.src_size {
        should_upload = true;
        reason = format!(
            "文件大小不一致，本地: {}，云端: {}",
            task.src_size,
            dst_size.unwrap()
        );
    } else {
        should_upload = false;
        reason = "文件已存在且大小一致".to_string();
    }

    if !should_upload {
        skipped_count.fetch_add(1, Ordering::Relaxed);
        return;
    }

    debug!(
        "[OpenList上传] 需要上传: {}，原因: {}",
        task.filename, reason
    );

    let remote_file_path = format!("{}/{}", task.remote_dir_path, task.filename);

    if upload_single_file_internal(&task.local_file_path, &remote_file_path, config, client).await {
        uploaded_count.fetch_add(1, Ordering::Relaxed);
        update_cache_after_upload(&task.remote_dir_path, &task.filename, task.src_size).await;
        info!("[OpenList上传] ✅ 上传成功: {}", task.filename);
    } else {
        failed_count.fetch_add(1, Ordering::Relaxed);
        error!("[OpenList上传] ❌ 上传失败: {}", task.filename);
    }
}

pub struct OpenListUploadManager;

async fn wait_for_uploads(uploads: &mut JoinSet<()>, failed_count: &AtomicUsize) {
    while let Some(result) = uploads.join_next().await {
        if let Err(e) = result {
            error!("[OpenList上传] 上传任务异常: {}", e);
            failed_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl OpenListUploadManager {
    pub async fn start() {
        info!("[OpenList上传] 自动上传服务已启动");
        load_cache().await;

        tokio::spawn(async {
            let mut last_hour: Option<u32> = None;

            loop {
                let config = CONFIG.read().await;
                if config.openlist_auto_upload {
                    drop(config);

                    let now = Local::now();
                    let current_hour = now.hour();
                    let current_minute = now.minute();

                    if last_hour != Some(current_hour) && current_minute < 2 {
                        debug!(
                            "[OpenList上传] 到达整点 {}:00，开始执行自动上传",
                            current_hour
                        );
                        last_hour = Some(current_hour);

                        Self::execute_upload_all().await;
                    }
                } else {
                    drop(config);
                }

                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    pub async fn execute_now() {
        info!("[OpenList上传] 立即执行一次完整上传");
        load_cache().await;
        Self::execute_upload_all().await;
    }

    async fn execute_upload_all() {
        let _execution = EXECUTION_LOCK.lock().await;
        load_cache().await;
        let config = Arc::new(CONFIG.read().await.clone());
        let media_root = config.media_library_path.clone();

        if !Path::new(&media_root).exists() {
            error!("[OpenList上传] 媒体库目录不存在: {}", media_root);
            return;
        }

        let client_guard = CLIENT.read().await;
        let client = match client_guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                error!("[OpenList上传] HTTP客户端未初始化");
                return;
            }
        };
        drop(client_guard);

        debug!("[OpenList上传] 开始扫描媒体库: {}", media_root);

        let uploaded_count = Arc::new(AtomicUsize::new(0));
        let failed_count = Arc::new(AtomicUsize::new(0));
        let skipped_count = Arc::new(AtomicUsize::new(0));
        let compared_count = Arc::new(AtomicUsize::new(0));
        let deleted_count = Arc::new(AtomicUsize::new(0));

        let concurrency = Arc::new(Semaphore::new(CONCURRENCY));

        let (tx, mut rx) = mpsc::channel::<UploadTask>(CHANNEL_BUFFER);

        let scan_task = tokio::spawn({
            let tx = tx.clone();
            let config = config.clone();
            let client = client.clone();
            let compared_count = compared_count.clone();
            let failed_count = failed_count.clone();
            let deleted_count = deleted_count.clone();

            async move {
                scan_dirs(
                    media_root,
                    tx,
                    config,
                    client,
                    compared_count,
                    failed_count,
                    deleted_count,
                )
                .await;
            }
        });

        drop(tx);

        let mut uploads = JoinSet::new();
        while let Some(task) = rx.recv().await {
            if uploads.len() >= CONCURRENCY {
                if let Some(Err(e)) = uploads.join_next().await {
                    error!("[OpenList上传] 上传任务异常: {}", e);
                    failed_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            let permit = concurrency.clone().acquire_owned().await.unwrap();

            let client = client.clone();
            let config = config.clone();
            let uploaded_count = uploaded_count.clone();
            let failed_count = failed_count.clone();
            let skipped_count = skipped_count.clone();

            uploads.spawn(async move {
                let _permit = permit;

                process_upload(
                    task,
                    &config,
                    &client,
                    uploaded_count,
                    failed_count,
                    skipped_count,
                )
                .await;
            });
        }

        if let Err(e) = scan_task.await {
            error!("[OpenList上传] 扫描任务异常: {}", e);
            failed_count.fetch_add(1, Ordering::Relaxed);
        }
        wait_for_uploads(&mut uploads, &failed_count).await;
        save_cache().await;

        let uploaded = uploaded_count.load(Ordering::Relaxed);
        let failed = failed_count.load(Ordering::Relaxed);
        let skipped = skipped_count.load(Ordering::Relaxed);
        let compared = compared_count.load(Ordering::Relaxed);
        let deleted = deleted_count.load(Ordering::Relaxed);

        debug!("[OpenList上传] 扫描完成，对比 {} 个文件，上传成功 {} 个，上传失败 {} 个，跳过 {} 个，删除多余 {} 个",
                 compared, uploaded, failed, skipped, deleted);
    }
}
