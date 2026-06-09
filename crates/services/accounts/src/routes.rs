use crate::repo::{Account, AccountsRepo, NewAccount};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use bank_common::errors::{ServiceError, ServiceResult};

pub fn router(pool: sqlx::PgPool) -> Router {
    let repo = AccountsRepo { pool };
    Router::new()
        .route("/accounts", post(create))
        .route("/accounts/:id", get(read))
        .route("/accounts/:id/adjust", post(adjust))
        .with_state(repo)
}

async fn create(
    State(repo): State<AccountsRepo>,
    Json(new): Json<NewAccount>,
) -> ServiceResult<(StatusCode, Json<Account>)> {
    bank_common::failure_modes::FailureModes::from_env("ACCOUNTS")
        .maybe_delay()
        .await;
    let a = repo.create(new).await.map_err(ServiceError::Internal)?;
    Ok((StatusCode::CREATED, Json(a)))
}

async fn read(
    State(repo): State<AccountsRepo>,
    Path(id): Path<String>,
) -> ServiceResult<Json<Account>> {
    repo.get(&id)
        .await
        .map_err(ServiceError::Internal)?
        .map(Json)
        .ok_or(ServiceError::NotFound)
}

#[derive(serde::Deserialize)]
struct Adjust {
    delta: i64,
}
async fn adjust(
    State(repo): State<AccountsRepo>,
    Path(id): Path<String>,
    Json(a): Json<Adjust>,
) -> ServiceResult<StatusCode> {
    repo.adjust_balance(&id, a.delta)
        .await
        .map_err(ServiceError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}
