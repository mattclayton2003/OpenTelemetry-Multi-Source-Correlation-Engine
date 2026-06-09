use crate::clients::{AccountResp, Adjust, Config, Notify};
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use bank_common::errors::{ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};
use tracing::Instrument;

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

    // Debit — wrapped in a CLIENT span so the accounts SERVER span links to it
    // (header injection must happen inside the span; that's why this is a future
    // instrumented by the span rather than a bare builder call).
    let r = async {
        cfg.http
            .post(format!("{}/accounts/{}/adjust", cfg.accounts_url, req.from))
            .headers(bank_common::otel::trace_headers())
            .json(&Adjust { delta: -req.amount })
            .send()
            .await
    }
    .instrument(bank_common::otel::client_span(
        "POST /accounts/:id/adjust",
        "accounts",
    ))
    .await
    .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    if !r.status().is_success() {
        return Err(ServiceError::BadRequest("debit failed".into()));
    }
    // Credit
    let r = async {
        cfg.http
            .post(format!("{}/accounts/{}/adjust", cfg.accounts_url, req.to))
            .headers(bank_common::otel::trace_headers())
            .json(&Adjust { delta: req.amount })
            .send()
            .await
    }
    .instrument(bank_common::otel::client_span(
        "POST /accounts/:id/adjust",
        "accounts",
    ))
    .await
    .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    if !r.status().is_success() {
        return Err(ServiceError::BadRequest("credit failed".into()));
    }
    // Notify
    let _ = async {
        cfg.http
            .post(format!("{}/notify", cfg.notifications_url))
            .headers(bank_common::otel::trace_headers())
            .json(&Notify {
                user: req.to.clone(),
                message: format!("received {}", req.amount),
            })
            .send()
            .await
    }
    .instrument(bank_common::otel::client_span(
        "POST /notify",
        "notifications",
    ))
    .await;
    let id = uuid::Uuid::now_v7().to_string();
    Ok((StatusCode::CREATED, Json(TxResp { id, status: "ok" })))
}

// silence unused warnings on stub types (used by future tests)
fn _unused(_: AccountResp) {}
