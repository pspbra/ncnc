//! Resolve media executables once when automatic compression is enabled.

use log::{error, info, warn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::OnceCell;

pub struct MediaTools {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
}

impl MediaTools {
    pub fn paths(&self) -> Option<(&Path, &Path)> {
        Some((self.ffmpeg.as_deref()?, self.ffprobe.as_deref()?))
    }
}

static TOOLS: OnceCell<MediaTools> = OnceCell::const_new();
static DISABLED_TOOLS: MediaTools = MediaTools {
    ffmpeg: None,
    ffprobe: None,
};

async fn usable(path: &Path) -> bool {
    if !tokio::fs::metadata(path).await.is_ok_and(|m| m.is_file()) {
        return false;
    }
    let mut command = Command::new(path);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let result = match command.spawn() {
        Ok(mut child) => match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
            Ok(result) => result.map(|status| status.success()),
            Err(_) => {
                let _ = child.kill().await;
                warn!("[媒体工具] 检测超时: {}", path.display());
                return false;
            }
        },
        Err(error) => Err(error),
    };
    match result {
        Ok(true) => true,
        Ok(false) => {
            warn!("[媒体工具] 版本检测失败，继续查找: {}", path.display());
            false
        }
        Err(error) => {
            warn!(
                "[媒体工具] 无法执行 {}，继续查找: {}",
                path.display(),
                error
            );
            false
        }
    }
}

async fn resolve(
    name: &str,
    executable_dir: Option<&Path>,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    let filename = format!("{}{}", name, std::env::consts::EXE_SUFFIX);
    let mut directories = Vec::new();
    if let Some(directory) = executable_dir {
        directories.push(directory.to_path_buf());
    }
    if let Some(search_path) = search_path {
        directories.extend(std::env::split_paths(search_path));
    }
    let mut seen = HashSet::new();
    for directory in directories {
        let candidate = directory.join(&filename);
        let Ok(candidate) = std::path::absolute(candidate) else {
            continue;
        };
        if !seen.insert(candidate.clone()) || !usable(&candidate).await {
            continue;
        }
        let path = tokio::fs::canonicalize(&candidate)
            .await
            .unwrap_or(candidate);
        info!("[媒体工具] {} 可用: {}", name, path.display());
        return Some(path);
    }
    error!(
        "[媒体工具] {} 缺失或不可执行：后端程序同级目录及运行环境 PATH 均未找到可用工具",
        name
    );
    None
}

pub async fn init() -> &'static MediaTools {
    // Check before accessing the cache or searching for executables. Keep the
    // read lock during detection so disabling cannot race with a new probe.
    let config = crate::CONFIG.read().await;
    init_enabled(config.media_auto_compress).await
}

async fn init_enabled(enabled: bool) -> &'static MediaTools {
    if !enabled {
        return &DISABLED_TOOLS;
    }
    // Do not initialize TOOLS while disabled: enabling later must still detect.
    TOOLS.get_or_init(|| async {
        let executable = match std::env::current_exe() {
            Ok(path) => Some(path),
            Err(error) => {
                warn!("[媒体工具] 无法获取后端程序路径，仅检查 PATH: {}", error);
                None
            }
        };
        let executable_dir = executable.as_deref().and_then(Path::parent);
        let search_path = std::env::var_os("PATH");
        let (ffmpeg, ffprobe) = tokio::join!(
            resolve("ffmpeg", executable_dir, search_path.as_deref()),
            resolve("ffprobe", executable_dir, search_path.as_deref())
        );
        let tools = MediaTools { ffmpeg, ffprobe };
        if tools.paths().is_none() {
            warn!("[媒体工具] 已标记媒体工具缺失：本次运行跳过媒体探测、压缩和字幕提取；无需更换封装时直接移动，需要转为 MKV 时保留源文件并报错；安装工具后需重启后端重新检测");
        }
        tools
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_skips_detection_and_overrides_cached_tools() {
        assert!(init_enabled(false).await.paths().is_none());
        assert!(
            TOOLS.get().is_none(),
            "disabled must not initialize detection"
        );

        // Simulate a successful detection without executing real media tools.
        assert!(TOOLS
            .set(MediaTools {
                ffmpeg: Some(PathBuf::from("ffmpeg")),
                ffprobe: Some(PathBuf::from("ffprobe")),
            })
            .is_ok());
        assert!(init_enabled(true).await.paths().is_some());
        assert!(init_enabled(false).await.paths().is_none());
        assert!(init_enabled(true).await.paths().is_some());
    }
}
