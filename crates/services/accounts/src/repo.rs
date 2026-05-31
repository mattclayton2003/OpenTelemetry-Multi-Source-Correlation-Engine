use sqlx::PgPool;

#[derive(Clone)]
pub struct AccountsRepo { pub pool: PgPool }
