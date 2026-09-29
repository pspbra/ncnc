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

pub struct DataWriteGuard<'a> {
    guard: RwLockWriteGuard<'a, DataJson>,
    version: &'a AtomicU64,
    modified: bool,
}

impl Deref for DataWriteGuard<'_> {
    type Target = DataJson;
    fn deref(&self) -> &DataJson {
        &self.guard
    }
}
impl DerefMut for DataWriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut DataJson {
        self.modified = true;
        &mut self.guard
    }
}
impl Drop for DataWriteGuard<'_> {
    fn drop(&mut self) {
        if self.modified {
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
            modified: false,
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
