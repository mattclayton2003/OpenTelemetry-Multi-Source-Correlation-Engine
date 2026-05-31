use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("bad request: {0}")] BadRequest(String),
    #[error("not found")] NotFound,
    #[error("unauthorized")] Unauthorized,
    #[error("internal: {0}")] Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrBody<'a> { error: &'a str, detail: Option<String> }

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (code, msg, detail) = match &self {
            ServiceError::BadRequest(d) => (StatusCode::BAD_REQUEST, "bad_request", Some(d.clone())),
            ServiceError::NotFound      => (StatusCode::NOT_FOUND,   "not_found", None),
            ServiceError::Unauthorized  => (StatusCode::UNAUTHORIZED,"unauthorized", None),
            ServiceError::Internal(e)   => {
                tracing::error!("internal error: {e:?}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", None)
            }
        };
        (code, Json(ErrBody { error: msg, detail })).into_response()
    }
}

pub type ServiceResult<T> = std::result::Result<T, ServiceError>;
