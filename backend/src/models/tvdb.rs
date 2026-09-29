use serde::{Deserialize, Deserializer, Serialize};

// 自定义反序列化函数，兼容字符串和整数
fn deserialize_string_or_int<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrInt;

    impl<'de> serde::de::Visitor<'de> for StringOrInt {
        type Value = Option<i32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or integer")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            value
                .parse::<i32>()
                .map(Some)
                .map_err(|e| serde::de::Error::custom(format!("无法解析为整数: {}", e)))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value as i32))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value as i32))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_any(StringOrInt)
}

#[derive(Serialize)]
pub struct TvdbLoginRequest {
    pub apikey: String,
}

#[derive(Deserialize)]
pub struct TvdbLoginResponse {
    pub data: Option<TvdbLoginData>,
}

#[derive(Deserialize)]
pub struct TvdbLoginData {
    pub token: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TvdbSearchResult {
    #[serde(deserialize_with = "deserialize_string_or_int")]
    pub tvdb_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct TvdbSearchResponse {
    pub data: Option<Vec<TvdbSearchResult>>,
}

#[derive(Deserialize)]
pub struct TvdbRemoteIdSearchResponse {
    pub data: Option<Vec<TvdbRemoteIdResult>>,
}

#[derive(Deserialize, Clone)]
pub struct TvdbRemoteIdResult {
    pub series: Option<TvdbSeries>,
}

#[derive(Deserialize)]
pub struct TvdbSeriesEpisodesDefaultResponse {
    pub data: Option<TvdbSeriesEpisodesData>,
    pub links: Option<TvdbLinks>,
}

#[derive(Deserialize)]
pub struct TvdbLinks {
    pub next: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TvdbSeriesEpisodesData {
    pub episodes: Option<Vec<TvdbEpisode>>,
}

#[derive(Deserialize, Clone)]
pub struct TvdbSeries {
    #[serde(deserialize_with = "deserialize_string_or_int")]
    pub id: Option<i32>,
}

#[derive(Deserialize, Clone)]
pub struct TvdbEpisode {
    #[serde(default, deserialize_with = "deserialize_string_or_int")]
    pub id: Option<i32>,
    #[serde(deserialize_with = "deserialize_string_or_int")]
    pub number: Option<i32>,
    #[serde(
        rename = "seasonNumber",
        deserialize_with = "deserialize_string_or_int"
    )]
    pub season_number: Option<i32>,
    pub aired: Option<String>,
    #[serde(
        rename = "absoluteNumber",
        deserialize_with = "deserialize_string_or_int"
    )]
    pub absolute_number: Option<i32>,
}
