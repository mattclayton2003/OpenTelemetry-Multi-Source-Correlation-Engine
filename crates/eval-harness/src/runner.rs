use crate::{db, scoring::*, coverage::*};
use correlation_core::{Engine, CorrelationConfig, MultiBackend, IncidentContext};
use correlation_core::time::WallClock;
use sqlx::SqlitePool;
use std::sync::Arc;
use chrono::{DateTime, Utc, Duration};

pub struct EvalContext {
    pub engine: Arc<Engine>,
    pub labels_db: SqlitePool,
    pub incidents_db: SqlitePool,
    pub eval_db: SqlitePool,
    pub weights: Weights,
    pub scoring_hash: String,
    pub coverage: CoverageTargets,
    pub invocation: AnomalyInvocation,
    pub config_hash: String,
    pub settle_sec: u64,
}

pub async fn run_suite(ctx: &EvalContext, yaml_paths: Vec<std::path::PathBuf>, tag: String) -> anyhow::Result<()> {
    let eval_run_id = uuid::Uuid::now_v7().to_string();
    let started = Utc::now();

    // 1. Run each experiment via experiment-runner
    for path in &yaml_paths {
        experiment_runner::runner::run_file(path, &ctx.labels_db, false).await?;
    }

    // 2. Settle for telemetry to land
    tokio::time::sleep(std::time::Duration::from_secs(ctx.settle_sec)).await;

    // 3. For each experiment row in labels DB, invoke engine in both modes and score
    let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT id, primary_faulted_service, failure_class, started_at, ended_at FROM experiments"
    ).fetch_all(&ctx.labels_db).await?;

    for (exp_id, primary, class, started_ns, ended_ns) in rows {
        let start = chrono_from_ns(started_ns);
        let end   = chrono_from_ns(ended_ns);

        // Trace mode: pick a trace_id. For v1, hardcoded "auto"; real impl uses TraceQL search.
        let trace_id = "auto".to_string();
        let ic_trace = ctx.engine.correlate_trace(trace_id).await
            .unwrap_or_else(|_| empty_incident_marker());
        score_and_record(ctx, &eval_run_id, &exp_id, "trace", &ic_trace, &primary, &class).await?;

        // Anomaly mode: look up per-class invocation
        let entry = ctx.invocation.classes.get(&class);
        let (metric, service, w_pre, w_post) = match entry {
            Some(e) => (e.metric.clone(), e.service.clone(), e.window_pre_sec, e.window_post_sec),
            None => (String::from("up"), primary.clone(), 0, 120),
        };
        let aw_start = start - Duration::seconds(w_pre);
        let aw_end   = end   + Duration::seconds(w_post);
        let ic_anom = ctx.engine.correlate_anomaly(metric, service, aw_start, aw_end, 1.0).await
            .unwrap_or_else(|_| empty_incident_marker());
        score_and_record(ctx, &eval_run_id, &exp_id, "anomaly", &ic_anom, &primary, &class).await?;
    }

    let ended = Utc::now();
    sqlx::query("INSERT INTO eval_runs (eval_run_id, tag, started_at, ended_at, config_hash, engine_version, runner_version, scoring_toml_hash, config_json) VALUES (?,?,?,?,?,?,?,?,?)")
        .bind(&eval_run_id).bind(&tag)
        .bind(started.timestamp_nanos_opt().unwrap_or(0))
        .bind(ended.timestamp_nanos_opt().unwrap_or(0))
        .bind(&ctx.config_hash)
        .bind(env!("CARGO_PKG_VERSION"))
        .bind(experiment_runner_version())
        .bind(&ctx.scoring_hash)
        .bind(serde_json::to_string(&ctx.engine.cfg)?)
        .execute(&ctx.eval_db).await?;
    Ok(())
}

fn chrono_from_ns(ns: i64) -> DateTime<Utc> { DateTime::<Utc>::from_timestamp_nanos(ns) }
fn experiment_runner_version() -> String { "0.1.0".into() }

fn empty_incident_marker() -> IncidentContext {
    use correlation_core::schema::*;
    IncidentContext {
        schema_version: SCHEMA_VERSION.into(),
        incident_id: uuid::Uuid::now_v7().to_string(),
        produced_at: Utc::now(), engine_version: "n/a".into(), config_hash: "n/a".into(),
        elapsed_ms: 0,
        trigger: Trigger::Trace { trace: TraceTrigger { trace_id: "n/a".into() } },
        window: Window { start: Utc::now(), end: Utc::now(), expanded: false },
        services: vec![], suspects: vec![], spans: vec![], span_tree: vec![],
        log_batches: vec![], metric_anomalies: vec![], timeline: vec![],
        notes: vec!["harness_failure: engine call failed".into()],
    }
}

