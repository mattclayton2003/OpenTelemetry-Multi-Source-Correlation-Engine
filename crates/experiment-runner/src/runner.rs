use crate::{recovery::*, spec::*};
use chaos::driver::{DefaultDriver, FaultDriver, FaultSpec};
use chaos::toxiproxy::ToxiproxyClient;
use chrono::Utc;
use sha2::Digest;
use sqlx::SqlitePool;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

const LOADGEN_STATS_PATH: &str = "/tmp/loadgen-stats.csv";

/// Runs one experiment YAML end to end and records it. Returns the experiment
/// id so callers (e.g. the eval harness) can scope downstream work to exactly
/// the experiments they ran.
pub async fn run_file(path: &Path, pool: &SqlitePool, dry_run: bool) -> anyhow::Result<String> {
    // Lock lives in a guaranteed-writable temp dir, NOT next to the YAML
    // (the YAML's parent may be a read-only mount in containers).
    let lock_path = std::env::temp_dir().join("experiment-runner.lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    fs2::FileExt::try_lock_exclusive(&lock_file).map_err(|_| {
        anyhow::anyhow!(
            "another experiment is running (lock: {})",
            lock_path.display()
        )
    })?;

    let yaml_text = std::fs::read_to_string(path)?;
    let exp: Experiment = serde_yaml::from_str(&yaml_text)?;
    let sha = {
        let mut h = sha2::Sha256::new();
        h.update(yaml_text.as_bytes());
        format!("sha256:{:x}", h.finalize())
    };

    let toxi = ToxiproxyClient::new(
        std::env::var("TOXIPROXY_URL").unwrap_or("http://toxiproxy:8474".into()),
    );
    let driver = DefaultDriver { toxi };

    tracing::info!(id = %exp.id, warmup_sec = exp.warmup_sec, "warmup");
    if dry_run {
        tracing::info!("dry_run: validated YAML and connectivity");
        return Ok(exp.id.clone());
    }
    tokio::time::sleep(Duration::from_secs(exp.warmup_sec as u64)).await;

    // t=0 for the experiment timeline. Every fault `at_sec`/`until_sec` and
    // `duration_sec` is an absolute offset from this instant, so the labels we
    // record below match exactly when faults were injected and reverted.
    let started = Utc::now();
    let started_ns = started.timestamp_nanos_opt().unwrap_or(0);
    let base = tokio::time::Instant::now();

    // Merged, time-ordered schedule of apply/revert events. This injects each
    // fault at its own `at_sec` and reverts it at its own `until_sec`, so faults
    // may overlap or nest correctly (the previous code applied faults at
    // cumulative relative offsets and reverted them all at once).
    #[derive(Clone, Copy)]
    enum Ev {
        Apply(usize),
        Revert(usize),
    }
    let mut events: Vec<(u32, Ev)> = Vec::with_capacity(exp.faults.len() * 2);
    for (i, f) in exp.faults.iter().enumerate() {
        events.push((f.at_sec, Ev::Apply(i)));
        events.push((f.until_sec, Ev::Revert(i)));
    }
    // Ties: apply before revert at the same instant.
    events.sort_by_key(|(t, ev)| (*t, matches!(ev, Ev::Revert(_))));

    let mut handles: Vec<Option<chaos::driver::FaultHandle>> =
        (0..exp.faults.len()).map(|_| None).collect();
    let mut status = "clean";
    for (t, ev) in events {
        tokio::time::sleep_until(base + Duration::from_secs(t as u64)).await;
        match ev {
            Ev::Apply(i) => {
                handles[i] = Some(driver.apply(&exp.faults[i].spec).await?);
            }
            Ev::Revert(i) => {
                if let Some(h) = handles[i].take() {
                    if let Err(e) = driver.revert(&h).await {
                        tracing::error!("revert failed: {e}");
                        status = "dirty";
                    }
                }
            }
        }
    }
    // Safety net for malformed specs (e.g. until_sec < at_sec): revert anything
    // still applied so we never leak a fault past the experiment.
    for h in handles.into_iter().flatten() {
        if let Err(e) = driver.revert(&h).await {
            tracing::error!("revert failed: {e}");
            status = "dirty";
        }
    }
    // Hold the experiment open until duration_sec from t=0 (no-op if already past).
    tokio::time::sleep_until(base + Duration::from_secs(exp.duration_sec as u64)).await;

    // Recovery detection. Only require signals that are actually observable in
    // this environment: the load-gen 5xx signal depends on a stats file that
    // may not exist, and treating its absence as "clear" would fabricate a
    // recovery, so it is excluded from the required set when unavailable.
    let loadgen_available = std::path::Path::new(LOADGEN_STATS_PATH).exists();
    let mut required = BTreeSet::from([Signal::Health, Signal::PromErrorRate]);
    if loadgen_available {
        required.insert(Signal::LoadGen5xx);
    } else {
        tracing::warn!(
            path = LOADGEN_STATS_PATH,
            "load-gen stats not found; excluding LoadGen5xx from recovery detection"
        );
    }
    let mut sm =
        SignalStateMachine::new(Duration::from_secs(exp.recovery_grace_sec as u64), required);
    let recovery_deadline = std::time::Instant::now()
        + Duration::from_secs((exp.cooldown_sec + exp.recovery_grace_sec) as u64);
    let mut recovery_ts: Option<i64> = None;
    while std::time::Instant::now() < recovery_deadline {
        let now_ns = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let health_ok = check_health("http://auth:8001/health").await
            && check_health("http://accounts:8002/health").await
            && check_health("http://transactions:8003/health").await
            && check_health("http://notifications:8004/health").await;
        sm.observe(Signal::Health, now_ns, health_ok);
        if loadgen_available {
            sm.observe(
                Signal::LoadGen5xx,
                now_ns,
                load_gen_clean(LOADGEN_STATS_PATH),
            );
        }
        let prom_ok = prom_error_rate_clean().await;
        sm.observe(Signal::PromErrorRate, now_ns, prom_ok);

        if let Some(ts) = sm.recovery_ts_if_held(now_ns) {
            recovery_ts = Some(ts);
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    if recovery_ts.is_none() && status == "clean" {
        status = "no_recovery";
    }

    let ended = Utc::now();
    sqlx::query("INSERT OR REPLACE INTO experiments (id, yaml_path, yaml_sha256, started_at, ended_at, primary_faulted_service, failure_class, blast_radius, clean_services, runner_version, status, notes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&exp.id).bind(path.to_string_lossy())
        .bind(&sha)
        .bind(started_ns)
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
        sqlx::query("INSERT OR REPLACE INTO fault_events (experiment_id, sequence_no, kind, target, started_at, ended_at, config_json) VALUES (?,?,?,?,?,?,?)")
            .bind(&exp.id).bind(i as i64).bind(spec_kind(&fault.spec)).bind(spec_target(&fault.spec))
            .bind(started_ns + (fault.at_sec as i64) * 1_000_000_000)
            .bind(started_ns + (fault.until_sec as i64) * 1_000_000_000)
            .bind(serde_json::to_string(&fault.spec)?)
            .execute(pool).await?;
    }
    Ok(exp.id)
}

fn spec_kind(s: &FaultSpec) -> &'static str {
    match s {
        FaultSpec::Toxiproxy { .. } => "toxiproxy",
        FaultSpec::PumbaKill { .. }
        | FaultSpec::PumbaPause { .. }
        | FaultSpec::PumbaStress { .. } => "pumba",
    }
}

