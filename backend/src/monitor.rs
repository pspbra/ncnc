use crate::{
    aria2_client::Aria2Client, media_processor, models::*, AUTODOWNLOAD_LOCK, CLIENT, CONFIG,
    DATA_JSON, DOWNLOAD_STATUS_CACHE, PROCESSING_TMDBIDS, UPLOAD_STATUS_CACHE,
};
use log::{debug, error, info};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::fs;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{Duration, Instant};
use walkdir::WalkDir;

static DIRECTORY_SCANS: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(2)));

#[derive(Clone, Copy)]
struct RetryState {
    attempts: u32,
    next_attempt: Instant,
}

fn retry_delay(attempts: u32) -> Duration {
    const DELAYS: [u64; 4] = [60, 5 * 60, 30 * 60, 60 * 60];
    Duration::from_secs(DELAYS[(attempts.saturating_sub(1) as usize).min(DELAYS.len() - 1)])
}

pub static ARIA2_CHILD: Lazy<Arc<Mutex<Option<Child>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

pub struct Monitor;

impl Monitor {
    pub async fn start_monitor() {
        info!("[下载监控] 开始监控下载任务");

        let processed_gids: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
        let retry_states: Arc<RwLock<HashMap<String, RetryState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let mut completion_task: Option<tokio::task::JoinHandle<()>> = None;

        let mut last_orphan_check = tokio::time::Instant::now() - Duration::from_secs(60);

        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            if completion_task
                .as_ref()
                .is_some_and(|task| task.is_finished())
            {
                if let Err(e) = completion_task.take().unwrap().await {
                    error!("[下载监控] 完成处理任务异常: {}", e);
                }
            }

            let (all_statuses, aria2_reachable) = Self::fetch_all_statuses().await;

            let mut completed_statuses: Vec<Aria2Status> = Vec::new();
            let mut known_gids: HashSet<String> = HashSet::new();
            let now = Instant::now();
            let processed_snapshot = processed_gids.read().await.clone();
            let retry_snapshot = retry_states.read().await.clone();
            for status in &all_statuses {
                known_gids.insert(status.gid.clone());
                known_gids.extend(status.followed_by.iter().cloned());
                let retry_ready = retry_snapshot
                    .get(&status.gid)
                    .is_none_or(|state| state.next_attempt <= now);
                if status.status == "complete"
                    && completion_task.is_none()
                    && !processed_snapshot.contains(&status.gid)
                    && retry_ready
                {
                    completed_statuses.push(status.clone());
                }
            }

            tokio::join!(
                Self::update_download_cache(&all_statuses),
                Self::update_upload_cache(),
            );

            if completion_task.is_none() && aria2_reachable {
                processed_gids
                    .write()
                    .await
                    .retain(|gid| known_gids.contains(gid));
                retry_states
                    .write()
                    .await
                    .retain(|gid, _| known_gids.contains(gid));
            }
            if !completed_statuses.is_empty() {
                let processed_gids_clone = processed_gids.clone();
                let retry_states_clone = retry_states.clone();
                completion_task = Some(tokio::spawn(async move {
                    Self::check_and_process_completed(
                        completed_statuses,
                        &processed_gids_clone,
                        &retry_states_clone,
                    )
                    .await;
                }));
            }

            if last_orphan_check.elapsed() >= Duration::from_secs(60) {
                Self::cleanup_orphan_caches(&known_gids, aria2_reachable).await;
                last_orphan_check = tokio::time::Instant::now();
            }
        }
    }

    pub fn start_aria2() {
        let work_dir = std::env::current_dir().unwrap();
        let conf_path = work_dir.join("aria2.conf");
        let current_pid = std::process::id();

        info!(
            "[下载监控] 启动 aria2c，配置文件: {}",
            conf_path.to_string_lossy()
        );

        match Command::new("/usr/bin/aria2c")
            .arg("--conf-path")
            .arg(conf_path)
            .arg(format!("--stop-with-process={}", current_pid))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                let mut child_guard = ARIA2_CHILD.lock().unwrap();
                *child_guard = Some(child);
                info!("[下载监控] aria2c 已启动，PID: {}", current_pid);
            }
            Err(e) => {
                error!("[下载监控] 启动 aria2c 失败: {}", e);
            }
        }
    }

    pub fn stop_aria2_sync() {
        let child = {
            let mut child_guard = ARIA2_CHILD.lock().unwrap();
            child_guard.take()
        };

        if let Some(mut child) = child {
            info!("[下载监控] 正在停止 aria2c...");
            let _ = child.kill();
            let _ = child.wait();
            info!("[下载监控] aria2c 已停止");
        }
    }

    async fn delete_cache(cache: &DownloadCache) -> bool {
        crate::download_cache::remove_download(&cache.gid).await
    }
    async fn delete_movie_cache(cache: &MovieDownloadCache) -> bool {
        crate::download_cache::remove_movie(&cache.gid).await
    }

    /// 返回 (所有任务状态, aria2是否可达)
    /// 等待/停止任务读取全部分页，只有全部调用成功才允许清理。
    async fn fetch_all_statuses() -> (Vec<Aria2Status>, bool) {
        let mut all: Vec<Aria2Status> = Vec::new();
        let mut aria2_reachable = true;

        match Aria2Client::tell_active().await {
            Ok(statuses) => all.extend(statuses),
            Err(_) => aria2_reachable = false,
        }
        match Aria2Client::tell_all_waiting().await {
            Ok(statuses) => all.extend(statuses),
            Err(e) => {
                error!("[下载监控] 等待任务列表不完整: {}", e);
                aria2_reachable = false;
            }
        }
        match Aria2Client::tell_all_stopped().await {
            Ok(statuses) => all.extend(statuses),
            Err(e) => {
                error!("[下载监控] 停止任务列表不完整: {}", e);
                aria2_reachable = false;
            }
        }

        (all, aria2_reachable)
    }

    /// 清理孤儿缓存：aria2 上已不存在的任务对应的缓存文件
    /// - aria2_reachable = false: 连接失败，跳过清理以免误删
    /// - 列表完整时对疑似孤儿再次查询，仅明确不存在且缓存未变化时删除
    async fn cleanup_orphan_caches(known_gids: &HashSet<String>, aria2_reachable: bool) {
        if !aria2_reachable {
            info!("[下载监控] aria2 连接失败，跳过孤儿缓存清理");
            return;
        }

        let mut removed_count = 0;
        let snapshot = match crate::download_cache::load_snapshot().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                error!("[下载监控] 读取统一运行状态失败: {}", error);
                return;
            }
        };
        for cache in snapshot.downloads.values() {
            if cache.file_paths.is_empty()
                && !known_gids.contains(&cache.gid)
                && matches!(Aria2Client::task_exists(&cache.gid).await, Ok(false))
                && crate::download_cache::remove_download(&cache.gid).await
            {
                removed_count += 1;
            }
        }
        for cache in snapshot.movies.values() {
            if cache.file_paths.is_empty()
                && !known_gids.contains(&cache.gid)
                && matches!(Aria2Client::task_exists(&cache.gid).await, Ok(false))
                && crate::download_cache::remove_movie(&cache.gid).await
            {
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            info!(
                "[下载监控] 孤儿缓存清理完成，共移除 {} 个文件",
                removed_count
            );
        }
    }

    async fn check_and_process_completed(
        completed_statuses: Vec<Aria2Status>,
        processed_gids: &Arc<RwLock<HashSet<String>>>,
        retry_states: &Arc<RwLock<HashMap<String, RetryState>>>,
    ) {
        let snapshot = match crate::download_cache::load_snapshot().await {
            Ok(snapshot) => snapshot,
            Err(e) => {
                error!("[下载监控] 扫描下载缓存失败: {}", e);
                return;
            }
        };
        for status in completed_statuses {
            let gid = &status.gid;
            if processed_gids.read().await.contains(gid) {
                continue;
            }
            let Some(first) = status.files.first() else {
                if !status.followed_by.is_empty() {
                    processed_gids.write().await.insert(gid.clone());
                }
                continue;
            };
            let Some((movie, cache_gid)) = snapshot.paths.get(&first.path) else {
                continue;
            };
            let mut successful = true;
            let mut permanent_failure = false;
            let mut retryable_failure = false;
            let mut candidates = 0;
            if *movie {
                let Some(cache) = snapshot.movies.get(cache_gid) else {
                    continue;
                };
                for file in status.files.iter().filter(|file| file.selected != "false") {
                    let path = Path::new(&file.path);
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !COMMON_MEDIA_EXTS_SET.contains(ext.as_str())
                        && !COMMON_CAPTION_EXTS_SET.contains(ext.as_str())
                    {
                        continue;
                    }
                    candidates += 1;
                    let outcome = media_processor::rename_and_move_movie(
                        &file.path,
                        &cache.display_name,
                        &cache.media_path,
                        path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                        &ext,
                    )
                    .await;
                    Self::record_process_outcome(
                        &outcome,
                        &mut successful,
                        &mut permanent_failure,
                        &mut retryable_failure,
                    );
                }
                if successful && candidates > 0 {
                    successful = Self::remove_movie_cache(cache).await;
                    retryable_failure |= !successful;
                }
            } else {
                let Some(cache) = snapshot.downloads.get(cache_gid) else {
                    continue;
                };
                if cache.is_multi_episode {
                    // Use aria2's original file list: completed files may already have
                    // moved, and their durable receipts must still be checked on retry.
                    for file in status.files.iter().filter(|file| file.selected != "false") {
                        let path = Path::new(&file.path);
                        let ext = path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if !COMMON_MEDIA_EXTS_SET.contains(ext.as_str())
                            && !COMMON_CAPTION_EXTS_SET.contains(ext.as_str())
                        {
                            continue;
                        }
                        candidates += 1;
                        let outcome = Self::process_autodownload_file(path).await;
                        Self::record_process_outcome(
                            &outcome,
                            &mut successful,
                            &mut permanent_failure,
                            &mut retryable_failure,
                        );
                    }
                } else {
                    let key = cache.tmdbid;
                    if !PROCESSING_TMDBIDS.insert(key) {
                        continue;
                    }
                    let media = { DATA_JSON.read().await.media.get(&key).cloned() };
                    if let Some(media) = media {
                        for file in status.files.iter().filter(|file| file.selected != "false") {
                            let path = Path::new(&file.path);
                            let ext = path
                                .extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if !COMMON_MEDIA_EXTS_SET.contains(ext.as_str())
                                && !COMMON_CAPTION_EXTS_SET.contains(ext.as_str())
                            {
                                continue;
                            }
                            candidates += 1;
                            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let outcome = media_processor::rename_and_move(
                                &media.media_path,
                                &media.display_name,
                                name.contains(&media.original_name),
                                &cache.season_number.to_string(),
                                &cache.episode_number.to_string(),
                                name,
                                &file.path,
                                &ext,
                                key,
                            )
                            .await;
                            Self::record_process_outcome(
                                &outcome,
                                &mut successful,
                                &mut permanent_failure,
                                &mut retryable_failure,
                            );
                        }
                    } else {
                        successful = false;
                        retryable_failure = true;
                    }
                    PROCESSING_TMDBIDS.remove(&key);
                }
                if successful && candidates > 0 {
                    successful = Self::remove_download_cache(cache).await;
                    retryable_failure |= !successful;
                }
            }
            if successful && candidates > 0 {
                retry_states.write().await.remove(gid);
                processed_gids.write().await.insert(gid.clone());
            } else if permanent_failure {
                retry_states.write().await.remove(gid);
                processed_gids.write().await.insert(gid.clone());
                error!(
                    "[下载监控] 任务 {} 存在确定性处理错误，本次运行不再自动重试；源文件和下载缓存已保留",
                    gid
                );
            } else if retryable_failure && candidates > 0 {
                let (attempts, delay) = {
                    let mut states = retry_states.write().await;
                    let attempts = states
                        .get(gid)
                        .map_or(1, |state| state.attempts.saturating_add(1));
                    let delay = retry_delay(attempts);
                    states.insert(
                        gid.clone(),
                        RetryState {
                            attempts,
                            next_attempt: Instant::now() + delay,
                        },
                    );
                    (attempts, delay)
                };
                error!(
                    "[下载监控] 任务 {} 第 {} 次处理失败，将在 {} 秒后重试",
                    gid,
                    attempts,
                    delay.as_secs()
                );
            }
        }
    }

    fn record_process_outcome(
        outcome: &media_processor::ProcessOutcome,
        successful: &mut bool,
        permanent_failure: &mut bool,
        retryable_failure: &mut bool,
    ) {
        if outcome.is_success() {
            return;
        }
        *successful = false;
        if outcome.is_permanent_failure() {
            *permanent_failure = true;
        } else {
            *retryable_failure = true;
        }
    }

    async fn remove_download_cache(cache: &DownloadCache) -> bool {
        if !Self::delete_cache(cache).await {
            return false;
        }
        // 缓存被成功消费后，同时移除 aria2 中对应的下载任务
        Aria2Client::remove_task(&cache.gid).await;
        true
    }

    async fn remove_movie_cache(cache: &MovieDownloadCache) -> bool {
        if !Self::delete_movie_cache(cache).await {
            return false;
        }
        // 缓存被成功消费后，同时移除 aria2 中对应的下载任务
        Aria2Client::remove_task(&cache.gid).await;
        true
    }

    fn convert_to_download_data(status: &Aria2Status) -> DownloadData {
        let total_length: u64 = status.total_length.parse().unwrap_or(0);
        let completed_length: u64 = status.completed_length.parse().unwrap_or(0);
        let download_speed: u64 = status.download_speed.parse().unwrap_or(0);

        let progress = if total_length > 0 {
            (completed_length as f64 / total_length as f64) * 100.0
        } else {
            0.0
        };

        let name = if !status.files.is_empty() {
            let first_file = &status.files[0];
            Path::new(&first_file.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        } else {
            "Metadata".to_string()
        };

        let target_path = if !status.files.is_empty() {
            status.files[0].path.clone()
        } else {
            String::new()
        };

        let is_metadata = !status.followed_by.is_empty();

        DownloadData {
            id: status.gid.clone(),
            is_metadata,
            name,
            target_path,
            dir: status.dir.clone(),
            total_length,
            progress,
            current_download: completed_length,
            status: status.status.clone(),
            current_speed: download_speed,
        }
    }

    async fn update_download_cache(all_statuses: &[Aria2Status]) {
        let downloads: Vec<DownloadData> = all_statuses
            .iter()
            .filter(|status| {
                matches!(
                    status.status.as_str(),
                    "active" | "waiting" | "paused" | "complete" | "error" | "removed"
                )
            })
            .map(Self::convert_to_download_data)
            .collect();

        let mut cache = DOWNLOAD_STATUS_CACHE.write().await;
        *cache = downloads;
    }

    async fn update_upload_cache() {
        let config = CONFIG.read().await.clone();

        if !config.openlist_auto_upload {
            return;
        }

        let client_guard = CLIENT.read().await;
        let client = match client_guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                return;
            }
        };
        drop(client_guard);

        let api_upload_done_url = format!(
            "http://{}:{}/api/task/upload/undone",
            config.openlist_address, config.openlist_port
        );

        let response = client
            .get(&api_upload_done_url)
            .header("Authorization", &config.openlist_apikey)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await;

        match response {
            Ok(res) => match res.json::<OpenlistUploadResponse>().await {
                Ok(openlist_res) => {
                    if openlist_res.code == 200 {
                        let mut uploads: Vec<UploadData> = Vec::new();

                        for task in openlist_res.data {
                            let file_name = Self::extract_file_name(&task.name);
                            uploads.push(UploadData {
                                id: task.id.clone(),
                                name: file_name,
                            });
                        }

                        let mut cache = UPLOAD_STATUS_CACHE.write().await;
                        *cache = uploads;
                    } else {
                        debug!(
                            "[上传监控] OpenList 返回错误 {}: {}",
                            openlist_res.code, openlist_res.message
                        );
                    }
                }
                Err(e) => {
                    debug!("[上传监控] 解析响应失败: {}", e);
                }
            },
            Err(e) => {
                debug!("[上传监控] 查询上传任务失败: {}", e);
            }
        }
    }

    fn extract_file_name(name: &str) -> String {
        let parts: Vec<&str> = name.split("upload ").collect();
        if parts.len() > 1 {
            let after_upload = parts[1];
            let parts_to: Vec<&str> = after_upload.split(" to ").collect();
            if parts_to.len() > 0 {
                return parts_to[0].to_string();
            }
        }
        name.to_string()
    }

    async fn is_download_cache_empty() -> bool {
        crate::download_cache::is_empty().await
    }

    fn parse_season_from_folder(folder_name: &str) -> Option<String> {
        let lower = folder_name.to_lowercase();
        let rest = lower.strip_prefix("season")?;
        let trimmed = rest.trim();
        if trimmed.is_empty() {
            return None;
        }
        let num: i32 = trimmed.parse().ok()?;
        Some(format!("{:02}", num))
    }

    /// 从文件路径向上逐级查找 Season 目录和对应的剧名目录
    /// 支持文件位于 Season 目录下的任意深度子目录中，例如:
    ///   /剧集名/Season 1/AAA/file.mkv
    ///   /剧集名/Season 1/AAA/BBB/file.mkv
    fn find_season_and_display_name(file_path: &Path) -> Option<(String, String)> {
        let mut current = file_path.parent()?;

        loop {
            let folder_name = current.file_name()?.to_str()?;
            if let Some(season) = Self::parse_season_from_folder(folder_name) {
                // 找到了 Season 目录，其父目录就是剧名目录
                let display_name = current.parent()?.file_name()?.to_str()?.to_string();
                return Some((season, display_name));
            }
            // 继续向上一级查找
            current = current.parent()?;
        }
    }

    pub async fn process_autodownload_file(file_path: &Path) -> media_processor::ProcessOutcome {
        let ext = match file_path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => {
                return media_processor::ProcessOutcome::PermanentFailure(
                    "文件没有有效扩展名".into(),
                )
            }
        };

        let is_media = COMMON_MEDIA_EXTS_SET.contains(ext.as_str());
        let is_caption = COMMON_CAPTION_EXTS_SET.contains(ext.as_str());

        if !is_media && !is_caption {
            debug!(
                "[自动下载扫描] 跳过非媒体/字幕文件: {}",
                file_path.display()
            );
            return media_processor::ProcessOutcome::PermanentFailure(
                "文件类型不是受支持的媒体或字幕".into(),
            );
        }

        let (season, display_name) = match Self::find_season_and_display_name(file_path) {
            Some(v) => v,
            None => {
                debug!(
                    "[自动下载扫描] 无法从路径中定位 Season 目录，跳过: {}",
                    file_path.display()
                );
                return media_processor::ProcessOutcome::PermanentFailure(
                    "无法从路径中定位 Season 目录".into(),
                );
            }
        };

        let (media_path, tmdbid, original_name) = {
            let data = DATA_JSON.read().await;
            let mut found: Option<(String, u32, String)> = None;

            for media in data.media.values() {
                if media.media_path.contains(&display_name) {
                    found = Some((
                        media.media_path.clone(),
                        media.tmdbid,
                        media.original_name.clone(),
                    ));
                    break;
                }
            }

            match found {
                Some(v) => v,
                None => {
                    debug!(
                        "[自动下载扫描] data.json 中找不到 media_path 包含 '{}' 的影视剧，跳过: {}",
                        display_name,
                        file_path.display()
                    );
                    return media_processor::ProcessOutcome::PermanentFailure(format!(
                        "data.json 中找不到媒体路径包含 '{display_name}' 的影视剧"
                    ));
                }
            }
        };

        let file_name_with_ext = match file_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                return media_processor::ProcessOutcome::PermanentFailure(
                    "文件名不是有效 UTF-8".into(),
                )
            }
        };

        let parse_result = crate::models::utils::parse_season_episode(
            file_path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
        );

        let ep = match &parse_result.ep {
            Some(e) => e.clone(),
            None => {
                debug!(
                    "[自动下载扫描] 无法从文件名中解析集数，跳过: {}",
                    file_name_with_ext
                );
                return media_processor::ProcessOutcome::PermanentFailure(
                    "无法从文件名中解析集数".into(),
                );
            }
        };

        // 自动下载扫描入库时，Season 仅从目录路径中的 Season 文件夹获取，不从文件名解析
        let season_str = season.clone();

        let (file_name_str, ext_str) = match file_name_with_ext.rfind('.') {
            Some(idx) => (&file_name_with_ext[..idx], &file_name_with_ext[idx + 1..]),
            None => (file_name_with_ext.as_str(), ""),
        };

        let file_path_str = match file_path.to_str() {
            Some(s) => s.to_string(),
            None => {
                return media_processor::ProcessOutcome::PermanentFailure(
                    "文件路径不是有效 UTF-8".into(),
                )
            }
        };

        let is_jp = original_name.is_empty() || file_name_str.contains(&original_name);

        if PROCESSING_TMDBIDS.insert(tmdbid) {
            info!(
                "[自动下载扫描] 处理文件: {} [S{}E{}] -> tmdbid={}",
                file_name_with_ext, season_str, ep, tmdbid
            );

            let outcome = media_processor::rename_and_move(
                &media_path,
                &display_name,
                is_jp,
                &season_str,
                &ep,
                file_name_str,
                &file_path_str,
                ext_str,
                tmdbid,
            )
            .await;

            PROCESSING_TMDBIDS.remove(&tmdbid);
            outcome
        } else {
            media_processor::ProcessOutcome::Deferred("同一媒体正在处理中".into())
        }
    }

    async fn process_media_directory(dir: &Path) -> usize {
        let permit = DIRECTORY_SCANS
            .clone()
            .acquire_owned()
            .await
            .expect("directory scan limiter closed");
        let directory = dir.to_path_buf();
        let (sender, mut receiver) = mpsc::channel::<PathBuf>(64);
        let producer = tokio::spawn(async move {
            let _permit = permit;
            crate::io_util::blocking(move || {
                for entry in WalkDir::new(directory)
                    .follow_links(false)
                    .into_iter()
                    .flatten()
                {
                    let path = entry.path();
                    if !entry.file_type().is_file()
                        && !(entry.file_type().is_symlink() && path.is_file())
                    {
                        continue;
                    }
                    let Some(extension) = path.extension().and_then(|extension| extension.to_str())
                    else {
                        continue;
                    };
                    let extension = extension.to_lowercase();
                    if COMMON_MEDIA_EXTS_SET.contains(extension.as_str())
                        || COMMON_CAPTION_EXTS_SET.contains(extension.as_str())
                    {
                        if sender.blocking_send(path.to_path_buf()).is_err() {
                            break;
                        }
                    }
                }
            })
            .await
        });
        let mut count = 0;
        while let Some(file) = receiver.recv().await {
            count += 1;
            Self::process_autodownload_file(&file).await;
        }
        match producer.await {
            Ok(Ok(())) => {}
            result => error!("[自动下载扫描] 目录扫描任务失败: {:?}", result),
        }
        count
    }

    pub async fn scan_autodownload_folder() {
        let base_dir = Path::new("/mnt/8tb/autodownload/");
        if !fs::try_exists(base_dir).await.unwrap_or(false) {
            debug!("[自动下载扫描] 目录不存在: {}", base_dir.display());
            return;
        }

        debug!("[自动下载扫描] 开始扫描目录: {}", base_dir.display());

        let count = Self::process_media_directory(base_dir).await;
        info!("[自动下载扫描] 扫描完成，找到 {} 个候选文件", count);

        debug!("[自动下载扫描] 扫描处理完成");
    }

    pub async fn hourly_autodownload_scan() {
        let _lock = AUTODOWNLOAD_LOCK.lock().await;

        if Self::is_download_cache_empty().await {
            info!("[定时任务] 统一运行状态中无下载任务，开始扫描 autodownload 目录");
            Self::scan_autodownload_folder().await;
        } else {
            debug!("[定时任务] 统一运行状态中仍有下载任务，跳过 autodownload 扫描");
        }
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn retry_delay_uses_bounded_backoff() {
        assert_eq!(retry_delay(1), Duration::from_secs(60));
        assert_eq!(retry_delay(2), Duration::from_secs(5 * 60));
        assert_eq!(retry_delay(3), Duration::from_secs(30 * 60));
        assert_eq!(retry_delay(4), Duration::from_secs(60 * 60));
        assert_eq!(retry_delay(100), Duration::from_secs(60 * 60));
    }
}
