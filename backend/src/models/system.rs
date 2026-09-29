use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub cpu_usage: f32,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub disk_total: u64,
    pub disk_used: u64,
    pub disk_available: u64,
    pub disk_usage_percent: f32,
    pub disk_temperature: Option<f32>,
}

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub success: bool,
    pub data: Option<SystemInfo>,
    pub message: Option<String>,
}
