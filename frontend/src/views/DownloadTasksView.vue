<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, type Component } from "vue";
import {
  NAlert,
  NButton,
  NEmpty,
  NIcon,
  NInput,
  NProgress,
  NSpin,
  NTag,
  useThemeVars,
} from "naive-ui";
import {
  AlertCircleOutline,
  CheckmarkCircleOutline,
  CloudDownloadOutline,
  ListOutline,
  PauseCircleOutline,
  RefreshOutline,
  SearchOutline,
  TimeOutline,
  TrashOutline,
} from "@vicons/ionicons5";
import type { DownloadData } from "@/client";
import { clearAll, getDownloads } from "@/services/downloadService";
import { formatBytes } from "@/utils/formatter";

type FilterKey =
  "all" | "downloading" | "waiting" | "paused" | "complete" | "failed";

interface TaskFilter {
  key: FilterKey;
  label: string;
  icon: Component;
}

const theme = useThemeVars();
const downloads = ref<DownloadData[]>([]);
const loading = ref(false);
const clearing = ref(false);
const error = ref("");
const query = ref("");
const selectedFilter = ref<FilterKey>("all");
let timer: number | undefined;

const taskFilters: TaskFilter[] = [
  { key: "all", label: "全部任务", icon: ListOutline },
  { key: "downloading", label: "下载中", icon: CloudDownloadOutline },
  { key: "waiting", label: "等待中", icon: TimeOutline },
  { key: "paused", label: "已暂停", icon: PauseCircleOutline },
  { key: "complete", label: "已完成", icon: CheckmarkCircleOutline },
  { key: "failed", label: "异常", icon: AlertCircleOutline },
];

const visibleDownloads = computed(() =>
  downloads.value.filter((item) => !item.is_metadata),
);

const sortedDownloads = computed(() => {
  const order: Record<string, number> = {
    active: 0,
    waiting: 1,
    paused: 2,
    complete: 3,
    error: 4,
    removed: 5,
  };
  return [...visibleDownloads.value].sort(
    (left, right) =>
      (order[left.status] ?? 9) - (order[right.status] ?? 9) ||
      left.name.localeCompare(right.name, "zh-CN"),
  );
});

const filteredDownloads = computed(() => {
  const keyword = query.value.trim().toLocaleLowerCase();
  return sortedDownloads.value.filter((task) => {
    const matchesStatus = statusMatchesFilter(
      task.status,
      selectedFilter.value,
    );
    const matchesQuery =
      !keyword ||
      task.name.toLocaleLowerCase().includes(keyword) ||
      task.target_path.toLocaleLowerCase().includes(keyword) ||
      task.dir.toLocaleLowerCase().includes(keyword) ||
      task.id.toLocaleLowerCase().includes(keyword);
    return matchesStatus && matchesQuery;
  });
});

const totalSpeed = computed(() =>
  visibleDownloads.value
    .filter((task) => task.status === "active")
    .reduce((sum, task) => sum + task.current_speed, 0),
);

const activeCount = computed(
  () =>
    visibleDownloads.value.filter((task) => task.status === "active").length,
);

function statusMatchesFilter(status: string, filter: FilterKey) {
  if (filter === "all") return true;
  if (filter === "downloading") return status === "active";
  if (filter === "waiting") return status === "waiting";
  if (filter === "paused") return status === "paused";
  if (filter === "complete") return status === "complete";
  return status === "error" || status === "removed";
}

function filterCount(filter: FilterKey) {
  return visibleDownloads.value.filter((task) =>
    statusMatchesFilter(task.status, filter),
  ).length;
}

function statusLabel(status: string) {
  return (
    {
      active: "下载中",
      waiting: "等待中",
      paused: "已暂停",
      complete: "已完成",
      error: "失败",
      removed: "已移除",
    }[status] || status
  );
}

function statusType(status: string): "success" | "warning" | "error" | "info" {
  if (status === "complete") return "success";
  if (status === "paused" || status === "waiting") return "warning";
  if (status === "error" || status === "removed") return "error";
  return "info";
}

function statusIcon(status: string): Component {
  if (status === "active") return CloudDownloadOutline;
  if (status === "waiting") return TimeOutline;
  if (status === "paused") return PauseCircleOutline;
  if (status === "complete") return CheckmarkCircleOutline;
  return AlertCircleOutline;
}

function progressStatus(
  status: string,
): "success" | "error" | "warning" | "default" {
  if (status === "complete") return "success";
  if (status === "error" || status === "removed") return "error";
  if (status === "paused" || status === "waiting") return "warning";
  return "default";
}

async function fetchDownloads(showLoading = false) {
  if (showLoading) loading.value = true;
  try {
    const response = await getDownloads();
    downloads.value = response.data.data || [];
    error.value = "";
  } catch {
    error.value = "下载任务加载失败，请重试。";
  } finally {
    loading.value = false;
  }
}

