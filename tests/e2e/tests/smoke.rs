#![cfg(feature = "e2e")]
use anyhow::Result;
use e2e::wait_for_url;

#[tokio::test]
async fn smoke_full_stack() -> Result<()> {
    wait_for_url("http://localhost:8001/health", 60).await?;
    wait_for_url("http://localhost:3200/ready",  60).await?;
    wait_for_url("http://localhost:3100/ready",  60).await?;
    wait_for_url("http://localhost:9090/-/ready",60).await?;

    // Make a login → a trace must appear in Tempo
    let client = reqwest::Client::new();
    let r = client.post("http://localhost:8001/auth/login")
        .json(&serde_json::json!({"user":"alice","password":"pw"}))
        .send().await?;
    assert!(r.status().is_success());

    // Allow telemetry to land
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Tempo: search traces for service.name=auth in the last 5 minutes
    let resp = client.get("http://localhost:3200/api/search")
        .query(&[("tags","service.name=auth"), ("limit","1")])
        .send().await?.error_for_status()?;
    let v: serde_json::Value = resp.json().await?;
    let traces = v["traces"].as_array().cloned().unwrap_or_default();
    assert!(!traces.is_empty(), "expected at least one auth trace");

    // Prometheus: scrape target up{job=services} present
    let q = client.get("http://localhost:9090/api/v1/query")
        .query(&[("query","up{job=\"services\"}")]).send().await?.error_for_status()?;
    let v: serde_json::Value = q.json().await?;
    assert!(v["data"]["result"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

    Ok(())
}
