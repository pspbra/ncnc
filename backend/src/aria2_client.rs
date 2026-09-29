use crate::{models::*, CLIENT, CONFIG};
use log::{debug, error, info};
use serde_json::json;
use std::path::Path;
use std::time::Duration;

use std::collections::HashSet;
use std::future::Future;

/// 分页中途失败或出现重复 GID，说明列表不完整，不能作为清理依据。
pub(crate) async fn fetch_status_pages<F, Fut>(mut fetch: F) -> Result<Vec<Aria2Status>, String>
where
    F: FnMut(i32, i32) -> Fut,
    Fut: Future<Output = Result<Vec<Aria2Status>, String>>,
{
    const PAGE_SIZE: i32 = 1000;
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for page_number in 0..100 {
        let page = fetch(page_number * PAGE_SIZE, PAGE_SIZE).await?;
        if page.len() > PAGE_SIZE as usize
            || page.iter().any(|status| !seen.insert(status.gid.clone()))
        {
            return Err("aria2 分页任务列表发生变化或格式异常".to_string());
        }
        let last_page = page.len() < PAGE_SIZE as usize;
        all.extend(page);
        if last_page {
            return Ok(all);
        }
    }
    Err("aria2 任务列表超过分页上限，跳过清理".to_string())
}

pub struct Aria2Client;

fn task_exists_from_response(
    response: Aria2Response<serde_json::Value>,
    gid: &str,
) -> Result<bool, String> {
    if let Some(error) = response.error {
        if error.code == 1 && error.message == format!("GID {} is not found", gid) {
            return Ok(false);
        }
        return Err(format!("Aria2错误: {} - {}", error.code, error.message));
    }
    if response
        .result
        .as_ref()
        .and_then(|value| value.get("gid"))
        .and_then(|gid| gid.as_str())
        == Some(gid)
    {
        Ok(true)
    } else {
        Err("Aria2未返回有效任务".to_string())
    }
}

impl Aria2Client {
    async fn call_aria2<T: serde::de::DeserializeOwned>(
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, String> {
        let response = Self::call_aria2_response(method, params).await?;
        if let Some(error) = response.error {
            return Err(format!("Aria2错误: {} - {}", error.code, error.message));
        }
        response.result.ok_or("Aria2未返回结果".to_string())
    }

    async fn call_aria2_response<T: serde::de::DeserializeOwned>(
        method: &str,
        params: serde_json::Value,
    ) -> Result<Aria2Response<T>, String> {
        let (aria_address, aria_port, secret) = {
            let config = CONFIG.read().await;
            (
                config.aria_address.clone(),
                config.aria_port,
                config.aria_rpc_secret.clone(),
            )
        };
        let client_guard = CLIENT.read().await;
        let client = client_guard.as_ref().cloned().ok_or("HTTP客户端未初始化")?;
        drop(client_guard);

        let base_rpc_url = format!("http://{}:{}", aria_address, aria_port);
        let rpc_url = format!("{}/jsonrpc", base_rpc_url);

        let params = if secret.is_empty() {
            params
        } else {
            let mut params_array = params.as_array().unwrap().clone();
            params_array.insert(0, json!(format!("token:{}", secret)));
            json!(params_array)
        };

        let payload = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": method,
            "params": params
        });

