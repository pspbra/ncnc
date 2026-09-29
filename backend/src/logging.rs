//! Daily application logs, retaining seven local calendar days.

use chrono::{Days, Local, NaiveDate};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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

pub fn read_date(date: &str) -> io::Result<String> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid log date"))?;
    if date != parsed.format("%Y-%m-%d").to_string() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid log date",
        ));
    }

    let directory = std::env::current_dir()?.join(LOG_DIRECTORY);
    fs::read_to_string(log_path(&directory, parsed))
}

struct DailyWriter {
    directory: PathBuf,
    date: NaiveDate,
    file: File,
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

    fn open(directory: &Path, date: NaiveDate) -> io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path(directory, date))
    }

    fn write_on(&mut self, date: NaiveDate, bytes: &[u8]) -> io::Result<()> {
        if date != self.date {
            // Open successfully before replacing the old handle; retry on the next write on failure.
            let file = Self::open(&self.directory, date)?;
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
    let writer = DailyWriter::new(directory.clone(), Local::now().date_naive())?;
    cleanup_or_report(&directory);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(writer)))
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
