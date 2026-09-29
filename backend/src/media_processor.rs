use crate::handlers::update;
use crate::models::constants::{COMMON_MEDIA_EXTS, KEEP_LANGS};
use crate::{openlist_uploader, CONFIG, MEDIA_PROCESS_SEMAPHORE, RUNTIME_STATE};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug)]
pub enum ProcessOutcome {
    Success,
    Deferred(String),
    Failed(String),
    PermanentFailure(String),
    CommittedCleanupFailed(String),
}
impl ProcessOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
    pub fn is_permanent_failure(&self) -> bool {
        matches!(self, Self::PermanentFailure(_))
    }
    pub fn log(&self, source: &str, target: &Path) {
        match self {
            Self::Success => info!(
                "[媒体处理] 处理完成，已移入媒体库: {}",
                target.file_name().unwrap_or_default().to_string_lossy()
            ),
            Self::Deferred(reason) => warn!("[媒体处理] 暂不可处理 {}: {}", source, reason),
            Self::Failed(reason) => {
                error!("[媒体处理] 失败，保留源文件和缓存 {}: {}", source, reason)
            }
            Self::PermanentFailure(reason) => error!(
                "[媒体处理] 无法处理，停止自动重试并保留源文件和缓存 {}: {}",
                source, reason
            ),
            Self::CommittedCleanupFailed(reason) => {
                warn!("[媒体处理] 已提交，等待清理重试 {}: {}", source, reason)
            }
        }
    }
}
#[derive(Debug, Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    format: Option<Format>,
}
#[derive(Debug, Deserialize)]
struct Format {
    duration: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Stream {
    index: u32,
    codec_type: String,
    codec_name: Option<String>,
    tags: Option<Tags>,
    disposition: Option<Disposition>,
}
#[derive(Debug, Deserialize)]
struct Tags {
    language: Option<String>,
    title: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Disposition {
    default: i32,
}
#[derive(Debug)]
struct Subtitle {
    index: u32,
    suffix: String,
}
#[derive(Debug)]
struct MediaPlan {
    video_compress: bool,
    audio_compress: bool,
    audio_count: usize,
    preferred_audio_order: Option<Vec<u32>>,
    subtitles: Vec<Subtitle>,
    embedded_pgs: Vec<u32>,
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
#[derive(Debug)]
struct PermanentProcessingError(String);
impl std::fmt::Display for PermanentProcessingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for PermanentProcessingError {}
fn permanent(message: impl Into<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        PermanentProcessingError(message.into()),
    )
}
fn is_permanent(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<PermanentProcessingError>())
}
fn safe_label(value: &str) -> String {
    value
        .chars()
        .take(80)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}
fn audio_language_is(stream: &Stream, expected: &str) -> bool {
    stream
        .tags
        .as_ref()
        .and_then(|tags| tags.language.as_deref())
        .is_some_and(|language| language.eq_ignore_ascii_case(expected))
}
fn media_plan(probe: Probe, size: u64, target_is_mkv: bool) -> io::Result<MediaPlan> {
    let duration = probe
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0)
        .ok_or_else(|| permanent("FFprobe 缺少有效容器时长"))?;
    if size == 0 || !probe.streams.iter().any(|s| s.codec_type == "video") {
        return Err(permanent("输入为空或没有视频流"));
    }
    let audio_compress = match probe.streams.iter().find(|s| s.codec_type == "audio") {
        None => false,
        Some(stream) => {
            let codec = stream
                .codec_name
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| permanent("首条音轨缺少 codec_name"))?
                .to_lowercase();
            !["aac", "flac", "pcm_s16le"]
                .iter()
                .any(|c| codec.contains(c))
        }
    };
    let audio_count = probe
        .streams
        .iter()
        .filter(|s| s.codec_type == "audio")
        .count();
    let audio_streams: Vec<_> = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "audio")
        .collect();
    let default_audio_is_chi = audio_streams.iter().any(|stream| {
        stream
            .disposition
            .as_ref()
            .is_some_and(|disposition| disposition.default == 1)
            && audio_language_is(stream, "chi")
    });
    let preferred_audio_order = default_audio_is_chi
        .then(|| {
            audio_streams
                .iter()
                .any(|stream| audio_language_is(stream, "jpn"))
        })
        .filter(|has_japanese_audio| *has_japanese_audio)
        .map(|_| {
            audio_streams
                .iter()
                .filter(|stream| audio_language_is(stream, "jpn"))
                .chain(
                    audio_streams
                        .iter()
                        .filter(|stream| !audio_language_is(stream, "jpn")),
                )
                .map(|stream| stream.index)
                .collect()
        });
    let mut subtitles = Vec::new();
    let mut embedded_pgs = Vec::new();
    let mut names = HashSet::new();
    let mut indices = HashSet::new();
    for stream in probe.streams {
        if !indices.insert(stream.index) {
            return Err(permanent("FFprobe 返回重复流索引"));
        }
        if stream.codec_type != "subtitle" {
            continue;
        }
        let language = stream
            .tags
            .as_ref()
            .and_then(|t| t.language.as_deref())
            .unwrap_or("")
            .to_lowercase();
        if !language.is_empty() && !KEEP_LANGS.contains(language.as_str()) {
            continue;
        }
        let codec = stream.codec_name.as_deref().unwrap_or("");
        if codec == "hdmv_pgs_subtitle" {
            if !target_is_mkv {
                return Err(permanent(format!(
                    "字幕流 {} 是 PGS 图形字幕，仅支持原样保留在 MKV 中",
                    stream.index
                )));
            }
            embedded_pgs.push(stream.index);
            continue;
        }
        if ![
            "ass",
            "ssa",
            "subrip",
            "text",
            "mov_text",
            "webvtt",
            "sami",
            "microdvd",
            "mpl2",
            "jacosub",
            "pjs",
            "realtext",
            "stl",
            "subviewer",
            "subviewer1",
            "vplayer",
        ]
        .contains(&codec)
        {
            return Err(permanent(format!(
                "字幕流 {} ({}) 不支持转换为 ASS",
                stream.index, codec
            )));
        }
        let title = stream
            .tags
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or("");
        let label = if !title.is_empty() {
            safe_label(title)
        } else if !language.is_empty() && language != "und" {
            safe_label(&language)
        } else {
            format!("track{}", stream.index)
        };
        let mut suffix = format!(".{}.ass", label);
        let mut collision = 0;
        while !names.insert(suffix.to_lowercase()) {
            collision += 1;
            suffix = format!(".{}.track{}-{}.ass", label, stream.index, collision);
        }
        subtitles.push(Subtitle {
            index: stream.index,
            suffix,
        });
    }
    Ok(MediaPlan {
        video_compress: size as f64 / duration / 1024.0 / 1024.0 * 8.0 > 3.5,
        audio_compress,
        audio_count,
        preferred_audio_order,
        subtitles,
        embedded_pgs,
    })
}

