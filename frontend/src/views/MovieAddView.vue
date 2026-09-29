<script lang="ts">
import { NInput, NButton, NCard, NImage, NSpace, NText, NModal, NForm, NFormItem, NCheckbox, NTag, NIcon, NProgress, NSpin } from 'naive-ui';
import { SearchOutline as SearchIcon, CheckmarkCircle, CloudUploadOutline as UploadIcon, DownloadOutline as DownloadIcon, CloseOutline as CloseIcon, RemoveCircleOutline as RemoveIcon } from '@vicons/ionicons5';
import axios from 'axios';
import { useMessageStore } from '@/stores/message';

export default {
  components: {
    NInput,
    NButton,
    NCard,
    NImage,
    NSpace,
    NText,
    SearchIcon,
    NModal,
    NForm,
    NFormItem,
    NCheckbox,
    NTag,
    NIcon,
    CheckmarkCircle,
    UploadIcon,
    DownloadIcon,
    CloseIcon,
    RemoveIcon,
    NProgress,
    NSpin
  },
  setup() {
    const messageStore = useMessageStore();
    return {
      messageStore
    }
  },
  data() {
    return {
      searchQuery: '',
      searchResults: [] as any[],
      isSearching: false,
      hasSearched: false,
      showModal: false,
      modalData: null as any,
      media_library_path: '',
      showDownloadModal: false,
      downloadLink: '',
      showUploadModal: false,
      uploadFiles: undefined as any,
      isUploading: false,
      uploadProgress: 0,
      currentUploadingFile: '',
      uploadSpeed: '0 KB/s',
      uploadedFileCount: 0,
      uploadSuccessMessage: '',
      isUploadComplete: false,
      uploadCancelSource: null as AbortController | null,
    }
  },
  mounted() {
    this.fetchSettings();
    this.uploadFiles = [];
  },
  computed: {
    uploadFilesList(): File[] {
      return this.uploadFiles || [];
    }
  },
  methods: {
    async fetchSettings() {
      try {
        const response = await axios.get('/v1/settings');
        if (response.data && response.data.media_library_path) {
          this.media_library_path = response.data.media_library_path;
        }
      } catch (error) {
        console.error('获取配置失败:', error);
      }
    },
    async handleSearch() {
      if (!this.searchQuery) return;
      
      this.isSearching = true;
      this.hasSearched = true;
      try {
        const response = await axios.post('/v1/upload/moviesearch', { query: this.searchQuery });
        this.searchResults = response.data.results;
      } catch (error) {
        console.error('搜索失败:', error);
        this.searchResults = [];
      } finally {
        this.isSearching = false;
      }
    },
    getImageUrl(posterPath: string) {
      if (!posterPath) return '';
      return `https://image.tmdb.org/t/p/w500${posterPath}`;
    },
    sanitizeFileName(name: string): string {
      const illegalChars = /[/:*?"<>|']/g;
      return name.replace(illegalChars, ' ');
    },
    async handleResultClick(result: any) {
      if (!this.media_library_path) {
        await this.fetchSettings();
      }
      
      const isAnime = result.genre_ids && result.genre_ids.includes(16);
      const year = result.release_date ? result.release_date.substring(0, 4) : '';
      const type = isAnime ? 'Anime Movie' : 'Movie';
      const sanitizedName = this.sanitizeFileName(result.title);
      const displayNameWithYear = year 
        ? `${sanitizedName} (${year})`
        : sanitizedName;
      const mediaPath = year 
        ? `${this.media_library_path}/media/${type}/${displayNameWithYear}`
        : `${this.media_library_path}/media/${type}/${sanitizedName}`;
      
      this.modalData = {
        id: result.id,
        title: result.title,
        original_title: result.original_title,
        poster_path: result.poster_path,
        is_anime: isAnime,
        media_library_path: this.media_library_path,
        year: year,
        media_path: mediaPath,
        display_name: displayNameWithYear
      };
      
      this.showModal = true;
    },
    closeModal() {
      this.showModal = false;
      this.modalData = null;
    },
    handleRemoteDownload() {
      this.showDownloadModal = true;
    },
    closeDownloadModal() {
      this.showDownloadModal = false;
      this.downloadLink = '';
    },
    isLinkValid(link: string): boolean {
      if (!link) return false;
      const magnetRegex = /^magnet:\?xt=urn:[a-z0-9]+:[a-z0-9]+/i;
      const httpRegex = /^(https?:\/\/|ftp:\/\/)/i;
      return magnetRegex.test(link) || httpRegex.test(link);
    },
    async confirmDownload() {
      if (!this.isLinkValid(this.downloadLink)) {
        this.messageStore.setMessage('链接格式不合法，请输入有效的磁力链或HTTP链接', 'error');
        return;
      }

      try {
        await axios.post('/v1/download/addmovie', {
          display_name: this.modalData.display_name,
          media_path: this.modalData.media_path,
          magnet_uri: this.downloadLink
        });
        this.closeDownloadModal();
        this.closeModal();
        this.messageStore.setMessage('下载已添加到队列', 'success');
      } catch (error) {
        console.error('添加下载失败:', error);
        this.messageStore.setMessage('添加下载失败', 'error');
      }
    },
    handleManualUpload() {
      this.showUploadModal = true;
      this.resetUploadState();
    },
    closeUploadModal() {
      this.showUploadModal = false;
    },
    isMediaOrSubtitleFile(fileName: string): boolean {
      const mediaExtensions = ['flv', 'mkv', 'mp4', 'avi', 'rmvb', 'm2ts', 'wmv'];
      const subtitleExtensions = ['srt', 'ass', 'ssa', 'sub', 'smi', 'xml'];
      const ext = fileName.split('.').pop()?.toLowerCase() || '';
      return mediaExtensions.includes(ext) || subtitleExtensions.includes(ext);
    },
    handleFileDrop(event: DragEvent) {
      event.preventDefault();
      event.stopPropagation();
      if (event.dataTransfer && event.dataTransfer.files) {
        this.addFiles(Array.from(event.dataTransfer.files));
      }
    },
    handleFileSelect(event: Event) {
      const target = event.target as HTMLInputElement;
      if (target.files) {
        this.addFiles(Array.from(target.files));
      }
    },
    addFiles(files: File[]) {
      for (const file of files) {
        if (this.isMediaOrSubtitleFile(file.name)) {
          if (!this.uploadFiles.some((f: File) => f.name === file.name)) {
            this.uploadFiles.push(file);
          }
        }
      }
    },
    removeFile(index: number) {
      this.uploadFiles.splice(index, 1);
    },
    formatSize(bytes: number): string {
      if (bytes === 0) return '0 B';
      const k = 1024;
      const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(k));
      return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    },
    formatSpeed(bytesPerSecond: number): string {
      if (bytesPerSecond >= 1024 * 1024) {
        return (bytesPerSecond / (1024 * 1024)).toFixed(2) + ' MB/s';
      } else if (bytesPerSecond >= 1024) {
        return (bytesPerSecond / 1024).toFixed(2) + ' KB/s';
      } else {
        return bytesPerSecond.toFixed(2) + ' B/s';
      }
    },
    async startUpload() {
      if (this.uploadFilesList.length === 0) return;
      
      this.isUploading = true;
      this.uploadProgress = 0;
      this.uploadSuccessMessage = '';
      this.isUploadComplete = false;
      
      let uploadedSize = 0;
      const startTime = Date.now();
      
      this.uploadCancelSource = new AbortController();
      
      try {
        const formData = new FormData();
        formData.append('display_name', this.modalData.display_name);
        formData.append('media_path', this.modalData.media_path);
        
        for (const file of this.uploadFilesList) {
          formData.append('file', file);
        }
        
        const response = await axios.post('/v1/upload/addmovie', formData, {
          headers: {
            'Content-Type': 'multipart/form-data'
          },
          onUploadProgress: (progressEvent) => {
            if (progressEvent.total) {
              uploadedSize = progressEvent.loaded;
              const progress = Math.round((uploadedSize / progressEvent.total) * 100);
              this.uploadProgress = progress;
              
              const elapsed = (Date.now() - startTime) / 1000;
              if (elapsed > 0) {
                const speed = uploadedSize / elapsed;
                this.uploadSpeed = this.formatSpeed(speed);
              }
            }
          },
          signal: this.uploadCancelSource.signal
        });
        
        if (response.data.success) {
          this.uploadProgress = 100;
          this.isUploadComplete = true;
          this.uploadedFileCount = response.data.file_count || 0;
          this.uploadSuccessMessage = `已经上传成功${this.uploadedFileCount}个文件`;
          this.messageStore.setMessage(response.data.message, 'success');
        } else {
          this.messageStore.setMessage(response.data.message, 'error');
        }
      } catch (error: any) {
        if (error.name === 'CanceledError') {
          console.log('上传已取消');
        } else {
          console.error('上传失败:', error);
          this.messageStore.setMessage('上传失败', 'error');
        }
      } finally {
        this.isUploading = false;
        this.uploadCancelSource = null;
      }
    },
    cancelUpload() {
      if (this.uploadCancelSource) {
        this.uploadCancelSource.abort();
      }
      this.resetUploadState();
    },
    resetUploadState() {
      this.isUploading = false;
      this.uploadProgress = 0;
      this.uploadSpeed = '0 KB/s';
      this.uploadedFileCount = 0;
      this.uploadSuccessMessage = '';
      this.isUploadComplete = false;
      this.uploadCancelSource = null;
      this.uploadFiles = [];
    },
    preventDragDefaults(event: DragEvent) {
      event.preventDefault();
      event.stopPropagation();
    }
  }
}
</script>