async function clearDownloads() {
  clearing.value = true;
  try {
    const response = await clearAll();
    if (response.data.success === false) {
      error.value = response.data.message || "清空下载任务失败。";
      return;
    }
    await fetchDownloads();
  } catch {
    error.value = "清空下载任务失败，请重试。";
  } finally {
    clearing.value = false;
  }
}

onMounted(() => {
  fetchDownloads(true);
  timer = window.setInterval(() => fetchDownloads(), 2000);
});

onBeforeUnmount(() => window.clearInterval(timer));
</script>

<template>
  <section
    class="tasks-view"
    :style="{
      '--accent': theme.primaryColor,
      '--surface': theme.cardColor,
      '--page': theme.bodyColor,
      '--hover': theme.hoverColor,
      '--border': theme.borderColor,
      '--divider': theme.dividerColor,
      '--muted': theme.textColor3,
      '--text': theme.textColor1,
      '--success': theme.successColor,
      '--warning': theme.warningColor,
      '--error': theme.errorColor,
    }"
  >
    <header class="page-heading">
      <div>
        <div class="eyebrow">ARIA2 任务中心</div>
        <h1>下载任务</h1>
        <p>直接显示 aria2 当前任务，每 2 秒自动同步。</p>
      </div>
      <div class="transfer-summary" aria-label="下载状态概览">
        <div>
          <span class="summary-label">下载速度</span>
          <strong>{{ formatBytes(totalSpeed) }}/s</strong>
        </div>
        <div>
          <span class="summary-label">活动任务</span>
          <strong>{{ activeCount }}</strong>
        </div>
        <div>
          <span class="summary-label">全部任务</span>
          <strong>{{ visibleDownloads.length }}</strong>
        </div>
      </div>
    </header>

    <n-alert v-if="error" type="error" class="load-error">{{ error }}</n-alert>

    <div class="task-console">
      <div class="task-toolbar">
        <div class="filter-strip" role="tablist" aria-label="任务状态筛选">
          <button
            v-for="filter in taskFilters"
            :key="filter.key"
            type="button"
            class="filter-button"
            :class="{ active: selectedFilter === filter.key }"
            role="tab"
            :aria-selected="selectedFilter === filter.key"
            @click="selectedFilter = filter.key"
          >
            <n-icon :component="filter.icon" />
            <span>{{ filter.label }}</span>
            <span class="filter-count">{{ filterCount(filter.key) }}</span>
          </button>
        </div>

        <div class="toolbar-actions">
          <n-input
            v-model:value="query"
            clearable
            size="small"
            placeholder="搜索任务"
            aria-label="搜索下载任务"
          >
            <template #prefix><n-icon :component="SearchOutline" /></template>
          </n-input>
          <n-button
            size="small"
            :loading="loading"
            aria-label="刷新下载任务"
            @click="fetchDownloads(true)"
          >
            <template #icon><n-icon :component="RefreshOutline" /></template>
            刷新
          </n-button>
          <n-button
            size="small"
            type="error"
            secondary
            :loading="clearing"
            :disabled="!visibleDownloads.length"
            @click="clearDownloads"
          >
            <template #icon><n-icon :component="TrashOutline" /></template>
            清空
          </n-button>
        </div>
      </div>

      <div class="list-heading" aria-hidden="true">
        <span>任务名称</span>
        <span>进度与传输信息</span>
      </div>

      <n-spin :show="loading">
        <div v-if="filteredDownloads.length" class="task-list">
          <article
            v-for="task in filteredDownloads"
            :key="task.id"
            class="task-row"
            :class="`status-${task.status}`"
          >
            <div class="status-mark" :title="statusLabel(task.status)">
              <n-icon :component="statusIcon(task.status)" />
            </div>

            <div class="task-main">
              <div class="task-title-row">
                <h2 :title="task.name">{{ task.name }}</h2>
                <n-tag
                  :type="statusType(task.status)"
                  size="small"
                  :bordered="false"
                >
                  {{ statusLabel(task.status) }}
                </n-tag>
              </div>
              <div class="task-path" :title="task.target_path || task.dir">
                {{ task.target_path || task.dir || "暂未获取保存路径" }}
              </div>
              <n-progress
                type="line"
                :percentage="Math.round(task.progress * 10) / 10"
                :processing="task.status === 'active'"
                :status="progressStatus(task.status)"
                :height="7"
                :border-radius="2"
              />
              <div class="task-details">
                <span class="detail-primary">
                  {{ formatBytes(task.current_download) }} /
                  {{ formatBytes(task.total_length) }}
                </span>
                <span v-if="task.status === 'active'" class="speed">
                  ↓ {{ formatBytes(task.current_speed) }}/s
                </span>
                <span>{{ Math.round(task.progress * 10) / 10 }}%</span>
                <span class="task-dir" :title="task.dir"
                  >目录：{{ task.dir || "—" }}</span
                >
                <span class="gid" :title="task.id">GID：{{ task.id }}</span>
              </div>
            </div>
          </article>
        </div>

        <n-empty
          v-else
          class="empty-tasks"
          :description="query ? '没有匹配的下载任务' : '当前分类暂无任务'"
        />
      </n-spin>
    </div>
  </section>
