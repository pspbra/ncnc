use crate::CONFIG;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use base64::Engine;
use futures_util::future::{ok, LocalBoxFuture, Ready};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

struct TokenKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}
impl TokenKeys {
    fn new(secret: &str) -> Self {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(secret)
            .unwrap_or_else(|_| secret.as_bytes().to_vec());
        Self {
            encoding: EncodingKey::from_secret(&bytes),
            decoding: DecodingKey::from_secret(&bytes),
        }
    }
}

static TOKEN_KEYS: Lazy<RwLock<Option<(String, Arc<TokenKeys>)>>> = Lazy::new(|| RwLock::new(None));

async fn token_keys() -> Arc<TokenKeys> {
    let config = CONFIG.read().await;
    let cached = TOKEN_KEYS.read().await;
    if let Some((secret, keys)) = cached.as_ref() {
        if secret == &config.jwt_secret {
            return keys.clone();
        }
    }
    let secret = config.jwt_secret.clone();
    drop(config);
    drop(cached);
    let keys = Arc::new(TokenKeys::new(&secret));
    *TOKEN_KEYS.write().await = Some((secret, keys.clone()));
    keys
}

pub const COOKIE_NAME: &str = "auth_token";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub async fn generate_token(username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let keys = token_keys().await;

    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: username.to_owned(),
        exp: expiration,
    };

    encode(&Header::default(), &claims, &keys.encoding)
}

pub async fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let keys = token_keys().await;

    decode::<Claims>(token, &keys.decoding, &Validation::default()).map(|data| data.claims)
}

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthMiddlewareService { service })
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();

        if path == "/v1/user/login" || path.starts_with("/webui") || path == "/" {
            let fut = self.service.call(req);
            return Box::pin(async move {
                let res = fut.await?;
                Ok(res)
            });
        }

        let token_opt = req.cookie(COOKIE_NAME);

        if let Some(cookie) = token_opt {
            let token = cookie.value().to_string();
            let fut = self.service.call(req);
            Box::pin(async move {
                match verify_token(&token).await {
                    Ok(_) => {
                        let res = fut.await?;
                        Ok(res)
                    }
                    Err(_) => Err(actix_web::error::ErrorUnauthorized("认证失效，请重新登录")),
                }
            })
        } else {
            Box::pin(async { Err(actix_web::error::ErrorUnauthorized("未登录，请先登录")) })
        }
    }
}
