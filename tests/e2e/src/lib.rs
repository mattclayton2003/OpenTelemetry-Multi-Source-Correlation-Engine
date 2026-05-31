pub async fn wait_for_url(url: &str, attempts: u32) -> anyhow::Result<()> {
    for _ in 0..attempts {
        if reqwest::get(url).await.map(|r| r.status().is_success()).unwrap_or(false) { return Ok(()); }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    anyhow::bail!("timeout waiting for {url}")
}
