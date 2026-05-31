use axum::Json;
use bank_common::errors::ServiceResult;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)] pub struct Req { pub user: String, pub password: String }
#[derive(Serialize)]  pub struct Resp { pub token: String }

pub async fn handler(Json(req): Json<Req>) -> ServiceResult<Json<Resp>> {
    let _ = req;
    Ok(Json(Resp { token: "stub".into() }))
}
