<script lang="ts">
import { useThemeStore } from "@/stores/theme";
import axios from "axios";
import { NEllipsis, NIcon, NInput } from "naive-ui";
import { useRouter } from "vue-router";
import { RefreshOutline } from "@vicons/ionicons5";

import {
  sortMediaByAirTime,
  isLatestSeasonComplete,
  episodeTimestamp,
  episodeDateKey,
  localDateKey,
  type MediaInfo,
  type FilterTerm,
} from "@/utils/airtime";

export default {
  setup() {
    const themeStore = useThemeStore();
    const router = useRouter();
    return {
      themeStore,
      router,
      RefreshOutline,
    };
  },
  mounted() {
    this.fetchMedia();
    this.clockTimer = window.setInterval(() => {
      this.now = Date.now();
    }, 1000);
  },
  beforeUnmount() {
    window.clearInterval(this.clockTimer);
  },
  data() {
    return {
      media: [] as MediaInfo[],
      now: Date.now(),
      clockTimer: undefined as number | undefined,
      refreshing: new Set<number>(),
      searchQuery: "",
    };
  },
  components: {
    NEllipsis,
    NIcon,
    NInput,
  },
  computed: {
    filteredMedia(): MediaInfo[] {
      let result = [...this.media];

      if (this.searchQuery) {
        const query = this.searchQuery.toLowerCase();
        result = result.filter((m) => {
          return (
            m.name.toLowerCase().includes(query) ||
            m.name_tw.toLowerCase().includes(query) ||
            m.original_name.toLowerCase().includes(query) ||
            m.optional_names.some((n) => {
              const formattedTerm = this.formatFilterTerm(n).toLowerCase();
              return formattedTerm.includes(query);
            })
          );
        });
      }

      return sortMediaByAirTime(result, this.now);
    },
  },
  methods: {
    isLatestSeasonComplete,
    formatFilterTerm(term: FilterTerm): string {
      if (Array.isArray(term)) {
        return term.join(" ");
      }
      return term;
    },
    async fetchMedia() {
      try {
        const response = await axios.get("/v1/media/all");
        this.media = response.data || [];
      } catch (error) {
        console.error("获取媒体库失败:", error);
        this.media = [];
      }
    },
    async refreshMedia(media: MediaInfo, event: Event) {
      event.stopPropagation();
      if (this.refreshing.has(media.tmdbid)) return;

      this.refreshing.add(media.tmdbid);
      try {
        await axios.get(`/v1/update-single/${media.tmdbid}`);
        await this.fetchMedia();
      } catch (error) {
        console.error("刷新媒体失败:", error);
      } finally {
        this.refreshing.delete(media.tmdbid);
      }
    },
    calculateProgress(media: MediaInfo) {
      let totalAired = 0;
      let totalExists = 0;
      const today = localDateKey(new Date(this.now));

      for (const season of media.seasons) {
        if (season.is_tracked) {
          for (const episode of season.episodes) {
            const timestamp = episodeTimestamp(episode);
            const date = episodeDateKey(episode);
            const aired =
              timestamp !== null
                ? timestamp <= this.now
                : date !== null && date <= today;
            if (aired) {
              totalAired++;
              if (episode.exists) totalExists++;
            }
          }
        }
      }

      return { totalAired, totalExists };
    },
    getProgressPercentage(media: MediaInfo) {
      const { totalAired, totalExists } = this.calculateProgress(media);
      if (totalAired === 0) return 0;
      return (totalExists / totalAired) * 100;
    },
    getImageUrl(posterPath?: string | null) {
      if (!posterPath) return "";
      const fileName = posterPath.split("/").pop();
      return `/webui/images/${fileName}`;
    },
    goToMediaDetail(media: MediaInfo) {
      this.router.push(`/library/media/${media.tmdbid}`);
    },
  },
};
</script>

