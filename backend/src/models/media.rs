pub use crate::models::config::FilterTerm;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SeasonInfo {
    pub name: String,
    pub episode_count: i32,
    pub episodes: Vec<EpisodeInfo>,
    pub is_tracked: bool,
    pub season_number: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EpisodeInfo {
    pub air_date: Option<String>,
    #[serde(default, flatten)]
    pub airtime: EpisodeAirTime,
    pub tvdb_episode_id: Option<i32>,
    pub name: String,
    pub exists: bool,
    pub episode_number: i32,
    pub absolute_number: i32,
    pub season_number: i32,
    pub tvdb_season_number: Option<i32>,
    pub tvdb_episode_number: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimeStatus {
    SourceUtc,
    Calculated,
    #[default]
    DateOnly,
    Invalid,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct EpisodeAirTime {
    pub air_date_utc: Option<String>,
    pub time_status: TimeStatus,
    pub time_zone_used: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MediaInfo {
    pub tmdbid: u32,
    pub tvdb_series_id: Option<u32>,
    pub name: String,
    pub original_name: String,
    pub name_tw: String,
    pub poster_path: Option<String>,
    pub optional_names: Vec<FilterTerm>,
    #[serde(default)]
    pub filter_names: Vec<FilterTerm>,
    pub media_path: String,
    pub number_of_seasons: i32,
    pub seasons: Vec<SeasonInfo>,
    pub display_name: String,
}

#[derive(Deserialize)]
pub struct DeleteMediaRequest {
    pub tmdbid: u32,
}

#[derive(Serialize)]
pub struct DeleteMediaResponse {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct MediaSettingsRequest {
    pub tmdbid: u32,
    pub optional_names: Option<Vec<FilterTerm>>,
    pub filter_names: Option<Vec<FilterTerm>>,
    pub seasons_tracked: Option<Vec<bool>>,
}

#[derive(Serialize)]
pub struct MediaSettingsResponse {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct DataJson {
    pub media: HashMap<u32, Arc<MediaInfo>>,
}
