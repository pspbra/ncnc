use crate::models::{hash_password, SettingsRequest, SettingsResponse, SettingsResponseData};
use crate::{create_client_from_config, save_current_config, CONFIG};
use actix_web::{web, HttpResponse};
use log::info;

/// 获取当前设置
pub async fn get_settings() -> HttpResponse {
    let config = CONFIG.read().await;
    let response_data = SettingsResponseData {
        media_auto_compress: config.media_auto_compress,
        media_library_path: config.media_library_path.clone(),
        server_port: config.server_port,
        proxy_enabled: config.proxy_enabled,
        proxy_address: config.proxy_address.clone(),
        proxy_port: config.proxy_port,
        tmdb_api_key: config.tmdb_api_key.clone(),
        tvdb_api_key: config.tvdb_api_key.clone(),
        jackett_address: config.jackett_address.clone(),
        jackett_port: config.jackett_port,
        jackett_api_key: config.jackett_api_key.clone(),
        jackett_auto_download: config.jackett_auto_download,
        aria_address: config.aria_address.clone(),
        aria_port: config.aria_port,
        aria_rpc_secret: config.aria_rpc_secret.clone(),
        openlist_address: config.openlist_address.clone(),
        openlist_port: config.openlist_port,
        openlist_apikey: config.openlist_apikey.clone(),
        openlist_auto_upload: config.openlist_auto_upload,
        jackett_auto_rss: config.jackett_auto_rss,
        global_filter_terms: config.global_filter_terms.clone(),
    };
    HttpResponse::Ok()
        .content_type("application/json")
        .json(response_data)
}

/// 更新设置
pub async fn update_settings(req: web::Json<SettingsRequest>) -> HttpResponse {
    let SettingsRequest {
        media_auto_compress,
        username,
        password,
        media_library_path,
        server_port,
        proxy_enabled,
        proxy_address,
        proxy_port,
        tmdb_api_key,
        tvdb_api_key,
        jackett_address,
        jackett_port,
        jackett_api_key,
        jackett_auto_download,
        aria_address,
        aria_port,
        aria_rpc_secret,
        openlist_address,
        openlist_port,
        openlist_apikey,
        openlist_auto_upload,
        jackett_auto_rss,
        global_filter_terms,
    } = req.into_inner();

    let need_recreate_client =
        proxy_enabled.is_some() || proxy_address.is_some() || proxy_port.is_some();

    let password_hash = match password {
        Some(password) => crate::io_util::blocking(move || hash_password(&password))
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    let mut config = CONFIG.write().await;
    let old_auto_download = config.jackett_auto_download;
    let old_auto_upload = config.openlist_auto_upload;

    if let Some(enabled) = media_auto_compress {
        config.media_auto_compress = enabled;
    }

    if let Some(u) = username {
        config.username = u;
    }
    if let Some(hashed) = password_hash {
        config.password = hashed;
    }
    if let Some(mlp) = media_library_path {
        config.media_library_path = mlp;
    }
    if let Some(sp) = server_port {
        config.server_port = sp;
    }
    if let Some(pe) = proxy_enabled {
        config.proxy_enabled = pe;
    }
    if let Some(pa) = proxy_address {
        config.proxy_address = pa;
    }
    if let Some(pp) = proxy_port {
        config.proxy_port = pp;
    }
    if let Some(tk) = tmdb_api_key {
        config.tmdb_api_key = tk;
    }
    if let Some(tvk) = tvdb_api_key {
        config.tvdb_api_key = tvk;
    }
    if let Some(ja) = jackett_address {
        config.jackett_address = ja;
    }
    if let Some(jp) = jackett_port {
        config.jackett_port = jp;
    }
    if let Some(jak) = jackett_api_key {
        config.jackett_api_key = jak;
    }
    let should_trigger_auto_download = if let Some(jad) = jackett_auto_download {
        let was_off = !old_auto_download;
        let is_on = jad;
        config.jackett_auto_download = jad;
        was_off && is_on
    } else {
        false
    };
    if let Some(ara) = aria_address {
        config.aria_address = ara;
    }
    if let Some(arp) = aria_port {
        config.aria_port = arp;
    }
    if let Some(ars) = aria_rpc_secret {
        config.aria_rpc_secret = ars;
    }
    if let Some(ola) = openlist_address {
        config.openlist_address = ola;
    }
    if let Some(olp) = openlist_port {
        config.openlist_port = olp;
    }
    if let Some(olk) = openlist_apikey {
        config.openlist_apikey = olk;
    }
    let should_trigger_auto_upload = if let Some(oau) = openlist_auto_upload {
        let was_off = !old_auto_upload;
        let is_on = oau;
        config.openlist_auto_upload = oau;
        was_off && is_on
    } else {
        false
    };

    let old_auto_rss = config.jackett_auto_rss;
    let should_trigger_auto_rss = if let Some(jar) = jackett_auto_rss {
        let was_off = !old_auto_rss;
        let is_on = jar;
        config.jackett_auto_rss = jar;
        was_off && is_on
    } else {
        false
    };

    if let Some(gft) = global_filter_terms {
        config.global_filter_terms = gft;
    }

    drop(config);
    save_current_config().await;

    if need_recreate_client {
        create_client_from_config().await;
    }

    if should_trigger_auto_download {
        info!("[设置] 自动下载已开启，立即执行一次自动下载");
        tokio::spawn(async {
            crate::auto_download::AutoDownloadManager::execute_now().await;
        });
    }

    if should_trigger_auto_upload {
        info!("[设置] 自动上传已开启，立即执行一次自动上传");
        tokio::spawn(async {
            crate::openlist_uploader::OpenListUploadManager::execute_now().await;
        });
    }

    if should_trigger_auto_rss {
        info!("[设置] 自动RSS已开启，立即执行一次RSS检查");
        tokio::spawn(async {
            crate::auto_rss::AutoRssManager::execute_now().await;
        });
    }

    let response = SettingsResponse {
        success: true,
        message: Some("设置已更新".to_string()),
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}
