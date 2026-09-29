<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NEmpty,
  NSelect,
  NSpin,
  useThemeVars,
} from "naive-ui";
import { getLogContent, getLogDates } from "@/services/logService";

const theme = useThemeVars();
const dates = ref<string[]>([]);
const selectedDate = ref<string | null>(null);
const content = ref("");
const loading = ref(false);
const error = ref("");
const logPanel = ref<HTMLElement | null>(null);

async function fetchDates() {
  loading.value = true;
  error.value = "";
  try {
    const response = await getLogDates();
    dates.value = response.data.data || [];
    if (!selectedDate.value || !dates.value.includes(selectedDate.value)) {
      selectedDate.value = dates.value[0] || null;
    }
    if (selectedDate.value) await fetchContent();
    else content.value = "";
  } catch {
    error.value = "日志日期加载失败，请重试。";
  } finally {
    loading.value = false;
  }
}

async function fetchContent() {
  if (!selectedDate.value) return;
  loading.value = true;
  error.value = "";
  try {
    const response = await getLogContent(selectedDate.value);
    content.value = response.data.data || "";
    await nextTick();
    if (logPanel.value) logPanel.value.scrollTop = logPanel.value.scrollHeight;
  } catch {
    error.value = "所选日期的日志加载失败，请重试。";
  } finally {
    loading.value = false;
  }
}

onMounted(fetchDates);
</script>

<template>
  <section
    class="log-view"
    :style="{
      '--accent': theme.primaryColor,
      '--surface': theme.cardColor,
      '--border': theme.borderColor,
      '--muted': theme.textColor3,
      '--text': theme.textColor1,
      '--code': theme.codeColor,
    }"
  >
    <header class="page-heading">
      <div>
        <div class="eyebrow">运行记录</div>
        <h1>查看日志</h1>
        <p>日志按服务器本地日期分割，保留最近 7 天。</p>
      </div>
      <div class="log-actions">
        <n-select
          v-model:value="selectedDate"
          :options="dates.map((date) => ({ label: date, value: date }))"
          placeholder="选择日志日期"
          :disabled="!dates.length"
          @update:value="fetchContent"
        />
        <n-button :loading="loading" @click="fetchDates">刷新</n-button>
      </div>
    </header>

    <n-alert v-if="error" type="error" class="load-error">{{ error }}</n-alert>
    <n-spin :show="loading">
      <div v-if="selectedDate" ref="logPanel" class="log-panel" tabindex="0">
        <pre v-if="content">{{ content }}</pre>
        <n-empty v-else description="当天暂无日志内容" />
      </div>
      <n-empty
        v-else
        description="服务器上暂无可查看的日志文件"
        class="empty-logs"
      />
    </n-spin>
  </section>
</template>

<style scoped>
.log-view {
  padding: 28px 24px;
  color: var(--text);
}
.page-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 24px;
}
.eyebrow {
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 2px;
}
h1 {
  margin: 4px 0 8px;
  font-size: 28px;
}
.page-heading p {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
}
.log-actions {
  display: grid;
  grid-template-columns: minmax(160px, 220px) auto;
  gap: 8px;
}
.log-panel {
  height: calc(100vh - 205px);
  min-height: 360px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: color-mix(in srgb, var(--surface) 92%, black);
}
pre {
  box-sizing: border-box;
  min-width: max-content;
  margin: 0;
  padding: 18px;
  color: var(--text);
  font-family: "Cascadia Code", "SFMono-Regular", Consolas, monospace;
  font-size: 12px;
  line-height: 1.7;
  white-space: pre;
}
.log-panel :deep(.n-empty),
.empty-logs {
  padding: 80px 20px;
}
.load-error {
  margin-bottom: 16px;
}
@media (max-width: 700px) {
  .log-view {
    padding: 20px 12px;
  }
  .page-heading {
    align-items: stretch;
    flex-direction: column;
  }
  .log-actions {
    grid-template-columns: minmax(0, 1fr) auto;
  }
  .log-panel {
    height: calc(100vh - 275px);
  }
}
</style>
