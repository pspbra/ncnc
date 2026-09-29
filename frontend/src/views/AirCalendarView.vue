<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { RouterLink } from "vue-router";
import axios from "axios";
import { NAlert, NButton, NEmpty, NInput, NSpin, useThemeVars } from "naive-ui";
import {
  calendarEpisodes,
  localDateKey,
  weekDates,
  type MediaInfo,
} from "@/utils/airtime";

const theme = useThemeVars();
const media = ref<MediaInfo[]>([]);
const loading = ref(false);
const error = ref("");
const query = ref("");
const now = ref(Date.now());
const weekOffset = ref(0);
const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const labels = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
const timeFormat = new Intl.DateTimeFormat("zh-CN", {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});
const dateFormat = new Intl.DateTimeFormat("zh-CN", {
  month: "long",
  day: "numeric",
});
const entries = computed(() =>
  calendarEpisodes(media.value).filter((entry) =>
    entry.title
      .toLocaleLowerCase()
      .includes(query.value.trim().toLocaleLowerCase()),
  ),
);
const days = computed(() => {
  const anchor = new Date(now.value);
  anchor.setDate(anchor.getDate() + weekOffset.value * 7);
  return weekDates(anchor).map((date, index) => {
    const key = localDateKey(date);
    return {
      key,
      date,
      label: labels[index],
      today: key === localDateKey(new Date(now.value)),
      entries: entries.value
        .filter((entry) => entry.dateKey === key)
        .sort(
          (a, b) =>
            (a.timestamp ?? Infinity) - (b.timestamp ?? Infinity) ||
            a.title.localeCompare(b.title, "zh-CN"),
        ),
    };
  });
});
const weekCount = computed(() =>
  days.value.reduce((sum, day) => sum + day.entries.length, 0),
);
const undated = computed(() =>
  entries.value.filter((entry) => entry.dateKey === null),
);
const weekTitle = computed(
  () =>
    `${days.value[0].date.getFullYear()}年 ${dateFormat.format(days.value[0].date)} — ${dateFormat.format(days.value[6].date)}`,
);
let timer: number | undefined;
const controller = new AbortController();

async function fetchMedia() {
  loading.value = true;
  error.value = "";
  try {
    const response = await axios.get<MediaInfo[]>("/v1/media/all", {
      signal: controller.signal,
    });
    media.value = response.data || [];
  } catch (cause) {
    if (!axios.isCancel(cause)) error.value = "播出日历加载失败，请重试。";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  fetchMedia();
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
});
onBeforeUnmount(() => {
  window.clearInterval(timer);
  controller.abort();
});
</script>

