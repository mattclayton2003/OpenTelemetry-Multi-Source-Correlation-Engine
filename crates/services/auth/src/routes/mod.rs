pub mod login;
pub mod verify;
use axum::{routing::post, Router};

pub fn router() -> Router {
    Router::new()
        .route("/auth/login", post(login::handler))
        .route("/auth/verify", post(verify::handler))
}