        let response = client
            .post(&rpc_url)
            .json(&payload)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("发送请求失败: {}", e))?;

        let text = response
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        let result: Aria2Response<T> = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}, 内容: {}", e, text))?;

        Ok(result)
    }

    /// 仅明确的 GID 不存在响应才允许清理；认证/连接/格式错误均保留缓存。
    pub async fn task_exists(gid: &str) -> Result<bool, String> {
        let response: Aria2Response<serde_json::Value> =
            Self::call_aria2_response("aria2.tellStatus", json!([gid, ["gid"]])).await?;
        task_exists_from_response(response, gid)
    }

    /// 创建剧集运行状态（仅更新内存，由统一状态文件低频批量保存）
    async fn create_cache_file(
        gid: &str,
        tmdbid: u32,
        season_number: u32,
        episode_number: u32,
        is_multi_episode: bool,
    ) -> Result<(), String> {
        let cache = DownloadCache {
            gid: gid.to_string(),
            tmdbid,
            season_number,
            episode_number,
            file_paths: Vec::new(),
            is_multi_episode,
        };

        crate::download_cache::put_download(cache).await;
        Ok(())
    }

    /// 创建电影缓存文件（立即创建，文件路径留空）
    async fn create_movie_cache_file(
        gid: &str,
        display_name: String,
        media_path: String,
    ) -> Result<(), String> {
        let cache = MovieDownloadCache {
            gid: gid.to_string(),
            display_name,
            media_path,
            file_paths: Vec::new(),
        };

        crate::download_cache::put_movie(cache).await;
        Ok(())
    }

    /// 更新剧集缓存文件，添加真实文件路径
    pub async fn update_cache_file(
        gid: &str,
        tmdbid: u32,
        season_number: u32,
        episode_number: u32,
        is_multi_episode: bool,
    ) -> Result<(), String> {
        let files = Self::get_files(gid).await?;
        let file_paths: Vec<String> = files.into_iter().map(|f| f.path).collect();

        if file_paths.is_empty() {
            return Err("获取到的文件列表为空".to_string());
        }

        let cache = DownloadCache {
            gid: gid.to_string(),
            tmdbid,
            season_number,
            episode_number,
            file_paths,
            is_multi_episode,
        };

        crate::download_cache::put_download(cache).await;
        Ok(())
    }

    /// 更新电影缓存文件，添加真实文件路径
    pub async fn update_movie_cache_file(
        gid: &str,
        display_name: String,
        media_path: String,
    ) -> Result<(), String> {
        let files = Self::get_files(gid).await?;
        let file_paths: Vec<String> = files.into_iter().map(|f| f.path).collect();

        if file_paths.is_empty() {
            return Err("获取到的电影文件列表为空".to_string());
        }

        let cache = MovieDownloadCache {
            gid: gid.to_string(),
            display_name: display_name.clone(),
            media_path,
            file_paths,
        };

        crate::download_cache::put_movie(cache).await;
        Ok(())
    }

    pub async fn add_uri(magnet_uri: &str, download_dir: &str) -> Result<String, String> {
        let params = json!([[magnet_uri], { "dir": download_dir }]);
        Self::call_aria2("aria2.addUri", params).await
    }

    /// 添加剧集下载并立即创建缓存文件
    pub async fn add_uri_with_cache(
        magnet_uri: &str,
        download_dir: &str,
        tmdbid: u32,
        season_number: u32,
        episode_number: u32,
        is_multi_episode: bool,
    ) -> Result<String, String> {
        let gid = Self::add_uri(magnet_uri, download_dir).await?;

        Self::create_cache_file(
            &gid,
            tmdbid,
            season_number,
            episode_number,
            is_multi_episode,
        )
        .await?;

        let gid_clone = gid.clone();
        let is_multi_episode_clone = is_multi_episode;
        tokio::spawn(async move {
            if let Err(e) = Self::monitor_and_update_cache(
                &gid_clone,
                tmdbid,
                season_number,
                episode_number,
                is_multi_episode_clone,
            )
            .await
            {
                error!("[Aria2] 监控和更新缓存失败: {}", e);
            }
        });

        Ok(gid)
    }

    /// 添加电影下载并立即创建缓存文件
    pub async fn add_uri_with_cache_movie(
        magnet_uri: &str,
        download_dir: &str,
        display_name: String,
        media_path: String,
    ) -> Result<String, String> {
        let gid = Self::add_uri(magnet_uri, download_dir).await?;

        Self::create_movie_cache_file(&gid, display_name.clone(), media_path.clone()).await?;

        let gid_clone = gid.clone();
        tokio::spawn(async move {
            if let Err(e) =
                Self::monitor_and_update_cache_movie(&gid_clone, display_name, media_path).await
            {
                error!("[Aria2] 电影监控和更新缓存失败: {}", e);
            }
        });

        Ok(gid)
    }

    pub async fn tell_active() -> Result<Vec<Aria2Status>, String> {
        Self::call_aria2(
            "aria2.tellActive",
            json!([[
                "gid",
                "status",
                "totalLength",
                "completedLength",
                "downloadSpeed",
                "dir",
                "files",
                "followedBy"
            ]]),
        )
        .await
    }

    pub async fn tell_waiting(offset: i32, num: i32) -> Result<Vec<Aria2Status>, String> {
        Self::call_aria2(
            "aria2.tellWaiting",
            json!([
                offset,
                num,
                [
                    "gid",
                    "status",
                    "totalLength",
                    "completedLength",
                    "downloadSpeed",
                    "dir",
                    "files",
                    "followedBy"
                ]
            ]),
        )
        .await
    }

    pub async fn tell_stopped(offset: i32, num: i32) -> Result<Vec<Aria2Status>, String> {
        Self::call_aria2(
            "aria2.tellStopped",
            json!([
                offset,
                num,
                [
                    "gid",
                    "status",
                    "totalLength",
                    "completedLength",
                    "downloadSpeed",
                    "dir",
                    "files",
                    "followedBy"
                ]
            ]),
        )
        .await
    }

    pub async fn tell_all_waiting() -> Result<Vec<Aria2Status>, String> {
        fetch_status_pages(Self::tell_waiting).await
    }

    pub async fn tell_all_stopped() -> Result<Vec<Aria2Status>, String> {
        fetch_status_pages(Self::tell_stopped).await
    }

    pub async fn tell_status(gid: &str) -> Result<Aria2Status, String> {
        Self::call_aria2("aria2.tellStatus", json!([gid])).await
    }

    pub async fn get_files(gid: &str) -> Result<Vec<Aria2File>, String> {
        Self::call_aria2("aria2.getFiles", json!([gid])).await
    }

    pub async fn remove(gid: &str) -> Result<String, String> {
        Self::call_aria2("aria2.remove", json!([gid])).await
    }

    pub async fn force_remove(gid: &str) -> Result<String, String> {
        Self::call_aria2("aria2.forceRemove", json!([gid])).await
    }

    pub async fn remove_download_result(gid: &str) -> Result<String, String> {
        Self::call_aria2("aria2.removeDownloadResult", json!([gid])).await
    }

    pub async fn purge_download_result() -> Result<String, String> {
        Self::call_aria2("aria2.purgeDownloadResult", json!([])).await
    }

    /// 从 aria2 移除任务：根据任务当前状态选择移除方式
    /// - 活跃/等待/暂停任务使用 aria2.remove
    /// - 已完成任务使用 aria2.removeDownloadResult
    /// - 任务已不存在时无需处理
    pub async fn remove_task(gid: &str) {
        let status = match Self::tell_status(gid).await {
            Ok(s) => s,
            Err(_) => {
                debug!("[Aria2] 任务不存在或已被移除，无需处理: {}", gid);
                return;
            }
        };

        if status.status == "active" || status.status == "waiting" || status.status == "paused" {
            match Self::remove(gid).await {
                Ok(_) => info!("[Aria2] 已从 aria2 移除任务: {}", gid),
                Err(e) => error!("[Aria2] 从 aria2 移除任务失败 {}: {}", gid, e),
            }
        } else {
            match Self::remove_download_result(gid).await {
                Ok(_) => info!("[Aria2] 已从 aria2 移除已完成任务记录: {}", gid),
                Err(e) => error!("[Aria2] 从 aria2 移除已完成任务记录失败 {}: {}", gid, e),
            }
        }
    }

    /// 监控剧集下载并更新缓存（当获取到真实文件后）
    pub async fn monitor_and_update_cache(
        gid: &str,
        tmdbid: u32,
        season_number: u32,
        episode_number: u32,
        is_multi_episode: bool,
    ) -> Result<(), String> {
        let mut current_gid = gid.to_string();
        let mut attempts = 0;
        let max_attempts = 600;

        loop {
            attempts += 1;
            if attempts > max_attempts {
                return Err("等待metadata下载超时".to_string());
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            let status = match Self::tell_status(&current_gid).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[Aria2] 获取状态出错: {}", e);
                    continue;
                }
            };

            if !status.followed_by.is_empty() {
                current_gid = status.followed_by[0].clone();
                break;
            }
        }

        // 阶段2：等待 follow-up 下载的文件列表就绪（独立超时 90 秒）
        let mut file_attempts = 0;
        let max_file_attempts = 30;
        loop {
            file_attempts += 1;
            if file_attempts > max_file_attempts {
                return Err("等待文件列表就绪超时".to_string());
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            let status = match Self::tell_status(&current_gid).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[Aria2] 获取follow-up状态出错: {}", e);
                    continue;
                }
            };

            if !status.files.is_empty() {
                break;
            }
        }

        Self::update_cache_file(
            &current_gid,
            tmdbid,
            season_number,
            episode_number,
            is_multi_episode,
        )
        .await?;
        if current_gid != gid {
            crate::download_cache::remove_download(gid).await;
        }

        // 缓存创建完成，磁力链元数据/种子文件任务已完成使命，从 aria2 中移除
        Self::remove_task(gid).await;

        Ok(())
    }

    /// 监控电影下载并更新缓存（当获取到真实文件后）
    pub async fn monitor_and_update_cache_movie(
        gid: &str,
        display_name: String,
        media_path: String,
    ) -> Result<(), String> {
        let mut current_gid = gid.to_string();
        let mut attempts = 0;
        let max_attempts = 600;

        loop {
            attempts += 1;
            if attempts > max_attempts {
                return Err("等待电影metadata下载超时".to_string());
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            let status = match Self::tell_status(&current_gid).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[Aria2] 获取电影状态出错: {}", e);
                    continue;
                }
            };

            if !status.followed_by.is_empty() {
                current_gid = status.followed_by[0].clone();
                break;
            }
        }

        // 阶段2：等待 follow-up 下载的文件列表就绪（独立超时 90 秒）
        let mut file_attempts = 0;
        let max_file_attempts = 30;
        loop {
            file_attempts += 1;
            if file_attempts > max_file_attempts {
                return Err("等待电影文件列表就绪超时".to_string());
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            let status = match Self::tell_status(&current_gid).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[Aria2] 获取电影follow-up状态出错: {}", e);
                    continue;
                }
            };

            if !status.files.is_empty() {
                break;
            }
        }

        Self::update_movie_cache_file(&current_gid, display_name, media_path).await?;
        if current_gid != gid {
            crate::download_cache::remove_movie(gid).await;
        }

        // 缓存创建完成，磁力链元数据/种子文件任务已完成使命，从 aria2 中移除
        Self::remove_task(gid).await;

        Ok(())
    }
}

pub fn build_download_path(media_path: &str, season_number: Option<u32>) -> String {
    let path = Path::new(media_path);
    let folder_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    match season_number {
        Some(0) => format!("/mnt/8tb/autodownload/{}/Specials", folder_name),
        Some(s) => format!("/mnt/8tb/autodownload/{}/Season {}", folder_name, s),
        None => format!("/mnt/8tb/autodownload/{}", folder_name),
    }
}
