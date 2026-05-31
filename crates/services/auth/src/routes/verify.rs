use axum::Json;
use bank_common::errors::ServiceResult;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)] pub struct Req { pub token: String }
#[derive(Serialize)]   pub struct Resp { pub user: String }

pub async fn handler(Json(req): Json<Req>) -> ServiceResult<Json<Resp>> {
    let _ = req;
    Ok(Json(Resp { user: "stub".into() }))
}
