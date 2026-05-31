use axum::Json;
use bank_common::errors::{ServiceError, ServiceResult};
use chrono::{Utc, Duration};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};

const SECRET: &[u8] = b"dev-only-secret"; // research artifact — not for prod

#[derive(Deserialize)] pub struct Req { pub user: String, pub password: String }
#[derive(Serialize)]  pub struct Resp { pub token: String }

#[derive(Serialize, Deserialize)]
struct Claims { sub: String, exp: i64 }

pub async fn handler(Json(req): Json<Req>) -> ServiceResult<Json<Resp>> {
    if req.password.is_empty() { return Err(ServiceError::Unauthorized); }
    let claims = Claims { sub: req.user, exp: (Utc::now() + Duration::hours(1)).timestamp() };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET))
        .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(Resp { token }))
}
