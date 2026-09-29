<script lang="ts">
import { NCard, NButton, NSpace, NAvatar, NSwitch, NIcon, NInput, NModal, NInputGroup } from 'naive-ui';
import { TrashOutline as TrashIcon } from '@vicons/ionicons5';
import { useThemeStore } from '@/stores/theme';
import { Sunny as LightIcon, Moon as DarkIcon } from '@vicons/ionicons5';
import { useUserStore } from '@/stores/user';
import { useMessageStore } from '@/stores/message';
import { restartApp } from '@/services/appService';
import { getSettings, updateSettings, type Config } from '@/services/settingService';

export default {
  emits: [
    'update:settings'
  ],
  components: {
    NCard,
    NButton,
    NSpace,
    NAvatar,
    NSwitch,
    LightIcon,
    DarkIcon,
    NIcon,
    NInput,
    NModal,
    NInputGroup,
    TrashIcon
  },
  setup() {
    const themeStore = useThemeStore()
    const userStore = useUserStore()
    const messageStore = useMessageStore()
    return {
      themeStore,
      userStore,
      messageStore
    }
  },
  data() {
    return {
      themes: [
        {
          name: 'Pink',
          value: 'pink',
          color: '#ff80aa'
        },
        {
          name: 'Blue',
          value: 'blue',
          color: '#33cbff'
        }
      ],
      showCreateUserModal: false,
      newUsername: '',
      newPassword: '',
      newPasswordAgain: '',
      config: {
        media_library_path: '',
        media_auto_compress: true,
        server_port: 3000,
        proxy_enabled: false,
        proxy_address: '',
        proxy_port: 7890,
        tmdb_api_key: '',
        tvdb_api_key: '',
        jackett_address: '127.0.0.1',
        jackett_port: 9117,
        jackett_api_key: '',
        jackett_auto_download: true,
        jackett_auto_rss: true,
        aria_address: '127.0.0.1',
        aria_port: 6800,
        aria_rpc_secret: '',
        openlist_address: '127.0.0.1',
        openlist_port: 5244,
        openlist_apikey: '',
        openlist_auto_upload: true,
        global_filter_terms: []
      } as Config,
      showGlobalFilterTermsModal: false,
      tempGlobalFilterTerm: '',
      localGlobalFilterTerms: [] as (string | string[])[]
    }
  },
  async mounted() {
    await this.fetchSettings();
  },
  methods: {
    async fetchSettings() {
      try {
        const response = await getSettings();
        this.config = response.data;
      } catch (error) {
        console.error('Failed to fetch settings:', error);
      }
    },
    async saveSettings() {
      try {
        await updateSettings({
          media_library_path: this.config.media_library_path,
          media_auto_compress: this.config.media_auto_compress,
          server_port: this.config.server_port,
          proxy_enabled: this.config.proxy_enabled,
          proxy_address: this.config.proxy_address,
          proxy_port: this.config.proxy_port,
          tmdb_api_key: this.config.tmdb_api_key,
          tvdb_api_key: this.config.tvdb_api_key,
          jackett_address: this.config.jackett_address,
          jackett_port: this.config.jackett_port,
          jackett_api_key: this.config.jackett_api_key,
          jackett_auto_download: this.config.jackett_auto_download,
          jackett_auto_rss: this.config.jackett_auto_rss,
          aria_address: this.config.aria_address,
          aria_port: this.config.aria_port,
          aria_rpc_secret: this.config.aria_rpc_secret,
          openlist_address: this.config.openlist_address,
          openlist_port: this.config.openlist_port,
          openlist_apikey: this.config.openlist_apikey,
          openlist_auto_upload: this.config.openlist_auto_upload,
          global_filter_terms: this.config.global_filter_terms
        });
        this.messageStore.setMessage("设置已保存", "success");
      } catch (error) {
        this.messageStore.setMessage("保存设置失败", "error");
        console.error('Failed to save settings:', error);
      }
    },
    formatFilterTerm(term: string | string[]): string {
      if (Array.isArray(term)) {
        return term.join(' ');
      }
      return term;
    },
    parseFilterInput(input: string): string | string[] {
      const parts = input.trim().split(/\s+/);
      if (parts.length > 1) {
        return parts;
      }
      return input.trim();
    },
    handleGlobalFilterTermsClick() {
      this.localGlobalFilterTerms = [...this.config.global_filter_terms];
      this.tempGlobalFilterTerm = '';
      this.showGlobalFilterTermsModal = true;
    },
    addGlobalFilterTerm() {
      if (this.tempGlobalFilterTerm.trim()) {
        const parsed = this.parseFilterInput(this.tempGlobalFilterTerm);
        const exists = this.localGlobalFilterTerms.some(term => {
          if (Array.isArray(term) && Array.isArray(parsed)) {
            return term.join(' ') === parsed.join(' ');
          }
          return term === parsed;
        });
        if (!exists) {
          this.localGlobalFilterTerms.push(parsed);
        }
        this.tempGlobalFilterTerm = '';
      }
    },
    removeGlobalFilterTerm(index: number) {
      this.localGlobalFilterTerms.splice(index, 1);
    },
    closeGlobalFilterTermsModal() {
      this.showGlobalFilterTermsModal = false;
    },
    async confirmGlobalFilterTerms() {
      this.config.global_filter_terms = [...this.localGlobalFilterTerms];
      this.messageStore.setMessage('全局过滤词已更新', 'success');
      this.showGlobalFilterTermsModal = false;
    },
    async emitUpdateSettings() {
      this.$emit('update:settings')
    },
    async updateUser() {
      if (this.newUsername === "") {
        this.messageStore.setMessage("用户名不能为空", "error")
        return
      }
      if (this.newPassword === "") {
        this.messageStore.setMessage("密码不能为空", "error")
        return
      }
      if (this.newPassword !== this.newPasswordAgain) {
        this.messageStore.setMessage("两次输入的密码不一致", "error")
        return
      }
      try {
        await updateSettings({
          username: this.newUsername,
          password: this.newPassword
        });
        this.messageStore.setMessage("用户更新成功, 请重新登陆", "success")
        this.showCreateUserModal = false
        this.userStore.logout()
        this.$router.push("/login")
      } catch (error) {
        this.messageStore.setMessage("更新用户失败", "error");
        console.error('Failed to update user:', error);
      }
    },
    logoutUser() {
      this.userStore.logout()
      this.messageStore.setMessage("已退出登录", "success")
      this.$router.push("/login")
    },
    async restartBackend() {
      try{
        await restartApp()
      } catch (e) {
        
      }
      this.messageStore.setMessage("NCNC后端已重启", "success")
      setTimeout(() => {
        location.reload()
      }, 2000)
    }
  },
  watch: {
    'themeStore.dark': {
      handler: function (val) {
        this.themeStore.setDark(val)
      },
      immediate: true
    }
  }
}

