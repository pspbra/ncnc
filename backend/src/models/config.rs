use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum FilterTerm {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub username: String,
    #[serde(default = "default_password_hash")]
    pub password: String,
    #[serde(default = "generate_random_jwt_secret")]
    pub jwt_secret: String,
    pub media_library_path: String,
    #[serde(default = "default_media_backup_path")]
    pub media_backup_path: String,
    #[serde(default = "default_media_combine_outputs")]
    pub media_combine_outputs: bool,
    #[serde(default = "default_media_auto_compress")]
    pub media_auto_compress: bool,
    pub server_port: u16,
    pub proxy_enabled: bool,
    pub proxy_address: String,
    pub proxy_port: u16,
    pub tmdb_api_key: String,
    pub tvdb_api_key: String,
    pub tvdb_token: Option<String>,
    pub jackett_address: String,
    pub jackett_port: u16,
    pub jackett_api_key: String,
    pub jackett_auto_download: bool,
    pub aria_address: String,
    pub aria_port: u16,
    pub aria_rpc_secret: String,
    pub openlist_address: String,
    pub openlist_port: u16,
    pub openlist_apikey: String,
    pub openlist_auto_upload: bool,
    pub jackett_auto_rss: bool,
    #[serde(default = "default_global_filters")]
    pub global_filter_terms: Vec<FilterTerm>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            username: "admin".to_string(),
            password: hash_password("12345").unwrap_or_else(|_| {
                "$2b$12$Lk1X7yG8z9a0b1c2d3e4f5g6h7i8j9k0l1m2n3o4p5q6r7s8t9u0v1w2x3y4z5".to_string()
            }),
            jwt_secret: generate_random_jwt_secret(),
            media_library_path: "/mnt/8tb/NAS".to_string(),
            media_backup_path: default_media_backup_path(),
            media_combine_outputs: default_media_combine_outputs(),
            media_auto_compress: default_media_auto_compress(),
            server_port: 3000,
            proxy_enabled: false,
            proxy_address: "127.0.0.1".to_string(),
            proxy_port: 7890,
            tmdb_api_key: "".to_string(),
            tvdb_api_key: "".to_string(),
            tvdb_token: None,
            jackett_address: "127.0.0.1".to_string(),
            jackett_port: 9117,
            jackett_api_key: "".to_string(),
            jackett_auto_download: true,
            aria_address: "127.0.0.1".to_string(),
            aria_port: 6800,
            aria_rpc_secret: "".to_string(),
            openlist_address: "127.0.0.1".to_string(),
            openlist_port: 5244,
            openlist_apikey: "".to_string(),
            openlist_auto_upload: true,
            jackett_auto_rss: true,
            global_filter_terms: default_global_filters(),
        }
    }
}

fn default_media_backup_path() -> String {
    "/mnt/8tb/backup".to_string()
}
fn default_media_combine_outputs() -> bool {
    true
}

fn default_media_auto_compress() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_compression_supports_legacy_config_and_partial_updates() {
        let mut json = serde_json::to_value(Config::default()).unwrap();
        assert_eq!(json["media_auto_compress"], true);
        json.as_object_mut().unwrap().remove("media_auto_compress");
        let legacy: Config = serde_json::from_value(json.clone()).unwrap();
        assert!(legacy.media_auto_compress);
        json["media_auto_compress"] = false.into();
        let disabled: Config = serde_json::from_value(json).unwrap();
        assert!(!disabled.media_auto_compress);
        assert_eq!(
            serde_json::to_value(disabled).unwrap()["media_auto_compress"],
            false
        );

        for body in ["{}", r#"{"media_auto_compress":null}"#] {
            let request: SettingsRequest = serde_json::from_str(body).unwrap();
            assert_eq!(request.media_auto_compress, None);
        }
        let request: SettingsRequest =
            serde_json::from_str(r#"{"media_auto_compress":false}"#).unwrap();
        assert_eq!(request.media_auto_compress, Some(false));
    }
}

fn default_global_filters() -> Vec<FilterTerm> {
    vec![
        FilterTerm::Single("生肉".to_string()),
        FilterTerm::Multiple(vec!["黒ネズミたち".to_string(), "CR".to_string()]),
        FilterTerm::Multiple(vec!["黒ネズミたち".to_string(), "ABEMA".to_string()]),
    ]
}

#[derive(Deserialize)]
pub struct SettingsRequest {
    pub media_auto_compress: Option<bool>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub media_library_path: Option<String>,
    pub server_port: Option<u16>,
    pub proxy_enabled: Option<bool>,
    pub proxy_address: Option<String>,
    pub proxy_port: Option<u16>,
    pub tmdb_api_key: Option<String>,
    pub tvdb_api_key: Option<String>,
    pub jackett_address: Option<String>,
    pub jackett_port: Option<u16>,
    pub jackett_api_key: Option<String>,
    pub jackett_auto_download: Option<bool>,
    pub aria_address: Option<String>,
    pub aria_port: Option<u16>,
    pub aria_rpc_secret: Option<String>,
    pub openlist_address: Option<String>,
    pub openlist_port: Option<u16>,
    pub openlist_apikey: Option<String>,
    pub openlist_auto_upload: Option<bool>,
    pub jackett_auto_rss: Option<bool>,
    pub global_filter_terms: Option<Vec<FilterTerm>>,
}

#[derive(Serialize, Clone)]
pub struct SettingsResponseData {
    pub media_auto_compress: bool,
    pub media_library_path: String,
    pub server_port: u16,
    pub proxy_enabled: bool,
    pub proxy_address: String,
    pub proxy_port: u16,
    pub tmdb_api_key: String,
    pub tvdb_api_key: String,
    pub jackett_address: String,
    pub jackett_port: u16,
    pub jackett_api_key: String,
    pub jackett_auto_download: bool,
    pub aria_address: String,
    pub aria_port: u16,
    pub aria_rpc_secret: String,
    pub openlist_address: String,
    pub openlist_port: u16,
    pub openlist_apikey: String,
    pub openlist_auto_upload: bool,
    pub jackett_auto_rss: bool,
    pub global_filter_terms: Vec<FilterTerm>,
}

#[derive(Serialize)]
pub struct SettingsResponse {
    pub success: bool,
    pub message: Option<String>,
}

fn default_password_hash() -> String {
    hash_password("12345").unwrap_or_else(|_| {
        "$2b$12$Lk1X7yG8z9a0b1c2d3e4f5g6h7i8j9k0l1m2n3o4p5q6r7s8t9u0v1w2x3y4z5".to_string()
    })
}

pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    bcrypt::verify(password, hash)
}

pub fn generate_random_jwt_secret() -> String {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    base64::engine::general_purpose::STANDARD.encode(key)
}
