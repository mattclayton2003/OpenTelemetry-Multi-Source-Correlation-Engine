use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ToxiproxyClient {
    pub base: String,
    pub http: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Toxic {
    pub name: String,
    pub r#type: String,
    pub stream: String,
    pub toxicity: f64,
    pub attributes: serde_json::Value,
}

impl ToxiproxyClient {
    pub fn new(base: String) -> Self {
        Self {
            base,
            http: reqwest::Client::new(),
        }
    }
    /// Clears all active toxics on every proxy (toxiproxy `POST /reset`, which
    /// also re-enables any disabled proxies). Used to guarantee a clean fault
    /// state at the start of each experiment, so a toxic a prior experiment
    /// failed to revert can't bleed into this one.
    pub async fn reset(&self) -> Result<()> {
        let r = self
            .http
            .post(format!("{}/reset", self.base))
            .send()
            .await?;
        if !r.status().is_success() {
            anyhow::bail!("toxiproxy reset: {}", r.status());
        }
        Ok(())
    }
    pub async fn add_toxic(&self, proxy: &str, toxic: Toxic) -> Result<String> {
        let url = format!("{}/proxies/{proxy}/toxics", self.base);
        let r = self.http.post(&url).json(&toxic).send().await?;
        if !r.status().is_success() {
            anyhow::bail!("toxiproxy add_toxic: {}", r.status());
        }
        Ok(toxic.name)
    }
    pub async fn remove_toxic(&self, proxy: &str, toxic_name: &str) -> Result<()> {
        let url = format!("{}/proxies/{proxy}/toxics/{toxic_name}", self.base);
        let r = self.http.delete(&url).send().await?;
        if !r.status().is_success() {
            anyhow::bail!("toxiproxy remove_toxic: {}", r.status());
        }
        Ok(())
    }
}