<template>
  <section
    class="calendar-view"
    :style="{
      '--accent': theme.primaryColor,
      '--surface': theme.cardColor,
      '--border': theme.borderColor,
      '--muted': theme.textColor3,
      '--text': theme.textColor1,
      '--hover': theme.hoverColor,
      '--available': theme.successColor,
      '--missing': theme.warningColor,
    }"
  >
    <header class="calendar-heading">
      <div>
        <div class="eyebrow">每周追剧计划</div>
        <h1>播出日历</h1>
        <p>按本地时间安排你的观影时光 · {{ timezone }}</p>
      </div>
      <n-button :loading="loading" @click="fetchMedia">重新加载</n-button>
    </header>

    <div class="calendar-toolbar">
      <div class="week-navigation">
        <n-button aria-label="上一周" @click="weekOffset--">‹</n-button>
        <n-button @click="weekOffset = 0">本周</n-button>
        <n-button aria-label="下一周" @click="weekOffset++">›</n-button>
      </div>
      <div class="week-title">
        {{ weekTitle }} <span>{{ weekCount }} 集</span>
      </div>
      <n-input
        v-model:value="query"
        clearable
        placeholder="搜索媒体或季集"
        aria-label="搜索日历"
        class="calendar-search"
      />
    </div>

    <ul class="calendar-legend" aria-label="剧集状态图例">
      <li class="legend-available">已播出 · 已入库</li>
      <li class="legend-missing">已播出 · 未入库</li>
      <li class="legend-upcoming">待播出</li>
      <li class="legend-uncertain">时间未知</li>
    </ul>

    <n-alert v-if="error" type="error" class="load-error">{{ error }}</n-alert>
    <n-spin :show="loading">
      <div
        class="calendar-scroll"
        role="region"
        aria-label="每周播出日程"
        tabindex="0"
      >
        <div class="week-grid">
          <section
            v-for="day in days"
            :key="day.key"
            class="day-column"
            :class="{ 'is-today': day.today }"
          >
            <header class="day-heading">
              <div>
                {{ day.label }}
                <span v-if="day.today" class="today-label">今天</span>
              </div>
              <div class="day-number">
                {{ day.date.getDate() }}
                <small>{{ day.date.getMonth() + 1 }}月</small>
              </div>
              <span class="day-total">{{ day.entries.length }} 集</span>
            </header>
            <div class="day-events">
              <RouterLink
                v-for="entry in day.entries"
                :key="entry.key"
                :to="`/library/media/${entry.mediaId}`"
                class="episode-event"
                :class="{
                  aired: entry.timestamp !== null && entry.timestamp <= now,
                  'in-library':
                    entry.timestamp !== null &&
                    entry.timestamp <= now &&
                    entry.exists,
                  'not-in-library':
                    entry.timestamp !== null &&
                    entry.timestamp <= now &&
                    !entry.exists,
                  uncertain: entry.timestamp === null,
                }"
              >
                <div class="event-time">
                  <span>{{
                    entry.timestamp === null
                      ? "时间未知"
                      : timeFormat.format(entry.timestamp)
                  }}</span>
                  <span v-if="entry.timestamp !== null" class="event-status">{{
                    entry.timestamp <= now ? "已播出" : "待播出"
                  }}</span>
                </div>
                <div class="event-title">{{ entry.title }}</div>
                <div
                  v-if="entry.timestamp !== null && entry.timestamp <= now"
                  class="library-status"
                >
                  <span aria-hidden="true">{{ entry.exists ? "✓" : "○" }}</span>
                  {{ entry.exists ? "已入库" : "未入库" }}
                </div>
              </RouterLink>
              <div v-if="!day.entries.length" class="empty-day">
                <span>—</span>暂无播出安排
              </div>
            </div>
          </section>
        </div>
      </div>
      <n-empty
        v-if="!loading && !error && !weekCount"
        description="本周暂无播出安排，可切换其他周查看"
        class="empty-week"
      />
      <details v-if="undated.length" class="undated">
        <summary>
          播出日期未知 <span>{{ undated.length }} 集</span>
        </summary>
        <div class="undated-list">
          <RouterLink
            v-for="entry in undated"
            :key="entry.key"
            :to="`/library/media/${entry.mediaId}`"
            >{{ entry.title }}</RouterLink
          >
        </div>
      </details>
    </n-spin>
    <p class="calendar-note">
      具体时间未知的分集按原始播出日期列出，播出安排以实际更新为准。
    </p>
  </section>
</template>

