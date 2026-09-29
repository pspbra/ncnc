<script lang="ts">
import { NInput, NButton, NCard, NImage, NSpace, NText, NModal, NForm, NFormItem, NCheckbox, NInputGroup, NTag, NIcon } from 'naive-ui';
import { useRouter } from 'vue-router';
import { SearchOutline as SearchIcon, CheckmarkCircle, TrashOutline as TrashIcon } from '@vicons/ionicons5';
import axios from 'axios';
import { useMessageStore } from '@/stores/message';
import { useThemeStore } from '@/stores/theme';

type FilterTerm = string | string[];

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
    NInputGroup,
    TrashIcon,
    NTag,
    NIcon,
    CheckmarkCircle
  },
  setup() {
    const router = useRouter();
    const messageStore = useMessageStore();
    const themeStore = useThemeStore();
    return {
      router,
      messageStore,
      themeStore
    }
  },
  mounted() {
    this.fetchSettings();
  },
  data() {
    return {
      searchQuery: '',
      searchResults: [] as any[],
      isSearching: false,
      hasSearched: false,
      showModal: false,
      modalData: null as any,
      tempOptionalName: '',
      tempFilterName: '',
      showAddNameModal: false,
      showAddFilterNameModal: false,
      media_library_path: '',
      localOptionalNames: [] as FilterTerm[],
      localFilterNames: [] as FilterTerm[]
    }
  },
  methods: {
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
      
      if (!this.media_library_path) {
        await this.fetchSettings();
      }
      
      this.isSearching = true;
      this.hasSearched = true;
      try {
        const response = await axios.post('/v1/search', { query: this.searchQuery });
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
      const illegalChars = /[/:*?"<>|]/g;
      return name.replace(illegalChars, ' ');
    },
    async handleResultClick(result: any) {
      if (result.is_in_library) {
        result._flashing = true;
        setTimeout(() => {
          result._flashing = false;
        }, 500);
        return;
      }
      
      if (!this.media_library_path) {
        await this.fetchSettings();
      }
      
      const isAnime = result.genre_ids && result.genre_ids.includes(16);
      const year = result.first_air_date ? result.first_air_date.substring(0, 4) : '';
      const type = isAnime ? 'Anime' : 'TV drama';
      const sanitizedName = this.sanitizeFileName(result.name);
      const displayNameWithYear = year 
        ? `${sanitizedName} (${year})`
        : sanitizedName;
      const mediaPath = year 
        ? `${this.media_library_path}/media/${type}/${displayNameWithYear}`
        : `${this.media_library_path}/media/${type}/${sanitizedName}`;
      
      this.modalData = {
        id: result.id,
        name: result.name,
        original_name: result.original_name,
        poster_path: result.poster_path,
        optional_names: [],
        filter_names: [],
        is_anime: isAnime,
        number_of_seasons: 0,
        media_library_path: this.media_library_path,
        year: year,
        media_path: mediaPath,
        display_name: displayNameWithYear
      };
      
      this.localOptionalNames = [];
      this.localFilterNames = [];
      this.showModal = true;
    },
    closeModal() {
      this.showModal = false;
      this.modalData = null;
    },
    handleAddNameClick() {
      this.tempOptionalName = '';
      this.showAddNameModal = true;
    },
    confirmAddName() {
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
      }
      this.showAddNameModal = false;
    },
    cancelAddName() {
      this.showAddNameModal = false;
    },
    removeOptionalName(index: number) {
      this.localOptionalNames.splice(index, 1);
    },
    handleAddFilterNameClick() {
      this.tempFilterName = '';
      this.showAddFilterNameModal = true;
    },
    confirmAddFilterName() {
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
      }
      this.showAddFilterNameModal = false;
    },
    cancelAddFilterName() {
      this.showAddFilterNameModal = false;
    },
    removeFilterName(index: number) {
      this.localFilterNames.splice(index, 1);
    },
    updateMediaPath() {
      if (this.modalData) {
        const type = this.modalData.is_anime ? 'Anime' : 'TV drama';
        const sanitizedName = this.sanitizeFileName(this.modalData.name);
        this.modalData.display_name = this.modalData.year 
          ? `${sanitizedName} (${this.modalData.year})`
          : sanitizedName;
        this.modalData.media_path = `${this.modalData.media_library_path}/media/${type}/${this.modalData.display_name}`;
      }
    },
    async handleSubscribe() {
      if (!this.modalData) return;
      
      try {
        const requestData = {
          tmdbid: this.modalData.id,
          name: this.modalData.name,
          original_name: this.modalData.original_name,
          name_tw: '',
          poster_path: this.modalData.poster_path,
          optional_names: this.localOptionalNames,
          filter_names: this.localFilterNames,
          media_path: this.modalData.media_path,
          is_anime: this.modalData.is_anime,
          display_name: this.modalData.display_name
        };
        
        const mediaId = this.modalData.id;
        await axios.post('/v1/subscribe', requestData);
        this.closeModal();
        
        const result = this.searchResults.find(r => r.id === mediaId);
        if (result) {
          result.is_in_library = true;
        }
        
        this.messageStore.setMessage('订阅成功', 'success');
      } catch (error) {
        console.error('订阅失败:', error);
        this.messageStore.setMessage('订阅失败', 'error');
      }
    }
  }
}

