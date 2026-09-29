use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AddDownloadRequest {
    pub tmdbid: u32,
    pub season_number: u32,
    pub episode_number: u32,
    pub magnet_uri: String,
    pub media_path: String,
    #[serde(default)]
    pub is_multi_episode: bool,
}

#[derive(Deserialize)]
pub struct AddMovieDownloadRequest {
    pub display_name: String,
    pub media_path: String,
    pub magnet_uri: String,
}

#[derive(Serialize)]
pub struct AddDownloadResponse {
    pub success: bool,
    pub message: String,
    pub gid: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DownloadCache {
    pub gid: String,
    pub tmdbid: u32,
    pub season_number: u32,
    pub episode_number: u32,
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub is_multi_episode: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MovieDownloadCache {
    pub gid: String,
    pub display_name: String,
    pub media_path: String,
    pub file_paths: Vec<String>,
}

#[derive(Deserialize)]
pub struct Aria2Response<T> {
    pub result: Option<T>,
    pub error: Option<Aria2Error>,
}

#[derive(Deserialize)]
pub struct Aria2Error {
    pub code: i32,
    pub message: String,
}

#[derive(Deserialize, Clone)]
pub struct Aria2Status {
    pub gid: String,
    pub status: String,
    #[serde(rename = "followedBy", default)]
    pub followed_by: Vec<String>,
    #[serde(default)]
    pub files: Vec<Aria2File>,
    #[serde(rename = "totalLength", default)]
    pub total_length: String,
    #[serde(rename = "completedLength", default)]
    pub completed_length: String,
    #[serde(rename = "downloadSpeed", default)]
    pub download_speed: String,
    #[serde(default)]
    pub dir: String,
}

#[derive(Deserialize, Clone)]
pub struct Aria2File {
    pub path: String,
    pub selected: String,
}

#[derive(Serialize, Clone)]
pub struct DownloadData {
    pub id: String,
    pub is_metadata: bool,
    pub name: String,
    pub target_path: String,
    pub dir: String,
    pub total_length: u64,
    pub progress: f64,
    pub current_download: u64,
    pub status: String,
    pub current_speed: u64,
}

#[derive(Serialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<Vec<DownloadData>>,
}

#[derive(Deserialize, Clone)]
pub struct OpenlistUploadTask {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Clone)]
pub struct OpenlistUploadResponse {
    pub code: i32,
    pub message: String,
    pub data: Vec<OpenlistUploadTask>,
}

#[derive(Serialize, Clone)]
pub struct UploadData {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<Vec<UploadData>>,
}
