use crate::models::{DownloadCache, MovieDownloadCache};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock};

pub const RUNTIME_STATE_PATH: &str = "runtime_state.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    Waiting,
    Processing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessingTask {
    pub source: String,
    pub target: String,
    pub status: ProcessingStatus,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RuntimeState {
    #[serde(default)]
    pub downloads: HashMap<String, DownloadCache>,
    #[serde(default)]
    pub movies: HashMap<String, MovieDownloadCache>,
    #[serde(default)]
    pub processing: HashMap<String, ProcessingTask>,
}

pub fn load() -> RuntimeState {
    let path = Path::new(RUNTIME_STATE_PATH);
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<RuntimeState>(&bytes) {
            Ok(mut state) => {
                // Processing is deliberately best-effort. Download ownership is
                // retained, while interrupted transcodes are rebuilt from source.
                for task in state.processing.values_mut() {
                    task.status = ProcessingStatus::Waiting;
                }
                state
            }
            Err(error) => {
                log::error!(
                    "[运行状态] {} 无法解析，使用空状态: {}",
                    path.display(),
                    error
                );
                RuntimeState::default()
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => RuntimeState::default(),
        Err(error) => {
            log::error!(
                "[运行状态] {} 无法读取，使用空状态: {}",
                path.display(),
                error
            );
            RuntimeState::default()
        }
    }
}

pub struct RuntimeStateStore {
    data: RwLock<RuntimeState>,
    version: AtomicU64,
    saved_version: AtomicU64,
    writer: Mutex<()>,
}

impl RuntimeStateStore {
    pub fn new(data: RuntimeState) -> Self {
        let dirty = u64::from(
            !data.downloads.is_empty() || !data.movies.is_empty() || !data.processing.is_empty(),
        );
        Self {
            data: RwLock::new(data),
            version: AtomicU64::new(dirty),
            saved_version: AtomicU64::new(0),
            writer: Mutex::new(()),
        }
    }

    fn changed(&self) {
        self.version.fetch_add(1, Ordering::Release);
    }

    pub async fn snapshot(&self) -> RuntimeState {
        self.data.read().await.clone()
    }

    pub async fn put_download(&self, cache: DownloadCache) {
        self.data
            .write()
            .await
            .downloads
            .insert(cache.gid.clone(), cache);
        self.changed();
    }

    pub async fn put_movie(&self, cache: MovieDownloadCache) {
        self.data
            .write()
            .await
            .movies
            .insert(cache.gid.clone(), cache);
        self.changed();
    }

    pub async fn remove_download(&self, gid: &str) -> bool {
        let removed = self.data.write().await.downloads.remove(gid).is_some();
        if removed {
            self.changed();
        }
        removed
    }

    pub async fn remove_movie(&self, gid: &str) -> bool {
        let removed = self.data.write().await.movies.remove(gid).is_some();
        if removed {
            self.changed();
        }
        removed
    }

    pub async fn clear_downloads(&self) {
        let mut data = self.data.write().await;
        if !data.downloads.is_empty() || !data.movies.is_empty() {
            data.downloads.clear();
            data.movies.clear();
            drop(data);
            self.changed();
        }
    }

    pub async fn downloads_empty(&self) -> bool {
        let data = self.data.read().await;
        data.downloads.is_empty() && data.movies.is_empty()
    }

    pub async fn begin_processing(&self, key: String, source: String, target: String) {
        self.data.write().await.processing.insert(
            key,
            ProcessingTask {
                source,
                target,
                status: ProcessingStatus::Processing,
            },
        );
        self.changed();
    }

    pub async fn finish_processing(&self, key: &str) {
        if self.data.write().await.processing.remove(key).is_some() {
            self.changed();
        }
    }

    pub async fn save_to(&self, path: PathBuf) -> io::Result<bool> {
        let _writer = self.writer.lock().await;
        let (version, snapshot) = {
            let version = self.version.load(Ordering::Acquire);
            if version == self.saved_version.load(Ordering::Acquire) {
                return Ok(false);
            }
            (version, self.data.read().await.clone())
        };
        crate::io_util::blocking(move || crate::io_util::atomic_json_relaxed(&path, &snapshot))
            .await
            .map_err(io::Error::other)??;
        self.saved_version.store(version, Ordering::Release);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unified_snapshot_contains_download_and_processing_state() {
        let path = std::env::temp_dir().join(format!(
            "ncnc-runtime-state-{:032x}.json",
            rand::random::<u128>()
        ));
        let store = RuntimeStateStore::new(RuntimeState::default());
        store
            .put_download(DownloadCache {
                gid: "gid-1".into(),
                tmdbid: 10,
                season_number: 1,
                episode_number: 2,
                file_paths: vec!["/download/episode.mkv".into()],
                is_multi_episode: false,
            })
            .await;
        store
            .begin_processing(
                "task-1".into(),
                "/download/episode.mkv".into(),
                "/library/episode.mkv".into(),
            )
            .await;

        assert!(store.save_to(path.clone()).await.unwrap());
        let saved: RuntimeState = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved.downloads.contains_key("gid-1"));
        assert!(saved.processing.contains_key("task-1"));
        let _ = std::fs::remove_file(path);
    }
}