// Drain concurrently and bound retained diagnostics, including on verbose failures.
async fn bounded_read(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
    tail: bool,
) -> io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        if tail {
            kept.extend_from_slice(&buffer[..n]);
            if kept.len() > limit {
                kept.drain(..kept.len() - limit);
            }
        } else {
            let take = n.min(limit.saturating_sub(kept.len()));
            kept.extend_from_slice(&buffer[..take]);
        }
    }
    Ok(kept)
}
async fn run_command(
    cmd: &mut Command,
    timeout: Duration,
    stdout_limit: usize,
) -> io::Result<Vec<u8>> {
    debug!(
        "[媒体处理] 启动子进程: {} {:?}",
        cmd.as_std().get_program().to_string_lossy(),
        cmd.as_std().get_args().collect::<Vec<_>>()
    );
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("stdout 不可用"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("stderr 不可用"))?;
    let result = tokio::time::timeout(timeout, async {
        tokio::try_join!(
            child.wait(),
            bounded_read(stdout, stdout_limit, false),
            bounded_read(stderr, 16384, true)
        )
    })
    .await;
    match result {
        Ok(Ok((status, stdout, stderr))) if status.success() => {
            if stdout_limit > 0 && stdout.len() == stdout_limit {
                return Err(invalid("子进程输出超过长度限制"));
            }
            if !stderr.is_empty() {
                debug!("[媒体处理] {}", String::from_utf8_lossy(&stderr));
            }
            Ok(stdout)
        }
        Ok(Ok((status, _, stderr))) => Err(io::Error::other(format!(
            "子进程退出 {}: {}",
            status,
            String::from_utf8_lossy(&stderr)
        ))),
        Ok(Err(error)) => {
            let _ = child.kill().await;
            Err(error)
        }
        Err(_) => {
            let _ = child.kill().await;
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("子进程超过 {} 秒", timeout.as_secs()),
            ))
        }
    }
}
async fn read_probe(program: &Path, source: &Path) -> io::Result<Probe> {
    let mut cmd = Command::new(program);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration:stream=index,codec_type,codec_name:stream_tags=language,title:stream_disposition=default",
        "-of",
        "json",
    ])
    .arg(source);
    let bytes = run_command(&mut cmd, Duration::from_secs(60), 1024 * 1024).await?;
    serde_json::from_slice(&bytes).map_err(|e| invalid(format!("FFprobe JSON 无效: {e}")))
}
fn cross_device(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(18)
    } // EXDEV on Linux
    #[cfg(windows)]
    {
        error.raw_os_error() == Some(17)
    } // ERROR_NOT_SAME_DEVICE
    #[cfg(not(any(unix, windows)))]
    {
        error.kind() == io::ErrorKind::CrossesDevices
    }
}
fn unique_sibling(path: &Path) -> PathBuf {
    path.with_file_name(format!(".ncnc-copy-{:032x}.tmp", rand::random::<u128>()))
}
async fn copy_synced(from: &Path, to: &Path) -> io::Result<()> {
    let mut input = fs::File::open(from).await?;
    let expected = input.metadata().await?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .await?;
    let copied = tokio::io::copy(&mut input, &mut output).await?;
    output.flush().await?;
    if copied != expected.len()
        || output.metadata().await?.len() != expected.len()
        || input.metadata().await?.modified()? != expected.modified()?
    {
        return Err(invalid("复制长度不符或源文件发生变化"));
    }
    output.set_permissions(expected.permissions()).await?;
    // Preserve mtime for recovery of a rename that succeeded before journal save.
    let modified = expected.modified()?;
    let output = output.into_std().await;
    crate::io_util::blocking(move || {
        output.set_times(std::fs::FileTimes::new().set_modified(modified))?;
        output.sync_all()
    })
    .await
    .map_err(io::Error::other)??;
    debug!(
        "[媒体处理] 完整文件复制: {} bytes, {} -> {}",
        copied,
        from.display(),
        to.display()
    );
    Ok(())
}
// Err means no commit. After a successful rename, cleanup errors are separate.
async fn move_file_safe(from: &Path, to: &Path) -> io::Result<Option<String>> {
    finish_move(from, to, fs::rename(from, to).await).await
}
async fn finish_move(
    from: &Path,
    to: &Path,
    rename_result: io::Result<()>,
) -> io::Result<Option<String>> {
    match rename_result {
        Ok(()) => Ok(None),
        Err(error) if cross_device(&error) => {
            let temporary = unique_sibling(to);
            let copied = async {
                copy_synced(from, &temporary).await?;
                fs::rename(&temporary, to).await
            }
            .await;
            if let Err(error) = copied {
                let _ = fs::remove_file(&temporary).await;
                return Err(error);
            }
            let cleanup = fs::remove_file(from).await;
            Ok(cleanup.err().map(|e| e.to_string()))
        }
        Err(error) => Err(error),
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Stamp {
    len: u64,
    modified_ns: u128,
}
impl Stamp {
    // Cross-filesystem copies may lose subsecond precision. Keep the serialized
    // nanoseconds (including old journals), and relax only destination checks.
    fn matches_destination(&self, other: &Self) -> bool {
        self.len == other.len
            && self.modified_ns / 1_000_000_000 == other.modified_ns / 1_000_000_000
    }

    async fn read(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path).await?;
        if !meta.is_file() {
            return Err(invalid("路径不是普通文件"));
        }
        Ok(Self {
            len: meta.len(),
            modified_ns: meta
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos(),
        })
    }
}
#[derive(Clone)]
struct Output {
    temporary: PathBuf,
    target: PathBuf,
    stamp: Option<Stamp>,
    committed: bool,
}
impl Output {
    async fn matches_target(&self) -> io::Result<bool> {
        let actual = Stamp::read(&self.target).await?;
        Ok(self
            .stamp
            .as_ref()
            .is_some_and(|expected| actual.matches_destination(expected)))
    }
}
struct ProcessingBatch {
    source: PathBuf,
    source_stamp: Stamp,
    outputs: Vec<Output>, // subtitles first, media last: no upload before all commit
    media: bool,
}
// Stable across restarts/toolchain changes; identity collisions are checked in the journal.
fn path_key(source: &Path, target: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in source
        .to_string_lossy()
        .bytes()
        .chain([0])
        .chain(target.to_string_lossy().bytes())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
type TargetLocks = HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>;
static TARGET_LOCKS: Lazy<Mutex<TargetLocks>> = Lazy::new(|| Mutex::new(HashMap::new()));
static SOURCE_LOCKS: Lazy<Mutex<TargetLocks>> = Lazy::new(|| Mutex::new(HashMap::new()));
fn target_lock(target: &Path) -> Arc<tokio::sync::Mutex<()>> {
    resource_lock(&TARGET_LOCKS, &target.with_extension(""))
}
fn resource_lock(registry: &Mutex<TargetLocks>, path: &Path) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = registry.lock().unwrap_or_else(|p| p.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    let key = path.to_path_buf();
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}
fn ffmpeg_command(
    program: &Path,
    source: &Path,
    plan: &MediaPlan,
    outputs: &[Output],
    media: bool,
    subtitles: bool,
) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["-nostdin", "-hide_banner", "-v", "error", "-xerror", "-y"]);
    if media && plan.video_compress {
        cmd.args(["-hwaccel", "qsv"]);
    }
    cmd.arg("-i").arg(source);
    if media {
        cmd.args(["-map", "0:v:0"]);
        if let Some(audio_order) = &plan.preferred_audio_order {
            for index in audio_order {
                cmd.arg("-map").arg(format!("0:{index}"));
            }
        } else {
            cmd.args(["-map", "0:a?"]);
        }
        for index in &plan.embedded_pgs {
            cmd.arg("-map").arg(format!("0:{index}"));
        }
        if plan.video_compress {
            // MP4's automatic sync can select VFR. QSV frame reordering can
            // then produce equal DTS near EOF on inputs with timestamp jitter.
            // Normalize the video cadence before encoding, keeping -xerror so
            // real decoding/muxing failures still prevent transaction commit.
            cmd.args([
                "-c:v",
                "hevc_qsv",
                "-preset",
                "fast",
                "-global_quality",
                "20",
                "-fps_mode:v:0",
                "cfr",
            ]);
        } else {
            cmd.args(["-c:v", "copy"]);
        }
        if plan.audio_compress {
            cmd.args(["-c:a", "aac", "-b:a", "320k"]);
        } else {
            cmd.args(["-c:a", "copy"]);
        }
        if plan.preferred_audio_order.is_some() {
            for output_index in 0..plan.audio_count {
                cmd.arg(format!("-disposition:a:{output_index}"))
                    .arg(if output_index == 0 {
                        "+default"
                    } else {
                        "-default"
                    });
            }
        }
        if !plan.embedded_pgs.is_empty() {
            cmd.args(["-c:s", "copy"]);
        }
        cmd.args(["-movflags", "use_metadata_tags"])
            .arg(&outputs.last().unwrap().temporary);
    }
    if subtitles {
        for (track, output) in plan.subtitles.iter().zip(outputs) {
            cmd.arg("-map")
                .arg(format!("0:{}", track.index))
                .args(["-c:s", "ass", "-f", "ass"])
                .arg(&output.temporary);
        }
    }
    cmd
}
async fn prepare(
    tx: &mut ProcessingBatch,
    work: &Path,
    target: &Path,
    combine: bool,
) -> io::Result<()> {
    let tools = crate::media_tools::init().await;
    let paths = tools.paths();
    let remux = tx.media
        && !tx.source.extension().is_some_and(|ext| {
            target.extension().is_some_and(|output| {
                ext.to_string_lossy()
                    .eq_ignore_ascii_case(&output.to_string_lossy())
            })
        });
    if remux && paths.is_none() {
        return Err(invalid(
            "FFmpeg/FFprobe 缺失，无法重新封装为 MKV，保留源文件",
        ));
    }
    let plan = if tx.media {
        if let Some((_, ffprobe)) = paths {
            Some(media_plan(
                read_probe(ffprobe, &tx.source).await?,
                tx.source_stamp.len,
                target
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("mkv")),
            )?)
        } else {
            debug!(
                "[媒体处理] FFmpeg/FFprobe 缺失，跳过探测、压缩和字幕提取，直接移动: {}",
                tx.source.display()
            );
            None
        }
    } else {
        None
    };
    let generate = remux
        || plan
            .as_ref()
            .is_some_and(|p| p.video_compress || p.audio_compress || !p.subtitles.is_empty());
    tx.outputs.clear();
    if let Some(plan) = &plan {
        let stem = target
            .file_stem()
            .ok_or_else(|| invalid("目标文件名缺失"))?
            .to_string_lossy();
        for track in &plan.subtitles {
            tx.outputs.push(Output {
                temporary: work.join(format!("subtitle-{}.ass", track.index)),
                target: target.with_file_name(format!("{}{}", stem, track.suffix)),
                stamp: None,
                committed: false,
            });
        }
    }
    tx.outputs.push(Output {
        temporary: if generate {
            work.join(format!(
                "media.{}",
                target.extension().unwrap_or_default().to_string_lossy()
            ))
        } else {
            tx.source.clone()
        },
        target: target.to_path_buf(),
        stamp: None,
        committed: false,
    });
    if generate {
        let (ffmpeg, ffprobe) = paths.ok_or_else(|| invalid("媒体工具缺失"))?;
        let plan = plan.as_ref().unwrap();
        let timeout = Duration::from_secs(24 * 60 * 60);
        if !combine && !plan.subtitles.is_empty() {
            run_command(
                &mut ffmpeg_command(ffmpeg, &tx.source, plan, &tx.outputs, false, true),
                timeout,
                0,
            )
            .await?;
            run_command(
                &mut ffmpeg_command(ffmpeg, &tx.source, plan, &tx.outputs, true, false),
                timeout,
                0,
            )
            .await?;
        } else {
            run_command(
                &mut ffmpeg_command(ffmpeg, &tx.source, plan, &tx.outputs, true, true),
                timeout,
                0,
            )
            .await?;
        }
        for output in &tx.outputs {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&output.temporary)
                .await?;
            if file.metadata().await?.len() == 0 {
                return Err(invalid("FFmpeg 输出为空"));
            }
            if output.temporary.extension().is_some_and(|e| e == "ass") {
                let mut header = [0; 4096];
                let n = file.read(&mut header).await?;
                if !String::from_utf8_lossy(&header[..n]).contains("[Script Info]") {
                    return Err(invalid("ASS 输出头无效"));
                }
            }
        }
        // Probe the generated output (not the input again) to reject invalid muxes.
        let output_probe = read_probe(ffprobe, &tx.outputs.last().unwrap().temporary).await?;
        if output_probe
            .streams
            .iter()
            .filter(|s| s.codec_type == "video")
            .count()
            != 1
            || output_probe
                .streams
                .iter()
                .filter(|s| s.codec_type == "audio")
                .count()
                != plan.audio_count
            || output_probe
                .streams
                .iter()
                .filter(|s| s.codec_type == "subtitle")
                .count()
                != plan.embedded_pgs.len()
            || output_probe
                .streams
                .iter()
                .filter(|s| s.codec_type == "subtitle")
                .any(|s| s.codec_name.as_deref() != Some("hdmv_pgs_subtitle"))
            || output_probe
                .format
                .and_then(|f| f.duration)
                .and_then(|d| d.parse::<f64>().ok())
                .filter(|d| d.is_finite() && *d > 0.0)
                .is_none()
        {
            return Err(invalid("生成媒体的流数量或时长无效"));
        }
        if Stamp::read(&tx.source).await? != tx.source_stamp {
            return Err(invalid("处理期间输入发生变化"));
        }
    }
    for output in &mut tx.outputs {
        output.stamp = Some(Stamp::read(&output.temporary).await?);
    }
    Ok(())
}