fn spec_target(s: &FaultSpec) -> String {
    match s {
        FaultSpec::Toxiproxy { proxy, .. } => proxy.clone(),
        FaultSpec::PumbaKill { container }
        | FaultSpec::PumbaPause { container, .. }
        | FaultSpec::PumbaStress { container, .. } => container.clone(),
    }
}

async fn check_health(url: &str) -> bool {
    reqwest::get(url)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn load_gen_clean(path: &str) -> bool {
    // A missing or empty stats file is "unknown", not "clean": returning true
    // here previously let recovery be declared with no evidence at all.
    let Ok(s) = std::fs::read_to_string(path) else {
        return false;
    };
    let recent: Vec<&str> = s.lines().rev().take(5).collect();
    if recent.is_empty() {
        return false;
    }
    recent
        .iter()
        .all(|line| line.split(',').nth(3).and_then(|v| v.parse::<u64>().ok()) == Some(0))
}

async fn prom_error_rate_clean() -> bool {
    let base = std::env::var("PROM_URL").unwrap_or("http://prometheus:9090".into());
    let query = "sum(rate(http_requests_total{status=~\"5..\"}[30s]))";
    let r = match reqwest::Client::new()
        .get(format!("{base}/api/v1/query"))
        .query(&[("query", query)])
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    let v: serde_json::Value = r.json().await.unwrap_or_default();
    let val: f64 = v["data"]["result"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["value"][1].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    val < 0.1
}
