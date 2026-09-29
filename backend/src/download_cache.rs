use crate::models::{DownloadCache, MovieDownloadCache};
use crate::RUNTIME_STATE;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct CacheSnapshot {
    pub downloads: HashMap<String, DownloadCache>,
    pub movies: HashMap<String, MovieDownloadCache>,
    pub paths: HashMap<String, (bool, String)>,
}

/// 下载归属只从统一的内存运行状态读取，不再扫描每任务 JSON 文件。
pub async fn load_snapshot() -> Result<CacheSnapshot, String> {
    let state = RUNTIME_STATE.snapshot().await;
    let mut snapshot = CacheSnapshot {
        downloads: state.downloads,
        movies: state.movies,
        paths: HashMap::new(),
    };
    for cache in snapshot.downloads.values() {
        for path in &cache.file_paths {
            snapshot
                .paths
                .insert(path.clone(), (false, cache.gid.clone()));
        }
    }
    for cache in snapshot.movies.values() {
        for path in &cache.file_paths {
            snapshot
                .paths
                .entry(path.clone())
                .or_insert((true, cache.gid.clone()));
        }
    }
    Ok(snapshot)
}

pub async fn episode_keys() -> Result<HashSet<(u32, u32, u32)>, String> {
    Ok(RUNTIME_STATE
        .snapshot()
        .await
        .downloads
        .values()
        .map(|cache| (cache.tmdbid, cache.season_number, cache.episode_number))
        .collect())
}

pub async fn put_download(cache: DownloadCache) {
    RUNTIME_STATE.put_download(cache).await;
}

pub async fn put_movie(cache: MovieDownloadCache) {
    RUNTIME_STATE.put_movie(cache).await;
}

pub async fn remove_download(gid: &str) -> bool {
    RUNTIME_STATE.remove_download(gid).await
}

pub async fn remove_movie(gid: &str) -> bool {
    RUNTIME_STATE.remove_movie(gid).await
}

pub async fn clear() {
    RUNTIME_STATE.clear_downloads().await;
}

pub async fn is_empty() -> bool {
    RUNTIME_STATE.downloads_empty().await
}
