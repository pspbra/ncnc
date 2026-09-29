export interface EpisodeInfo {
  air_date?: string | null;
  air_date_utc?: string | null;
  time_status?: string;
  time_zone_used?: string | null;
  tvdb_episode_id?: number | null;
  name: string;
  exists: boolean;
  episode_number?: number;
  season_number?: number;
}

export interface SeasonInfo {
  name: string;
  episode_count: number;
  episodes: EpisodeInfo[];
  is_tracked: boolean;
  season_number?: number;
}

export type FilterTerm = string | string[];

export interface MediaInfo {
  tmdbid: number;
  name: string;
  original_name: string;
  name_tw: string;
  poster_path?: string | null;
  optional_names: FilterTerm[];
  filter_names: FilterTerm[];
  media_path: string;
  number_of_seasons: number;
  seasons: SeasonInfo[];
  display_name: string;
}

export function episodeTimestamp(episode: EpisodeInfo): number | null {
  if (
    !["source_utc", "calculated"].includes(episode.time_status ?? "") ||
    !episode.air_date_utc ||
    !/(Z|[+-]\d{2}:\d{2})$/i.test(episode.air_date_utc)
  )
    return null;
  const timestamp = Date.parse(episode.air_date_utc);
  return Number.isFinite(timestamp) ? timestamp : null;
}

export function localDateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

export function episodeDateKey(episode: EpisodeInfo): string | null {
  const timestamp = episodeTimestamp(episode);
  if (timestamp !== null) return localDateKey(new Date(timestamp));
  const raw = episode.air_date;
  if (!raw || !/^\d{4}-\d{2}-\d{2}$/.test(raw)) return null;
  const date = new Date(`${raw}T12:00:00`);
  return Number.isFinite(date.getTime()) && localDateKey(date) === raw
    ? raw
    : null;
}

export function latestAiredTimestamp(
  media: MediaInfo,
  now: number,
): number | null {
  let latest: number | null = null;
  for (const season of media.seasons ?? []) {
    for (const episode of season.episodes ?? []) {
      const time = episodeTimestamp(episode);
      if (time !== null && time <= now && (latest === null || time > latest))
        latest = time;
    }
  }
  return latest;
}

export function isLatestSeasonComplete(media: MediaInfo, now: number): boolean {
  let latest: SeasonInfo | undefined;
  for (const season of media.seasons ?? []) {
    const number = season.season_number;
    if (number === undefined || number <= 0) continue;
    if (!latest || number > latest.season_number!) latest = season;
  }
  if (
    !latest ||
    !latest.episodes?.length ||
    latest.episodes.length < latest.episode_count
  )
    return false;

  const today = localDateKey(new Date(now));
  return latest.episodes.every((episode) => {
    const timestamp = episodeTimestamp(episode);
    if (timestamp !== null) return timestamp <= now;
    const date = episodeDateKey(episode);
    return date !== null && date <= today;
  });
}

export function sortMediaByAirTime(
  media: MediaInfo[],
  now: number,
): MediaInfo[] {
  return media
    .map((item) => ({ item, time: latestAiredTimestamp(item, now) }))
    .sort((a, b) => {
      if (a.time !== b.time) {
        if (a.time === null) return 1;
        if (b.time === null) return -1;
        return b.time - a.time;
      }
      return (
        a.item.name.localeCompare(b.item.name, "zh-CN") ||
        a.item.tmdbid - b.item.tmdbid
      );
    })
    .map(({ item }) => item);
}

export function weekDates(anchor: Date): Date[] {
  const monday = new Date(anchor);
  monday.setHours(0, 0, 0, 0);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  return Array.from({ length: 7 }, (_, index) => {
    const day = new Date(monday);
    day.setDate(day.getDate() + index);
    return day;
  });
}

export interface CalendarEpisode {
  key: string;
  mediaId: number;
  exists: boolean;
  title: string;
  timestamp: number | null;
  dateKey: string | null;
}

export function calendarEpisodes(media: MediaInfo[]): CalendarEpisode[] {
  return media.flatMap((item) =>
    (item.seasons ?? []).flatMap((season, seasonIndex) =>
      (season.episodes ?? []).map((episode, episodeIndex) => ({
        key: `${item.tmdbid}-${seasonIndex}-${episodeIndex}`,
        mediaId: item.tmdbid,
        exists: episode.exists,
        title: `${item.display_name || item.name} 第${episode.season_number ?? season.season_number ?? "?"}季${episode.episode_number ?? "?"}集`,
        timestamp: episodeTimestamp(episode),
        dateKey: episodeDateKey(episode),
      })),
    ),
  );
}
