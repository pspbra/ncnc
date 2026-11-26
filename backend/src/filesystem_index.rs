//! Event-driven revisions; daily reconciliation also covers missed filesystem events.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const RECONCILE: Duration = Duration::from_secs(24 * 60 * 60);
#[derive(Default)]
struct Revisions {
    sequence: u64,
    paths: HashMap<PathBuf, u64>,
    healthy: bool,
}
struct Root {
    _watcher: RecommendedWatcher,
    revisions: Arc<Mutex<Revisions>>,
}
static ROOTS: Lazy<Mutex<HashMap<PathBuf, Root>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn revision(root: &Path, path: &Path) -> Option<u64> {
    let root = std::path::absolute(root).ok()?;
    let path = std::path::absolute(path).ok()?;
    let mut roots = ROOTS.lock().unwrap_or_else(|p| p.into_inner());
    if !roots.contains_key(&root) {
        let revisions = Arc::new(Mutex::new(Revisions {
            healthy: true,
            ..Default::default()
        }));
        let events = revisions.clone();
        let boundary = root.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let mut state = events.lock().unwrap_or_else(|p| p.into_inner());
            if !state.healthy {
                return;
            }
            match event {
                Ok(event) => {
                    if event.need_rescan() {
                        state.healthy = false;
                    }
                    if event.paths.iter().any(|path| path == &boundary)
                        && matches!(
                            event.kind,
                            EventKind::Remove(_)
                                | EventKind::Modify(notify::event::ModifyKind::Name(_))
                        )
                    {
                        // A moved/removed watch root can no longer report changes at this path.
                        state.healthy = false;
                    }
                    if matches!(event.kind, EventKind::Access(_)) {
                        return;
                    }
                    if event.paths.is_empty() {
                        state.healthy = false;
                        return;
                    }
                    state.sequence += 1;
                    let sequence = state.sequence;
                    for path in event.paths {
                        // Moving/removing a directory also invalidates cached descendants.
                        for (cached, revision) in state.paths.iter_mut() {
                            if cached.starts_with(&path) {
                                *revision = sequence;
                            }
                        }
                        for ancestor in path.ancestors().take_while(|p| p.starts_with(&boundary)) {
                            state.paths.insert(ancestor.to_path_buf(), sequence);
                        }
                    }
                    if state.paths.len() > 100_000 {
                        state.paths.clear();
                        state.healthy = false;
                        log::warn!("[文件监控] 变更索引达到容量上限，回退到定期扫描");
                    }
                }
                Err(_) => state.healthy = false,
            }
        })
        .ok()?;
        if let Err(error) = watcher.watch(&root, RecursiveMode::Recursive) {
            log::warn!(
                "[文件监控] 无法监控 {}，使用定期扫描: {}",
                root.display(),
                error
            );
            return None;
        }
        roots.insert(
            root.clone(),
            Root {
                _watcher: watcher,
                revisions,
            },
        );
    }
    let mut state = roots[&root]
        .revisions
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !state.healthy {
        return None;
    }
    Some(*state.paths.entry(path).or_insert(0))
}

pub fn fresh(previous: Option<(u64, Instant)>, revision: Option<u64>) -> bool {
    matches!((previous, revision), (Some((old, time)), Some(new)) if old == new && time.elapsed() < RECONCILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_events_invalidate_ancestors_but_reads_do_not() {
        let base = std::env::var_os("NCNC_IO_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = base.join(format!("ncnc-watch-test-{:032x}", rand::random::<u128>()));
        std::fs::create_dir_all(root.join("season")).unwrap();
        let file = root.join("season/episode.mkv");
        std::fs::write(&file, b"original").unwrap();
        let before = revision(&root, &root).expect("native watcher unavailable");
        std::fs::read(&file).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(revision(&root, &root), Some(before));
        std::fs::write(&file, b"updated").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while revision(&root, &root) == Some(before) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_ne!(revision(&root, &root), Some(before));
        assert!(revision(&root, &root.join("season")).unwrap() > 0);
        assert!(!fresh(
            Some((before, Instant::now())),
            revision(&root, &root)
        ));
        assert!(!fresh(Some((before, Instant::now())), None));
        ROOTS.lock().unwrap().remove(&root);
        std::fs::remove_dir_all(root).unwrap();
    }
}
