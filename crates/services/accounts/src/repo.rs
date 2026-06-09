use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AccountsRepo {
    pub pool: PgPool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub owner: String,
    pub balance: i64,
}

#[derive(Deserialize)]
pub struct NewAccount {
    pub owner: String,
}

impl AccountsRepo {
    pub async fn create(&self, new: NewAccount) -> anyhow::Result<Account> {
        let id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO accounts (id, owner) VALUES ($1, $2)")
            .bind(&id)
            .bind(&new.owner)
            .execute(&self.pool)
            .await?;
        Ok(Account {
            id,
            owner: new.owner,
            balance: 0,
        })
    }
    pub async fn get(&self, id: &str) -> anyhow::Result<Option<Account>> {
        let row =
            sqlx::query_as::<_, Account>("SELECT id, owner, balance FROM accounts WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }
    pub async fn adjust_balance(&self, id: &str, delta: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE accounts SET balance = balance + $1 WHERE id = $2")
            .bind(delta)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
