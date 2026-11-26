use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct LogDatesResponse {
    success: bool,
    message: Option<String>,
    data: Vec<String>,
}

#[derive(Serialize)]
struct LogContentResponse {
    success: bool,
    message: Option<String>,
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
}

#[derive(Deserialize)]
pub struct LogQuery {
    date: String,
    offset: Option<u64>,
    limit: Option<usize>,
}

pub async fn get_log_dates() -> HttpResponse {
    match crate::io_util::blocking(crate::logging::available_dates).await {
        Ok(Ok(dates)) => HttpResponse::Ok().json(LogDatesResponse {
            success: true,
            message: None,
            data: dates,
        }),
        Ok(Err(error)) => HttpResponse::InternalServerError().json(LogDatesResponse {
            success: false,
            message: Some(format!("读取日志日期失败: {error}")),
            data: Vec::new(),
        }),
        Err(error) => HttpResponse::InternalServerError().json(LogDatesResponse {
            success: false,
            message: Some(format!("日志读取任务失败: {error}")),
            data: Vec::new(),
        }),
    }
}

pub async fn get_log_content(query: web::Query<LogQuery>) -> HttpResponse {
    let date = query.date.clone();
    let offset = query.offset;
    let limit = query.limit.unwrap_or(256 * 1024);
    match crate::io_util::blocking(move || crate::logging::read_page(&date, offset, limit)).await {
        Ok(Ok(content)) => HttpResponse::Ok().json(LogContentResponse {
            success: true,
            message: None,
            offset: Some(content.offset),
            next_offset: Some(content.next_offset),
            total: Some(content.total),
            data: Some(content.content),
        }),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::InvalidInput => {
            HttpResponse::BadRequest().json(LogContentResponse {
                success: false,
                message: Some("日志日期格式无效".to_string()),
                data: None,
                offset: None,
                next_offset: None,
                total: None,
            })
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => HttpResponse::NotFound()
            .json(LogContentResponse {
                success: false,
                message: Some("所选日期的日志不存在".to_string()),
                data: None,
                offset: None,
                next_offset: None,
                total: None,
            }),
        Ok(Err(error)) => HttpResponse::InternalServerError().json(LogContentResponse {
            success: false,
            message: Some(format!("读取日志失败: {error}")),
            data: None,
            offset: None,
            next_offset: None,
            total: None,
        }),
        Err(error) => HttpResponse::InternalServerError().json(LogContentResponse {
            success: false,
            message: Some(format!("日志读取任务失败: {error}")),
            data: None,
            offset: None,
            next_offset: None,
            total: None,
        }),
    }
}
