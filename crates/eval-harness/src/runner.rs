use crate::{coverage::*, scoring::*};
use chrono::{DateTime, Duration, Utc};
use correlation_core::schema::Trigger;
use correlation_core::time::WallClock;
use correlation_core::{CorrelationConfig, Engine, IncidentContext, MultiBackend};
use correlation_tempo::TempoClient;
use sqlx::SqlitePool;
use std::sync::Arc;

pub struct EvalContext {
    pub engine: Arc<Engine>,
    /// Separate Tempo client used to *discover* a representative trace id per
    /// experiment (the engine's backend is type-erased, so search is not
    /// reachable through it).
    pub tempo: Arc<TempoClient>,
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

pub async fn run_suite(
    ctx: &EvalContext,
    yaml_paths: Vec<std::path::PathBuf>,
    tag: String,
) -> anyhow::Result<()> {
    let eval_run_id = uuid::Uuid::now_v7().to_string();
    let started = Utc::now();

    // 1. Run each experiment via experiment-runner, collecting the ids we
    //    actually ran so scoring is scoped to this invocation (the labels DB
    //    may contain experiments from previous runs).
    let mut ran_ids: Vec<String> = Vec::new();
    for path in &yaml_paths {
        ran_ids.push(experiment_runner::runner::run_file(path, &ctx.labels_db, false).await?);
    }
    ran_ids.sort();
    ran_ids.dedup();

    // 2. Settle for telemetry to land
    tokio::time::sleep(std::time::Duration::from_secs(ctx.settle_sec)).await;

    // 2b. Insert eval_runs parent row FIRST so eval_results FK is satisfied.
    //     ended_at is updated at the end of scoring.
    sqlx::query("INSERT INTO eval_runs (eval_run_id, tag, started_at, ended_at, config_hash, engine_version, runner_version, scoring_toml_hash, config_json) VALUES (?,?,?,?,?,?,?,?,?)")
        .bind(&eval_run_id).bind(&tag)
        .bind(started.timestamp_nanos_opt().unwrap_or(0))
        .bind(started.timestamp_nanos_opt().unwrap_or(0)) // placeholder; updated after scoring
        .bind(&ctx.config_hash)
        .bind(env!("CARGO_PKG_VERSION"))
        .bind(experiment_runner_version())
        .bind(&ctx.scoring_hash)
        .bind(serde_json::to_string(&ctx.engine.cfg)?)
        .execute(&ctx.eval_db).await?;

    // 3. Score only the experiments we ran this invocation, in both modes.
    for exp_id in &ran_ids {
        let (primary, class, started_ns, ended_ns): (String, String, i64, i64) = sqlx::query_as(
            "SELECT primary_faulted_service, failure_class, started_at, ended_at FROM experiments WHERE id = ?",
        )
        .bind(exp_id)
        .fetch_one(&ctx.labels_db)
        .await?;
        let start = chrono_from_ns(started_ns);
        let end = chrono_from_ns(ended_ns);

        // Trace mode: discover a real, representative trace for this experiment
        // from Tempo (preferring an error trace on the faulted service within
        // the window). If none is found we record an explicit degraded incident
        // rather than scoring a fabricated trace id.
        let ic_trace = match discover_trace_id(&ctx.tempo, &primary, start, end).await {
            TraceDiscovery::Found(trace_id) => ctx
                .engine
                .correlate_trace(trace_id)
                .await
                .unwrap_or_else(|_| {
                    degraded_incident("harness_failure: engine call failed", start, end)
                }),
            TraceDiscovery::NoneFound => degraded_incident(
                &format!("trace_mode: no trace found for service '{primary}' in window"),
                start,
                end,
            ),
            TraceDiscovery::Unreachable => degraded_incident(
                &format!("trace_mode: tempo search unreachable for service '{primary}'"),
                start,
                end,
            ),
        };
        score_and_record(
            ctx,
            &eval_run_id,
            exp_id,
            "trace",
            &ic_trace,
            &primary,
            &class,
        )
        .await?;

        // Anomaly mode: look up per-class invocation
        let entry = ctx.invocation.classes.get(&class);
        let (metric, service, w_pre, w_post) = match entry {
            Some(e) => (
                e.metric.clone(),
                e.service.clone(),
                e.window_pre_sec,
                e.window_post_sec,
            ),
            None => (String::from("up"), primary.clone(), 0, 120),
        };
        let aw_start = start - Duration::seconds(w_pre);
        let aw_end = end + Duration::seconds(w_post);
        let ic_anom = ctx
            .engine
            .correlate_anomaly(metric, service, aw_start, aw_end, 1.0)
            .await
            .unwrap_or_else(|_| {
                degraded_incident("harness_failure: engine call failed", aw_start, aw_end)
            });
        score_and_record(
            ctx,
            &eval_run_id,
            exp_id,
            "anomaly",
            &ic_anom,
            &primary,
            &class,
        )
        .await?;
    }

    let ended = Utc::now();
    sqlx::query("UPDATE eval_runs SET ended_at = ? WHERE eval_run_id = ?")
        .bind(ended.timestamp_nanos_opt().unwrap_or(0))
        .bind(&eval_run_id)
        .execute(&ctx.eval_db)
        .await?;
    Ok(())
}

/// Outcome of looking for a representative trace for an experiment.
enum TraceDiscovery {
    /// A trace id was found.
    Found(String),
    /// Tempo was reachable but had no matching trace in the window.
    NoneFound,
    /// Tempo could not be reached (distinct from "no trace exists" so the
    /// degraded incident's note doesn't misreport infra outages as empty data).
    Unreachable,
}

/// Picks the most representative trace among `hits` for a fault on `faulted`.
///
/// Prefers a trace where the faulted service is a *dependency* of a larger
/// request (root != faulted — i.e. the full request path that exercised it),
/// then the longest-duration trace (the one most impacted by a latency fault).
/// This avoids selecting trivial single-span `/health` or `/metrics` traces
/// that merely touch the faulted service.
fn pick_best(hits: Vec<correlation_tempo::TraceHit>, faulted: &str) -> Option<String> {
    hits.into_iter()
        .max_by_key(|h| (h.root_service != faulted, h.duration_ms))
        .map(|h| h.trace_id)
}

/// Finds a representative trace id for `service` within `[start, end]`,
/// preferring an error trace, then the most fault-impacted request trace.
async fn discover_trace_id(
    tempo: &TempoClient,
    service: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> TraceDiscovery {
    let (s, e) = (start.timestamp(), end.timestamp());
    let error_q = format!("{{ resource.service.name = \"{service}\" && status = error }}");
    if let Ok(hits) = tempo.search_traces(&error_q, s, e, 10).await {
        if let Some(id) = pick_best(hits, service) {
            return TraceDiscovery::Found(id);
        }
    }
    // Latency faults produce slow-but-successful traces. Target them directly
    // with a duration filter so the impacted trace isn't missed just because
    // many fast traces are more recent (Tempo search returns newest-first).
    let slow_q = format!("{{ resource.service.name = \"{service}\" && duration > 250ms }}");
    if let Ok(hits) = tempo.search_traces(&slow_q, s, e, 20).await {
        if let Some(id) = pick_best(hits, service) {
            return TraceDiscovery::Found(id);
        }
    }
    // Fall back to any trace touching the service. This last query also tells
    // us whether Tempo is actually reachable (vs. the earlier queries just being
    // transient misses).
    let any_q = format!("{{ resource.service.name = \"{service}\" }}");
    match tempo.search_traces(&any_q, s, e, 20).await {
        Ok(hits) if hits.is_empty() => TraceDiscovery::NoneFound,
        Ok(hits) => match pick_best(hits, service) {
            Some(id) => TraceDiscovery::Found(id),
            None => TraceDiscovery::NoneFound,
        },
        Err(_) => TraceDiscovery::Unreachable,
    }
}

fn chrono_from_ns(ns: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_nanos(ns)
}
fn experiment_runner_version() -> String {
    "0.1.0".into()
}

/// An incident placeholder used when the engine could not produce a real one
/// (engine error, or no trace found). The `note` records why so the report can
/// distinguish a degraded run from a genuine empty result.
fn degraded_incident(note: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> IncidentContext {
    use correlation_core::schema::*;
    IncidentContext {
        schema_version: SCHEMA_VERSION.into(),
        incident_id: uuid::Uuid::now_v7().to_string(),
        produced_at: Utc::now(),
        engine_version: "n/a".into(),
        config_hash: "n/a".into(),
        elapsed_ms: 0,
        trigger: Trigger::Trace {
            trace: TraceTrigger {
                trace_id: "n/a".into(),
            },
        },
        window: Window {
            start,
            end,
            expanded: false,
        },
        services: vec![],
        suspects: vec![],
        spans: vec![],
        span_tree: vec![],
        log_batches: vec![],
        metric_anomalies: vec![],
        timeline: vec![],
        notes: vec![note.to_string()],
    }
}

/// Persists the produced incident so reproduce/canary tooling has the actual
/// document to compare against (previously the incidents DB was never written).
async fn persist_incident(
    ctx: &EvalContext,
    exp_id: &str,
    ic: &IncidentContext,
) -> anyhow::Result<()> {
    let (trigger_kind, trigger_input) = match &ic.trigger {
        Trigger::Trace { trace } => ("trace", trace.trace_id.clone()),
        Trigger::Anomaly { anomaly } => {
            ("anomaly", format!("{}@{}", anomaly.metric, anomaly.service))
        }
    };
    sqlx::query("INSERT OR REPLACE INTO incidents (incident_id, schema_version, engine_version, config_hash, trigger_kind, trigger_input, window_start, window_end, elapsed_ms, produced_at, document, experiment_id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&ic.incident_id)
        .bind(&ic.schema_version)
        .bind(&ic.engine_version)
        .bind(&ic.config_hash)
        .bind(trigger_kind)
        .bind(trigger_input)
        .bind(ic.window.start.timestamp_nanos_opt().unwrap_or(0))
        .bind(ic.window.end.timestamp_nanos_opt().unwrap_or(0))
        .bind(ic.elapsed_ms as i64)
        .bind(ic.produced_at.timestamp_nanos_opt().unwrap_or(0))
        .bind(serde_json::to_string(ic)?)
        .bind(exp_id)
        .execute(&ctx.incidents_db)
        .await?;
    Ok(())
}

async fn score_and_record(
    ctx: &EvalContext,
    eval_run_id: &str,
    exp_id: &str,
    mode: &str,
    ic: &IncidentContext,
    primary: &str,
    class: &str,
) -> anyhow::Result<()> {
    persist_incident(ctx, exp_id, ic).await?;

    let suspects: Vec<String> = ic.suspects.iter().map(|s| s.service.clone()).collect();
    let r1 = recall_at_k(&suspects, primary, 1);
    let r3 = recall_at_k(&suspects, primary, 3);
    let r5 = recall_at_k(&suspects, primary, 5);

    let row: (String, String) =
        sqlx::query_as("SELECT blast_radius, clean_services FROM experiments WHERE id = ?")
            .bind(exp_id)
            .fetch_one(&ctx.labels_db)
            .await?;
    let blast: Vec<String> = serde_json::from_str(&row.0).unwrap_or_default();
    let clean: Vec<String> = serde_json::from_str(&row.1).unwrap_or_default();
    let blast_refs: Vec<&str> = blast.iter().map(|s| s.as_str()).collect();
    let clean_set: std::collections::HashSet<&str> = clean.iter().map(|s| s.as_str()).collect();

    let p1 = precision_at_k(&suspects, &[primary], &blast_refs, 1);
    let p3 = precision_at_k(&suspects, &[primary], &blast_refs, 3);
    let p5 = precision_at_k(&suspects, &[primary], &blast_refs, 5);
    let clean_fps = suspects
        .iter()
        .take(3)
        .filter(|s| clean_set.contains(s.as_str()))
        .count() as i64;
    let normalized_clean_fps = (clean_fps as f64) / 3.0;

    let expected_metrics = ctx.coverage.expected_for(class);
    let trace_cov = trace_coverage(ic, 1);
    let log_cov = error_log_coverage(ic, 1);
    let anom_cov = anomaly_coverage(ic, &expected_metrics);
    let tree = tree_integrity(ic);
    let mean = (trace_cov + log_cov + anom_cov + tree) / 4.0;

    let comp = composite(
        &ScoreInputs {
            recall_at_3: r3,
            precision_at_3: p3,
            completeness_mean: mean,
            elapsed_ms: ic.elapsed_ms as i64,
            normalized_clean_fps,
        },
        &ctx.weights,
    );

    sqlx::query("INSERT OR REPLACE INTO eval_results (eval_run_id, experiment_id, invocation_mode, incident_id, recall_at_1, recall_at_3, recall_at_5, precision_at_1, precision_at_3, precision_at_5, trace_coverage, error_log_coverage, anomaly_coverage, tree_integrity, elapsed_ms, clean_fps, composite, notes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(eval_run_id).bind(exp_id).bind(mode).bind(&ic.incident_id)
        .bind(r1).bind(r3).bind(r5).bind(p1).bind(p3).bind(p5)
        .bind(trace_cov).bind(log_cov).bind(anom_cov).bind(tree)
        .bind(ic.elapsed_ms as i64).bind(clean_fps).bind(comp)
        .bind(serde_json::to_string(&ic.notes)?)
        .execute(&ctx.eval_db).await?;
    Ok(())
}

/// Inputs for a full eval run, bundled to keep the entrypoint signature small.
pub struct EvalRunArgs {
    pub labels: SqlitePool,
    pub incidents: SqlitePool,
    pub eval: SqlitePool,
    pub suite_glob: String,
    pub config_path: std::path::PathBuf,
    pub scoring_path: std::path::PathBuf,
    pub coverage_path: std::path::PathBuf,
    pub invocation_path: std::path::PathBuf,
    pub tag: String,
}

pub async fn run_from_files(args: EvalRunArgs) -> anyhow::Result<()> {
    let (weights, scoring_hash) = crate::scoring::load_weights(&args.scoring_path)?;
    let coverage = CoverageTargets::load(&args.coverage_path)?;
    let invocation = AnomalyInvocation::load(&args.invocation_path)?;

    // Engine config: load from the --config TOML when present, else defaults.
    // A *missing* file falls back to defaults (the flag is optional); a parse
    // error or any other read error (permissions, etc.) is surfaced rather than
    // silently defaulting, so the recorded config_hash always matches reality.
    let cfg = match std::fs::read_to_string(&args.config_path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("parse config {}: {e}", args.config_path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => CorrelationConfig::default(),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "read config {}: {e}",
                args.config_path.display()
            ))
        }
    };

    let tempo_url = std::env::var("TEMPO_URL").unwrap_or("http://tempo:3200".into());
    let tempo = Arc::new(TempoClient::new(tempo_url.clone()));
    let backend = MultiBackend {
        traces: Arc::new(TempoClient::new(tempo_url)),
        logs: Arc::new(correlation_loki::LokiClient::new(
            std::env::var("LOKI_URL").unwrap_or("http://loki:3100".into()),
        )),
        metrics: Arc::new(correlation_prom::PromClient::new(
            std::env::var("PROM_URL").unwrap_or("http://prometheus:9090".into()),
        )),
    };
    let cfg_hash = cfg.hash();
    let engine = Arc::new(Engine::new(Arc::new(backend), cfg, Arc::new(WallClock)));
    let ctx = EvalContext {
        engine,
        tempo,
        labels_db: args.labels,
        incidents_db: args.incidents,
        eval_db: args.eval,
        weights,
        scoring_hash,
        coverage,
        invocation,
        config_hash: cfg_hash,
        settle_sec: 15,
    };
    let yaml_paths: Vec<std::path::PathBuf> = glob::glob(&args.suite_glob)?
        .filter_map(|e| e.ok())
        .collect();
    run_suite(&ctx, yaml_paths, args.tag).await
}