<template>
  <div class="movie-add-view">
    <div class="search-container">
      <n-input 
        v-model:value="searchQuery" 
        placeholder="请输入您想添加的电影进行搜索" 
        size="large"
        class="search-input"
        @keyup.enter="handleSearch"
      />
      <n-button 
        type="primary" 
        size="large" 
        @click="handleSearch"
        class="search-button"
        :loading="isSearching"
      >
        <template #icon>
          <search-icon />
        </template>
        搜索
      </n-button>
    </div>
    
    <div v-if="searchResults.length > 0" class="search-results">
      <n-card 
        v-for="result in searchResults" 
        :key="result.id" 
        hoverable 
        style="margin-bottom: 20px; cursor: pointer;"
        @click="handleResultClick(result)"
      >
        <template #header>
          <div class="result-header">
            <n-text strong>{{ result.title }}</n-text>
          </div>
        </template>
        <div class="result-content">
          <n-image 
            v-if="result.poster_path" 
            :src="getImageUrl(result.poster_path)" 
            width="120" 
            height="180"
            class="result-image"
          />
          <div class="result-info">
            <div class="result-title">
              <n-text strong>{{ result.title }}</n-text>
            </div>
            <div class="result-original-name">
              <n-text size="small">{{ result.original_title }}</n-text>
              <n-text size="small" class="result-date">上映时间：{{ result.release_date }}</n-text>
            </div>
            <div class="result-overview">
              <n-text size="small">{{ result.overview }}</n-text>
            </div>
          </div>
        </div>
      </n-card>
    </div>
    <div v-else-if="hasSearched && searchQuery && !isSearching && searchResults.length === 0" class="no-results">
      <n-text>未找到相关电影</n-text>
    </div>

    <n-modal v-model:show="showModal" preset="card" style="width: 1050px;" title="添加电影">
      <div v-if="modalData" class="modal-content">
        <div class="modal-left">
          <div class="poster-container">
            <n-image 
              v-if="modalData.poster_path" 
              :src="getImageUrl(modalData.poster_path)" 
              width="320"
              height="480"
              class="poster-image"
            />
          </div>
        </div>
        <div class="modal-right">
          <h2 class="modal-title">{{ modalData.display_name }}</h2>
          <n-form label-placement="left" label-width="120px">
            <n-form-item label="中文名">
              <n-input :value="modalData.title" disabled />
            </n-form-item>
            <n-form-item label="原名">
              <n-input :value="modalData.original_title" disabled />
            </n-form-item>
            <n-form-item label="类型">
              <n-space>
                <n-checkbox :checked="modalData.is_anime" @update:checked="(v) => { modalData.is_anime = v; }">动画电影</n-checkbox>
                <n-checkbox :checked="!modalData.is_anime" @update:checked="(v) => { if (v) { modalData.is_anime = false; } }">真人电影</n-checkbox>
              </n-space>
            </n-form-item>
            <n-form-item label="媒体地址">
              <n-input :value="modalData.media_path" disabled />
            </n-form-item>
          </n-form>
          <div class="modal-actions">
            <n-button @click="closeModal">取消</n-button>
            <n-button type="primary" @click="handleRemoteDownload">
              <template #icon>
                <n-icon><download-icon /></n-icon>
              </template>
              远程下载
            </n-button>
            <n-button type="primary" @click="handleManualUpload">
              <template #icon>
                <n-icon><upload-icon /></n-icon>
              </template>
              手动上传
            </n-button>
          </div>
        </div>
      </div>
    </n-modal>

    <n-modal v-model:show="showDownloadModal" preset="card" style="width: 500px;" title="远程下载">
      <div class="download-modal">
        <p>输入该电影的下载链接</p>
        <n-input 
          v-model:value="downloadLink" 
          type="textarea" 
          placeholder="请输入磁力链或HTTP链接" 
          :rows="3"
          style="margin: 10px 0;" 
        />
        <div class="modal-actions">
          <n-button @click="closeDownloadModal">取消</n-button>
          <n-button type="primary" @click="confirmDownload">确认</n-button>
        </div>
      </div>
    </n-modal>

    <n-modal v-model:show="showUploadModal" preset="card" style="width: 600px;" :title="`上传${modalData?.display_name || ''}`" @close="closeUploadModal">
      <div class="upload-modal">
        <div 
          class="upload-dropzone"
          v-if="!isUploadComplete"
          @drop="handleFileDrop"
          @dragover="preventDragDefaults"
          @dragenter="preventDragDefaults"
          @dragleave="preventDragDefaults"
          @click="(($refs.uploadFileInput) as HTMLInputElement)?.click()"
        >
          <input 
            ref="uploadFileInput"
            type="file"
            multiple
            style="display: none;"
            @change="handleFileSelect"
          />
          <div v-if="uploadFilesList.length === 0" class="dropzone-placeholder">
            <n-icon size="48"><upload-icon /></n-icon>
            <n-text style="color: #999; margin-top: 10px;">拖放文件至此上传</n-text>
          </div>
          <div v-else class="file-list">
            <div 
              v-for="(file, index) in uploadFilesList" 
              :key="index"
              class="file-item"
            >
              <span class="file-name">{{ file.name }}</span>
              <span class="file-size">{{ formatSize(file.size) }}</span>
              <n-button type="error" size="tiny" quaternary @click.stop="removeFile(index)" :disabled="isUploading">
                <template #icon>
                  <n-icon><close-icon /></n-icon>
                </template>
              </n-button>
            </div>
          </div>
        </div>
        <div v-else class="upload-complete-message">
          <n-icon size="48" color="#18a058"><checkmark-circle /></n-icon>
          <n-text style="color: #18a058; font-size: 16px; margin-top: 10px;">{{ uploadSuccessMessage }}</n-text>
        </div>
        <div v-if="isUploading" class="upload-progress-container">
          <n-progress 
            type="line" 
            :percentage="uploadProgress" 
            :show-indicator="true"
            style="width: 100%;"
          />
          <div style="display: flex; justify-content: space-between; width: 100%; margin-top: 8px;">
            <n-text style="color: #666; font-size: 12px;">
              {{ uploadProgress < 100 ? '上传中...' : '上传完成，处理中...' }}
            </n-text>
            <n-text style="color: #666; font-size: 12px;">
              速度: {{ uploadSpeed }}
            </n-text>
          </div>
        </div>
      </div>
      <template #footer>
        <div class="modal-actions" v-if="!isUploading">
          <n-button @click="closeUploadModal">关闭</n-button>
          <n-button 
            type="primary" 
            :disabled="uploadFilesList.length === 0 || isUploading"
            @click="startUpload"
            v-if="!isUploadComplete"
          >
            开始上传
          </n-button>
        </div>
        <div class="modal-actions" v-else>
          <n-button @click="closeUploadModal">隐藏</n-button>
          <n-button 
            type="error"
            @click="cancelUpload"
          >
            <template #icon>
              <n-icon><remove-icon /></n-icon>
            </template>
            取消上传
          </n-button>
        </div>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.movie-add-view {
  padding: 1rem;
  height: 100%;
}

