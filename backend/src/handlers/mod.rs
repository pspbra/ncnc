use actix_files::NamedFile;
use actix_web::{HttpRequest, HttpResponse};

pub mod app;
pub mod auth;
pub mod download;
pub mod jackett;
pub mod logs;
pub mod media;
pub mod search;
pub mod settings;
pub mod subscribe;
pub mod system;
pub mod tvdb;
pub mod update;
pub mod upload;

pub use app::*;
pub use auth::*;
pub use download::*;
pub use jackett::*;
pub use logs::*;
pub use media::*;
pub use search::*;
pub use settings::*;
pub use subscribe::*;
pub use system::*;
pub use update::*;
pub use upload::*;

/// Handle webui static files and SPA fallback
pub async fn webui_handler(req: HttpRequest) -> HttpResponse {
    let path = req.path();

    // Remove /webui prefix
    let file_path = if path == "/webui" || path == "/webui/" {
        "webui/index.html".to_string()
    } else {
        let stripped = path.strip_prefix("/webui").unwrap_or(path);
        format!("webui{}", stripped)
    };

    if let Ok(Ok(file)) = crate::io_util::blocking(move || NamedFile::open(&file_path)).await {
        if file.metadata().is_file() {
            let file_path = file.path().to_string_lossy();
            let content_type = if file_path.ends_with(".html") {
                "text/html"
            } else if file_path.ends_with(".css") {
                "text/css"
            } else if file_path.ends_with(".js") {
                "application/javascript"
            } else if file_path.ends_with(".png") {
                "image/png"
            } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
                "image/jpeg"
            } else if file_path.ends_with(".svg") {
                "image/svg+xml"
            } else if file_path.ends_with(".ico") {
                "image/x-icon"
            } else {
                "application/octet-stream"
            };

            return file
                .set_content_type(content_type.parse().expect("static MIME type"))
                .set_content_disposition(actix_web::http::header::ContentDisposition {
                    disposition: actix_web::http::header::DispositionType::Inline,
                    parameters: Vec::new(),
                })
                .into_response(&req);
        }
    }
    if let Ok(Ok(file)) = crate::io_util::blocking(|| NamedFile::open("webui/index.html")).await {
        return file.into_response(&req);
    }

    HttpResponse::NotFound().body("Not found")
}
