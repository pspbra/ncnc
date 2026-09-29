use actix_web::HttpResponse;
use log::info;
use serde::Serialize;

#[derive(Serialize)]
pub struct RestartResponse {
    pub success: bool,
    pub message: String,
}

pub async fn restart_app() -> HttpResponse {
    info!("[系统] 收到重启请求，正在重启程序...");

    crate::RESTART_REQUESTED.store(true, std::sync::atomic::Ordering::Release);
    tokio::spawn(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        crate::SHUTDOWN_REQUEST.notify_one();
    });

    HttpResponse::Ok().json(RestartResponse {
        success: true,
        message: "重启命令已发送，程序将在 0.5 秒后重启".to_string(),
    })
}