</script>
<template>
    <NCard size="medium" title="通用设置">
        <template #header-extra>
            <NSpace justify="end">
                <NButton type="error" size="small" @click="restartBackend">重启</NButton>
                <NButton type="primary" size="small" @click="saveSettings">保存</NButton>
            </NSpace>
            
        </template>
        <NCard size="small" title="主题设置">
          <template #header-extra>
            <NSwitch v-model:value="themeStore.dark">
              <template #checked-icon>
                <NIcon>
                  <DarkIcon />
                </NIcon>
              </template>
              <template #unchecked-icon>
                <NIcon>
                  <LightIcon />
                </NIcon>
              </template>
            </NSwitch>
          </template>
            <NSpace>
                <div v-for="theme in themes" :key="theme.name">
                    <NAvatar :style="{ backgroundColor: theme.color }" @click="themeStore.setTheme(theme.value)"/>
                </div>
            </NSpace>
        </NCard>
        
        <NCard size="small" title="媒体库设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <NInput 
              placeholder="媒体库地址" 
              v-model:value="config.media_library_path"
              label="媒体库路径"
            />
            <div style="display: flex; align-items: center; gap: 10px;">
              <span>是否自动压缩</span>
              <NSwitch v-model:value="config.media_auto_compress" aria-label="是否自动压缩">
                <template #checked>是</template>
                <template #unchecked>否</template>
              </NSwitch>
            </div>
          </NSpace>
        </NCard>
        
        <NCard size="small" title="启动端口" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <NInput 
              placeholder="启动端口" 
              :value="String(config.server_port)"
              @update:value="config.server_port = Number($event)"
              label="端口"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="代理设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <div style="display: flex; align-items: center; gap: 10px;">
              <span>启用代理</span>
              <NSwitch v-model:value="config.proxy_enabled" />
            </div>
            <div style="display: flex; gap: 10px;">
              <NInput 
                placeholder="代理地址" 
                v-model:value="config.proxy_address"
                label="地址"
                style="flex: 1;"
                :disabled="!config.proxy_enabled"
              />
              <NInput 
                placeholder="端口" 
                :value="String(config.proxy_port)"
                @update:value="config.proxy_port = Number($event)"
                label="端口"
                style="width: 150px;"
                :disabled="!config.proxy_enabled"
              />
            </div>
          </NSpace>
        </NCard>
        
        <NCard size="small" title="全局过滤词设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <div class="optional-names-container">
              <n-input-group class="optional-names-input-group">
                <n-input :value="config.global_filter_terms.map(term => formatFilterTerm(term)).join(' | ')" disabled placeholder="" />
                <n-button 
                  type="primary" 
                  @click="handleGlobalFilterTermsClick"
                  style="font-size: 22px; font-weight: bold; padding: 0 14px; min-width: auto; height: 34px;"
                >
                  +
                </n-button>
              </n-input-group>
            </div>
          </NSpace>
        </NCard>
        
        <NCard size="small" title="Tmdb apikey" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <NInput 
              placeholder="Tmdb API Key" 
              v-model:value="config.tmdb_api_key"
              label="API Key"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="Tvdb apikey" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <NInput 
              placeholder="Tvdb API Key" 
              v-model:value="config.tvdb_api_key"
              label="API Key"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="Openlist 设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <div style="display: flex; align-items: center; gap: 10px;">
              <span>自动上传</span>
              <NSwitch v-model:value="config.openlist_auto_upload" />
            </div>
            <div style="display: flex; gap: 10px;">
              <NInput 
                placeholder="Openlist地址" 
                v-model:value="config.openlist_address"
                label="地址"
                style="flex: 1;"
              />
              <NInput 
                placeholder="Openlist端口" 
                :value="String(config.openlist_port)"
                @update:value="config.openlist_port = Number($event)"
                label="端口"
                style="width: 150px;"
              />
            </div>
            <NInput 
              placeholder="Openlist 令牌" 
              v-model:value="config.openlist_apikey"
              label="Openlist 令牌"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="Jackett设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <div style="display: flex; align-items: center; gap: 40px;">
              <div style="display: flex; align-items: center; gap: 10px;">
                <span>自动搜索下载</span>
                <NSwitch v-model:value="config.jackett_auto_download" />
              </div>
              <div style="display: flex; align-items: center; gap: 10px;">
                <span>自动RSS追踪</span>
                <NSwitch v-model:value="config.jackett_auto_rss" />
              </div>
            </div>
            <div style="display: flex; gap: 10px;">
              <NInput 
                placeholder="Jackett地址" 
                v-model:value="config.jackett_address"
                label="地址"
                style="flex: 1;"
              />
              <NInput 
                placeholder="Jackett端口" 
                :value="String(config.jackett_port)"
                @update:value="config.jackett_port = Number($event)"
                label="端口"
                style="width: 150px;"
              />
            </div>
            <NInput 
              placeholder="Jackett API Key" 
              v-model:value="config.jackett_api_key"
              label="Jackett apikey"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="Aria 设置" style="margin-top: 10px;">
          <NSpace vertical style="width: 100%;">
            <div style="display: flex; gap: 10px;">
              <NInput 
                placeholder="Aria地址" 
                v-model:value="config.aria_address"
                label="地址"
                style="flex: 1;"
              />
              <NInput 
                placeholder="Aria端口" 
                :value="String(config.aria_port)"
                @update:value="config.aria_port = Number($event)"
                label="端口"
                style="width: 150px;"
              />
            </div>
            <NInput 
              placeholder="Aria RPC 密钥" 
              v-model:value="config.aria_rpc_secret"
              label="Aria RPC 密钥"
            />
          </NSpace>
        </NCard>
        
        <NCard size="small" title="账号设置" style="margin-top: 10px;">
          <NSpace justify="start">
            <NButton type="primary" @click="showCreateUserModal = true">更新用户</NButton>
            <NButton @click="logoutUser">退出登录</NButton>
          </NSpace>
        </NCard>
    </NCard>
    <NModal class="modal" v-model:show="showCreateUserModal">
      <NCard size="small" title="修改用户名和密码">
        <div class="input-container">
          <NInput placeholder="用户名" v-model:value="newUsername"/>
          <NInput placeholder="密码" type="password" v-model:value="newPassword"/>
          <NInput placeholder="请再次输入密码" type="password" v-model:value="newPasswordAgain"/>
        </div>
        <template #action>
          <NSpace justify="end">
            <NButton @click="showCreateUserModal = false">取消</NButton>
            <NButton type="primary" @click="updateUser">确认</NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>

    <NModal v-model:show="showGlobalFilterTermsModal" preset="card" style="width: 500px;" title="全局过滤词增减">
      <div class="optional-names-modal">
        <n-input-group class="add-name-input-group">
          <n-input v-model:value="tempGlobalFilterTerm" placeholder="输入全局过滤词（多个词用空格分隔）" @keyup.enter="addGlobalFilterTerm" />
          <n-button type="primary" @click="addGlobalFilterTerm">添加</n-button>
        </n-input-group>
        
        <div class="optional-names-list">
          <div 
            v-for="(term, index) in localGlobalFilterTerms" 
            :key="'global-filter-' + index"
            class="optional-name-item"
          >
            <span class="name-text">{{ formatFilterTerm(term) }}</span>
            <n-button type="error" size="tiny" quaternary @click="removeGlobalFilterTerm(index)">
              <template #icon>
                <n-icon><trash-icon /></n-icon>
              </template>
            </n-button>
          </div>
        </div>
      </div>
      <template #footer>
        <n-button @click="closeGlobalFilterTermsModal">关闭</n-button>
        <n-button type="primary" @click="confirmGlobalFilterTerms">确认</n-button>
      </template>
    </NModal>

</template>

<style scoped>
.modal {
  width: 300px;
}
.input-container {
  width: 100%;
  display: grid;
  row-gap: 5px;
}

.optional-names-container {
  width: 100%;
}

.optional-names-input-group {
  display: flex;
  flex-wrap: nowrap;
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

</style>
