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
}

#[derive(Deserialize)]
pub struct LogQuery {
    date: String,
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
    match crate::io_util::blocking(move || crate::logging::read_date(&date)).await {
        Ok(Ok(content)) => HttpResponse::Ok().json(LogContentResponse {
            success: true,
            message: None,
            data: Some(content),
        }),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::InvalidInput => {
            HttpResponse::BadRequest().json(LogContentResponse {
                success: false,
                message: Some("日志日期格式无效".to_string()),
                data: None,
            })
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => HttpResponse::NotFound()
            .json(LogContentResponse {
                success: false,
                message: Some("所选日期的日志不存在".to_string()),
                data: None,
            }),
        Ok(Err(error)) => HttpResponse::InternalServerError().json(LogContentResponse {
            success: false,
            message: Some(format!("读取日志失败: {error}")),
            data: None,
        }),
        Err(error) => HttpResponse::InternalServerError().json(LogContentResponse {
            success: false,
            message: Some(format!("日志读取任务失败: {error}")),
            data: None,
        }),
    }
}
