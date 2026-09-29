use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub tmdbid: u32,
    pub name: String,
    pub original_name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub name_tw: String,
    pub poster_path: Option<String>,
    pub optional_names: Vec<String>,
    #[serde(default)]
    pub filter_names: Vec<String>,
    pub media_path: String,
    #[allow(dead_code)]
    pub is_anime: bool,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct SubscribeResponse {
    pub success: bool,
    pub message: Option<String>,
}
