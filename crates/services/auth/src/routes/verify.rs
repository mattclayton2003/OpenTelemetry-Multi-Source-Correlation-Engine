use axum::Json;
use bank_common::errors::{ServiceError, ServiceResult};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

const SECRET: &[u8] = b"dev-only-secret";

#[derive(Deserialize)] pub struct Req { pub token: String }
#[derive(Serialize)]   pub struct Resp { pub user: String }
#[derive(Deserialize)] struct Claims { sub: String, exp: i64 }

pub async fn handler(Json(req): Json<Req>) -> ServiceResult<Json<Resp>> {
    let data = decode::<Claims>(&req.token, &DecodingKey::from_secret(SECRET), &Validation::default())
        .map_err(|_| ServiceError::Unauthorized)?;
    Ok(Json(Resp { user: data.claims.sub }))
}
