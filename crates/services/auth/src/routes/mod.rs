pub mod login;
pub mod verify;
use axum::{Router, routing::post};

pub fn router() -> Router {
    Router::new()
        .route("/auth/login",  post(login::handler))
        .route("/auth/verify", post(verify::handler))
}
