use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Config {
    pub accounts_url: String,
    pub notifications_url: String,
    pub http: reqwest::Client,
}
impl Config {
    pub fn from_env() -> Self {
        Self {
            accounts_url: std::env::var("ACCOUNTS_URL").unwrap_or("http://accounts:8002".into()),
            notifications_url: std::env::var("NOTIFICATIONS_URL").unwrap_or("http://notifications:8004".into()),
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)] pub struct Adjust { pub delta: i64 }
#[derive(Serialize)] pub struct Notify { pub user: String, pub message: String }
#[derive(Deserialize)] pub struct AccountResp { pub id: String, pub balance: i64 }
