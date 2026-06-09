use crate::profile::Stage;
use crate::stats::Stats;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub async fn run_stage(stage: Stage, stats: Stats) -> anyhow::Result<()> {
    if let Some(off) = stage.start_offset_sec {
        tokio::time::sleep(Duration::from_secs(off as u64)).await;
    }
    let (method, url) = parse_endpoint(&stage.endpoint);
    let client = reqwest::Client::new();
    let interval = Duration::from_micros(1_000_000 / stage.rps.max(1) as u64);
    let end = Instant::now() + Duration::from_secs(stage.duration_sec as u64);
    let body = stage.body.clone();
    while Instant::now() < end {
        let next = Instant::now() + interval;
        let url = url.clone();
        let method = method.clone();
        let stats = stats.clone();
        let body = body.clone();
        let client = client.clone();
        tokio::spawn(async move {
            let mut req = client.request(method.parse().unwrap_or(reqwest::Method::GET), &url);
            if let Some(b) = body {
                req = req.json(&b);
            }
            match req.send().await {
                Ok(r) if r.status().is_success() => {
                    stats.current.success.fetch_add(1, Ordering::SeqCst);
                }
                Ok(r) if r.status().as_u16() < 500 => {
                    stats.current.four_xx.fetch_add(1, Ordering::SeqCst);
                }
                Ok(_) => {
                    stats.current.five_xx.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => {
                    stats.current.error.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        tokio::time::sleep_until(next.into()).await;
    }
    Ok(())
}

fn parse_endpoint(s: &str) -> (String, String) {
    let parts: Vec<_> = s.splitn(2, ' ').collect();
    if parts.len() == 2 {
        (parts[0].into(), parts[1].into())
    } else {
        ("GET".into(), s.into())
    }
}
