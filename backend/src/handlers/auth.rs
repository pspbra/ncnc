use crate::middleware::auth::{generate_token, COOKIE_NAME};
use crate::models::{verify_password, LoginRequest, LoginResponse};
use crate::CONFIG;
use actix_web::{cookie::Cookie, web, HttpRequest, HttpResponse};

/// 重定向到WebUI页面
pub async fn redirect_to_webui(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", "/webui"))
        .finish()
}

/// 处理用户登录请求
pub async fn login(req: web::Json<LoginRequest>) -> HttpResponse {
    let LoginRequest { username, password } = req.into_inner();

    let (configured_username, password_hash) = {
        let config = CONFIG.read().await;
        (config.username.clone(), config.password.clone())
    };
    let is_valid = username == configured_username
        && crate::io_util::blocking(move || {
            verify_password(&password, &password_hash).unwrap_or(false)
        })
        .await
        .unwrap_or(false);

    if is_valid {
        match generate_token(&username).await {
            Ok(token) => {
                let cookie: Cookie = Cookie::build(COOKIE_NAME, token.clone())
                    .path("/")
                    .http_only(true)
                    .same_site(actix_web::cookie::SameSite::Strict)
                    .max_age(actix_web::cookie::time::Duration::hours(24))
                    .finish();

                let response = LoginResponse {
                    success: true,
                    data: Some(token),
                    message: None,
                };

                HttpResponse::Ok()
                    .content_type("application/json")
                    .cookie(cookie)
                    .json(response)
            }
            Err(_) => {
                let response = LoginResponse {
                    success: false,
                    data: None,
                    message: Some("登录失败，请稍后重试".to_string()),
                };

                HttpResponse::Ok()
                    .content_type("application/json")
                    .json(response)
            }
        }
    } else {
        let response = LoginResponse {
            success: false,
            data: None,
            message: Some("用户名或密码错误".to_string()),
        };

        HttpResponse::Ok()
            .content_type("application/json")
            .json(response)
    }
}