<style scoped>
.calendar-view {
  padding: 28px 24px;
  color: var(--text);
}
.calendar-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 28px;
}
.eyebrow {
  color: var(--accent);
  font-size: 12px;
  letter-spacing: 2px;
  font-weight: 600;
}
h1 {
  font-size: 28px;
  margin: 4px 0 8px;
  font-weight: 700;
}
.calendar-heading p,
.calendar-note {
  color: var(--muted);
  font-size: 12px;
  margin: 0;
}
.calendar-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 16px;
  margin-bottom: 20px;
}
.week-navigation {
  display: flex;
  gap: 6px;
}
.week-title {
  font-size: 15px;
  font-weight: 600;
}
.week-title span,
.undated summary span {
  color: var(--muted);
  font-size: 12px;
  font-weight: 400;
  margin-left: 10px;
}
.calendar-search {
  width: 220px;
  margin-left: auto;
}
.calendar-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 20px;
  margin: 0 0 16px;
  padding: 0;
  list-style: none;
  color: var(--muted);
  font-size: 12px;
}
.calendar-legend li {
  display: flex;
  align-items: center;
  gap: 7px;
}
.calendar-legend li::before {
  content: "";
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--status-color, var(--accent));
}
.legend-available,
.episode-event.in-library {
  --status-color: var(--available);
}
.legend-missing,
.episode-event.not-in-library {
  --status-color: var(--missing);
}
.calendar-legend .legend-uncertain::before {
  background: transparent;
  border: 1px dashed var(--muted);
  box-sizing: border-box;
}
.calendar-scroll {
  overflow-x: auto;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--surface);
}
.week-grid {
  display: grid;
  grid-template-columns: repeat(7, minmax(150px, 1fr));
  min-height: 420px;
}
.day-column {
  min-width: 0;
  border-right: 1px solid var(--border);
}
.day-column:last-child {
  border-right: 0;
}
.day-heading {
  padding: 16px 14px;
  border-bottom: 1px solid var(--border);
  position: relative;
}
.day-heading > div:first-child {
  color: var(--muted);
  font-size: 12px;
}
.day-number {
  font-size: 26px;
  font-weight: 600;
  margin-top: 6px;
}
.day-number small {
  font-size: 12px;
  font-weight: 400;
  color: var(--muted);
}
.day-total {
  position: absolute;
  right: 14px;
  bottom: 22px;
  color: var(--muted);
  font-size: 11px;
}
.is-today .day-heading {
  box-shadow: inset 0 3px var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}
.is-today .day-number {
  color: var(--accent);
}
.today-label {
  float: right;
  color: var(--accent);
  font-weight: 600;
}
.day-events {
  padding: 12px 8px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.episode-event {
  display: block;
  padding: 12px 10px;
  border: 1px solid var(--border);
  border-left: 3px solid var(--accent);
  border-radius: 8px;
  text-decoration: none;
  color: var(--text);
  background: color-mix(in srgb, var(--accent) 6%, var(--surface));
  transition:
    background 0.15s,
    transform 0.15s;
}
.episode-event:hover {
  background: var(--hover);
  transform: translateY(-2px);
}
.episode-event:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.episode-event.aired {
  background: color-mix(in srgb, var(--status-color) 8%, var(--surface));
  border-color: color-mix(in srgb, var(--status-color) 25%, var(--border));
  border-left-color: var(--status-color);
}
.episode-event.aired:hover {
  background: color-mix(in srgb, var(--status-color) 15%, var(--surface));
}
.library-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-top: 10px;
  padding: 2px 7px;
  border-radius: 5px;
  background: color-mix(in srgb, var(--status-color) 12%, var(--surface));
  color: var(--text);
  font-size: 11px;
  line-height: 1.6;
}
.library-status span {
  color: var(--status-color);
  font-weight: 700;
}
.episode-event.uncertain {
  border-left-style: dashed;
  border-left-color: var(--muted);
  background: var(--surface);
}
.event-time {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 4px;
  color: var(--accent);
  font-size: 13px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.aired .event-time,
.uncertain .event-time {
  color: var(--muted);
}
.event-status {
  font-size: 10px;
  font-weight: 400;
}
.event-title {
  margin-top: 8px;
  font-size: 13px;
  line-height: 1.7;
  overflow-wrap: anywhere;
}
.empty-day {
  color: var(--muted);
  text-align: center;
  font-size: 12px;
  padding: 40px 0;
}
.empty-day span {
  display: block;
  margin-bottom: 8px;
  opacity: 0.5;
}
.empty-week {
  padding: 24px;
}
.calendar-note {
  margin-top: 18px;
}
.load-error {
  margin-bottom: 16px;
}
.undated {
  margin-top: 20px;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 10px;
}
.undated summary {
  cursor: pointer;
}
.undated-list {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 24px;
  padding-top: 16px;
}
.undated-list a {
  color: var(--muted);
  font-size: 13px;
}
@media (max-width: 700px) {
  .calendar-view {
    padding: 20px 12px;
  }
  .calendar-heading {
    align-items: flex-start;
  }
  h1 {
    font-size: 24px;
  }
  .calendar-search {
    width: 100%;
    margin-left: 0;
  }
  .week-title {
    font-size: 13px;
  }
  .week-grid {
    grid-template-columns: repeat(7, minmax(165px, 1fr));
  }
}
</style>