</template>

<style scoped>
.tasks-view {
  padding: 24px;
  color: var(--text);
}

.page-heading {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 20px;
}

.eyebrow {
  color: var(--accent);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1.8px;
}

h1 {
  margin: 3px 0 5px;
  font-size: 27px;
  line-height: 1.2;
}

.page-heading p {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
}

.transfer-summary {
  display: flex;
  align-items: stretch;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface);
}

.transfer-summary > div {
  min-width: 94px;
  padding: 9px 14px;
  border-left: 1px solid var(--divider);
  text-align: right;
}

.transfer-summary > div:first-child {
  border-left: 0;
}

.summary-label {
  display: block;
  margin-bottom: 2px;
  color: var(--muted);
  font-size: 10px;
}

.transfer-summary strong {
  color: var(--accent);
  font-size: 14px;
  font-variant-numeric: tabular-nums;
}

.task-console {
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 9px;
  background: var(--surface);
  box-shadow: 0 4px 18px color-mix(in srgb, var(--text) 5%, transparent);
}

.task-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 9px 12px;
  border-bottom: 1px solid var(--divider);
}

.filter-strip {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.filter-button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 9px;
  border: 0;
  border-radius: 5px;
  color: var(--muted);
  background: transparent;
  cursor: pointer;
  font: inherit;
  font-size: 12px;
  white-space: nowrap;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.filter-button:hover {
  color: var(--text);
  background: var(--hover);
}

.filter-button.active {
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 13%, transparent);
  font-weight: 600;
}

.filter-count {
  min-width: 18px;
  padding: 0 4px;
  border-radius: 8px;
  background: color-mix(in srgb, currentColor 10%, transparent);
  font-size: 10px;
  line-height: 17px;
  text-align: center;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 7px;
  flex: 0 0 auto;
}

.toolbar-actions .n-input {
  width: 180px;
}

.list-heading {
  display: grid;
  grid-template-columns: minmax(240px, 1fr) minmax(360px, 1.2fr);
  gap: 24px;
  padding: 7px 56px;
  border-bottom: 1px solid var(--divider);
  color: var(--muted);
  background: color-mix(in srgb, var(--page) 55%, var(--surface));
  font-size: 10px;
  letter-spacing: 0.5px;
}

.task-list {
  min-height: 80px;
}

.task-row {
  display: grid;
  grid-template-columns: 30px minmax(0, 1fr);
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--divider);
  transition: background-color 0.15s;
}

.task-row:last-child {
  border-bottom: 0;
}

.task-row:hover {
  background: var(--hover);
}

.status-mark {
  display: grid;
  place-items: center;
  align-self: start;
  width: 27px;
  height: 27px;
  margin-top: 1px;
  border-radius: 50%;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  font-size: 17px;
}

.status-paused .status-mark,
.status-waiting .status-mark {
  color: var(--warning);
  background: color-mix(in srgb, var(--warning) 12%, transparent);
}

.status-complete .status-mark {
  color: var(--success);
  background: color-mix(in srgb, var(--success) 12%, transparent);
}

.status-error .status-mark,
.status-removed .status-mark {
  color: var(--error);
  background: color-mix(in srgb, var(--error) 12%, transparent);
}

.task-main {
  min-width: 0;
}

.task-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.task-title-row h2 {
  min-width: 0;
  margin: 0;
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.5;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-path {
  margin: 3px 0 9px;
  overflow: hidden;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.5;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-details {
  display: flex;
  align-items: center;
  gap: 8px 18px;
  margin-top: 7px;
  overflow: hidden;
  color: var(--muted);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.detail-primary {
  color: var(--text);
}

.speed {
  color: var(--accent);
  font-weight: 600;
}

.task-dir {
  min-width: 60px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.gid {
  margin-left: auto;
  overflow: hidden;
  text-overflow: ellipsis;
}

.empty-tasks {
  padding: 80px 20px;
}

.load-error {
  margin-bottom: 14px;
}

@media (max-width: 1180px) {
  .task-toolbar {
    align-items: stretch;
    flex-direction: column;
  }

  .filter-strip {
    overflow-x: auto;
    padding-bottom: 2px;
  }

  .toolbar-actions .n-input {
    flex: 1;
    width: auto;
  }
}

@media (max-width: 760px) {
  .tasks-view {
    padding: 16px 10px;
  }

  .page-heading {
    align-items: stretch;
    flex-direction: column;
  }

  .transfer-summary {
    width: 100%;
  }

  .transfer-summary > div {
    flex: 1;
    min-width: 0;
    padding: 8px 9px;
  }

  .list-heading {
    display: none;
  }

  .toolbar-actions {
    flex-wrap: wrap;
  }

  .toolbar-actions .n-input {
    flex-basis: 100%;
  }

  .task-row {
    gap: 9px;
    padding: 13px 11px;
  }

  .task-details {
    flex-wrap: wrap;
    white-space: normal;
  }

  .task-dir {
    flex-basis: 100%;
  }

  .gid {
    display: none;
  }
}
</style>
