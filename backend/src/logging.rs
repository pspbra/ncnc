//! Daily application logs, retaining seven local calendar days.

use chrono::{Days, Local, NaiveDate};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

const LOG_DIRECTORY: &str = "logs";
const RETENTION_DAYS: u64 = 7;

pub fn available_dates() -> io::Result<Vec<String>> {
    let directory = std::env::current_dir()?.join(LOG_DIRECTORY);
    let mut dates = Vec::new();

    match fs::read_dir(directory) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(date) = name
                    .strip_prefix("backend-")
                    .and_then(|value| value.strip_suffix(".log"))
                else {
                    continue;
                };
                let Ok(parsed) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
                    continue;
                };
                if name == format!("backend-{}.log", parsed.format("%Y-%m-%d")) {
                    dates.push(date.to_string());
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(dates),
        Err(error) => return Err(error),
    }

    dates.sort_unstable_by(|left, right| right.cmp(left));
    Ok(dates)
}

pub struct LogPage {
    pub content: String,
    pub offset: u64,
    pub next_offset: u64,
    pub total: u64,
}

pub fn read_page(date: &str, offset: Option<u64>, limit: usize) -> io::Result<LogPage> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid log date"))?;
    if date != parsed.format("%Y-%m-%d").to_string() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid log date",
        ));
    }
    flush_pending()?;
    let path = log_path(&std::env::current_dir()?.join(LOG_DIRECTORY), parsed);
    page_from_path(&path, offset, limit)
}

fn page_from_path(path: &Path, offset: Option<u64>, limit: usize) -> io::Result<LogPage> {
    let mut file = File::open(path)?;
    let total = file.metadata()?.len();
    let limit = limit.clamp(4, 1024 * 1024);
    let mut start = offset
        .unwrap_or_else(|| total.saturating_sub(limit as u64))
        .min(total);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(limit as u64).read_to_end(&mut bytes)?;
    if offset.is_none() && start > 0 {
        if let Some(newline) = bytes.iter().position(|b| *b == b'\n') {
            start += (newline + 1) as u64;
            bytes.drain(..=newline);
        }
        while bytes.first().is_some_and(|byte| byte & 0xc0 == 0x80) {
            bytes.remove(0);
            start += 1;
        }
    }
    if let Err(error) = std::str::from_utf8(&bytes) {
        if error.error_len().is_none() && error.valid_up_to() > 0 {
            bytes.truncate(error.valid_up_to());
        }
    }
    Ok(LogPage {
        next_offset: start + bytes.len() as u64,
        offset: start,
        total,
        content: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

enum Message {
    Record(NaiveDate, Vec<u8>),
    Flush(mpsc::Sender<io::Result<()>>),
}
static LOG_QUEUE: OnceLock<mpsc::SyncSender<Message>> = OnceLock::new();
struct QueuedWriter(mpsc::SyncSender<Message>);
impl Write for QueuedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .send(Message::Record(Local::now().date_naive(), bytes.to_vec()))
            .map_err(io::Error::other)?;
        Ok(bytes.len())
    }
    // env_logger flushes each record. The worker batches them instead.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn flush_pending() -> io::Result<()> {
    if let Some(queue) = LOG_QUEUE.get() {
        let (sender, receiver) = mpsc::channel();
        queue
            .send(Message::Flush(sender))
            .map_err(io::Error::other)?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(io::Error::other)??;
    }
    Ok(())
}

struct DailyWriter {
    directory: PathBuf,
    date: NaiveDate,
    file: BufWriter<File>,
}

fn log_path(directory: &Path, date: NaiveDate) -> PathBuf {
    directory.join(format!("backend-{}.log", date.format("%Y-%m-%d")))
}

impl DailyWriter {
    fn new(directory: PathBuf, date: NaiveDate) -> io::Result<Self> {
        fs::create_dir_all(&directory)?;
        let file = Self::open(&directory, date)?;
        Ok(Self {
            directory,
            date,
            file,
        })
    }

    fn open(directory: &Path, date: NaiveDate) -> io::Result<BufWriter<File>> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path(directory, date))?;
        Ok(BufWriter::with_capacity(64 * 1024, file))
    }

    fn write_on(&mut self, date: NaiveDate, bytes: &[u8]) -> io::Result<()> {
        if date != self.date {
            // Open successfully before replacing the old handle; retry on the next write on failure.
            let file = Self::open(&self.directory, date)?;
            self.file.flush()?;
            self.file = file;
            self.date = date;
        }
        self.file.write_all(bytes)
    }
}

