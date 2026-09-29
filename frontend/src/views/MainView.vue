<script lang="ts">
import MainMenu from '@/components/MainMenu.vue'
import MainTitleBar from '@/components/MainTitleBar.vue'
import SystemDashboard from '@/components/SystemDashboard.vue'
import { NBadge, NFloatButton, NIcon, NFloatButtonGroup, NPopover } from 'naive-ui';
import { RouterView } from 'vue-router'
import { useRouter } from 'vue-router';
import { 
  ArrowDownCircleOutline as DownloadIcon,
  ArrowUpCircleOutline as UploadIcon,
  RefreshOutline as RefreshIcon,
  RemoveCircleOutline as RemoveIcon,
  AddCircleOutline as AddIcon
} from '@vicons/ionicons5'
import DownloadContainer from '@/components/DownloadContainer.vue';
import UploadContainer from '@/components/UploadContainer.vue';
import { getDownloads } from '@/services/downloadService';
import { getUploads } from '@/services/uploadService';
import { type DownloadData } from '@/client';
import { runAllEngines } from '@/services/engineService';
import { useMessageStore } from '@/stores/message';
import { clearAll } from '@/services/downloadService';

export default {
  components: {
    RouterView,
    MainMenu,
    MainTitleBar,
    SystemDashboard,
    NFloatButton,
    NFloatButtonGroup,
    NIcon,
    DownloadIcon,
    UploadIcon,
    DownloadContainer,
    UploadContainer,
    NPopover,
    NBadge,
    RefreshIcon,
    RemoveIcon,
    AddIcon
  },
  setup() {
    const messageStore = useMessageStore();
    const router = useRouter();
    return {
      messageStore,
      router
    }
  },
  mounted() {
    this.needRefresh = true;
    this.repeatFetchDownloads();
  },
  unmounted() {
    this.needRefresh = false;
  },
  data() {
    return {
      downloads: [] as DownloadData[],
      uploads: [] as Array<{ id: string; name: string }>,
      needRefresh: true,
    };
  },
  computed: {
    activeDownloads(): DownloadData[] {
      return this.downloads.filter((download) =>
        ['active', 'waiting', 'paused'].includes(download.status)
      );
    }
  },
  methods: {
    async fetchDownloads() {
          this.downloads = (await getDownloads()).data.data!;
      },
    async fetchUploads() {
          this.uploads = (await getUploads()).data || [];
      },
    async repeatFetchDownloads() {
        while (this.needRefresh) {
            await this.fetchDownloads();
            await this.fetchUploads();
            await new Promise(resolve => setTimeout(resolve, 1000));
        }
    },
    handleRefresh() {
      runAllEngines().then(() => {
        this.messageStore.setMessage('已刷新订阅', 'success');
      });
    },
    handleClearAll() {
      clearAll().then(() => {
        this.messageStore.setMessage('已清除所有下载任务', 'success');
      });
    },
    goToSubscribe() {
      this.router.push('/library/subscribe');
    }
  }
}

</script>


<template>
  <div>
    <MainTitleBar />
    <div class="container">
      <MainMenu />
      <n-scrollbar style="max-height: calc(100vh - 60px);">
        <RouterView />
      </n-scrollbar>
    </div>
    <div class="button-group">
      <n-float-button-group shape="square" position="relative">
        <n-float-button>
          <n-popover placement="left-start" trigger="hover" scrollable>
            <template #trigger>
              <n-badge :value="activeDownloads.length" :offset="[6, -8]" :processing="true">
                <n-icon><download-icon /></n-icon>
              </n-badge>
            </template>
            <DownloadContainer :downloads="activeDownloads" />
          </n-popover>
        </n-float-button>
        <n-float-button>
          <n-popover placement="left-start" trigger="hover" scrollable>
            <template #trigger>
              <n-badge :value="uploads.length" :offset="[6, -8]" :processing="true">
                <n-icon><upload-icon /></n-icon>
              </n-badge>
            </template>
            <UploadContainer :uploads="uploads" />
          </n-popover>
        </n-float-button>
        <n-float-button @click="handleRefresh">
          <n-popover placement="left" trigger="hover" scrollable>
            <template #trigger>
              <n-icon><refresh-icon /></n-icon>
            </template>
            <div>更新全部媒体信息</div>
          </n-popover>
        </n-float-button>
        <n-float-button @click="handleClearAll">
          <n-popover placement="left" trigger="hover" scrollable>
            <template #trigger>
              <n-icon><remove-icon /></n-icon>
            </template>
            <div>清除下载队列中所有的任务</div>
          </n-popover>
        </n-float-button>
        <n-float-button @click="goToSubscribe">
          <n-popover placement="left" trigger="hover" scrollable>
            <template #trigger>
              <n-icon><add-icon /></n-icon>
            </template>
            <div>媒体库添加</div>
          </n-popover>
        </n-float-button>
      
      </n-float-button-group>
    </div>
  </div>
</template>


<style scoped>
.container {
  display: grid;
  /* height: calc(100vh - 200px); */
}

@media (min-width: 1024px) {
  .container {
    grid-template-columns: 200px minmax(0, 1fr);
  }
}

@media (max-width: 1023px) {
  .container {
    grid-template-rows: 50px 1fr;
  }
}

.button-group {
  position: fixed;
  z-index: 10;
  right: 1rem;
  bottom: 1rem;
}

</style>
