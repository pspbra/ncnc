import globalAxios from "@/api/modifiedAxios";

export type FilterTerm = string | string[];

export interface Config {
  username?: string;
  password?: string;
  media_library_path: string;
  media_auto_compress: boolean;
  server_port: number;
  proxy_enabled: boolean;
  proxy_address: string;
  proxy_port: number;
  tmdb_api_key: string;
  tvdb_api_key: string;
  jackett_address: string;
  jackett_port: number;
  jackett_api_key: string;
  jackett_auto_download: boolean;
  jackett_auto_rss: boolean;
  aria_address: string;
  aria_port: number;
  aria_rpc_secret: string;
  openlist_address: string;
  openlist_port: number;
  openlist_apikey: string;
  openlist_auto_upload: boolean;
  global_filter_terms: FilterTerm[];
}

// 新的设置API
export async function getSettings() {
  const response = await globalAxios.get('/v1/settings');
  return response;
}

export async function updateSettings(settings: Partial<Config>) {
  const response = await globalAxios.post('/v1/settings', settings);
  return response;
}

// 兼容旧代码的函数（占位实现）
export async function addSetting(key: string, value: string | null) {
  // 旧API的占位实现，暂时不做任何事
  console.log('addSetting called (compatibility mode)', key, value);
  return { data: { success: true } };
}
