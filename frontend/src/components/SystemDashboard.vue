<script lang="ts">
import { NProgress, NCard, NSpace, NText } from 'naive-ui';
import { defineComponent, ref, onMounted, onUnmounted, computed } from 'vue';
import axios from 'axios';
import { useThemeStore } from '@/stores/theme';

interface SystemInfo {
  cpu_usage: number;
  upload_speed: number;
  download_speed: number;
  disk_total: number;
  disk_used: number;
  disk_available: number;
  disk_usage_percent: number;
  disk_temperature: number | null;
}

function getCpuColor(percentage: number, primaryColor: string = '#2080f0'): string {
  if (percentage < 50) return primaryColor;
  if (percentage < 80) return '#f0a020';
  return '#f05050';
}

function getDiskColor(percentage: number, primaryColor: string = '#2080f0'): string {
  if (percentage < 60) return primaryColor;
  if (percentage < 85) return '#f0a020';
  return '#f05050';
}

export default defineComponent({
  name: 'SystemDashboard',
  components: {
    NProgress,
    NCard,
    NSpace,
    NText
  },
  setup() {
    const themeStore = useThemeStore();
    const systemInfo = ref<SystemInfo | null>(null);
    const pollingActive = ref(true);
    
    const dashboardCardStyle = computed(() => {
      const primaryColor = themeStore.theme.common?.primaryColor || '#4dd2ff';
      const lighterColor = lightenColor(primaryColor, 40);
      return {
        background: `linear-gradient(135deg, ${lighterColor}20 0%, ${lighterColor}10 100%)`
      };
    });
    
    const lightenColor = (color: string, percent: number): string => {
      const num = parseInt(color.replace('#', ''), 16);
      const amt = Math.round(2.55 * percent);
      const R = (num >> 16) + amt;
      const G = (num >> 8 & 0x00FF) + amt;
      const B = (num & 0x0000FF) + amt;
      return '#' + (0x1000000 + 
        (R < 255 ? R < 1 ? 0 : R : 255) * 0x10000 + 
        (G < 255 ? G < 1 ? 0 : G : 255) * 0x100 + 
        (B < 255 ? B < 1 ? 0 : B : 255)
      ).toString(16).slice(1);
    };

    const formatBytes = (bytes: number): string => {
      if (bytes === 0) return '0 B';
      const k = 1024;
      const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(k));
      return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    const formatSpeed = (bytesPerSec: number): string => {
      if (bytesPerSec === 0) return '0 B/s';
      const k = 1024;
      const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
      const i = Math.floor(Math.log(bytesPerSec) / Math.log(k));
      return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    const formatPercent = (value: number): string => {
      return value.toFixed(1) + '%';
    };

    const fetchSystemInfo = async () => {
      try {
        const response = await axios.get('/v1/systems/info');
        if (response.data.success && response.data.data) {
          systemInfo.value = response.data.data;
        }
      } catch (error) {
        console.error('Failed to fetch system info:', error);
      }
    };

    const startPolling = async () => {
      while (pollingActive.value) {
        await fetchSystemInfo();
        await new Promise(resolve => setTimeout(resolve, 3000));
      }
    };

    onMounted(() => {
      fetchSystemInfo();
      startPolling();
    });

    onUnmounted(() => {
      pollingActive.value = false;
    });

    const uploadIconStyle = computed(() => {
      const primaryColor = themeStore.theme.common?.primaryColor || '#4dd2ff';
      return {
        background: `${primaryColor}20`,
        color: primaryColor
      };
    });
    
    const downloadIconStyle = computed(() => {
      const primaryColor = themeStore.theme.common?.primaryColor || '#4dd2ff';
      const lighterColor = lightenColor(primaryColor, 20);
      return {
        background: `${lighterColor}30`,
        color: primaryColor
      };
    });
    
    const getTemperatureStyle = (temp: number | null | undefined) => {
      if (temp === null || temp === undefined) return {};
      let color = 'inherit';
      if (temp >= 50) color = '#f0a020';
      if (temp >= 60) color = '#f05050';
      return { color };
    };
    
    return {
      systemInfo,
      formatBytes,
      formatSpeed,
      formatPercent,
      getCpuColor,
      getDiskColor,
      getTemperatureStyle,
      dashboardCardStyle,
      uploadIconStyle,
      downloadIconStyle,
      themeStore
    };
  }
});
</script>

<template>
  <div class="system-dashboard">
    <n-card :bordered="false" size="small" class="dashboard-card" :style="dashboardCardStyle">
      <template #header>
        <div class="card-header">
          <span class="card-title">系统监控</span>
        </div>
      </template>
      <n-space vertical :size="16">
        <div class="dashboard-item">
          <div class="item-header">
            <span class="item-label">CPU 使用率</span>
            <span class="item-value">{{ formatPercent(systemInfo?.cpu_usage ?? 0) }}</span>
          </div>
          <n-progress 
            type="line" 
            :percentage="systemInfo?.cpu_usage ?? 0" 
            :show-indicator="false"
            :color="getCpuColor(systemInfo?.cpu_usage ?? 0, themeStore.theme.common?.primaryColor)"
            :stroke-width="6"
          />
        </div>

        <div class="dashboard-item">
          <div class="item-header">
            <span class="item-label">网络带宽</span>
          </div>
          <div class="bandwidth-container">
            <div class="bandwidth-item upload">
              <div class="bandwidth-icon" :style="uploadIconStyle">↑</div>
              <div class="bandwidth-info">
                <span class="bandwidth-label">上传</span>
                <span class="bandwidth-value" :style="{ color: themeStore.theme.common?.primaryColor }">{{ formatSpeed(systemInfo?.upload_speed ?? 0) }}</span>
              </div>
            </div>
            <div class="bandwidth-item download">
              <div class="bandwidth-icon" :style="downloadIconStyle">↓</div>
              <div class="bandwidth-info">
                <span class="bandwidth-label">下载</span>
                <span class="bandwidth-value" :style="{ color: themeStore.theme.common?.primaryColor }">{{ formatSpeed(systemInfo?.download_speed ?? 0) }}</span>
              </div>
            </div>
          </div>
        </div>

        <div class="dashboard-item">
          <div class="item-header">
            <span class="item-label">媒体库磁盘</span>
            <span class="item-value">{{ formatPercent(systemInfo?.disk_usage_percent ?? 0) }}</span>
          </div>
          <n-progress 
            type="line" 
            :percentage="systemInfo?.disk_usage_percent ?? 0" 
            :show-indicator="false"
            :color="getDiskColor(systemInfo?.disk_usage_percent ?? 0, themeStore.theme.common?.primaryColor)"
            :stroke-width="6"
          />
          <div class="disk-details">
            <span class="disk-detail">已用: {{ formatBytes(systemInfo?.disk_used ?? 0) }}</span>
            <span class="disk-detail">总计: {{ formatBytes(systemInfo?.disk_total ?? 0) }}</span>
          </div>
        </div>

        <div class="dashboard-item temperature-display" v-if="systemInfo?.disk_temperature != null">
          <div class="temperature-label">
            <svg class="temperature-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M9 14.8V5a3 3 0 0 1 6 0v9.8a5 5 0 1 1-6 0Z" />
              <path d="M12 8v9" />
              <circle cx="12" cy="18" r="1.5" fill="currentColor" stroke="none" />
            </svg>
            <span class="item-label">硬盘温度</span>
          </div>
          <div class="temperature-value" :style="getTemperatureStyle(systemInfo?.disk_temperature)">
            <span>{{ systemInfo?.disk_temperature?.toFixed(1) }}</span>
            <span class="temperature-unit">°C</span>
          </div>
        </div>
      </n-space>
    </n-card>
  </div>
</template>

<style scoped>
.system-dashboard {
  width: 100%;
}

.dashboard-card {
  border-radius: 8px;
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.card-title {
  font-size: 14px;
  font-weight: 600;
  color: #1e293b;
}

.dashboard-item {
  width: 100%;
}

.item-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.item-label {
  font-size: 12px;
  color: #64748b;
  font-weight: 500;
}

.item-value {
  font-size: 14px;
  font-weight: 700;
  color: #1e293b;
}

.bandwidth-container {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.bandwidth-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: #ffffff;
  border-radius: 6px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.bandwidth-icon {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  font-size: 16px;
  font-weight: 700;
}



.bandwidth-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.bandwidth-label {
  font-size: 11px;
  color: #94a3b8;
}

.bandwidth-value {
  font-size: 13px;
  font-weight: 600;
  color: #1e293b;
}

.disk-details {
  display: flex;
  justify-content: space-between;
  margin-top: 6px;
}

.disk-detail {
  font-size: 11px;
  color: #64748b;
}

.temperature-display {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 12px;
  border-top: 1px solid rgba(100, 116, 139, 0.14);
  color: #64748b;
}

.temperature-label {
  display: flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}

.temperature-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

.temperature-value {
  display: flex;
  align-items: baseline;
  gap: 3px;
  font-size: 14px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  transition: color 0.3s;
}

.temperature-unit {
  font-size: 11px;
  font-weight: 400;
}
</style>
