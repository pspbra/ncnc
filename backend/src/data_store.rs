use crate::models::DataJson;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 可变访问自动更新版本，所有写入路径共用同一套脏数据跟踪。
pub struct DataStore {
    data: RwLock<DataJson>,
    version: AtomicU64,
    saved_version: AtomicU64,
    writer: Mutex<()>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unchanged_mutations_do_not_advance_version_or_write() {
        let store = DataStore::new(DataJson::default());
        {
            let mut data = store.write().await;
            data.media.remove(&42);
        }
        assert_eq!(store.version(), 0);
        let path = std::env::temp_dir().join(format!("ncnc-data-{}.json", rand::random::<u64>()));
        assert!(!store.save_to(path.clone()).await.unwrap());
        assert!(!path.exists());
        let media = serde_json::from_value(serde_json::json!({
            "tmdbid": 42, "tvdb_series_id": null, "name": "test", "original_name": "test",
            "name_tw": "test", "poster_path": null, "optional_names": [], "media_path": "/tmp/test",
            "number_of_seasons": 0, "seasons": [], "display_name": "test"
        }))
        .unwrap();
        {
            store
                .write()
                .await
                .media
                .insert(42, std::sync::Arc::new(media));
        }
        assert_eq!(store.version(), 1);
        {
            let mut data = store.write().await;
            std::sync::Arc::make_mut(data.media.get_mut(&42).unwrap()).name = "test".into();
        }
        assert_eq!(store.version(), 1);
        assert!(store.save_to(path.clone()).await.unwrap());
        assert!(!store.save_to(path.clone()).await.unwrap());
        std::fs::remove_file(path).unwrap();
    }
}

pub struct DataWriteGuard<'a> {
    guard: RwLockWriteGuard<'a, DataJson>,
    version: &'a AtomicU64,
    original: Option<DataJson>,
}

impl Deref for DataWriteGuard<'_> {
    type Target = DataJson;
    fn deref(&self) -> &DataJson {
        &self.guard
    }
}
impl DerefMut for DataWriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut DataJson {
        if self.original.is_none() {
            self.original = Some(self.guard.clone());
        }
        &mut self.guard
    }
}
impl Drop for DataWriteGuard<'_> {
    fn drop(&mut self) {
        if self
            .original
            .as_ref()
            .is_some_and(|old| old != &*self.guard)
        {
            self.version.fetch_add(1, Ordering::Release);
        }
    }
}

impl DataStore {
    pub fn new(data: DataJson) -> Self {
        Self {
            data: RwLock::new(data),
            version: AtomicU64::new(0),
            saved_version: AtomicU64::new(0),
            writer: Mutex::new(()),
        }
    }
    pub async fn read(&self) -> RwLockReadGuard<'_, DataJson> {
        self.data.read().await
    }
    pub async fn write(&self) -> DataWriteGuard<'_> {
        DataWriteGuard {
            guard: self.data.write().await,
            version: &self.version,
            original: None,
        }
    }
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub async fn save_to(&self, path: PathBuf) -> io::Result<bool> {
        let _writer = self.writer.lock().await;
        let (version, snapshot) = {
            let data = self.read().await;
            let version = self.version();
            if version == self.saved_version.load(Ordering::Acquire) {
                return Ok(false);
            }
            (version, data.clone())
        };
        crate::io_util::blocking(move || crate::io_util::atomic_json(&path, &snapshot))
            .await
            .map_err(io::Error::other)??;
        // 只确认已写入快照的版本，写盘期间的新修改仍然处于待保存状态。
        self.saved_version.store(version, Ordering::Release);
        Ok(true)
    }
}
