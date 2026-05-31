use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ToxiproxyClient { pub base: String, pub http: reqwest::Client }

#[derive(Serialize, Deserialize, Clone)]
pub struct Toxic {
    pub name: String,
    pub r#type: String,
    pub stream: String,
    pub toxicity: f64,
    pub attributes: serde_json::Value,
}

impl ToxiproxyClient {
    pub fn new(base: String) -> Self { Self { base, http: reqwest::Client::new() } }
    pub async fn add_toxic(&self, proxy: &str, toxic: Toxic) -> Result<String> {
        let url = format!("{}/proxies/{proxy}/toxics", self.base);
        let r = self.http.post(&url).json(&toxic).send().await?;
        if !r.status().is_success() { anyhow::bail!("toxiproxy add_toxic: {}", r.status()); }
        Ok(toxic.name)
    }
    pub async fn remove_toxic(&self, proxy: &str, toxic_name: &str) -> Result<()> {
        let url = format!("{}/proxies/{proxy}/toxics/{toxic_name}", self.base);
        let r = self.http.delete(&url).send().await?;
        if !r.status().is_success() { anyhow::bail!("toxiproxy remove_toxic: {}", r.status()); }
        Ok(())
    }
}
