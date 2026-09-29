use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct JackettSearchResponse {
    #[serde(rename = "Results")]
    pub results: Option<Vec<JackettResult>>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct JackettResult {
    #[serde(rename = "Title")]
    pub title: Option<String>,
    #[serde(rename = "Size")]
    pub size: Option<u64>,
    #[serde(rename = "CategoryDesc")]
    pub category_desc: Option<String>,
    #[serde(rename = "Guid")]
    pub guid: Option<String>,
    #[serde(rename = "MagnetUri")]
    pub magnet_uri: Option<String>,
    #[serde(rename = "Link")]
    pub link: Option<String>,
    #[serde(rename = "Details")]
    pub details: Option<String>,
    #[serde(rename = "Seeders")]
    pub seeders: Option<i32>,
    #[serde(rename = "Peers")]
    pub peers: Option<i32>,
    #[serde(rename = "PublishDate")]
    pub publish_date: Option<String>,
    #[serde(rename = "IsMultiEpisode", default)]
    pub is_multi_episode: bool,
}

impl std::hash::Hash for JackettResult {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.link.hash(state);
        self.guid.hash(state);
    }
}

#[derive(Clone)]
pub struct SearchEpisodeParams {
    pub name: String,
    pub name_tw: Option<String>,
    pub optional_names: Vec<crate::models::media::FilterTerm>,
    pub filter_names: Vec<crate::models::media::FilterTerm>,
    pub season_number: i32,
    pub season_name: Option<String>,
    pub episode_number: i32,
    pub absolute_number: i32,
    pub tvdb_season_number: Option<i32>,
    pub tvdb_episode_number: Option<i32>,
    pub jackett_address: String,
    pub jackett_port: u16,
    pub jackett_api_key: String,
}

pub struct SearchEpisodeResult {
    pub results: Vec<JackettResult>,
}

#[derive(Deserialize)]
pub struct ResourceSearchRequest {
    pub tmdbid: u32,
    pub season_number: i32,
    pub episode_number: i32,
}

#[derive(Serialize)]
pub struct ResourceSearchResponse {
    pub success: bool,
    pub results: Option<Vec<JackettResult>>,
    pub message: Option<String>,
}
