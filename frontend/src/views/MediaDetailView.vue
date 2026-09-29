<script lang="ts">
import { useThemeStore } from '@/stores/theme';
import axios from 'axios';
import { useRouter, useRoute } from 'vue-router';
import {
  TrashOutline as TrashIcon,
  CloseOutline as CloseIcon,
  ChevronForward as ChevronForwardIcon,
  CheckmarkCircleOutline as CheckIcon,
  EyeOutline as EyeIcon,
  EyeOffOutline as EyeOffIcon,
  SearchOutline as SearchIcon,
  DownloadOutline as DownloadIcon,
  CloudUploadOutline as UploadIcon,
  RemoveCircleOutline as RemoveIcon
} from '@vicons/ionicons5';
import { NButton, NModal, NInput, NForm, NFormItem, NIcon, NCard, NText, NSpace, NTag, NSpin, NProgress } from 'naive-ui';
import { useMessageStore } from '@/stores/message';
import { episodeDateKey, episodeTimestamp, type EpisodeInfo as EpisodeAirTimeInfo } from '@/utils/airtime';

interface SeasonInfo {
  name: string;
  episode_count: number;
  episodes: EpisodeInfo[];
  is_tracked: boolean;
  season_number: number;
}

interface EpisodeInfo extends EpisodeAirTimeInfo {
  name: string;
  exists: boolean;
  episode_number: number;
  season_number: number;
  tvdb_season_number?: number;
  tvdb_episode_number?: number;
}

interface JackettResult {
  Title?: string;
  Size?: number;
  CategoryDesc?: string;
  Guid?: string;
  MagnetUri?: string;
  Link?: string;
  Details?: string;
  Seeders?: number;
  Peers?: number;
  PublishDate?: string;
  IsMultiEpisode?: boolean;
}

type FilterTerm = string | string[];

interface MediaInfo {
  tmdbid: number;
  name: string;
  original_name: string;
  name_tw: string;
  poster_path?: string;
  optional_names: FilterTerm[];
  filter_names: FilterTerm[];
  media_path: string;
  number_of_seasons: number;
  seasons: SeasonInfo[];
  first_air_date?: string;
  display_name: string;
}

