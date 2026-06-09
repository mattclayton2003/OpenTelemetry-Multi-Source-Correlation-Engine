use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use bank_common::errors::{ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Req {
    pub user: String,
    pub message: String,
}
#[derive(Serialize)]
pub struct Resp {
    pub queued: bool,
}

#[derive(Clone)]
pub struct Ctx {
    pub smtp_url: String,
    pub http: reqwest::Client,
}

pub fn router(smtp_url: String) -> Router {
    Router::new()
        .route("/notify", post(handler))
        .with_state(Ctx {
            smtp_url,
            http: reqwest::Client::new(),
        })
}

#[tracing::instrument(skip(ctx))]
async fn handler(
    State(ctx): State<Ctx>,
    Json(req): Json<Req>,
) -> ServiceResult<(StatusCode, Json<Resp>)> {
    bank_common::failure_modes::FailureModes::from_env("NOTIFICATIONS")
        .maybe_delay()
        .await;
    let r = ctx
        .http
        .post(&ctx.smtp_url)
        .json(&req)
        .send()
        .await
        .map_err(|e| ServiceError::Internal(anyhow::anyhow!(e)))?;
    if !r.status().is_success() {
        return Err(ServiceError::Internal(anyhow::anyhow!("smtp non-2xx")));
    }
    Ok((StatusCode::CREATED, Json(Resp { queued: true })))
}
