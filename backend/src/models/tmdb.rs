use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SearchResultItem {
    pub adult: bool,
    pub backdrop_path: Option<String>,
    pub genre_ids: Vec<u32>,
    pub id: u32,
    pub origin_country: Vec<String>,
    pub original_language: String,
    pub original_name: String,
    pub overview: String,
    pub popularity: f64,
    pub poster_path: Option<String>,
    pub first_air_date: String,
    pub name: String,
    pub vote_average: f64,
    pub vote_count: u32,
    #[serde(default)]
    pub is_in_library: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MovieSearchResultItem {
    pub adult: bool,
    pub backdrop_path: Option<String>,
    pub genre_ids: Vec<u32>,
    pub id: u32,
    pub original_language: String,
    pub original_title: String,
    pub overview: String,
    pub popularity: f64,
    pub poster_path: Option<String>,
    pub release_date: String,
    pub title: String,
    pub video: bool,
    pub vote_average: f64,
    pub vote_count: u32,
    #[serde(default)]
    pub is_in_library: bool,
}

#[derive(Serialize, Deserialize)]
pub struct MovieSearchResponse {
    pub page: u32,
    pub results: Vec<MovieSearchResultItem>,
    pub total_pages: u32,
    pub total_results: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub page: u32,
    pub results: Vec<SearchResultItem>,
    pub total_pages: u32,
    pub total_results: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TmdbTvDetail {
    pub adult: bool,
    pub backdrop_path: Option<String>,
    pub created_by: Option<Vec<serde_json::Value>>,
    pub episode_run_time: Option<Vec<i32>>,
    pub first_air_date: String,
    pub genres: Vec<Genre>,
    pub homepage: Option<String>,
    pub id: u32,
    pub in_production: Option<bool>,
    pub languages: Option<Vec<String>>,
    pub last_air_date: Option<String>,
    pub last_episode_to_air: Option<serde_json::Value>,
    pub name: String,
    pub next_episode_to_air: Option<serde_json::Value>,
    pub networks: Option<Vec<serde_json::Value>>,
    pub number_of_episodes: Option<i32>,
    pub number_of_seasons: i32,
    pub origin_country: Vec<String>,
    pub original_language: String,
    pub original_name: String,
    pub overview: String,
    pub popularity: f64,
    pub poster_path: Option<String>,
    pub production_companies: Option<Vec<serde_json::Value>>,
    pub production_countries: Option<Vec<serde_json::Value>>,
    pub seasons: Vec<Season>,
    pub spoken_languages: Option<Vec<serde_json::Value>>,
    pub status: Option<String>,
    pub tagline: Option<String>,
    #[serde(rename = "type")]
    pub show_type: Option<String>,
    pub vote_average: f64,
    pub vote_count: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Genre {
    pub id: u32,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Season {
    pub air_date: Option<String>,
    pub episode_count: i32,
    pub id: u32,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub season_number: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TmdbSeasonDetail {
    pub _id: Option<String>,
    pub air_date: Option<String>,
    pub episodes: Vec<Episode>,
    pub name: String,
    pub overview: Option<String>,
    pub id: u32,
    pub poster_path: Option<String>,
    pub season_number: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Episode {
    pub air_date: Option<String>,
    pub episode_number: i32,
    pub id: u32,
    pub name: String,
    pub overview: Option<String>,
    pub production_code: Option<String>,
    pub season_number: i32,
    pub still_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<i32>,
}
