//! Optional airtime enrichment; failures never prevent TMDB/TVDB updates.
use crate::models::media::{EpisodeAirTime, EpisodeInfo, TimeStatus};
use chrono::{
    DateTime, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeZone, Utc,
};
use chrono_tz::Tz;
use log::warn;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const TTL: i64 = 6 * 3600;
const STALE_TTL: i64 = 7 * 86400;
const FAILURE_TTL: u64 = 300;

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
fn number(value: &Value, key: &str) -> Option<i32> {
    let v = value.get(key)?;
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .or_else(|| v.as_str()?.parse().ok())
}
fn zone(value: &Value) -> Option<&str> {
    text(value, "timeZone").or_else(|| text(value, "timezone"))
}
fn utc_string(date: DateTime<Utc>) -> String {
    date.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Never treat a date or an offset-free datetime as a UTC instant.
fn source_utc(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| utc_string(d.with_timezone(&Utc)))
}

pub(crate) fn parse_airtime(series: &Value, episode: &Value) -> EpisodeAirTime {
    let mut invalid = false;
    if let Some(value) = episode.get("airDateUtc").filter(|v| !v.is_null()) {
        if let Some(utc) = value.as_str().and_then(source_utc) {
            return EpisodeAirTime {
                air_date_utc: Some(utc),
                time_status: TimeStatus::SourceUtc,
                time_zone_used: None,
            };
        }
        invalid = true;
    }

    let air_date = text(episode, "airDate");
    if let Some(utc) = air_date.and_then(source_utc) {
        return EpisodeAirTime {
            air_date_utc: Some(utc),
            time_status: TimeStatus::SourceUtc,
            time_zone_used: None,
        };
    }
    let date = air_date.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let local_datetime = air_date.and_then(|d| {
        NaiveDateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .or_else(|| NaiveDateTime::parse_from_str(d, "%Y-%m-%dT%H:%M").ok())
    });
    let air_time = text(episode, "airTime").or_else(|| text(series, "airTime"));
    let time = air_time.and_then(|t| {
        ["%H:%M", "%H:%M:%S", "%I:%M %p"]
            .iter()
            .find_map(|fmt| NaiveTime::parse_from_str(t, fmt).ok())
    });
    let zone_name = zone(episode).or_else(|| zone(series));
    let timezone = zone_name.and_then(|z| z.parse::<Tz>().ok());
    invalid |= air_date.is_some() && date.is_none() && local_datetime.is_none();
    invalid |= air_time.is_some() && time.is_none();
    invalid |= zone_name.is_some() && timezone.is_none();
    let local = local_datetime.or_else(|| Some(date?.and_time(time?)));
    if let (Some(local), Some(timezone)) = (local, timezone) {
        match timezone.from_local_datetime(&local) {
            LocalResult::Single(datetime) => {
                return EpisodeAirTime {
                    air_date_utc: Some(utc_string(datetime.with_timezone(&Utc))),
                    time_status: TimeStatus::Calculated,
                    time_zone_used: Some(timezone.name().to_owned()),
                }
            }
            LocalResult::Ambiguous(_, _) | LocalResult::None => {
                // Do not guess which side of a DST transition the broadcaster intended.
                warn!(
                    "[SkyHook] 本地播出时间有歧义或不存在: {} {}",
                    local, timezone
                );
                invalid = true;
            }
        }
    }
    EpisodeAirTime {
        time_status: if invalid {
            TimeStatus::Invalid
        } else {
            TimeStatus::DateOnly
        },
        ..Default::default()
    }
}

/// An internal model isolates the rest of the application from SkyHook's JSON schema.
pub(crate) struct SeriesAirTimes {
    stale: bool,
    by_id: HashMap<i32, Option<EpisodeAirTime>>,
    by_number: HashMap<(i32, i32), Option<EpisodeAirTime>>,
}

fn insert_unique<K: std::hash::Hash + Eq>(
    map: &mut HashMap<K, Option<EpisodeAirTime>>,
    key: K,
    time: EpisodeAirTime,
) {
    map.entry(key)
        .and_modify(|v| *v = None)
        .or_insert(Some(time));
}

impl SeriesAirTimes {
    fn parse(raw: &Value, series_id: u32) -> Option<Self> {
        if number(raw, "tvdbId").and_then(|n| u32::try_from(n).ok()) != Some(series_id) {
            return None;
        }
        let mut result = Self {
            stale: false,
            by_id: HashMap::new(),
            by_number: HashMap::new(),
        };
        for episode in raw.get("episodes")?.as_array()? {
            if !episode.is_object() {
                continue;
            }
            if episode.get("tvdbShowId").is_some_and(|v| !v.is_null())
                && number(episode, "tvdbShowId").and_then(|n| u32::try_from(n).ok())
                    != Some(series_id)
            {
                continue;
            }
            let time = parse_airtime(raw, episode);
            if let Some(id) = number(episode, "tvdbId").filter(|n| *n > 0) {
                insert_unique(&mut result.by_id, id, time.clone());
            }
            if let (Some(season), Some(number)) = (
                number(episode, "seasonNumber"),
                number(episode, "episodeNumber"),
            ) {
                if season >= 0 && number > 0 {
                    insert_unique(&mut result.by_number, (season, number), time);
                }
            }
        }
        Some(result)
    }