export default {
  setup() {
    const themeStore = useThemeStore();
    const router = useRouter();
    const route = useRoute();
    const messageStore = useMessageStore();
    return {
      themeStore,
      router,
      route,
      messageStore
    }
  },
  mounted() {
    this.fetchMediaDetail();
  },
  data() {
    return {
      media: null as MediaInfo | null,
      showDeleteModal: false,
      showOptionalNamesModal: false,
      showFilterNamesModal: false,
      showSearchResultsModal: false,
      showSeasonUploadModal: false,
      showEpisodeUploadModal: false,
      tempOptionalName: '',
      tempFilterName: '',
      localOptionalNames: [] as FilterTerm[],
      localFilterNames: [] as FilterTerm[],
      searchResults: [] as JackettResult[],
      isSearching: false,
      currentSearchEpisode: {
        season_number: 0,
        episode_number: 0
      },
      currentUploadSeason: {
        season_number: 0,
        name: ''
      },
      currentUploadEpisode: {
        season_number: 0,
        episode_number: 0
      },
      seasonUploadFiles: [] as File[],
      episodeUploadFiles: [] as File[],
      isUploading: false,
      uploadProgress: 0,
      currentUploadingFile: '',
      uploadSpeed: '0 KB/s',
      uploadedFileCount: 0,
      uploadSuccessMessage: '',
      isUploadComplete: false,
      uploadCancelSource: null as AbortController | null,
      expandedSeasons: new Set<number>(),
    };
  },
  components: {
    TrashIcon,
    CloseIcon,
    ChevronForwardIcon,
    CheckIcon,
    EyeIcon,
    EyeOffIcon,
    SearchIcon,
    DownloadIcon,
    UploadIcon,
    RemoveIcon,
    NButton,
    NModal,
    NInput,
    NForm,
    NFormItem,
    NIcon,
    NCard,
    NText,
    NSpace,
    NTag,
    NSpin,
    NProgress
  },
  methods: {
    formatEpisodeDate(episode: EpisodeInfo): string {
      return episodeDateKey(episode) || '未知';
    },
    formatEpisodeTime(episode: EpisodeInfo): string {
      const timestamp = episodeTimestamp(episode);
      if (timestamp === null) return '未知';
      const date = new Date(timestamp);
      return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
    },
    formatFilterTerm(term: FilterTerm): string {
      if (Array.isArray(term)) {
        return term.join(' ');
      }
      return term;
    },
    parseFilterInput(input: string): FilterTerm {
      const parts = input.trim().split(/\s+/);
      if (parts.length > 1) {
        return parts;
      }
      return input.trim();
    },
    async fetchMediaDetail() {
      try {
        const tmdbId = parseInt(this.route.params.tmdb_id as string);
        const response = await axios.get(`/v1/media/${tmdbId}`);
        this.media = response.data;
        if (this.media) {
          this.localOptionalNames = [...this.media.optional_names];
          this.localFilterNames = [...this.media.filter_names];
        }
      } catch (error) {
        console.error('获取媒体详情失败:', error);
      }
    },
    toggleSeason(seasonNumber: number) {
      if (this.expandedSeasons.has(seasonNumber)) {
        this.expandedSeasons.delete(seasonNumber);
      } else {
        this.expandedSeasons.add(seasonNumber);
      }
    },
    isSeasonExpanded(seasonNumber: number): boolean {
      return this.expandedSeasons.has(seasonNumber);
    },
    getImageUrl(posterPath?: string) {
      if (!posterPath) return '';
      const fileName = posterPath.split('/').pop();
      return `/webui/images/${fileName}`;
    },
    getYearFromPath(): string {
      if (!this.media) return '';
      const path = this.media.media_path;
      const match = path.match(/\((\d{4})\)/);
      if (match) {
        return match[1];
      }
      return '';
    },
    handleDeleteClick() {
      this.showDeleteModal = true;
    },
    async confirmDelete() {
      if (!this.media) return;
      try {
        await axios.post('/v1/media/delete', { tmdbid: this.media.tmdbid });
        this.messageStore.setMessage('删除成功', 'success');
        this.router.push('/library/media');
      } catch (error) {
        console.error('删除失败:', error);
        this.messageStore.setMessage('删除失败', 'error');
      }
    },
    handleOptionalNamesClick() {
      this.localOptionalNames = this.media ? [...this.media.optional_names] : [];
      this.tempOptionalName = '';
      this.showOptionalNamesModal = true;
    },
    addOptionalName() {
      if (this.tempOptionalName.trim()) {
        const parsed = this.parseFilterInput(this.tempOptionalName);
        const exists = this.localOptionalNames.some(term => {
          if (Array.isArray(term) && Array.isArray(parsed)) {
            return term.join(' ') === parsed.join(' ');
          }
          return term === parsed;
        });
        if (!exists) {
          this.localOptionalNames.push(parsed);
        }
        this.tempOptionalName = '';
      }
    },
    removeOptionalName(index: number) {
      this.localOptionalNames.splice(index, 1);
    },
    closeOptionalNamesModal() {
      this.showOptionalNamesModal = false;
    },
    async confirmOptionalNames() {
      if (!this.media) return;
      
      try {
        await axios.post('/v1/media/settings', {
          tmdbid: this.media.tmdbid,
          optional_names: this.localOptionalNames,
          filter_names: this.localFilterNames
        });
        this.media.optional_names = [...this.localOptionalNames];
        this.messageStore.setMessage('可选搜索名已更新', 'success');
        this.showOptionalNamesModal = false;
      } catch (error) {
        console.error('更新可选搜索名失败:', error);
        this.messageStore.setMessage('更新可选搜索名失败', 'error');
      }
    },
    handleFilterNamesClick() {
      this.localFilterNames = this.media ? [...this.media.filter_names] : [];
      this.tempFilterName = '';
      this.showFilterNamesModal = true;
    },
    addFilterName() {
      if (this.tempFilterName.trim()) {
        const parsed = this.parseFilterInput(this.tempFilterName);
        const exists = this.localFilterNames.some(term => {
          if (Array.isArray(term) && Array.isArray(parsed)) {
            return term.join(' ') === parsed.join(' ');
          }
          return term === parsed;
        });
        if (!exists) {
          this.localFilterNames.push(parsed);
        }
        this.tempFilterName = '';
      }
    },
    removeFilterName(index: number) {
      this.localFilterNames.splice(index, 1);
    },
    closeFilterNamesModal() {
      this.showFilterNamesModal = false;
    },
    async confirmFilterNames() {
      if (!this.media) return;
      
      try {
        await axios.post('/v1/media/settings', {
          tmdbid: this.media.tmdbid,
          optional_names: this.localOptionalNames,
          filter_names: this.localFilterNames
        });
        this.media.filter_names = [...this.localFilterNames];
        this.messageStore.setMessage('过滤搜索名已更新', 'success');
        this.showFilterNamesModal = false;
      } catch (error) {
        console.error('更新过滤搜索名失败:', error);
        this.messageStore.setMessage('更新过滤搜索名失败', 'error');
      }
    },
    async toggleSeasonTrack(sortedSeasonIndex: number) {
      if (!this.media) return;
      const sortedSeasons = this.getSortedSeasons();
      const targetSeason = sortedSeasons[sortedSeasonIndex];
      
      const originalIndex = this.media.seasons.findIndex(s => s.name === targetSeason.name);
      if (originalIndex === -1) return;
      
      const season = this.media.seasons[originalIndex];
      season.is_tracked = !season.is_tracked;
      
      try {
        const seasonsTracked = this.media.seasons.map(s => s.is_tracked);
        await axios.post('/v1/media/settings', {
          tmdbid: this.media.tmdbid,
          seasons_tracked: seasonsTracked
        });
        this.messageStore.setMessage('已更新追踪状态', 'success');
      } catch (error) {
        console.error('更新追踪状态失败:', error);
        season.is_tracked = !season.is_tracked;
        this.messageStore.setMessage('更新追踪状态失败', 'error');
      }
    },
    getSortedSeasons(): SeasonInfo[] {
      if (!this.media) return [];
      return [...this.media.seasons].sort((a, b) => {
        return b.season_number - a.season_number;
      });
    },
    extractSeasonNumber(name: string): number {
      const match = name.match(/第?(\d+)季?/);
      if (match) {
        return parseInt(match[1]);
      }
      const enMatch = name.match(/Season\s*(\d+)/i);
      if (enMatch) {
        return parseInt(enMatch[1]);
      }
      if (name.includes('第0季') || name.includes('Specials') || name.includes('特别篇')) {
        return 0;
      }
      return 999;
    },
    getSortedEpisodes(episodes: EpisodeInfo[]): EpisodeInfo[] {
      return [...episodes].reverse();
    },
    formatSize(bytes: number): string {
      if (bytes === 0) return '0 B';
      const k = 1024;
      const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(k));
      return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    },
    async handleSearchEpisode(seasonNumber: number, episodeNumber: number) {
      if (!this.media) return;
      
      this.currentSearchEpisode.season_number = seasonNumber;
      this.currentSearchEpisode.episode_number = episodeNumber;
      
      this.isSearching = true;
      this.searchResults = [];
      this.showSearchResultsModal = true;
      
      try {
        const response = await axios.post('/v1/resourcesearch', {
          tmdbid: this.media.tmdbid,
          season_number: seasonNumber,
          episode_number: episodeNumber
        });
        
        if (response.data.success && response.data.results) {
          this.searchResults = response.data.results;
        } else {
          this.messageStore.setMessage(response.data.message || '未找到搜索结果', 'warning');
        }
      } catch (error) {
        console.error('搜索失败:', error);
        this.messageStore.setMessage('搜索失败', 'error');
      } finally {
        this.isSearching = false;
      }
    },
    async handleDownload(result: JackettResult) {
      if (!this.media) return;
      if (!result.MagnetUri) {
        this.messageStore.setMessage('该搜索结果没有磁力链接', 'error');
        return;
      }
      
      try {
        const response = await axios.post('/v1/download/add', {
          tmdbid: this.media.tmdbid,
          season_number: this.currentSearchEpisode.season_number,
          episode_number: this.currentSearchEpisode.episode_number,
          magnet_uri: result.MagnetUri,
          media_path: this.media.media_path,
          is_multi_episode: result.IsMultiEpisode || false
        });
        
        if (response.data.success) {
          this.messageStore.setMessage(response.data.message, 'success');
        } else {
          this.messageStore.setMessage(response.data.message, 'error');
        }
      } catch (error) {
        console.error('添加下载失败:', error);
        this.messageStore.setMessage('添加下载失败', 'error');
      }
    },
    isMediaOrSubtitleFile(fileName: string): boolean {
      const mediaExtensions = ['flv', 'mkv', 'mp4', 'avi', 'rmvb', 'm2ts', 'wmv'];
      const subtitleExtensions = ['srt', 'ass', 'ssa', 'sub', 'smi', 'xml'];
      const ext = fileName.split('.').pop()?.toLowerCase() || '';
      return mediaExtensions.includes(ext) || subtitleExtensions.includes(ext);
    },
    handleSeasonFileDrop(event: DragEvent) {
      event.preventDefault();
      event.stopPropagation();
      if (event.dataTransfer && event.dataTransfer.files) {
        this.addSeasonFiles(Array.from(event.dataTransfer.files));
      }
    },
    handleEpisodeFileDrop(event: DragEvent) {
      event.preventDefault();
      event.stopPropagation();
      if (event.dataTransfer && event.dataTransfer.files) {
        this.addEpisodeFiles(Array.from(event.dataTransfer.files));
      }
    },
    handleSeasonFileSelect(event: Event) {
      const target = event.target as HTMLInputElement;
      if (target.files) {
        this.addSeasonFiles(Array.from(target.files));
      }
    },
    handleEpisodeFileSelect(event: Event) {
      const target = event.target as HTMLInputElement;
      if (target.files) {
        this.addEpisodeFiles(Array.from(target.files));
      }
    },
    addSeasonFiles(files: File[]) {
      for (const file of files) {
        if (this.isMediaOrSubtitleFile(file.name)) {
          if (!this.seasonUploadFiles.some(f => f.name === file.name)) {
            this.seasonUploadFiles.push(file);
          }
        }
      }
    },
    addEpisodeFiles(files: File[]) {
      for (const file of files) {
        if (this.isMediaOrSubtitleFile(file.name)) {
          if (!this.episodeUploadFiles.some(f => f.name === file.name)) {
            this.episodeUploadFiles.push(file);
          }
        }
      }
    },
    removeSeasonFile(index: number) {
      this.seasonUploadFiles.splice(index, 1);
    },
    removeEpisodeFile(index: number) {
      this.episodeUploadFiles.splice(index, 1);
    },
    async uploadSeasonFiles() {
      if (!this.media || this.seasonUploadFiles.length === 0) return;
      
      this.isUploading = true;
      this.uploadProgress = 0;
      this.uploadSuccessMessage = '';
      this.isUploadComplete = false;
      
      let uploadedSize = 0;
      const startTime = Date.now();
      
      this.uploadCancelSource = new AbortController();
      
      try {
        const formData = new FormData();
        formData.append('tmdbid', this.media.tmdbid.toString());
        formData.append('season_number', this.currentUploadSeason.season_number.toString());
        
        for (const file of this.seasonUploadFiles) {
          formData.append('file', file);
        }
        
        const response = await axios.post('/v1/upload/addseason', formData, {
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
    async uploadEpisodeFiles() {
      if (!this.media || this.episodeUploadFiles.length === 0) return;
      
      this.isUploading = true;
      this.uploadProgress = 0;
      this.uploadSuccessMessage = '';
      this.isUploadComplete = false;
      
      let uploadedSize = 0;
      const startTime = Date.now();
      
      this.uploadCancelSource = new AbortController();
      
      try {
        const formData = new FormData();
        formData.append('tmdbid', this.media.tmdbid.toString());
        formData.append('season_number', this.currentUploadEpisode.season_number.toString());
        formData.append('episode_number', this.currentUploadEpisode.episode_number.toString());
        
        for (const file of this.episodeUploadFiles) {
          formData.append('file', file);
        }
        
        const response = await axios.post('/v1/upload/addepisode', formData, {
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
    formatSpeed(bytesPerSecond: number): string {
      if (bytesPerSecond >= 1024 * 1024) {
        return (bytesPerSecond / (1024 * 1024)).toFixed(2) + ' MB/s';
      } else if (bytesPerSecond >= 1024) {
        return (bytesPerSecond / 1024).toFixed(2) + ' KB/s';
      } else {
        return bytesPerSecond.toFixed(2) + ' B/s';
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
    },
    handleSeasonUploadClick(season: SeasonInfo) {
      this.currentUploadSeason = {
        season_number: season.season_number,
        name: season.name
      };
      this.seasonUploadFiles = [];
      this.resetUploadState();
      this.showSeasonUploadModal = true;
    },
    handleEpisodeUploadClick(seasonNumber: number, episodeNumber: number) {
      this.currentUploadEpisode = {
        season_number: seasonNumber,
        episode_number: episodeNumber
      };
      this.currentUploadSeason = {
        season_number: seasonNumber,
        name: ''
      };
      this.episodeUploadFiles = [];
      this.resetUploadState();
      this.showEpisodeUploadModal = true;
    },
    hideSeasonUploadModal() {
      this.showSeasonUploadModal = false;
    },
    hideEpisodeUploadModal() {
      this.showEpisodeUploadModal = false;
    },
    preventDragDefaults(event: DragEvent) {
      event.preventDefault();
      event.stopPropagation();
    }
  }
}
</script>

<template>
  <div v-if="media" class="media-detail-view">
    <div class="detail-header">
      <div class="poster-section">
        <img 
          :src="getImageUrl(media.poster_path)" 
          :alt="media.name" 
          class="detail-poster"
        />
      </div>
      <div class="info-section">
        <div class="title-row">
          <h1 class="media-title">
            {{ media.display_name || media.name }}
          </h1>
          <n-button 
            quaternary 
            type="error" 
            @click="handleDeleteClick"
            class="delete-button"
          >
            <template #icon>
              <n-icon><trash-icon /></n-icon>
            </template>
          </n-button>
        </div>
        
        <n-form label-placement="left" label-width="120px" class="detail-form">
          <n-form-item label="原名">
            <n-input :value="media.original_name" disabled />
          </n-form-item>
          <n-form-item label="可选搜索名">
            <div class="optional-names-container">
              <n-input-group class="optional-names-input-group">
                <n-input :value="media.optional_names.map(term => formatFilterTerm(term)).join(' | ')" disabled placeholder="" />
                <n-button 
                  type="primary" 
                  @click="handleOptionalNamesClick"
                  class="add-name-button"
                >
                  +
                </n-button>
              </n-input-group>
            </div>
          </n-form-item>
          <n-form-item label="过滤搜索名">
            <div class="optional-names-container">
              <n-input-group class="optional-names-input-group">
                <n-input :value="media.filter_names.map(term => formatFilterTerm(term)).join(' | ')" disabled placeholder="" />
                <n-button 
                  type="primary" 
                  @click="handleFilterNamesClick"
                  class="add-name-button"
                >
                  +
                </n-button>
              </n-input-group>
            </div>
          </n-form-item>
          <n-form-item label="媒体库地址">
            <n-input :value="media.media_path" disabled />
          </n-form-item>
        </n-form>
      </div>
    </div>
    
    <div class="seasons-section">
      <n-card 
        v-for="(season, index) in getSortedSeasons()" 
        :key="index"
        class="season-card"
      >
        <template #header>
          <div class="season-header" @click="toggleSeason(season.season_number)" style="cursor: pointer;">
            <div class="season-title-group">
              <n-icon size="20" style="margin-right: 8px; transition: transform 0.2s;" :style="{ transform: isSeasonExpanded(season.season_number) ? 'rotate(90deg)' : 'rotate(0deg)' }">
                <ChevronForwardIcon />
              </n-icon>
              <n-text strong>{{ season.name }}</n-text>
              <n-tag size="small" type="info">{{ season.episode_count }} 集</n-tag>
              <n-button 
                quaternary 
                size="small"
                @click.stop="handleSeasonUploadClick(season)"
                title="上传本季度文件"
              >
                <template #icon>
                  <n-icon><upload-icon /></n-icon>
                </template>
              </n-button>
            </div>
            <n-button 
              :type="season.is_tracked ? 'primary' : 'default'"
              quaternary
              @click.stop="toggleSeasonTrack(getSortedSeasons().indexOf(season))"
            >
              <template #icon>
                <n-icon>
                  <eye-icon v-if="season.is_tracked" />
                  <eye-off-icon v-else />
                </n-icon>
              </template>
              {{ season.is_tracked ? '追踪中' : '未追踪' }}
            </n-button>
          </div>
        </template>
        
        <div v-if="isSeasonExpanded(season.season_number)" class="episodes-list">
          <div 
            v-for="(episode, epIndex) in getSortedEpisodes(season.episodes)" 
            :key="epIndex"
            class="episode-item"
          >
            <div class="episode-number">{{ episode.episode_number }}</div>
            <div class="episode-info">
              <div class="episode-name">{{ episode.name }}</div>
              <div class="episode-date">
                {{ formatEpisodeDate(episode) }} · 播出时间：{{ formatEpisodeTime(episode) }}
              </div>
            </div>
            <div class="episode-actions">
              <n-button 
                v-if="extractSeasonNumber(season.name) !== 0"
                quaternary 
                size="tiny"
                title="点击进行手动搜索"
                @click="handleSearchEpisode(episode.season_number, episode.episode_number)"
              >
                <template #icon>
                  <n-icon><search-icon /></n-icon>
                </template>
                <span class="tooltip-text">手动搜索该集</span>
              </n-button>
              <n-button 
                quaternary 
                size="tiny"
                title="上传本集文件"
                @click="handleEpisodeUploadClick(episode.season_number, episode.episode_number)"
              >
                <template #icon>
                  <n-icon><upload-icon /></n-icon>
                </template>
              </n-button>
              <n-icon>
                <check-icon 
                  :color="episode.exists ? themeStore.theme.common?.primaryColor : '#ccc'" 
                />
              </n-icon>
            </div>
          </div>
        </div>
      </n-card>
    </div>
    
    <n-modal v-model:show="showDeleteModal" preset="card" style="width: 400px;" title="确认删除">
      <p>确定要删除 "{{ media.name }}" 吗？此操作不可恢复。</p>
      <div class="modal-actions">
        <n-button @click="showDeleteModal = false">取消</n-button>
        <n-button type="error" @click="confirmDelete">确认删除</n-button>
      </div>
    </n-modal>
    
    <n-modal v-model:show="showOptionalNamesModal" preset="card" style="width: 500px;" title="可选搜索名增减">
      <div class="optional-names-modal">
        <n-input-group class="add-name-input-group">
          <n-input v-model:value="tempOptionalName" placeholder="输入可选搜索名（多个词用空格分隔）" @keyup.enter="addOptionalName" />
          <n-button type="primary" @click="addOptionalName">添加</n-button>
        </n-input-group>
        
        <div class="optional-names-list">
          <div 
            v-for="(name, index) in localOptionalNames" 
            :key="index"
            class="optional-name-item"
          >
            <span class="name-text">{{ formatFilterTerm(name) }}</span>
            <n-button type="error" size="tiny" quaternary @click="removeOptionalName(index)">
              <template #icon>
                <n-icon><trash-icon /></n-icon>
              </template>
            </n-button>
          </div>
        </div>
      </div>
      <template #footer>
        <n-button @click="closeOptionalNamesModal">关闭</n-button>
        <n-button type="primary" @click="confirmOptionalNames">确认</n-button>
      </template>
    </n-modal>

    <n-modal v-model:show="showFilterNamesModal" preset="card" style="width: 500px;" title="过滤搜索名增减">
      <div class="optional-names-modal">
        <n-input-group class="add-name-input-group">
          <n-input v-model:value="tempFilterName" placeholder="输入过滤搜索名（多个词用空格分隔）" @keyup.enter="addFilterName" />
          <n-button type="primary" @click="addFilterName">添加</n-button>
        </n-input-group>
        
        <div class="optional-names-list">
          <div 
            v-for="(name, index) in localFilterNames" 
            :key="'filter-' + index"
            class="optional-name-item"
          >
            <span class="name-text">{{ formatFilterTerm(name) }}</span>
            <n-button type="error" size="tiny" quaternary @click="removeFilterName(index)">
              <template #icon>
                <n-icon><trash-icon /></n-icon>
              </template>
            </n-button>
          </div>
        </div>
      </div>
      <template #footer>
        <n-button @click="closeFilterNamesModal">关闭</n-button>
        <n-button type="primary" @click="confirmFilterNames">确认</n-button>
      </template>
    </n-modal>

    <n-modal v-model:show="showSearchResultsModal" preset="card" style="width: 800px; max-height: 80vh;" title="搜索结果">
      <div v-if="isSearching" class="search-loading">
        <n-spin size="large" />
        <n-text style="margin-left: 10px;">搜索中...</n-text>
      </div>
      <div v-else-if="searchResults.length === 0" class="search-empty">
        <n-text>未找到搜索结果</n-text>
      </div>
      <div v-else class="search-results-list">
        <div 
          v-for="(result, index) in searchResults" 
          :key="index"
          class="search-result-item"
        >
          <div class="result-info">
              <div class="result-title-row">
                <span v-if="result.IsMultiEpisode" class="multi-episode-badge">多集资源</span>
                <div class="result-title">{{ result.Title || '未知标题' }}</div>
              </div>
              <div class="result-meta">
                <n-tag size="small" v-if="result.Size">{{ formatSize(result.Size) }}</n-tag>
                <n-tag size="small" v-if="result.PublishDate">{{ result.PublishDate }}</n-tag>
              </div>
            </div>
          <n-button quaternary size="tiny" @click="handleDownload(result)">
            <template #icon>
              <n-icon><download-icon /></n-icon>
            </template>
          </n-button>
        </div>
      </div>
      <template #footer>
        <n-button @click="showSearchResultsModal = false">关闭</n-button>
      </template>
    </n-modal>

    <n-modal v-model:show="showSeasonUploadModal" preset="card" style="width: 600px;" :title="`上传${media?.display_name || ''}${currentUploadSeason.name}`" @close="hideSeasonUploadModal">
      <div class="upload-modal">
        <div 
          class="upload-dropzone"
          v-if="!isUploadComplete"
          @drop="handleSeasonFileDrop"
          @dragover="preventDragDefaults"
          @dragenter="preventDragDefaults"
          @dragleave="preventDragDefaults"
          @click="(($refs.seasonFileInput) as HTMLInputElement)?.click()"
        >
          <input 
            ref="seasonFileInput"
            type="file"
            multiple
            style="display: none;"
            @change="handleSeasonFileSelect"
          />
          <div v-if="seasonUploadFiles.length === 0" class="dropzone-placeholder">
            <n-icon size="48"><upload-icon /></n-icon>
            <n-text style="color: #999; margin-top: 10px;">拖放文件至此上传</n-text>
          </div>
          <div v-else class="file-list">
            <div 
              v-for="(file, index) in seasonUploadFiles" 
              :key="index"
              class="file-item"
            >
              <span class="file-name">{{ file.name }}</span>
              <span class="file-size">{{ formatSize(file.size) }}</span>
              <n-button type="error" size="tiny" quaternary @click.stop="removeSeasonFile(index)" :disabled="isUploading">
                <template #icon>
                  <n-icon><close-icon /></n-icon>
                </template>
              </n-button>
            </div>
          </div>
        </div>
        <div v-else class="upload-complete-message">
          <n-icon size="48" color="#18a058"><check-icon /></n-icon>
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
          <n-button @click="hideSeasonUploadModal">关闭</n-button>
          <n-button 
            type="primary" 
            :disabled="seasonUploadFiles.length === 0 || isUploading"
            @click="uploadSeasonFiles"
            v-if="!isUploadComplete"
          >
            开始上传
          </n-button>
        </div>
        <div class="modal-actions" v-else>
          <n-button @click="hideSeasonUploadModal">隐藏</n-button>
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

    <n-modal v-model:show="showEpisodeUploadModal" preset="card" style="width: 600px;" :title="`上传${media?.display_name || ''}第${currentUploadEpisode.episode_number}集`" @close="hideEpisodeUploadModal">
      <div class="upload-modal">
        <div 
          class="upload-dropzone"
          v-if="!isUploadComplete"
          @drop="handleEpisodeFileDrop"
          @dragover="preventDragDefaults"
          @dragenter="preventDragDefaults"
          @dragleave="preventDragDefaults"
          @click="(($refs.episodeFileInput) as HTMLInputElement)?.click()"
        >
          <input 
            ref="episodeFileInput"
            type="file"
            multiple
            style="display: none;"
            @change="handleEpisodeFileSelect"
          />
          <div v-if="episodeUploadFiles.length === 0" class="dropzone-placeholder">
            <n-icon size="48"><upload-icon /></n-icon>
            <n-text style="color: #999; margin-top: 10px;">拖放文件至此上传</n-text>
          </div>
          <div v-else class="file-list">
            <div 
              v-for="(file, index) in episodeUploadFiles" 
              :key="index"
              class="file-item"
            >
              <span class="file-name">{{ file.name }}</span>
              <span class="file-size">{{ formatSize(file.size) }}</span>
              <n-button type="error" size="tiny" quaternary @click.stop="removeEpisodeFile(index)" :disabled="isUploading">
                <template #icon>
                  <n-icon><close-icon /></n-icon>
                </template>
              </n-button>
            </div>
          </div>
        </div>
        <div v-else class="upload-complete-message">
          <n-icon size="48" color="#18a058"><check-icon /></n-icon>
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
          <n-button @click="hideEpisodeUploadModal">关闭</n-button>
          <n-button 
            type="primary" 
            :disabled="episodeUploadFiles.length === 0 || isUploading"
            @click="uploadEpisodeFiles"
            v-if="!isUploadComplete"
          >
            开始上传
          </n-button>
        </div>
        <div class="modal-actions" v-else>
          <n-button @click="hideEpisodeUploadModal">隐藏</n-button>
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
  
  <div v-else class="loading-container">
    <n-text>加载中...</n-text>
  </div>
</template>

<style scoped>
.media-detail-view {
  padding: 1.5rem;
  height: 100%;
  overflow-y: auto;
}

.detail-header {
  display: flex;
  gap: 2rem;
  margin-bottom: 2rem;
}

.poster-section {
  flex-shrink: 0;
}

.detail-poster {
  width: 300px;
  border-radius: 8px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}

.info-section {
  flex: 1;
}

.title-row {
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 1.5rem;
  position: relative;
}

.media-title {
  font-size: 36px;
  font-weight: 600;
  margin: 0;
  text-align: center;
}

.delete-button {
  padding: 8px 12px;
  position: absolute;
  right: 0;
}

.detail-form {
  max-width: 600px;
}

.optional-names-container {
  width: 100%;
}

.optional-names-input-group {
  display: flex;
  flex-wrap: nowrap;
}

.add-name-button {
  font-size: 22px;
  font-weight: bold;
  padding: 0 14px;
  min-width: auto;
  height: 34px;
  flex-shrink: 0;
}

.seasons-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.season-card {
  margin-bottom: 0;
}

.season-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.episodes-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.episode-item {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 8px 12px;
  background: #f5f5f5;
  border-radius: 6px;
}

.episode-number {
  width: 40px;
  font-weight: 600;
  color: #666;
}

.episode-info {
  flex: 1;
}

.episode-name {
  font-weight: 500;
  margin-bottom: 2px;
}

.episode-date {
  font-size: 12px;
  color: #888;
}

.episode-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.tooltip-text {
  position: absolute;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.2s;
}

.search-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px;
}

.search-empty {
  display: flex;
  justify-content: center;
  padding: 40px;
  color: #888;
}

.search-results-list {
  max-height: 50vh;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.search-result-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px;
  background: #f5f5f5;
  border-radius: 6px;
}

.result-info {
  flex: 1;
  margin-right: 12px;
  overflow: hidden;
}

.result-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.result-title {
  font-weight: 500;
  white-space: normal;
  word-wrap: break-word;
}

.multi-episode-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 2px 6px;
  font-size: 10px;
  font-weight: 600;
  color: #fff;
  background-color: #18a058;
  border-radius: 2px;
  flex-shrink: 0;
}

.result-meta {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.modal-actions {
  margin-top: 20px;
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.optional-names-modal {
  padding: 10px 0;
}

.add-name-input-group {
  display: flex;
  flex-wrap: nowrap;
  margin-bottom: 15px;
}

.optional-names-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.optional-name-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: #f5f5f5;
  border-radius: 6px;
}

.name-text {
  flex: 1;
}

.loading-container {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
}

.season-title-group {
  display: flex;
  align-items: center;
  gap: 8px;
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