<template>
  <div class="media-view">
    <div class="search-container">
      <n-input
        v-model:value="searchQuery"
        placeholder="搜索媒体"
        clearable
        class="search-input"
      />
    </div>
    <div class="media-grid">
      <div v-if="filteredMedia.length === 0" class="day-empty">暂无媒体</div>
      <div
        v-for="m in filteredMedia"
        :key="m.tmdbid"
        class="media-card"
        @click="goToMediaDetail(m)"
      >
        <div class="poster-wrapper">
          <img
            :src="getImageUrl(m.poster_path)"
            :alt="m.name"
            class="poster-image"
          />
          <span
            v-if="isLatestSeasonComplete(m, now)"
            class="season-complete-badge"
            :style="{
              backgroundColor: themeStore.theme.common?.primaryColor,
              borderColor: themeStore.theme.common?.primaryColor,
            }"
          >季度完结</span>
          <button
            type="button"
            class="refresh-button"
            :class="{ 'is-refreshing': refreshing.has(m.tmdbid) }"
            :disabled="refreshing.has(m.tmdbid)"
            :aria-busy="refreshing.has(m.tmdbid)"
            :aria-label="`${refreshing.has(m.tmdbid) ? '正在刷新' : '刷新'} ${m.name}`"
            :title="refreshing.has(m.tmdbid) ? '正在刷新…' : '刷新媒体信息'"
            @click="refreshMedia(m, $event)"
          >
            <span
              class="refresh-icon"
              :class="{ 'is-spinning': refreshing.has(m.tmdbid) }"
            >
              <n-icon :component="RefreshOutline" :size="18" />
            </span>
          </button>
          <div class="progress-overlay">
            <div class="progress-bar-container">
              <div
                class="progress-bar"
                :style="{
                  width: getProgressPercentage(m) + '%',
                  backgroundColor:
                    getProgressPercentage(m) === 100
                      ? themeStore.theme.common?.primaryColor
                      : '#e74c3c',
                }"
              ></div>
              <div class="progress-text">
                {{ calculateProgress(m).totalExists }} /
                {{ calculateProgress(m).totalAired }}
              </div>
            </div>
          </div>
        </div>
        <div class="media-title">
          <n-ellipsis line-clamp="1">
            {{ m.name }}
          </n-ellipsis>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.media-view {
  position: relative;
  z-index: 0;
  padding: 1rem;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.search-container {
  margin-bottom: 1rem;
  flex-shrink: 0;
}

.search-input {
  width: 100%;
  max-width: 10000px;
}

.media-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 20px 16px;
  align-content: start;
}

.day-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 120px;
  height: 180px;
  color: #999;
  font-size: 14px;
  border: 1px dashed #ddd;
  border-radius: 8px;
  grid-column: 1 / -1;
}

.media-card {
  cursor: pointer;
  transition: all 0.3s ease;
  overflow: hidden;
  width: 100%;
  max-width: 200px;
  justify-self: center;
}

.media-card:hover {
  box-shadow: 0 8px 30px rgba(0, 0, 0, 0.3);
  transform: translateY(-4px);
}

.poster-wrapper {
  width: 100%;
  aspect-ratio: 2/3;
  overflow: hidden;
  position: relative;
}

.poster-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.season-complete-badge {
  position: absolute;
  top: 8px;
  right: 8px;
  padding: 3px 6px;
  border: 1px solid currentColor;
  border-radius: 6px;
  color: #fff;
  font-size: 12px;
  font-weight: 700;
  line-height: 1.4;
  white-space: nowrap;
  pointer-events: none;
}

.refresh-button {
  position: absolute;
  bottom: 21px;
  right: 3px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 25px;
  height: 25px;
  padding: 0;
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  background: rgba(24, 30, 40, 0.62);
  color: #fff;
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease;
}

.refresh-button:hover:not(:disabled) {
  background: rgba(24, 30, 40, 0.85);
  border-color: rgba(255, 255, 255, 0.65);
}

.refresh-button:focus-visible {
  outline: 2px solid #fff;
  outline-offset: 2px;
}

.refresh-button:active:not(:disabled) {
  background: rgba(24, 30, 40, 0.95);
}

.refresh-button:disabled {
  cursor: default;
}

.refresh-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transform-origin: center;
}

.refresh-icon.is-spinning {
  animation: refresh-spin 1s linear infinite;
}

@keyframes refresh-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .refresh-button {
    transition: none;
  }

  .refresh-icon.is-spinning {
    animation: none;
  }
}

.progress-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
}

.progress-bar-container {
  height: 18px;
  background: #666;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.progress-bar {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  transition: width 0.3s ease;
}

.progress-text {
  position: relative;
  z-index: 1;
  color: white;
  font-size: 12px;
  font-weight: bold;
  text-shadow: 1px 1px 2px rgba(0, 0, 0, 0.8);
}

.media-title {
  padding: 8px 0 0 0;
  text-align: center;
  color: #333;
  font-size: 14px;
  font-weight: 500;
}
</style>