.search-container {
  display: flex;
  margin-bottom: 2rem;
  width: 100%;
  max-width: 2500px;
}

.search-input {
  flex: 1;
  margin-right: 10px;
}

.search-button {
  white-space: nowrap;
}

.search-results {
  width: 100%;
  max-width: 2500px;
}

.result-content {
  display: flex;
  padding: 10px 0;
}

.result-image {
  margin-right: 20px;
  flex-shrink: 0;
}

.result-info {
  flex: 1;
}

.result-header {
  margin-bottom: 10px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.result-title {
  margin-bottom: 5px;
}

.result-original-name {
  display: flex;
  justify-content: space-between;
  margin-bottom: 10px;
  color: #666;
}

.result-date {
  margin-left: 10px;
}

.result-overview {
  line-height: 1.4;
  color: #333;
}

.no-results {
  margin-top: 2rem;
  text-align: center;
  color: #666;
}

.modal-content {
  display: flex;
  gap: 30px;
}

.modal-left {
  flex-shrink: 0;
}

.poster-container {
  padding: 8px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}

.poster-image {
  border-radius: 4px;
}

.modal-right {
  flex: 1;
}

.modal-title {
  margin-top: 0;
  margin-bottom: 30px;
  font-size: 26px;
  text-align: center;
  font-weight: 600;
}

.modal-actions {
  margin-top: 30px;
  display: flex;
  justify-content: flex-end;
  gap: 15px;
}

.download-modal {
  padding: 15px 0;
}

.upload-modal {
  padding: 10px 0;
}

.upload-dropzone {
  border: 2px dashed #d9d9d9;
  border-radius: 8px;
  padding: 40px 20px;
  text-align: center;
  cursor: pointer;
  transition: all 0.3s;
  min-height: 200px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.upload-dropzone:hover {
  border-color: #18a058;
  background-color: #f0fff4;
}

.dropzone-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.file-list {
  width: 100%;
  max-height: 300px;
  overflow-y: auto;
}

.file-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: #f5f5f5;
  border-radius: 6px;
  margin-bottom: 8px;
}

.file-name {
  flex: 1;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-right: 10px;
}

.file-size {
  color: #666;
  font-size: 12px;
  margin-right: 10px;
  flex-shrink: 0;
}

.upload-progress-container {
  margin-top: 20px;
  padding: 15px;
  background: #f5f5f5;
  border-radius: 6px;
  text-align: center;
}

.upload-complete-message {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  padding: 40px 20px;
}
</style>
