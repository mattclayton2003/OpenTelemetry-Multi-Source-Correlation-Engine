use axum::Router;
use crate::repo::AccountsRepo;

pub fn router(pool: sqlx::PgPool) -> Router {
    let _repo = AccountsRepo { pool };
    Router::new()
}