async fn commit(tx: &mut ProcessingBatch) -> io::Result<()> {
    for index in 0..tx.outputs.len() {
        let output = &mut tx.outputs[index];
        if output.committed {
            if !output.matches_target().await? {
                return Err(invalid("先前已提交输出被修改"));
            }
            continue;
        }
        let expected = output.stamp.as_ref().ok_or_else(|| invalid("输出未验证"))?;
        match Stamp::read(&output.temporary).await {
            Ok(stamp) => {
                if &stamp != expected {
                    return Err(invalid("暂存文件被修改，停止提交"));
                }
                if let Some(warning) = move_file_safe(&output.temporary, &output.target).await? {
                    warn!("[媒体处理] 提交后清理待重试: {}", warning);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if !Stamp::read(&output.target)
                    .await?
                    .matches_destination(expected)
                {
                    return Err(invalid("暂存输出缺失且目标与提交记录不符"));
                }
            }
            Err(e) => return Err(e),
        }
        output.stamp = Some(Stamp::read(&output.target).await?);
        output.committed = true;
    }
    Ok(())
}
async fn cleanup(tx: &ProcessingBatch) -> io::Result<()> {
    for output in &tx.outputs {
        if !output.matches_target().await? {
            return Err(invalid("已提交文件被修改，保留源文件等待检查"));
        }
    }
    if tx.media {
        let target = &tx.outputs.last().unwrap().target;
        for ext in COMMON_MEDIA_EXTS {
            let old = target.with_extension(ext);
            if old == *target || old == tx.source {
                continue;
            }
            match fs::remove_file(&old).await {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    match Stamp::read(&tx.source).await {
        Ok(stamp) if stamp == tx.source_stamp => fs::remove_file(&tx.source).await?,
        Ok(_) => return Err(invalid("源路径出现新文件，拒绝清理")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(())
}
async fn process_inner(
    source: &Path,
    target: &Path,
    backup_dir: &Path,
    combine: bool,
) -> io::Result<ProcessOutcome> {
    if fs::symlink_metadata(source)
        .await
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(invalid("源文件是符号链接，拒绝移动"));
    }
    if source == target
        || match (
            fs::canonicalize(source).await,
            fs::canonicalize(target).await,
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    {
        return Ok(ProcessOutcome::Deferred("源路径与目标相同".into()));
    }
    if fs::try_exists(source.with_file_name(format!(
        "{}.aria2",
        source.file_name().unwrap_or_default().to_string_lossy()
    )))
    .await?
    {
        return Ok(ProcessOutcome::Deferred("下载尚未完成".into()));
    }
    let parent = target.parent().ok_or_else(|| invalid("目标目录缺失"))?;
    let work = backup_dir.join(path_key(source, target));
    let source_stamp = match Stamp::read(source).await {
        Ok(stamp) => stamp,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(ProcessOutcome::Deferred("源文件不存在".into()))
        }
        Err(e) => return Err(e),
    };
    match fs::remove_dir_all(&work).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir_all(&work).await?;
    let mut tx = ProcessingBatch {
        source: source.to_path_buf(),
        source_stamp,
        outputs: Vec::new(),
        media: target
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| COMMON_MEDIA_EXTS.contains(&e.to_lowercase().as_str())),
    };
    prepare(&mut tx, &work, target, combine).await?;
    fs::create_dir_all(parent).await?;
    if let Err(e) = commit(&mut tx).await {
        if tx.outputs.iter().all(|o| o.committed) {
            return Ok(ProcessOutcome::CommittedCleanupFailed(e.to_string()));
        }
        return Err(e);
    }
    if let Err(e) = cleanup(&tx).await {
        return Ok(ProcessOutcome::CommittedCleanupFailed(e.to_string()));
    }
    match fs::remove_dir_all(&work).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => warn!(
            "[媒体处理] 清理转码工作目录失败 {}: {}",
            work.display(),
            error
        ),
    }
    Ok(ProcessOutcome::Success)
}
async fn process(source: &str, target: &str) -> ProcessOutcome {
    let _permit = match MEDIA_PROCESS_SEMAPHORE.acquire().await {
        Ok(p) => p,
        Err(e) => return ProcessOutcome::Failed(e.to_string()),
    };
    let source_path = match std::path::absolute(source) {
        Ok(p) => p,
        Err(e) => return ProcessOutcome::Failed(e.to_string()),
    };
    let target_path = match std::path::absolute(target) {
        Ok(p) => p,
        Err(e) => return ProcessOutcome::Failed(e.to_string()),
    };
    let lock = target_lock(&target_path);
    let source_lock = resource_lock(&SOURCE_LOCKS, &source_path);
    let _source_guard = source_lock.lock().await;
    let _guard = lock.lock().await;
    let (backup, combine) = {
        let config = CONFIG.read().await;
        (
            match std::path::absolute(&config.media_backup_path) {
                Ok(path) => path,
                Err(e) => return ProcessOutcome::Failed(e.to_string()),
            },
            config.media_combine_outputs,
        )
    };
    info!("[媒体处理] 开始处理文件: {}", source_path.display());
    let task_key = path_key(&source_path, &target_path);
    RUNTIME_STATE
        .begin_processing(
            task_key.clone(),
            source_path.to_string_lossy().into_owned(),
            target_path.to_string_lossy().into_owned(),
        )
        .await;
    let outcome = process_inner(&source_path, &target_path, &backup, combine)
        .await
        .unwrap_or_else(|e| {
            if is_permanent(&e) {
                ProcessOutcome::PermanentFailure(e.to_string())
            } else {
                ProcessOutcome::Failed(e.to_string())
            }
        });
    RUNTIME_STATE.finish_processing(&task_key).await;
    outcome.log(source, &target_path);
    outcome
}
fn output_extension(ext: &str) -> &str {
    if ext.eq_ignore_ascii_case("mp4") {
        "mp4"
    } else if COMMON_MEDIA_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
        "mkv"
    } else {
        ext
    }
}

pub async fn rename_and_move(
    media_path: &str,
    display_name: &str,
    is_jp: bool,
    season: &str,
    ep: &str,
    _file_name: &str,
    file_path: &str,
    ext: &str,
    tmdbid: u32,
) -> ProcessOutcome {
    let season_num = season.parse::<u32>().unwrap_or(0);
    let ep_num = ep.parse::<u32>().unwrap_or(0);
    let suffix = if ext == "xml" {
        if is_jp {
            "-JP"
        } else {
            "-CN"
        }
    } else {
        ""
    };
    let folder = if season_num == 0 {
        format!("{media_path}/Specials")
    } else {
        format!("{media_path}/Season {season_num}")
    };
    let ext = output_extension(ext);
    let target = format!("{folder}/{display_name} - S{season_num:02}E{ep_num:02}{suffix}.{ext}");
    let outcome = process(file_path, &target).await;
    if outcome.is_success() {
        update::mark_episode_exists(tmdbid, season_num as i32, ep_num as i32, &target).await;
        openlist_uploader::enqueue_upload(target).await;
    }
    outcome
}
pub async fn rename_and_move_movie(
    file_path: &str,
    display_name: &str,
    media_path: &str,
    _file_name: &str,
    ext: &str,
) -> ProcessOutcome {
    let suffix = if ext == "xml" { "-CN" } else { "" };
    let ext = output_extension(ext);
    let target = format!("{media_path}/{display_name}{suffix}.{ext}");
    let outcome = process(file_path, &target).await;
    if outcome.is_success() {
        openlist_uploader::enqueue_upload(target).await;
    }
    outcome
}

#[cfg(test)]
mod workspace_tests {
    use super::*;

    fn probe_with_audio_tracks(tracks: &[(&str, bool)]) -> Probe {
        let mut streams = vec![Stream {
            index: 0,
            codec_type: "video".into(),
            codec_name: Some("h264".into()),
            tags: None,
            disposition: None,
        }];
        streams.extend(
            tracks
                .iter()
                .enumerate()
                .map(|(position, (language, is_default))| Stream {
                    index: position as u32 + 1,
                    codec_type: "audio".into(),
                    codec_name: Some("aac".into()),
                    tags: Some(Tags {
                        language: Some((*language).into()),
                        title: None,
                    }),
                    disposition: Some(Disposition {
                        default: i32::from(*is_default),
                    }),
                }),
        );
        Probe {
            streams,
            format: Some(Format {
                duration: Some("1200".into()),
            }),
        }
    }

    fn probe_with_text_and_pgs() -> Probe {
        Probe {
            streams: vec![
                Stream {
                    index: 0,
                    codec_type: "video".into(),
                    codec_name: Some("h264".into()),
                    tags: None,
                    disposition: None,
                },
                Stream {
                    index: 1,
                    codec_type: "audio".into(),
                    codec_name: Some("aac".into()),
                    tags: None,
                    disposition: None,
                },
                Stream {
                    index: 3,
                    codec_type: "subtitle".into(),
                    codec_name: Some("subrip".into()),
                    tags: Some(Tags {
                        language: Some("chi".into()),
                        title: None,
                    }),
                    disposition: None,
                },
                Stream {
                    index: 4,
                    codec_type: "subtitle".into(),
                    codec_name: Some("hdmv_pgs_subtitle".into()),
                    tags: Some(Tags {
                        language: Some("chi".into()),
                        title: None,
                    }),
                    disposition: None,
                },
            ],
            format: Some(Format {
                duration: Some("1200".into()),
            }),
        }
    }

    #[test]
    fn text_subtitles_are_extracted_and_pgs_is_kept_in_mkv() {
        let plan = media_plan(probe_with_text_and_pgs(), 500_000_000, true).unwrap();
        assert_eq!(plan.subtitles.len(), 1);
        assert_eq!(plan.subtitles[0].index, 3);
        assert_eq!(plan.embedded_pgs, vec![4]);
    }

    #[test]
    fn pgs_is_rejected_when_the_target_is_not_mkv() {
        let error = media_plan(probe_with_text_and_pgs(), 500_000_000, false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(is_permanent(&error));
        assert!(error.to_string().contains("仅支持原样保留在 MKV 中"));
    }

    #[test]
    fn japanese_audio_is_promoted_only_when_default_audio_is_chi() {
        let plan = media_plan(
            probe_with_audio_tracks(&[("chi", true), ("jpn", false)]),
            500_000_000,
            true,
        )
        .unwrap();
        assert_eq!(plan.preferred_audio_order, Some(vec![2, 1]));

        let output = Output {
            temporary: PathBuf::from("output.mkv"),
            target: PathBuf::from("target.mkv"),
            stamp: None,
            committed: false,
        };
        let command = ffmpeg_command(
            Path::new("ffmpeg"),
            Path::new("input.mkv"),
            &plan,
            &[output],
            true,
            false,
        );
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args
            .windows(4)
            .any(|args| args == ["-map", "0:2", "-map", "0:1"]));
        assert!(args
            .windows(2)
            .any(|args| args == ["-disposition:a:0", "+default"]));
        assert!(args
            .windows(2)
            .any(|args| args == ["-disposition:a:1", "-default"]));

        let japanese_already_default = media_plan(
            probe_with_audio_tracks(&[("jpn", true), ("chi", false)]),
            500_000_000,
            true,
        )
        .unwrap();
        assert_eq!(japanese_already_default.preferred_audio_order, None);

        let non_chi_default = media_plan(
            probe_with_audio_tracks(&[("eng", true), ("jpn", false)]),
            500_000_000,
            true,
        )
        .unwrap();
        assert_eq!(non_chi_default.preferred_audio_order, None);
        let output = Output {
            temporary: PathBuf::from("output.mkv"),
            target: PathBuf::from("target.mkv"),
            stamp: None,
            committed: false,
        };
        let command = ffmpeg_command(
            Path::new("ffmpeg"),
            Path::new("input.mkv"),
            &non_chi_default,
            &[output],
            true,
            false,
        );
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|args| args == ["-map", "0:a?"]));
        assert!(!args.iter().any(|arg| arg.starts_with("-disposition:a:")));

        let chi_default_without_japanese = media_plan(
            probe_with_audio_tracks(&[("chi", true), ("eng", false)]),
            500_000_000,
            true,
        )
        .unwrap();
        assert_eq!(chi_default_without_japanese.preferred_audio_order, None);
    }

    #[test]
    fn ordinary_invalid_data_is_not_automatically_permanent() {
        assert!(!is_permanent(&invalid("处理期间输入发生变化")));
    }

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("ncnc-work-test-{:032x}", rand::random::<u128>())),
            )
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn stale_work_is_discarded_and_no_transaction_is_written() {
        let root = TestDirectory::new();
        let source = root.0.join("source.ass");
        let target = root.0.join("library/episode.ass");
        let backup = root.0.join("backup");
        let work = backup.join(path_key(&source, &target));
        fs::create_dir_all(&work).await.unwrap();
        fs::write(work.join("stale.part"), b"partial")
            .await
            .unwrap();
        fs::write(&source, b"subtitle content").await.unwrap();

        assert!(process_inner(&source, &target, &backup, true)
            .await
            .unwrap()
            .is_success());
        assert_eq!(fs::read(&target).await.unwrap(), b"subtitle content");
        assert!(!source.exists());
        assert!(!work.exists());
    }

    #[tokio::test]
    async fn missing_source_is_deferred_without_touching_library() {
        let root = TestDirectory::new();
        let source = root.0.join("missing.ass");
        let target = root.0.join("library/episode.ass");
        let backup = root.0.join("backup");
        assert!(matches!(
            process_inner(&source, &target, &backup, true)
                .await
                .unwrap(),
            ProcessOutcome::Deferred(_)
        ));
        assert!(!target.exists());
    }
}