async fn score_and_record(ctx: &EvalContext, eval_run_id: &str, exp_id: &str, mode: &str,
                          ic: &IncidentContext, primary: &str, class: &str) -> anyhow::Result<()> {
    let suspects: Vec<String> = ic.suspects.iter().map(|s| s.service.clone()).collect();
    let r1 = recall_at_k(&suspects, primary, 1);
    let r3 = recall_at_k(&suspects, primary, 3);
    let r5 = recall_at_k(&suspects, primary, 5);

    let row: (String, String) = sqlx::query_as(
        "SELECT blast_radius, clean_services FROM experiments WHERE id = ?"
    ).bind(exp_id).fetch_one(&ctx.labels_db).await?;
    let blast: Vec<String> = serde_json::from_str(&row.0).unwrap_or_default();
    let clean: Vec<String> = serde_json::from_str(&row.1).unwrap_or_default();
    let blast_refs: Vec<&str> = blast.iter().map(|s| s.as_str()).collect();
    let clean_set: std::collections::HashSet<&str> = clean.iter().map(|s| s.as_str()).collect();

    let p1 = precision_at_k(&suspects, &[primary], &blast_refs, 1);
    let p3 = precision_at_k(&suspects, &[primary], &blast_refs, 3);
    let p5 = precision_at_k(&suspects, &[primary], &blast_refs, 5);
    let clean_fps = suspects.iter().take(3).filter(|s| clean_set.contains(s.as_str())).count() as i64;
    let normalized_clean_fps = (clean_fps as f64) / 3.0;

    let expected_metrics = ctx.coverage.expected_for(class);
    let trace_cov = trace_coverage(ic, 1);
    let log_cov   = error_log_coverage(ic, 1);
    let anom_cov  = anomaly_coverage(ic, &expected_metrics);
    let tree      = tree_integrity(ic);
    let mean = (trace_cov + log_cov + anom_cov + tree) / 4.0;

    let comp = composite(&ScoreInputs {
        recall_at_3: r3, precision_at_3: p3,
        completeness_mean: mean, elapsed_ms: ic.elapsed_ms as i64,
        normalized_clean_fps,
    }, &ctx.weights);

    sqlx::query("INSERT OR REPLACE INTO eval_results (eval_run_id, experiment_id, invocation_mode, incident_id, recall_at_1, recall_at_3, recall_at_5, precision_at_1, precision_at_3, precision_at_5, trace_coverage, error_log_coverage, anomaly_coverage, tree_integrity, elapsed_ms, clean_fps, composite, notes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(eval_run_id).bind(exp_id).bind(mode).bind(&ic.incident_id)
        .bind(r1).bind(r3).bind(r5).bind(p1).bind(p3).bind(p5)
        .bind(trace_cov).bind(log_cov).bind(anom_cov).bind(tree)
        .bind(ic.elapsed_ms as i64).bind(clean_fps).bind(comp)
        .bind(serde_json::to_string(&ic.notes)?)
        .execute(&ctx.eval_db).await?;
    Ok(())
}

pub async fn run_from_files(
    labels: SqlitePool, incidents: SqlitePool, eval: SqlitePool,
    suite_glob: String, _config_path: std::path::PathBuf,
    scoring_path: std::path::PathBuf, coverage_path: std::path::PathBuf,
    invocation_path: std::path::PathBuf, tag: String,
) -> anyhow::Result<()> {
    let _ = db::open;
    let (weights, scoring_hash) = crate::scoring::load_weights(&scoring_path)?;
    let coverage = CoverageTargets::load(&coverage_path)?;
    let invocation = AnomalyInvocation::load(&invocation_path)?;
    let cfg = CorrelationConfig::default();
    let backend = MultiBackend {
        traces:  Arc::new(correlation_tempo::TempoClient::new(std::env::var("TEMPO_URL").unwrap_or("http://tempo:3200".into()))),
        logs:    Arc::new(correlation_loki::LokiClient::new(std::env::var("LOKI_URL").unwrap_or("http://loki:3100".into()))),
        metrics: Arc::new(correlation_prom::PromClient::new(std::env::var("PROM_URL").unwrap_or("http://prometheus:9090".into()))),
    };
    let cfg_hash = cfg.hash();
    let engine = Arc::new(Engine::new(Arc::new(backend), cfg, Arc::new(WallClock)));
    let ctx = EvalContext {
        engine, labels_db: labels, incidents_db: incidents, eval_db: eval,
        weights, scoring_hash, coverage, invocation, config_hash: cfg_hash, settle_sec: 15,
    };
    let yaml_paths: Vec<std::path::PathBuf> = glob::glob(&suite_glob)?
        .filter_map(|e| e.ok()).collect();
    run_suite(&ctx, yaml_paths, tag).await
}