    fn apply(&self, episode: &mut EpisodeInfo) {
        if self.stale && episode.airtime.air_date_utc.is_some() {
            return;
        }
        let time = if let Some(id) = episode.tvdb_episode_id {
            // A known episode ID must not silently match a different episode by number.
            self.by_id.get(&id)
        } else {
            episode
                .tvdb_season_number
                .zip(episode.tvdb_episode_number)
                .and_then(|key| self.by_number.get(&key))
        };
        // A successful fresh response can explicitly remove a previously published time.
        let time = time.and_then(Option::as_ref);
        if !self.stale || time.is_some_and(|t| t.air_date_utc.is_some()) {
            episode.airtime = time.cloned().unwrap_or_default();
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedSeries {
    fetched_at_utc: i64,
    raw_metadata: Value,
}
impl CachedSeries {
    fn parse(&self, series_id: u32) -> Option<SeriesAirTimes> {
        if !self.within(STALE_TTL) {
            return None;
        }
        let mut times = SeriesAirTimes::parse(&self.raw_metadata, series_id)?;
        times.stale = !self.within(TTL);
        Some(times)
    }
    fn within(&self, seconds: i64) -> bool {
        let age = Utc::now().timestamp() - self.fetched_at_utc;
        age >= 0 && age < seconds
    }
}
struct CacheEntry {
    data: Option<CachedSeries>,
    retry_at: Instant,
}
#[derive(Default)]
struct ProviderState {
    entries: HashMap<u32, CacheEntry>,
    next_request: Option<Instant>,
}
// Serializes requests and coalesces simultaneous refreshes for the same show.
static STATE: Lazy<Mutex<ProviderState>> = Lazy::new(|| Mutex::new(ProviderState::default()));

fn retry_after(value: Option<&str>) -> Option<Duration> {
    let value = value?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    Some(Duration::from_secs(
        (date - Utc::now()).num_seconds().max(0) as u64,
    ))
}

struct FetchFailure {
    cooldown: Duration,
    global_pause: Option<Duration>,
}

async fn fetch_raw(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    series_id: u32,
) -> Result<Value, FetchFailure> {
    fetch_raw_with_timeout(client, url, headers, series_id, Duration::from_secs(15)).await
}

async fn fetch_raw_with_timeout(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    series_id: u32,
    timeout: Duration,
) -> Result<Value, FetchFailure> {
    let failure = || FetchFailure {
        cooldown: Duration::from_secs(FAILURE_TTL),
        global_pause: None,
    };
    for attempt in 0..3 {
        let response = client
            .get(url)
            .headers(headers.clone())
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                concat!("ncnc-backend/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(timeout)
            .send()
            .await;
        let delay = match response {
            Ok(response) if response.status().is_success() => {
                match response.json::<Value>().await {
                    Ok(raw) => {
                        return SeriesAirTimes::parse(&raw, series_id)
                            .map(|_| raw)
                            .ok_or_else(|| {
                                warn!("[SkyHook] TVDB {} 的剧集标识或分集数据无效", series_id);
                                failure()
                            })
                    }
                    Err(error) if error.is_body() || error.is_timeout() => {
                        warn!("[SkyHook] TVDB {} 的响应正文读取中断", series_id);
                        Duration::from_secs(1 << attempt)
                    }
                    Err(_) => {
                        warn!("[SkyHook] TVDB {} 返回的 JSON 无效", series_id);
                        return Err(failure());
                    }
                }
            }
            Ok(response) => {
                let status = response.status();
                warn!("[SkyHook] TVDB {} HTTP {}", series_id, status.as_u16());
                if status.as_u16() != 429 && !status.is_server_error() {
                    return Err(failure());
                }
                let requested = retry_after(
                    response
                        .headers()
                        .get("Retry-After")
                        .and_then(|h| h.to_str().ok()),
                );
                let delay = requested.unwrap_or(Duration::from_secs(1 << attempt));
                // Honor long Retry-After without keeping an update task asleep indefinitely.
                if delay > Duration::from_secs(5) || attempt == 2 {
                    return Err(FetchFailure {
                        cooldown: delay.max(Duration::from_secs(FAILURE_TTL)),
                        global_pause: requested.or_else(|| {
                            (status.as_u16() == 429).then_some(Duration::from_secs(FAILURE_TTL))
                        }),
                    });
                }
                delay
            }
            Err(_) => {
                // reqwest errors may include URLs containing future credentials; do not log them.
                warn!(
                    "[SkyHook] TVDB {} 网络请求失败（第 {} 次尝试）",
                    series_id,
                    attempt + 1
                );
                Duration::from_secs(1 << attempt)
            }
        };
        if attempt < 2 {
            tokio::time::sleep(delay).await;
        }
    }
    Err(failure())
}

async fn get_series(series_id: u32, client: &reqwest::Client) -> Option<SeriesAirTimes> {
    if series_id == 0 {
        return None;
    }
    let base =
        std::env::var("SKYHOOK_BASE_URL").unwrap_or_else(|_| "https://skyhook.sonarr.tv".into());
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(json) = std::env::var("SKYHOOK_HEADERS_JSON") {
        let entries: HashMap<String, String> = serde_json::from_str(&json).ok()?;
        for (key, value) in entries {
            headers.insert(
                key.parse::<reqwest::header::HeaderName>().ok()?,
                value.parse().ok()?,
            );
        }
    }
    get_series_cached(
        &STATE,
        std::path::Path::new("skyhook_cache"),
        &base,
        &headers,
        series_id,
        client,
    )
    .await
}

async fn get_series_cached(
    state: &Mutex<ProviderState>,
    cache_dir: &std::path::Path,
    base: &str,
    headers: &reqwest::header::HeaderMap,
    series_id: u32,
    client: &reqwest::Client,
) -> Option<SeriesAirTimes> {
    let mut state = state.lock().await;
    let path = cache_dir.join(format!("{series_id}.json"));
    if !state.entries.contains_key(&series_id) {
        // Bound resident raw responses. Eviction does not remove the persistent cache.
        if state.entries.len() >= 128 {
            if let Some(id) = state
                .entries
                .iter()
                .min_by_key(|(_, e)| e.retry_at)
                .map(|(id, _)| *id)
            {
                state.entries.remove(&id);
            }
        }
        let data = tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CachedSeries>(&bytes).ok())
            .filter(|cache| {
                cache.within(STALE_TTL)
                    && SeriesAirTimes::parse(&cache.raw_metadata, series_id).is_some()
            });
        state.entries.insert(
            series_id,
            CacheEntry {
                data,
                retry_at: Instant::now(),
            },
        );
    }
    let entry = &state.entries[&series_id];
    if entry.data.as_ref().is_some_and(|c| c.within(TTL)) || Instant::now() < entry.retry_at {
        return entry.data.as_ref().and_then(|c| c.parse(series_id));
    }
    if let Some(next) = state.next_request {
        if next.saturating_duration_since(Instant::now()) > Duration::from_secs(5) {
            return state.entries[&series_id]
                .data
                .as_ref()
                .and_then(|c| c.parse(series_id));
        }
        tokio::time::sleep(next.saturating_duration_since(Instant::now())).await;
    }
    let url = format!(
        "{}/v1/tvdb/shows/en/{}",
        base.trim_end_matches('/'),
        series_id
    );
    let result = fetch_raw(client, &url, headers, series_id).await;
    let global_delay = result
        .as_ref()
        .err()
        .and_then(|e| e.global_pause)
        .unwrap_or(Duration::from_secs(1));
    state.next_request = Instant::now().checked_add(global_delay);
    let entry = state.entries.get_mut(&series_id)?;
    match result {
        Ok(raw_metadata) => {
            let cache = CachedSeries {
                fetched_at_utc: Utc::now().timestamp(),
                raw_metadata,
            };
            let saved = cache.clone();
            let save_result = crate::io_util::blocking(move || {
                std::fs::create_dir_all(path.parent().unwrap())?;
                crate::io_util::atomic_json(&path, &saved)
            })
            .await;
            if !matches!(save_result, Ok(Ok(()))) {
                warn!("[SkyHook] 无法持久化 TVDB {} 的缓存", series_id);
            }
            entry.data = Some(cache);
            entry.retry_at = Instant::now();
        }
        Err(failure) => {
            entry.retry_at = Instant::now()
                .checked_add(failure.cooldown)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(STALE_TTL as u64));
            warn!("[SkyHook] TVDB {} 暂不可用，保留现有播出时间", series_id);
        }
    }
    entry.data.as_ref().and_then(|c| c.parse(series_id))
}

pub async fn refresh_media(tmdbid: u32, client: &Arc<reqwest::Client>) {
    let series_id = crate::DATA_JSON
        .read()
        .await
        .media
        .get(&tmdbid)
        .and_then(|m| m.tvdb_series_id);
    let Some(series_id) = series_id else {
        return;
    };
    let Some(times) = get_series(series_id, client).await else {
        return;
    };
    let mut data = crate::DATA_JSON.write().await;
    let Some(media) = data.media.get_mut(&tmdbid) else {
        return;
    };
    if media.tvdb_series_id != Some(series_id) {
        return;
    }
    for season in &mut Arc::make_mut(media).seasons {
        for episode in &mut season.episodes {
            times.apply(episode);
        }
    }
}

pub fn is_episode_aired(episode: &EpisodeInfo, now: DateTime<Utc>, today: NaiveDate) -> bool {
    if matches!(
        episode.airtime.time_status,
        TimeStatus::SourceUtc | TimeStatus::Calculated
    ) {
        if let Some(utc) = episode
            .airtime
            .air_date_utc
            .as_deref()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        {
            return now >= utc;
        }
    }
    // Preserve the application's previous date-only fallback, including unknown dates.
    episode
        .air_date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .is_none_or(|date| today >= date)
}