impl Write for DailyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(error) = self.write_on(Local::now().date_naive(), bytes) {
            // env_logger may discard writer errors, so make failures visible and preserve the record.
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "[logging] Failed to write daily log: {error}");
            stderr.write_all(bytes)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn cleanup(directory: &Path, today: NaiveDate) -> io::Result<()> {
    let cutoff = today
        .checked_sub_days(Days::new(RETENTION_DAYS - 1))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid retention cutoff"))?;
    let mut first_error = None;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        // Only delete regular files with our exact filename format, never directories or symlinks.
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(date) = name
            .strip_prefix("backend-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if name != format!("backend-{}.log", date.format("%Y-%m-%d")) || date >= cutoff {
            continue;
        }
        if let Err(error) = fs::remove_file(entry.path()) {
            if error.kind() != io::ErrorKind::NotFound && first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn cleanup_or_report(directory: &Path) {
    if let Err(error) = cleanup(directory, Local::now().date_naive()) {
        eprintln!("[日志] 清理过期日志失败: {error}");
    }
}

fn until_next_midnight() -> Duration {
    let now = Local::now();
    let next_midnight = now
        .date_naive()
        .succ_opt()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest());
    next_midnight
        .and_then(|midnight| (midnight - now).to_std().ok())
        // Retry when a timezone transition makes local midnight unavailable.
        .unwrap_or(Duration::from_secs(3600))
}

pub fn init() -> io::Result<()> {
    let directory = std::env::current_dir()?.join(LOG_DIRECTORY);
    let mut writer = DailyWriter::new(directory.clone(), Local::now().date_naive())?;
    let (sender, receiver) = mpsc::sync_channel(1024);
    LOG_QUEUE
        .set(sender.clone())
        .map_err(|_| io::Error::other("logging already initialized"))?;
    std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || {
            let mut last_flush = std::time::Instant::now();
            loop {
                match receiver.recv_timeout(Duration::from_secs(1)) {
                    Ok(Message::Record(date, bytes)) => {
                        if let Err(error) = writer.write_on(date, &bytes) {
                            eprintln!("[日志] 写入失败: {error}");
                            let _ = io::stderr().write_all(&bytes);
                        }
                    }
                    Ok(Message::Flush(reply)) => {
                        let _ = reply.send(writer.flush());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = writer.flush();
                        break;
                    }
                }
                if last_flush.elapsed() >= Duration::from_secs(1) {
                    if let Err(error) = writer.flush() {
                        eprintln!("[日志] 刷新失败: {error}");
                    }
                    last_flush = std::time::Instant::now();
                }
            }
        })?;
    cleanup_or_report(&directory);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(QueuedWriter(sender))))
        .write_style(env_logger::WriteStyle::Never)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {} {}] {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .try_init()
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;

    // Wait for each local midnight rather than scanning the directory periodically.
    std::thread::Builder::new()
        .name("log-cleanup".into())
        .spawn(move || {
            let mut last_cleanup_date = Local::now().date_naive();
            loop {
                std::thread::sleep(until_next_midnight());
                let today = Local::now().date_naive();
                if today != last_cleanup_date {
                    cleanup_or_report(&directory);
                    last_cleanup_date = today;
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_pages_are_bounded_and_offsets_read_only_new_bytes() {
        let path = std::env::temp_dir().join(format!("ncnc-log-{}.log", rand::random::<u64>()));
        fs::write(&path, "first line\n第二行\nlast line\n").unwrap();
        let page = page_from_path(&path, None, 15).unwrap();
        assert_eq!(page.content, "last line\n");
        assert_eq!(page.next_offset, page.total);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"new line\n").unwrap();
        let delta = page_from_path(&path, Some(page.next_offset), 1024).unwrap();
        assert_eq!(delta.content, "new line\n");
        let first = page_from_path(&path, Some(0), 11).unwrap();
        assert_eq!(first.content, "first line\n");
        drop(file);
        fs::remove_file(path).unwrap();
    }
}
