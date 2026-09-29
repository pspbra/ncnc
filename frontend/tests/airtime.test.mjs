import assert from "node:assert/strict";
import { test } from "vitest";
import {
  calendarEpisodes,
  episodeDateKey,
  episodeTimestamp,
  sortMediaByAirTime,
  weekDates,
  localDateKey,
  isLatestSeasonComplete,
} from "../src/utils/airtime.ts";

const episode = (time, extra = {}) => ({
  name: "Episode",
  exists: false,
  season_number: 1,
  episode_number: 1,
  time_status: "source_utc",
  air_date_utc: time,
  ...extra,
});
const media = (id, episodes) => ({
  tmdbid: id,
  name: `Show ${id}`,
  seasons: [{ season_number: 1, episodes }],
});

test("season completion follows the highest regular season, independent of tracking and specials", () => {
  const now = Date.parse("2026-09-18T02:00:00Z");
  const past = episode("2026-09-18T01:00:00Z");
  const future = episode("2026-09-19T01:00:00Z");
  const show = {
    seasons: [
      { season_number: 3, episode_count: 1, episodes: [past], is_tracked: false },
      { season_number: 1, episode_count: 1, episodes: [future], is_tracked: true },
      { season_number: 0, episode_count: 1, episodes: [future] },
    ],
  };
  assert.equal(isLatestSeasonComplete(show, now), true);
  show.seasons[0].episodes.push(future);
  assert.equal(isLatestSeasonComplete(show, now), false);
  assert.equal(isLatestSeasonComplete(show, Date.parse(future.air_date_utc)), true);
});

test("season completion does not infer completion from empty, incomplete or unknown data", () => {
  const now = Date.parse("2026-09-18T02:00:00Z");
  const past = episode("2026-09-18T01:00:00Z");
  for (const seasons of [
    [],
    [{ season_number: 0, episodes: [past] }],
    [{ episodes: [past] }],
    [{ season_number: 1, episodes: [] }],
    [{ season_number: 1, episode_count: 2, episodes: [past] }],
    [{ season_number: 1, episodes: [past, episode(null)] }],
    [{ season_number: 1, episodes: [episode("invalid")] }],
    [
      { season_number: 1, episodes: [past] },
      { season_number: 2, episodes: [] },
    ],
  ]) {
    assert.equal(isLatestSeasonComplete({ seasons }, now), false);
  }
});

test("season completion prioritizes exact airtime and falls back to local calendar dates", () => {
  const now = new Date(2026, 8, 18, 12).getTime();
  const check = (item) => isLatestSeasonComplete(media(1, [item]), now);
  assert.equal(check(episode(new Date(now + 1000).toISOString(), { air_date: "2026-09-17" })), false);
  assert.equal(check(episode(new Date(now).toISOString())), true);
  for (const [date, expected] of [
    ["2026-09-17", true],
    ["2026-09-18", true],
    ["2026-09-19", false],
    ["2026-02-30", false],
    [null, false],
  ]) {
    assert.equal(check(episode(null, { air_date: date, time_status: "date_only" })), expected);
  }
});

test("sorts by the latest already aired episode across all seasons, ignoring future episodes", () => {
  const now = Date.parse("2026-09-18T02:00:00Z");
  const items = [
    media(1, [
      episode("2026-09-17T01:00:00Z"),
      episode("2026-09-19T01:00:00Z"),
    ]),
    media(2, [episode("2026-09-18T01:00:00Z")]),
    media(3, [episode("2026-09-19T01:00:00Z")]),
    media(4, [
      episode(null, { air_date: "2026-09-18", time_status: "date_only" }),
    ]),
    media(5, []),
    {
      ...media(6, []),
      seasons: [
        { episodes: [] },
        { episodes: [episode("2026-09-18T02:00:00Z")] },
      ],
    },
  ];
  assert.deepEqual(
    sortMediaByAirTime(items, now).map((item) => item.tmdbid),
    [6, 2, 1, 3, 4, 5],
  );
  assert.deepEqual(
    items.map((item) => item.tmdbid),
    [1, 2, 3, 4, 5, 6],
  );
  assert.equal(
    sortMediaByAirTime(items, Date.parse("2026-09-19T01:00:00Z"))[0].tmdbid,
    1,
  );
});

test("rejects unknown, missing and invalid times without fabricating midnight", () => {
  for (const status of ["date_only", "invalid", "future_status", undefined]) {
    assert.equal(
      episodeTimestamp(
        episode("2026-09-18T01:00:00Z", { time_status: status }),
      ),
      null,
    );
  }
  for (const value of [null, "", "bad", "2026-09-18", "2026-09-18T01:00:00"]) {
    assert.equal(episodeTimestamp(episode(value)), null);
  }
  assert.equal(
    episodeTimestamp(episode("2026-09-18T00:00:00Z")),
    Date.parse("2026-09-18T00:00:00Z"),
  );
  assert.equal(
    episodeTimestamp(
      episode("2026-09-18T09:00:00+08:00", { time_status: "calculated" }),
    ),
    Date.parse("2026-09-18T01:00:00Z"),
  );
});

test("converts UTC to local dates once and preserves date-only values across timezones", () => {
  const previous = process.env.TZ;
  try {
    process.env.TZ = "Asia/Shanghai";
    assert.equal(episodeDateKey(episode("2026-09-17T23:00:00Z")), "2026-09-18");
    process.env.TZ = "America/Los_Angeles";
    assert.equal(episodeDateKey(episode("2026-09-18T01:00:00Z")), "2026-09-17");
    assert.equal(
      episodeDateKey(episode(null, { air_date: "2026-09-18" })),
      "2026-09-18",
    );
    assert.equal(
      episodeDateKey(episode(null, { air_date: "2026-02-30" })),
      null,
    );
  } finally {
    if (previous === undefined) delete process.env.TZ;
    else process.env.TZ = previous;
  }
});

test("weeks start on Monday, including Sundays and year boundaries", () => {
  assert.deepEqual(weekDates(new Date(2027, 0, 3)).map(localDateKey), [
    "2026-12-28",
    "2026-12-29",
    "2026-12-30",
    "2026-12-31",
    "2027-01-01",
    "2027-01-02",
    "2027-01-03",
  ]);
});

test("calendar keeps each episode and uses local season/episode numbers including specials", () => {
  const entries = calendarEpisodes([
    media(1, [
      episode("2026-09-18T01:00:00Z", { episode_number: 2 }),
      episode("2026-09-18T01:00:00Z", { season_number: 0, episode_number: 3 }),
      episode(null),
    ]),
    media(2, []),
  ]);
  assert.equal(entries.length, 3);
  assert.equal(new Set(entries.map((entry) => entry.key)).size, 3);
  assert.equal(entries[0].title, "Show 1 第1季2集");
  assert.equal(entries[1].title, "Show 1 第0季3集");
  assert.equal(entries[2].dateKey, null);
});
