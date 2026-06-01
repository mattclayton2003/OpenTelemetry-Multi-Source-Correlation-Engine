use crate::{spec::*, recovery::*, db as _db};
use sqlx::SqlitePool;
use sha2::Digest;
use chrono::Utc;
use chaos::driver::{FaultDriver, DefaultDriver, FaultSpec};
use chaos::toxiproxy::ToxiproxyClient;
use std::path::Path;
use std::time::Duration;

pub async fn run_file(path: &Path, pool: &SqlitePool, dry_run: bool) -> anyhow::Result<()> {
    // Lock lives in a guaranteed-writable temp dir, NOT next to the YAML
    // (the YAML's parent may be a read-only mount in containers).
    let lock_path = std::env::temp_dir().join("experiment-runner.lock");
    let lock_file = std::fs::OpenOptions::new().create(true).write(true).open(&lock_path)?;
    fs2::FileExt::try_lock_exclusive(&lock_file)
        .map_err(|_| anyhow::anyhow!("another experiment is running (lock: {})", lock_path.display()))?;

    let yaml_text = std::fs::read_to_string(path)?;
    let exp: Experiment = serde_yaml::from_str(&yaml_text)?;
    let sha = {
        let mut h = sha2::Sha256::new(); h.update(yaml_text.as_bytes());
        format!("sha256:{:x}", h.finalize())
    };

    let toxi = ToxiproxyClient::new(std::env::var("TOXIPROXY_URL").unwrap_or("http://toxiproxy:8474".into()));
    let driver = DefaultDriver { toxi };

    let started = Utc::now();
    tracing::info!(id = %exp.id, "warmup");
    if dry_run {
        tracing::info!("dry_run: validated YAML and connectivity");
        return Ok(());
    }
    tokio::time::sleep(Duration::from_secs(exp.warmup_sec as u64)).await;

    let mut handles: Vec<chaos::driver::FaultHandle> = vec![];
    for fault in &exp.faults {
        tokio::time::sleep(Duration::from_secs(fault.at_sec as u64)).await;
        let h = driver.apply(&fault.spec).await?;
        handles.push(h);
    }
    tokio::time::sleep(Duration::from_secs(exp.duration_sec as u64)).await;
    let mut status = "clean";
    for h in &handles {
        if let Err(e) = driver.revert(h).await {
            tracing::error!("revert failed: {e}");
            status = "dirty";
        }
    }

    let mut sm = SignalStateMachine::new(Duration::from_secs(exp.recovery_grace_sec as u64));
    let recovery_deadline = std::time::Instant::now() + Duration::from_secs((exp.cooldown_sec + exp.recovery_grace_sec) as u64);
    let mut recovery_ts: Option<i64> = None;
    while std::time::Instant::now() < recovery_deadline {
        let now_ns = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let health_ok = check_health("http://auth:8001/health").await
            && check_health("http://accounts:8002/health").await
            && check_health("http://transactions:8003/health").await
            && check_health("http://notifications:8004/health").await;
        sm.observe(Signal::Health, now_ns, health_ok);
        let load_ok = load_gen_clean("/tmp/loadgen-stats.csv");
        sm.observe(Signal::LoadGen5xx, now_ns, load_ok);
        let prom_ok = prom_error_rate_clean().await;
        sm.observe(Signal::PromErrorRate, now_ns, prom_ok);

        if let Some(ts) = sm.recovery_ts_if_held(now_ns) { recovery_ts = Some(ts); break; }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    if recovery_ts.is_none() && status == "clean" { status = "no_recovery"; }

    let ended = Utc::now();
    sqlx::query("INSERT INTO experiments (id, yaml_path, yaml_sha256, started_at, ended_at, primary_faulted_service, failure_class, blast_radius, clean_services, runner_version, status, notes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&exp.id).bind(path.to_string_lossy())
        .bind(&sha)
        .bind(started.timestamp_nanos_opt().unwrap_or(0))
        .bind(ended.timestamp_nanos_opt().unwrap_or(0))
        .bind(&exp.ground_truth.primary_faulted_service)
        .bind(&exp.ground_truth.failure_class)
        .bind(serde_json::to_string(&exp.ground_truth.expected_blast_radius)?)
        .bind(serde_json::to_string(&exp.ground_truth.expected_clean_services)?)
        .bind(env!("CARGO_PKG_VERSION"))
        .bind(status)
        .bind::<Option<String>>(None)
        .execute(pool).await?;

    for (i, fault) in exp.faults.iter().enumerate() {
        sqlx::query("INSERT INTO fault_events (experiment_id, sequence_no, kind, target, started_at, ended_at, config_json) VALUES (?,?,?,?,?,?,?)")
            .bind(&exp.id).bind(i as i64).bind(spec_kind(&fault.spec)).bind(spec_target(&fault.spec))
            .bind(started.timestamp_nanos_opt().unwrap_or(0) + (fault.at_sec as i64) * 1_000_000_000)
            .bind(started.timestamp_nanos_opt().unwrap_or(0) + (fault.until_sec as i64) * 1_000_000_000)
            .bind(serde_json::to_string(&fault.spec)?)
            .execute(pool).await?;
    }
    let _ = _db::open; // silence unused
    Ok(())
}

fn spec_kind(s: &FaultSpec) -> &'static str {
    match s { FaultSpec::Toxiproxy { .. } => "toxiproxy",
              FaultSpec::PumbaKill { .. } | FaultSpec::PumbaPause { .. } | FaultSpec::PumbaStress { .. } => "pumba" }
}

fn spec_target(s: &FaultSpec) -> String {
    match s {
        FaultSpec::Toxiproxy { proxy, .. } => proxy.clone(),
        FaultSpec::PumbaKill { container } | FaultSpec::PumbaPause { container, .. } | FaultSpec::PumbaStress { container, .. } => container.clone(),
    }
}

async fn check_health(url: &str) -> bool {
    reqwest::get(url).await.map(|r| r.status().is_success()).unwrap_or(false)
}

fn load_gen_clean(path: &str) -> bool {
    let s = std::fs::read_to_string(path).unwrap_or_default();
    s.lines().rev().take(5).all(|line| {
        line.split(',').nth(3).and_then(|v| v.parse::<u64>().ok()).unwrap_or(1) == 0
    })
}

async fn prom_error_rate_clean() -> bool {
    let base = std::env::var("PROM_URL").unwrap_or("http://prometheus:9090".into());
    let query = "sum(rate(http_requests_total{status=~\"5..\"}[30s]))";
    let r = match reqwest::Client::new().get(format!("{base}/api/v1/query"))
        .query(&[("query", query)]).send().await {
        Ok(r) => r, Err(_) => return false,
    };
    let v: serde_json::Value = r.json().await.unwrap_or_default();
    let val: f64 = v["data"]["result"].as_array().and_then(|a| a.first())
        .and_then(|p| p["value"][1].as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    val < 0.1
}
