use crate::clients::{AccountResp, Adjust, Config, Notify};
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use bank_common::errors::{ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TxReq {
    pub from: String,
    pub to: String,
    pub amount: i64,
}
#[derive(Serialize)]
pub struct TxResp {
    pub id: String,
    pub status: &'static str,
}

pub fn router(cfg: Config) -> Router {
    Router::new()
        .route("/transactions", post(create))
        .with_state(cfg)
}

#[tracing::instrument(skip(cfg))]
async fn create(
    State(cfg): State<Config>,
    Json(req): Json<TxReq>,
) -> ServiceResult<(StatusCode, Json<TxResp>)> {
    bank_common::failure_modes::FailureModes::from_env("TRANSACTIONS")
        .maybe_delay()
        .await;

    // Debit
    let r = cfg
        .http
        .post(format!("{}/accounts/{}/adjust", cfg.accounts_url, req.from))
        .json(&Adjust { delta: -req.amount })
        .send()
        .await
        .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    if !r.status().is_success() {
        return Err(ServiceError::BadRequest("debit failed".into()));
    }
    // Credit
    let r = cfg
        .http
        .post(format!("{}/accounts/{}/adjust", cfg.accounts_url, req.to))
        .json(&Adjust { delta: req.amount })
        .send()
        .await
        .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    if !r.status().is_success() {
        return Err(ServiceError::BadRequest("credit failed".into()));
    }
    // Notify
    let _ = cfg
        .http
        .post(format!("{}/notify", cfg.notifications_url))
        .json(&Notify {
            user: req.to.clone(),
            message: format!("received {}", req.amount),
        })
        .send()
        .await;
    let id = uuid::Uuid::now_v7().to_string();
    Ok((StatusCode::CREATED, Json(TxResp { id, status: "ok" })))
}

// silence unused warnings on stub types (used by future tests)
fn _unused(_: AccountResp) {}