</script>


<template>
  <div class="subscribe-view">
    <div class="search-container">
      <n-input 
        v-model:value="searchQuery" 
        placeholder="请输入搜索内容" 
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
        style="margin-bottom: 20px; cursor: pointer; position: relative;"
        @click="handleResultClick(result)"
      >
        <template #header>
          <div class="result-header">
            <n-text strong>{{ result.name }}</n-text>
            <div v-if="result.is_in_library" class="in-library-badge" :class="{ 'badge-flash': result._flashing }">
              <n-tag type="primary" size="small">
                <template #icon>
                  <n-icon><CheckmarkCircle /></n-icon>
                </template>
                已在媒体库
              </n-tag>
            </div>
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
              <n-text strong>{{ result.name }}</n-text>
            </div>
            <div class="result-original-name">
              <n-text size="small">{{ result.original_name }}</n-text>
              <n-text size="small" class="result-date">播出时间：{{ result.first_air_date }}</n-text>
            </div>
            <div class="result-overview">
              <n-text size="small">{{ result.overview }}</n-text>
            </div>
          </div>
        </div>
      </n-card>
    </div>
    <div v-else-if="hasSearched && searchQuery && !isSearching && searchResults.length === 0" class="no-results">
      <n-text>未找到相关剧集</n-text>
    </div>

    <n-modal v-model:show="showModal" preset="card" style="width: 1050px;" title="添加到媒体库">
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
              <n-input :value="modalData.name" disabled />
            </n-form-item>
            <n-form-item label="原名">
              <n-input :value="modalData.original_name" disabled />
            </n-form-item>
            <n-form-item label="TMDB ID">
              <n-input :value="modalData.id.toString()" disabled />
            </n-form-item>
            <n-form-item label="可选搜索名">
              <div class="optional-names-container">
                <n-input-group class="optional-names-input-group">
                  <n-input :value="localOptionalNames.map(term => formatFilterTerm(term)).join(' | ')" disabled placeholder="" />
                  <n-button 
                    type="primary" 
                    @click="handleAddNameClick"
                    class="add-name-button"
                  >
                    +
                  </n-button>
                </n-input-group>
                <div class="optional-names-list">
                  <div 
                    v-for="(name, index) in localOptionalNames" 
                    :key="index" 
                    class="optional-name-item"
                  >
                    <span class="optional-name-text">{{ formatFilterTerm(name) }}</span>
                    <n-button 
                      type="error" 
                      size="tiny" 
                      quaternary
                      @click="removeOptionalName(index)"
                    >
                      <template #icon>
                        <n-icon><TrashIcon /></n-icon>
                      </template>
                    </n-button>
                  </div>
                </div>
              </div>
            </n-form-item>
            <n-form-item label="过滤搜索名">
              <div class="optional-names-container">
                <n-input-group class="optional-names-input-group">
                  <n-input :value="localFilterNames.map(term => formatFilterTerm(term)).join(' | ')" disabled placeholder="" />
                  <n-button 
                    type="primary" 
                    @click="handleAddFilterNameClick"
                    class="add-name-button"
                  >
                    +
                  </n-button>
                </n-input-group>
                <div class="optional-names-list">
                  <div 
                    v-for="(name, index) in localFilterNames" 
                    :key="'filter-' + index" 
                    class="optional-name-item"
                  >
                    <span class="optional-name-text">{{ formatFilterTerm(name) }}</span>
                    <n-button 
                      type="error" 
                      size="tiny" 
                      quaternary
                      @click="removeFilterName(index)"
                    >
                      <template #icon>
                        <n-icon><TrashIcon /></n-icon>
                      </template>
                    </n-button>
                  </div>
                </div>
              </div>
            </n-form-item>
            <n-form-item label="类型">
              <n-space>
                <n-checkbox :checked="modalData.is_anime" @update:checked="(v) => { modalData.is_anime = v; updateMediaPath(); }">动画</n-checkbox>
                <n-checkbox :checked="!modalData.is_anime" @update:checked="(v) => { if (v) { modalData.is_anime = false; updateMediaPath(); } }">影视剧</n-checkbox>
              </n-space>
            </n-form-item>
            <n-form-item label="媒体地址">
              <n-input :value="modalData.media_path" disabled />
            </n-form-item>
          </n-form>
          <div class="modal-actions">
            <n-button @click="closeModal">取消</n-button>
            <n-button type="primary" @click="handleSubscribe">确认添加</n-button>
          </div>
        </div>
      </div>
    </n-modal>

    <n-modal v-model:show="showAddNameModal" preset="card" style="width: 500px;" title="可选搜索名增减">
      <div class="optional-names-modal">
        <n-input-group class="add-name-input-group">
          <n-input v-model:value="tempOptionalName" placeholder="输入可选搜索名（多个词用空格分隔）" @keyup.enter="confirmAddName" />
          <n-button type="primary" @click="confirmAddName">添加</n-button>
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
        <n-button @click="cancelAddName">关闭</n-button>
        <n-button type="primary" @click="confirmAddName">确认</n-button>
      </template>
    </n-modal>

    <n-modal v-model:show="showAddFilterNameModal" preset="card" style="width: 500px;" title="过滤搜索名增减">
      <div class="optional-names-modal">
        <n-input-group class="add-name-input-group">
          <n-input v-model:value="tempFilterName" placeholder="输入过滤搜索名（多个词用空格分隔）" @keyup.enter="confirmAddFilterName" />
          <n-button type="primary" @click="confirmAddFilterName">添加</n-button>
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
        <n-button @click="cancelAddFilterName">关闭</n-button>
        <n-button type="primary" @click="confirmAddFilterName">确认</n-button>
      </template>
    </n-modal>
  </div>
</template>


<style scoped>
.subscribe-view {
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

.in-library-badge {
  flex-shrink: 0;
}

.badge-flash {
  animation: badge-flash 0.5s ease-in-out;
}

@keyframes badge-flash {
  0%, 100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(1.15);
    opacity: 0.7;
  }
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

.optional-names-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 12px;
}

.optional-name-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background-color: #f5f5f5;
  border-radius: 4px;
}

.optional-name-text {
  flex: 1;
  color: #333;
}

.optional-names-modal {
  padding: 10px 0;
}

.add-name-input-group {
  display: flex;
  flex-wrap: nowrap;
  margin-bottom: 15px;
}

.name-text {
  flex: 1;
}
</style>
