<script lang="ts">
import { h, type Component } from 'vue'
import { NIcon, NMenu } from 'naive-ui'
import type { MenuOption } from 'naive-ui'
import SystemDashboard from './SystemDashboard.vue'

import {
  CalendarOutline as CalendarIcon,
  BookOutline as BookIcon,
  SettingsOutline as SettingsIcon,
  HomeOutline as HomeIcon,
  FilmOutline as FilmIcon,
  ArrowDownCircleOutline as DownloadIcon,
  DocumentTextOutline as LogIcon
} from '@vicons/ionicons5'


function renderIcon (icon: Component) {
  return () => h(NIcon, null, { default: () => h(icon) })
}

const menuOptions: MenuOption[] = [
  {
    label: '媒体库',
    key: 'media',
    icon: renderIcon(HomeIcon)
  },
  {
    label: '播出日历',
    key: 'calendar',
    icon: renderIcon(CalendarIcon)
  },
  {
    label: '媒体库添加',
    key: 'subscribe',
    icon: renderIcon(BookIcon),
    disabled: false,
  },
  {
    label: '电影添加',
    key: 'movie-search',
    icon: renderIcon(FilmIcon),
    disabled: false,
  },
  {
    label: '下载任务',
    key: 'downloads',
    icon: renderIcon(DownloadIcon)
  },
  {
    label: '查看日志',
    key: 'logs',
    icon: renderIcon(LogIcon)
  },
  {
    label: '系统设置',
    key: 'settings',
    icon: renderIcon(SettingsIcon),
    disabled: false,
  }
]

function isHorizontal(): 'horizontal' | 'vertical' {
  let width = window.innerWidth
  if (width < 1024) {
    return 'horizontal'
  } else {
    return 'vertical'
  }
}

export default {
  setup() {
  
    return {

    };
  },
  mounted() {
    window.addEventListener('resize', this.updateWidth);
  },

  data() {
    return {
      menuOptions,
      menuMode: isHorizontal()
    }
  },
  methods: {
    updateWidth() {
      this.menuMode = isHorizontal()
    },
    handleSelect(key: string) {
      this.$router.push({ name: key })
    }
  },
  components: {
    NMenu,
    SystemDashboard
  },
  computed: {
    currentPage() {
      return this.$route.name?.toString() || 'media'
    }
  }
}

</script>

<template>
  <div class="main-menu-container">
    <n-menu
      :on-update:value="handleSelect"
      :mode="menuMode"
      :options="menuOptions"
      :value="currentPage"
      responsive
    />
    <div v-if="menuMode === 'vertical'" class="system-dashboard-wrapper">
      <system-dashboard />
    </div>
  </div>
</template>

<style scoped>
.main-menu-container {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.system-dashboard-wrapper {
  margin-top: auto;
  padding: 8px;
  border-top: 1px solid #e0e0e0;
}
</style>
