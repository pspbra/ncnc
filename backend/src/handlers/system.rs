use crate::models::{SystemInfo, SystemInfoResponse};
use crate::CONFIG;
use crate::SYSTEM_INFO_CACHE;
use actix_web::HttpResponse;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::path::Path;
use sysinfo::{CpuExt, DiskExt, NetworkExt, System, SystemExt};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

#[derive(Clone)]
struct NetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
    timestamp: std::time::Instant,
}

static NETWORK_STATS: Lazy<RwLock<Option<NetworkStats>>> = Lazy::new(|| RwLock::new(None));

static TEMPERATURE: Lazy<RwLock<(String, Option<f32>)>> =
    Lazy::new(|| RwLock::new((String::new(), None)));

pub async fn start_system_monitor() {
    info!("[系统监控] 系统采样器已启动");
    tokio::spawn(async {
        loop {
            let media_path = CONFIG.read().await.media_library_path.clone();
            let temperature = get_disk_temperature(&media_path).await;
            *TEMPERATURE.write().await = (media_path, temperature);
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    let mut system = System::new();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_discovery = Instant::now() - Duration::from_secs(60);
    let mut last_disk = Instant::now() - Duration::from_secs(15);
    let mut previous_path = String::new();
    let mut canonical_path = None;
    let mut disk_info = (0, 0, 0, 0.0);
    loop {
        interval.tick().await;
        let media_path = CONFIG.read().await.media_library_path.clone();
        let discover =
            previous_path != media_path || last_discovery.elapsed() >= Duration::from_secs(60);
        let sample_disk = discover || last_disk.elapsed() >= Duration::from_secs(15);
        if discover {
            canonical_path = tokio::fs::canonicalize(&media_path).await.ok();
            previous_path = media_path.clone();
            last_discovery = Instant::now();
        }
        let path = canonical_path.clone();
        let (refreshed, new_disk_info) = crate::io_util::blocking(move || {
            system.refresh_cpu();
            if discover {
                system.refresh_disks_list();
                system.refresh_networks_list();
            }
            if sample_disk {
                system.refresh_disks();
            }
            system.refresh_networks();
            let disk = if sample_disk {
                path.as_deref()
                    .map(|path| get_disk_info(path, &system))
                    .unwrap_or((0, 0, 0, 0.0))
            } else {
                disk_info
            };
            (system, disk)
        })
        .await
        .expect("system sampler task failed");
        system = refreshed;
        disk_info = new_disk_info;
        if sample_disk {
            last_disk = Instant::now();
        }
        let (upload_speed, download_speed) = get_network_speed(&system).await;
        let temperature = TEMPERATURE.read().await;
        let disk_temperature = if temperature.0 == media_path {
            temperature.1
        } else {
            None
        };
        drop(temperature);
        *SYSTEM_INFO_CACHE.write().await = Some(SystemInfo {
            cpu_usage: system.global_cpu_info().cpu_usage(),
            upload_speed,
            download_speed,
            disk_total: disk_info.0,
            disk_used: disk_info.1,
            disk_available: disk_info.2,
            disk_usage_percent: disk_info.3,
            disk_temperature,
        });
    }
}

pub async fn get_system_info() -> HttpResponse {
    let cache = SYSTEM_INFO_CACHE.read().await;

    HttpResponse::Ok().json(SystemInfoResponse {
        success: true,
        data: cache.clone(),
        message: None,
    })
}

fn get_disk_info(path: &Path, system: &System) -> (u64, u64, u64, f32) {
    let disk = system
        .disks()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len());
    let Some(disk) = disk else {
        return (0, 0, 0, 0.0);
    };
    let total = disk.total_space();
    let available = disk.available_space();
    let used = total.saturating_sub(available);
    let percent = if total > 0 {
        used as f32 / total as f32 * 100.0
    } else {
        0.0
    };
    (total, used, available, percent)
}

async fn get_network_speed(system: &System) -> (u64, u64) {
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;

    for (_, network) in system.networks() {
        total_rx += network.total_received();
        total_tx += network.total_transmitted();
    }

    let now = std::time::Instant::now();
    let mut stats_guard = NETWORK_STATS.write().await;

    if let Some(prev) = stats_guard.as_ref() {
        let duration = now.duration_since(prev.timestamp);
        let seconds = duration.as_secs_f64();

        if seconds > 0.1 {
            let rx_delta = total_rx.saturating_sub(prev.rx_bytes);
            let tx_delta = total_tx.saturating_sub(prev.tx_bytes);

            let rx_speed = (rx_delta as f64 / seconds) as u64;
            let tx_speed = (tx_delta as f64 / seconds) as u64;

            *stats_guard = Some(NetworkStats {
                rx_bytes: total_rx,
                tx_bytes: total_tx,
                timestamp: now,
            });

            return (tx_speed, rx_speed);
        }
    }

    *stats_guard = Some(NetworkStats {
        rx_bytes: total_rx,
        tx_bytes: total_tx,
        timestamp: now,
    });

    (0, 0)
}

fn mount_device(path: &Path, mounts: &str) -> Option<String> {
    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = fields.next()?;
            let mount = fields
                .next()?
                .replace(r"\040", " ")
                .replace(r"\011", "\t")
                .replace(r"\012", "\n")
                .replace(r"\134", "\\");
            if path.starts_with(&mount) {
                Some((mount.len(), device.to_string()))
            } else {
                None
            }
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, device)| device)
}

async fn get_disk_temperature(path: &str) -> Option<f32> {
    let canonical = tokio::fs::canonicalize(path).await.ok()?;
    let mounts = tokio::fs::read_to_string("/proc/mounts").await.ok()?;
    let device = mount_device(&canonical, &mounts)?;
    if !device.starts_with("/dev/") {
        return None;
    }
    // Try directly first (root needs no sudo); retain noninteractive sudo for
    // deployments that grant SMART access through sudoers.
    for elevated in [false, true] {
        let mut command = if elevated {
            let mut command = Command::new("sudo");
            command.args(["-n", "/usr/sbin/smartctl"]);
            command
        } else {
            Command::new("/usr/sbin/smartctl")
        };
        command.args(["-a", "-j", &device]).kill_on_drop(true);
        match tokio::time::timeout(Duration::from_secs(10), command.output()).await {
            Ok(Ok(output)) => {
                if let Some(value) = smart_temperature(&output.stdout, output.status.code()) {
                    return Some(value);
                }
                // Only retry with sudo if opening the device was denied/failed.
                if !elevated && output.status.code().is_some_and(|code| code & 2 != 0) {
                    continue;
                }
                warn!(
                    "[系统监控] 无法获取 {} 的 SMART 温度（退出码：{:?}）",
                    device,
                    output.status.code()
                );
                return None;
            }
            Ok(Err(error)) => {
                warn!(
                    "[系统监控] 无法执行 {} 的 SMART 温度查询: {}",
                    device, error
                );
                return None;
            }
            Err(_) => {
                warn!("[系统监控] 查询 {} 的 SMART 温度超时", device);
                return None;
            }
        }
    }
    None
}

fn smart_temperature(bytes: &[u8], status: Option<i32>) -> Option<f32> {
    // smartctl returns a bitmask, not a simple success/failure flag. Health,
    // historical errors, or a failed optional command can coexist with a valid
    // temperature. Reject syntax/device-open failures, but inspect other output.
    let status = status?;
    if !(0..=255).contains(&status) || status & 3 != 0 {
        return None;
    }
    let report: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let value = report.get("temperature")?.get("current")?.as_f64()?;
    (value.is_finite() && (-60.0..=150.0).contains(&value)).then_some(value as f32)
}
