# OpenTelemetry Multi-Source Correlation Engine — Design

- **Date:** 2026-05-23
- **Status:** Design proposed; awaiting user review
- **Approach:** Evidence graph + deterministic scoring (Approach B from brainstorming)

## Summary

Build a small distributed banking sandbox (4 Rust microservices) instrumented with OpenTelemetry, ship logs/metrics/traces into Tempo + Loki + Prometheus, and on top of that build a Rust correlation engine that, given a `trace_id` or an anomalous metric, produces a structured `IncidentContext` document containing the related spans, logs, metric anomalies, ranked suspect services, and an audit trail of why each suspect was ranked. A dedicated experiment runner injects labeled chaos (Toxiproxy + Pumba) and produces a ground-truth dataset; an evaluation harness scores the engine against that dataset on three metrics (recall on root cause, completeness, time-to-context).

The research artifacts produced by this project are: (1) the correlation engine itself, (2) the labeled chaos dataset, and (3) a reproducible benchmark harness. All three are intended as the foundation for downstream ML/GNN research.

## Goals

- A reproducible Docker Compose stack: `git clone && docker compose up` yields a working observability sandbox.
- A correlation engine that handles all four levels of correlation scope: trace-anchored lookup, temporal context expansion, anomaly-driven correlation, and span-tree-based incident reconstruction.
- A labeled ground-truth dataset of chaos experiments expressed as YAML, with `(experiment_id, scenario, fault, recovery_ts, primary_faulted_service, blast_radius, clean_services, failure_class)` tuples persisted to SQLite.
- An evaluation harness that scores the engine on precision/recall@k for root-cause service, four completeness ratios, and time-to-context, combined into a documented composite score, with parameter-sweep support.
- Determinism: same telemetry snapshot + same `CorrelationConfig` → byte-identical `IncidentContext` JSON. Verified by an `eval reproduce` canary.
- Loud degradation: every adapter failure, retention miss, or empty result is attributed in `IncidentContext.notes[]` rather than silently affecting scores.

## Non-goals (v1)

- Image publishing to a private registry. Compose builds local images. Documented as an easy retrofit if cross-machine eval snapshots become necessary.
- Rich Grafana dashboards beyond one default. The sandbox is browsable; dashboards are future work.
- Web UI for incident contexts. Markdown renderer covers human reading.
- Authentication on the HTTP shell. Binds to 127.0.0.1 by default; documented security model.
- ML-based anomaly detection or ranking. Z-score and EWMA are deliberate baselines for future research to beat.
- ClickHouse adapter. The `TelemetryBackend` trait is ready; implementation deferred.
- Multi-tenant telemetry. Single namespace per Compose stack.

---

## 1. System Architecture

Three planes, one Compose file. The research plane lives behind a `profiles: [research]` block so a plain `docker compose up` boots only the sandbox.

```
┌─────────────────── docker-compose ───────────────────┐
│                                                       │
│  ┌─── application plane ───┐    ┌── chaos plane ──┐  │
│  │  auth   accounts        │    │  toxiproxy      │  │
│  │  transactions           │◄───┤  pumba          │  │
│  │  notifications          │    └─────────────────┘  │
│  └────────┬────────────────┘                          │
│           │ OTLP (gRPC)                               │
│  ┌────────▼─────────┐                                 │
│  │ OTel Collector   │── traces ──► Tempo              │
│  │ (otelcol-contrib)│── logs ───► Loki                │
│  │                  │── metrics ► Prometheus          │
│  └──────────────────┘                                 │
│                                                       │
│  ┌── research plane (profile: research) ─────────┐    │
│  │  experiment-runner  ─► labels DB (sqlite)     │    │
│  │  bank-loadgen                                  │    │
│  │  correlation-engine ─► incidents DB (sqlite)  │    │
│  │     (lib + CLI + HTTP)                        │    │
│  │  eval-harness       ─► eval_runs DB (sqlite)  │    │
│  └───────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────┘
```

### Application plane

Four Rust services (axum):

- `auth` — JWT issuance and verification; no DB.
- `accounts` — Postgres-backed account CRUD.
- `transactions` — calls `accounts` then `notifications`.
- `notifications` — calls a fake external SMTP we can break.

Each is instrumented with:

- `opentelemetry-rust` for traces (W3C tracecontext propagation across HTTP calls).
- `tracing` + `tracing-opentelemetry` for logs (structured, exported via OTLP).
- A Prometheus metrics endpoint scraped by the collector.

Each service has a `failure_modes.rs` module with env-gated knobs (`*_INJECT_LATENCY_MS`, `*_INJECT_ERROR_RATE`, etc.) so application-level fault classes (error-rate spike, cold-start slowness on first N requests) can be triggered without restarting containers. Toxiproxy and Pumba cover network and container-level chaos.

### Telemetry plane

A single OTel collector (contrib distro) receives OTLP from all services and routes:

- traces → Tempo
- logs → Loki
- metrics → Prometheus (via remote-write)

Single ingestion path keeps semantic conventions consistent across services. Toxiproxy sits in front of Postgres and the fake SMTP for network-level faults.

### Research plane

- `experiment-runner` — loads YAML experiment definitions, drives load via `bank-loadgen`, triggers Toxiproxy/Pumba at scheduled offsets, writes a row per experiment to `labels.db` with full provenance.
- `correlation-engine` — Approach B implementation; library + thin CLI + thin HTTP. Persists each correlation result to `incidents.db` so eval re-runs are cheap.
- `eval-harness` — joins `labels.db` and `incidents.db`, computes metrics, produces markdown reports under `results/<tag>/`.

Three planes are cleanly separable. The collector is the only shared component, and it is the natural seam if the application plane is ever swapped for a different domain.

---

## 2. Correlation Engine Internals

The engine is split into a pure library and three IO adapters so future ML/GNN work can swap in without rebuilding the rest.

### Crate layout

```
crates/
  correlation-core/      ← the library (no IO)
    src/
      graph/             ← EvidenceGraph, node/edge types, builder, invariants
      ranking/           ← centrality + suspect scoring + propagation
      anomaly/           ← z-score + EWMA detectors
      schema/            ← IncidentContext (serde) + Markdown renderer + version
      backend.rs         ← TelemetryBackend trait
      config.rs          ← CorrelationConfig (TOML-loadable)
      time.rs            ← clock injection trait + WallClock + TestClock
  correlation-tempo/     ← Tempo adapter (TraceQL)
  correlation-loki/      ← Loki adapter (LogQL)
  correlation-prom/      ← Prometheus adapter (PromQL)
  correlation-cli/       ← `corr` binary
  correlation-http/      ← axum HTTP shell
```

`correlation-core` has zero IO and zero HTTP clients. The three adapter crates implement `TelemetryBackend`. The two binary crates wire everything at startup.

### `TelemetryBackend` trait

```rust
#[async_trait]
trait TelemetryBackend {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError>;
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError>;
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError>;
    async fn query_metric_window(&self, q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError>;
}
```

Production implementations stitch Tempo/Loki/Prom; tests use a `MockBackend` populated from fixture files. This seam is what lets us add a ClickHouse adapter later without touching `correlation-core`.

### The Evidence Graph

Built per invocation; not persisted (the *output* is — see Section 4).

**Nodes (typed enum):**

- `Service { name }` — one per service the incident touches.
- `Span { span_id, service, op, status, duration, parent }`
- `LogBatch { service, level, time_bucket, count, sample_messages }` — logs grouped by `(service, 10s bucket, level)` so we don't graph 50k INFO lines individually.
- `MetricAnomaly { service, metric, window, severity }`

**Edges (typed enum):**

- `ParentOf` — span parent/child (from Tempo, direct).
- `EmittedBy` — log batch or metric anomaly → service (direct).
- `CoOccurs` — log batch ↔ span when they share `(service, time-window)` and the span is in-progress at the log timestamp (direct).
- `CausedBy` — span B → span A when B has `status=ERROR` and A is its parent (heuristic; deterministic; documented).

`CausedBy` is the only edge with inference; the rest are direct from telemetry. Confining inference to one edge type keeps the ranking algorithm auditable.

### Correlation paths

**Trace-anchored — `correlate_trace(trace_id)`:**

1. `fetch_trace(trace_id)` → spans → derive `service_set` and `[t_min, t_max]`.
2. Expand window by `±30s` (configurable) for temporal context.
3. For each service in `service_set`: `fetch_logs` over expanded window; `fetch_metric_series` over expanded window.
4. Build `EvidenceGraph`: spans + log batches + per-service services + any metric anomalies the detector flags inside the window.
5. Run ranking → emit `IncidentContext`.

**Anomaly-driven — `correlate_anomaly(metric, window, value)`:**

1. Run anomaly detector on the metric series over a baseline window (default 4× the alert window, ending at alert start). Confirm `value` is anomalous; if not, return an explanatory non-incident result rather than fabricating one.
2. `query_metric_window` to find which services have anomalous co-movement in the same window (cross-metric correlation by simple Pearson on z-scored series).
3. Use TraceQL to find traces in the window touching those services; pick the top-N traces by `(error_status, duration, span_count)`.
4. Each top trace flows through the trace-anchored path; results merged into one `EvidenceGraph` (deduped by `trace_id` and log time-bucket).
5. Rank → emit.

### Anomaly detection (deliberate baselines)

Two stateless detectors in `correlation-core::anomaly`:

- **Z-score:** flag points where `|x − μ| > k · σ` over the baseline window. Default `k = 3`.
- **EWMA residual:** flag points where the residual against an EWMA forecast exceeds `k · σ_residual`. Default `α = 0.3`, `k = 3`.

Both return `Vec<MetricAnomaly>`; the graph builder consumes the union. No ML in v1 — these are the baselines downstream research must beat on the labeled dataset.

### Ranking

`SuspectScore` per service node, combining:

1. **Direct evidence weight** — sum of `severity` over error/anomaly nodes whose `EmittedBy` edge points to the service.
2. **Causal-propagation weight** — for each `CausedBy` edge `B→A`, attribute a fraction `β` (default 0.5) of B's evidence to A's service. Applied transitively up to depth 3.
3. **Temporal-tightness multiplier** — services whose evidence clusters near the anomaly start window get a small boost (rewards "fired first").

Output is a ranked `Vec<(Service, SuspectScore, evidence_breakdown)>`. The breakdown is included verbatim in `IncidentContext` so a human or LLM consumer can audit *why* a service was ranked high.

All constants (`k`, `α`, `β`, window expansion, log bucket size, propagation depth) live in `CorrelationConfig`, serialized to TOML, so the evaluation harness can sweep them.

### Output flow

`EvidenceGraph` → `IncidentContext` (Section 4) → pass-through (`--json`) or Markdown renderer. The graph is never serialized; the `IncidentContext` is the stable public contract.

### Defaults committed (overridable in `CorrelationConfig`)

- `CausedBy` heuristic as described; deterministic; tunable after first labeled data.
- Log time bucket: 10s.
- Anomaly baselines: z-score + EWMA; MAD as a future addition.

---

## 3. Experiment Runner & Labeled Dataset

This produces the ground-truth dataset. Its outputs are what the eval harness scores the engine against.

### YAML experiment definition

Each experiment is one file in `experiments/`:

```yaml
id: payment-storm-001
description: |
  Burst of transaction requests while notifications-fake-smtp has 800ms
  added latency. Expect cascading slowness in transactions → accounts.
duration_sec: 180
warmup_sec: 30
cooldown_sec: 60
recovery_grace_sec: 20   # all three recovery signals must hold for this
                         # window before recovery_ts is recorded

load:
  generator: bank-loadgen
  profile:
    - endpoint: POST /transactions
      rps: 200
      duration_sec: 120
    - endpoint: POST /auth/login
      rps: 20
      duration_sec: 180

faults:
  - at_sec: 45                         # offset from experiment start
    until_sec: 135
    kind: toxiproxy
    target: notifications-smtp
    toxic:
      type: latency
      attributes: { latency: 800, jitter: 150 }

ground_truth:
  primary_faulted_service: notifications
  expected_blast_radius: [transactions]
  expected_clean_services: [auth]
  failure_class: dependency_latency
```

**`failure_class` enum** (drives per-class scoring):
`dependency_latency`, `dependency_outage`, `db_connection_exhaustion`, `cpu_saturation`, `memory_pressure`, `container_restart`, `network_partition`, `error_rate_spike`, `cold_start`.

### Runner lifecycle

```
load YAML → preflight checks → start warmup load
        → record experiment_start_ts
        → at offset for each fault: trigger toxiproxy/pumba
        → continuously sample health endpoints (every 2s)
        → at fault end: revert toxiproxy/pumba
        → poll for recovery: all healthchecks green AND no
          5xx in load-generator stats for `recovery_grace_sec`
          AND prom error_rate below baseline + 2σ
        → record recovery_ts (last of three signals to clear)
        → cooldown
        → write row to labels DB; write manifest with config snapshot
```

Health is detected externally — the runner doesn't trust the service to know it's recovered. Recovery requires all three signals to clear; the latest sets `recovery_ts`. Conjunctive definition is documented so consumers know what "recovered" means.

### Labels DB schema (SQLite)

```sql
CREATE TABLE experiments (
    id                       TEXT PRIMARY KEY,
    yaml_path                TEXT NOT NULL,
    yaml_sha256              TEXT NOT NULL,
    started_at               INTEGER NOT NULL,   -- unix nanos
    ended_at                 INTEGER NOT NULL,
    primary_faulted_service  TEXT NOT NULL,
    failure_class            TEXT NOT NULL,
    blast_radius             TEXT NOT NULL,      -- JSON array
    clean_services           TEXT NOT NULL,      -- JSON array
    runner_version           TEXT NOT NULL,
    status                   TEXT NOT NULL,      -- see enum below
    notes                    TEXT
);

CREATE TABLE fault_events (
    experiment_id   TEXT NOT NULL REFERENCES experiments(id),
    sequence_no     INTEGER NOT NULL,
    kind            TEXT NOT NULL,        -- toxiproxy | pumba
    target          TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    ended_at        INTEGER NOT NULL,
    config_json     TEXT NOT NULL,
    PRIMARY KEY (experiment_id, sequence_no)
);

CREATE TABLE recovery_signals (
    experiment_id   TEXT NOT NULL REFERENCES experiments(id),
    signal          TEXT NOT NULL,        -- health | load_5xx | prom_error_rate
    cleared_at      INTEGER NOT NULL,
    PRIMARY KEY (experiment_id, signal)
);
```

**`status` enum:** `clean`, `dirty`, `no_recovery`, `unexpected_crash`, `aborted`.

`yaml_sha256` lets eval detect when an experiment YAML has been edited; stale-hash results are flagged.

### Fault injection surface

- **`toxiproxy`** — network-level: latency, jitter, bandwidth caps, packet drop, slow_close. Targets: `accounts-db` (postgres), `notifications-smtp`, inter-service HTTP (one proxy per service pair).
- **`pumba`** — container-level: kill, pause, stop, network emulation, stress. Any service container by name.

Runner has a `FaultDriver` trait with `apply(spec) -> Handle` and `revert(handle)`. Two impls: `ToxiproxyDriver` (HTTP to toxiproxy admin API) and `PumbaDriver` (shells out to `pumba` in a sibling container with docker socket access). New chaos tools become a new impl.

### Load generator (`bank-loadgen`)

Custom Rust binary using `reqwest` + `tokio`. Reads profile from YAML, fires at prescribed RPS, emits its own OTel traces (so load is observable in Tempo), and writes a per-second stats stream (success / 4xx / 5xx counts, latency percentiles) to a sidecar file the runner reads for the recovery signal.

Custom over k6/Locust because it keeps the stack monolingual, ensures load-side traces share the same OTel SDK and propagation, and gives tight integration for the recovery signal.

### Starter scenario catalog (10 experiments)

| ID | Class | Brief |
|---|---|---|
| `payment-storm-001` | dependency_latency | SMTP latency under transaction load |
| `accounts-db-down-001` | dependency_outage | toxiproxy cuts postgres |
| `db-connpool-exhaust-001` | db_connection_exhaustion | drive accounts past pool size |
| `transactions-cpu-001` | cpu_saturation | pumba stress on transactions |
| `notifications-oom-001` | memory_pressure | pumba memstress on notifications |
| `auth-restart-loop-001` | container_restart | pumba kill auth on a 30s cycle |
| `inter-service-partition-001` | network_partition | toxiproxy blocks transactions↔accounts |
| `transactions-error-spike-001` | error_rate_spike | feature flag → 30% 500s |
| `auth-cold-start-001` | cold_start | pumba stop+start auth, measure first-N requests |
| `combined-cascade-001` | dependency_latency + container_restart | latency + a kill mid-experiment |

`combined-cascade-001` is a deliberate stretch test for ranking.

### Reproducibility guarantees

- YAML is authoritative; SHA recorded.
- Random seeds (load-gen jitter, anomaly noise) are in the YAML and recorded.
- Runner version recorded; built from the same monorepo commit as everything else.
- Per-experiment manifest captures: container image digests, Tempo/Loki/Prom versions, OS/kernel.

`docker compose up` + YAML reproduces the labeled dataset within sampling noise.

### Concurrency

Experiments must not overlap. Runner takes a process-level lock on `labels.db`. Concurrent invocations are serialized with a clear error message. Chaos faults are global to the Compose network — overlapping experiments are meaningless.

---

## 4. Output Schema (`IncidentContext`)

This is the engine's public contract — what the eval harness scores, what an LLM consumes, what the Markdown renderer formats. **Stable from v1 forward; changes require a schema version bump.**

### Canonical JSON

```jsonc
{
  "schema_version": "1.0.0",
  "incident_id": "01HZ8K7R4W…",         // UUID v7 (time-sortable)
  "produced_at": "2026-05-23T12:34:56.789Z",
  "engine_version": "0.1.0",
  "config_hash": "sha256:…",             // hash of CorrelationConfig used
  "elapsed_ms": 1842,                    // time-to-context

  "trigger": {
    "kind": "trace" | "anomaly",
    "trace": { "trace_id": "…" },        // present iff kind=trace
    "anomaly": {                          // present iff kind=anomaly
      "metric": "http_request_duration_seconds:p99",
      "service": "transactions",
      "window": { "start": "…", "end": "…" },
      "observed_value": 2.34,
      "baseline_mean": 0.12,
      "baseline_stddev": 0.04,
      "z_score": 55.5,
      "detector": "z_score"
    }
  },

  "window": { "start": "…", "end": "…", "expanded": true },

  "services": [
    { "name": "transactions", "span_count": 142, "error_span_count": 31,
      "log_count": 4012, "error_log_count": 88 }
  ],

  "suspects": [
    {
      "rank": 1,
      "service": "notifications",
      "score": 8.74,
      "evidence_breakdown": {
        "direct_error_weight": 5.20,
        "direct_anomaly_weight": 2.10,
        "propagated_weight": 1.10,
        "temporal_tightness_multiplier": 1.08,
        "contributors": [
          { "kind": "metric_anomaly", "ref": "anom_3",  "weight": 2.10 },
          { "kind": "log_batch",      "ref": "lb_14",   "weight": 3.40 },
          { "kind": "span",           "ref": "span_88", "weight": 1.80 },
          { "kind": "propagated_from","ref": "transactions", "weight": 1.10 }
        ]
      }
    }
  ],

  "spans": [
    { "id": "span_88", "trace_id": "…", "parent_id": "span_42",
      "service": "notifications", "operation": "smtp.send",
      "start": "…", "duration_ms": 812, "status": "ERROR",
      "status_message": "context deadline exceeded",
      "attributes": { "net.peer.name": "smtp-fake" } }
  ],
  "span_tree": [                          // forest of root span IDs
    { "span_id": "span_root", "children": [ { "span_id": "span_42", "children": [...] } ] }
  ],

  "log_batches": [
    { "id": "lb_14", "service": "notifications", "level": "ERROR",
      "time_bucket": "2026-05-23T12:34:50Z/10s", "count": 23,
      "sample_messages": [
        "smtp send failed: i/o timeout",
        "retry exhausted for message id=…"
      ] }
  ],

  "metric_anomalies": [
    { "id": "anom_3", "service": "notifications",
      "metric": "smtp_send_latency_seconds:p99",
      "window": { "start": "…", "end": "…" },
      "severity": 0.91, "detector": "ewma",
      "baseline_mean": 0.08, "observed_peak": 1.42 }
  ],

  "timeline": [
    { "ts": "…", "kind": "metric_anomaly_start", "ref": "anom_3" },
    { "ts": "…", "kind": "log_batch",            "ref": "lb_14"  },
    { "ts": "…", "kind": "span_error",           "ref": "span_88"}
  ],

  "notes": []                              // engine-emitted caveats
}
```

### Key schema choices

- **Suspects carry their own audit trail.** `evidence_breakdown.contributors` lists every node that contributed to the score with `weight` and a `ref` into the canonical lists. An LLM or human can answer "why was X ranked #1?" directly from the document.
- **Refs, not embedding.** Nodes appear once in their canonical list; everywhere else uses `ref` IDs. Keeps the document compact and lets consumers reason about identity.
- **Spans denormalized; tree separate.** Lets consumers index by `span_id` directly without walking the tree.
- **`notes[]` is the engine's pressure-relief valve.** Every degradation is attributed there (trace not found, retention boundary hit, anomaly path fallback, etc.). Eval joins on `notes` to find degraded paths.
- **`config_hash`** groups results by configuration when sweeping.
- **Sample messages** per log batch: up to 3 distinct messages plus the most recent.

### Markdown renderer

One deterministic template, snapshot-tested with `insta`. Layout:

```
# Incident <incident_id>
**Trigger:** anomaly on `transactions:http_request_duration_seconds:p99`
**Window:** 12:34:30 → 12:35:00 (expanded ±30s)
**Engine:** 0.1.0  ·  config 9a2c…  ·  elapsed 1.8s

## Top suspects
1. **notifications** — score 8.74
   - 2 metric anomalies (smtp_send_latency p99 1.42s vs baseline 0.08s)
   - 23 ERROR logs (sample: "smtp send failed: i/o timeout")
   - propagated 1.10 from transactions
2. **transactions** — score 5.62
   - ...

## Span tree (errors highlighted)
trace 4f3a…
└── transactions/POST /transactions [202ms]
    └── notifications/smtp.send [812ms ⚠ ERROR]
        └── (deadline exceeded)

## Anomalies
- notifications · smtp_send_latency_seconds:p99 · z=12.3 · 12:34:48 → 12:34:56

## Error logs (sampled, by service)
### notifications · 23 ERROR @ 12:34:50/10s
- smtp send failed: i/o timeout
- retry exhausted for message id=…

## Timeline
12:34:48  anom start  notifications.smtp_send_latency p99 climbs
12:34:50  logs        notifications: 23 ERROR ("smtp send failed…")
12:34:51  span        notifications/smtp.send ERROR (deadline exceeded)

## Notes
(none)
```

### Persistence — `incidents.db` (SQLite)

```sql
CREATE TABLE incidents (
    incident_id      TEXT PRIMARY KEY,
    schema_version   TEXT NOT NULL,
    engine_version   TEXT NOT NULL,
    config_hash      TEXT NOT NULL,
    trigger_kind     TEXT NOT NULL,      -- 'trace' | 'anomaly'
    trigger_input    TEXT NOT NULL,      -- trace_id or "metric@service@window"
    window_start     INTEGER NOT NULL,
    window_end       INTEGER NOT NULL,
    elapsed_ms       INTEGER NOT NULL,
    produced_at      INTEGER NOT NULL,
    document         TEXT NOT NULL,      -- the full JSON
    experiment_id    TEXT                -- soft FK; set by eval-harness
);
CREATE INDEX idx_incidents_window      ON incidents(window_start, window_end);
CREATE INDEX idx_incidents_experiment  ON incidents(experiment_id);
CREATE INDEX idx_incidents_trigger     ON incidents(trigger_kind, trigger_input);
```

`experiment_id` is set by the eval harness post-hoc, not by the engine itself; this keeps the engine ignorant of experiment context (clean separation).

### Schema versioning

- **Additive non-breaking** (new optional field): bump patch.
- **Additive but downstream-visible** (new required field): bump minor.
- **Rename/remove/retype**: bump major; renderer must keep a `v1` fallback path until eval migrates.

Eval harness checks `schema_version` and fails loudly on a major mismatch.

---

## 5. Evaluation Harness

Joins `labels.db` to `incidents.db`, scores the engine against ground truth, prints per-experiment and aggregate reports, supports parameter sweeps.

### Top-level lifecycle

```
eval run --suite experiments/*.yaml --config configs/default.toml --tag v0.1

for each experiment:
    1. read experiment YAML
    2. run experiment-runner  → writes a row into labels.db
    3. wait `settle_sec` (default 15s) for telemetry to land
    4. invoke correlation-engine in TWO modes:
         a. trace mode: highest-latency trace in window whose root span
                        touches the load-gen target
         b. anomaly mode: per-failure_class metric defined in
                          configs/anomaly_invocation.toml (one metric +
                          service + window-rule per failure_class)
    5. score each incident against the experiment's ground_truth
    6. emit per-experiment row to eval_runs.db
finally:
    7. aggregate, render markdown to results/<tag>/report.md
```

Both modes are scored on every experiment so we see how the two entry paths compare per scenario.

### Metrics

**1. Recall on root cause — `precision@k` and `recall@k` for k ∈ {1, 3, 5}.**

- `recall@k = 1` if `primary_faulted_service ∈ top_k(incident.suspects)` else `0`.
- `precision@k = |top_k ∩ ({primary} ∪ blast_radius)| / k`.
- A service in `expected_clean_services` appearing in `top_k` is recorded as a **clean-service false-positive** (count, not failure).
- Aggregates: average across suite; broken out by `failure_class`.

**2. Completeness — four coverage ratios.**

- `trace_coverage` = traces touching faulted service or blast radius in window, present in incident's `spans` set / such traces in Tempo over window. Denominator capped at 100.
- `error_log_coverage` = same shape, for ERROR logs from involved services.
- `metric_anomaly_coverage` = injected anomalies that we expect detectable / anomalies in `metric_anomalies`. "Expected detectable" set is curated per `failure_class` in `coverage_targets.toml`.
- `span_tree_integrity` = 1.0 if every span in `span_tree` has its `parent_id` present in `spans` (or is documented root), else fraction.

**3. Time-to-context — `elapsed_ms`.** Reported as p50 / p95 across suite.

### Composite score

Single number per experiment for sortability:

```
composite = 0.50 · recall@3
          + 0.10 · precision@3
          + 0.25 · mean(trace_coverage, error_log_coverage,
                        metric_anomaly_coverage, span_tree_integrity)
          + 0.10 · max(0, 1 − elapsed_ms / 10000)
          − 0.05 · normalized_clean_service_false_positives
```

Weights live in `configs/scoring.toml`; starting values are a defensible guess to be revised once we have data. Headline metric is recall on root cause; completeness matters but is graded separately; latency matters but doesn't dominate; clean-service FPs are a penalty, not a hard fail.

### Parameter sweeps

`eval sweep` takes `sweep.toml`:

```toml
[anomaly]
z_score_k = [2.5, 3.0, 3.5]
ewma_alpha = [0.2, 0.3, 0.5]

[ranking]
causal_propagation_beta = [0.3, 0.5, 0.7]

[window]
expansion_sec = [15, 30, 60]
```

Harness:

- Enumerates Cartesian product (or sampled subset if `--max-cells N`).
- Runs suite per cell; tags incidents with `config_hash`.
- Aggregates and emits heatmap-style table (top N configs + per-class breakdown).
- Caches: same `(experiment_id, yaml_sha256, config_hash)` is never re-evaluated. Adding a new param value reuses prior cells.

### `eval_runs.db` schema

```sql
CREATE TABLE eval_runs (
    eval_run_id        TEXT PRIMARY KEY,    -- UUID v7
    tag                TEXT NOT NULL,
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER NOT NULL,
    config_hash        TEXT NOT NULL,
    engine_version     TEXT NOT NULL,
    runner_version     TEXT NOT NULL,
    scoring_toml_hash  TEXT NOT NULL
);

CREATE TABLE eval_results (
    eval_run_id        TEXT NOT NULL REFERENCES eval_runs(eval_run_id),
    experiment_id      TEXT NOT NULL,
    invocation_mode    TEXT NOT NULL,       -- 'trace' | 'anomaly'
    incident_id        TEXT NOT NULL,
    recall_at_1        REAL NOT NULL,
    recall_at_3        REAL NOT NULL,
    recall_at_5        REAL NOT NULL,
    precision_at_1     REAL NOT NULL,
    precision_at_3     REAL NOT NULL,
    precision_at_5     REAL NOT NULL,
    trace_coverage     REAL NOT NULL,
    error_log_coverage REAL NOT NULL,
    anomaly_coverage   REAL NOT NULL,
    tree_integrity     REAL NOT NULL,
    elapsed_ms         INTEGER NOT NULL,
    clean_fps          INTEGER NOT NULL,
    composite          REAL NOT NULL,
    notes              TEXT,
    PRIMARY KEY (eval_run_id, experiment_id, invocation_mode)
);
CREATE INDEX idx_eval_results_composite ON eval_results(composite);
CREATE INDEX idx_eval_results_mode ON eval_results(eval_run_id, invocation_mode);
-- failure_class breakdowns join through experiments table on experiment_id;
-- no dedicated index needed (experiments.id is PK in labels.db).
```

Three SQLite DBs (`labels.db`, `incidents.db`, `eval_runs.db`) live under `data/`. Joined by `experiment_id` and `incident_id`. Easy to ship as a tarball.

### Report (`results/<tag>/report.md`)

Headline → per failure class → per invocation mode → misses (recall@3 = 0, with top suspects, ground truth, harness note) → sweep top 5 if applicable. Hashes embedded at top.

### Reproducibility

- `eval_run_id` is UUIDv7.
- Every input that affects scoring is hashed (scenario YAMLs, `CorrelationConfig`, `scoring.toml`, `coverage_targets.toml`); hashes in `eval_runs.db`.
- Eval results are content-addressable: same hashes → cached row served from DB.
- `eval reproduce <eval_run_id>` re-runs from stored hashes and diffs against stored result — surfaces non-determinism.

---

## 6. Error Handling & Failure Semantics

**Default policy: degrade loudly, attribute every degradation in `IncidentContext.notes[]`, never fabricate.**

### Adapter failures

| Error | Engine behavior | Affects scoring? |
|---|---|---|
| `Unreachable` / `Timeout` (after retry budget) | Abort correlation; return `IncidentContext` with `notes` + `suspects: []`; exit 2. | Eval-side: `eval_results.notes = "harness_failure: <adapter>"`; result excluded from headline metrics, listed under "computational issues" in the report. The experiment's own `status` (e.g. `clean`) is unaffected. |
| `PartialContent` | Continue; `notes` records what was missing. | Counts; degradation visible. |
| `RetentionMiss` | Continue with available data; `notes` records boundary. | Same. |
| `RateLimited` | Exponential backoff up to budget; then treat as `Timeout`. | Same. |
| `Empty` | Continue. No note (empty is valid). | Same. |
| `MalformedResponse` | Abort like `Unreachable`. We don't try to half-parse. | Same. |

Retry budget: 3 attempts at 100ms / 400ms / 1600ms, per adapter call, independent across adapters.

### Query semantics edge cases

- **Trace not found:** structured non-result with `notes`, `suspects: []`. No silent fall-through to anomaly mode unless `--fallback-anomaly`.
- **Empty window:** non-result with `notes: ["window contains no telemetry"]`. Eval treats as a real miss (correct).
- **Anomaly path with no detected anomalies:** non-result with `notes`. Engine never invents an anomaly.
- **Baseline too short:** skip that series; note it; other series proceed.
- **Zero-variance series:** switch to constant-detector; note it.
- **Clock skew:** if span's `start > parent.start + parent.duration + 5s`, flag in `notes`; exclude from `CausedBy` inference.

Each has a fixture-pinned unit test under `correlation-core/tests/edge_cases/`.

### Runner failures (strict contract — labels are ground truth)

| Situation | Behavior |
|---|---|
| Toxiproxy unreachable applying a fault | Abort; no labels row; exit 3. |
| Pumba CLI nonzero exit | Same. Container chaos not retried. |
| `revert()` fails | Abort; best-effort cleanup; row with `status='dirty'`; loud warning. |
| Recovery never observed in `cooldown_sec + recovery_grace_sec` | Row with `status='no_recovery'` and `recovery_ts = NULL`; recovery-time metric NaN. |
| Unexpected service crash | Health monitor detects; row with `status='unexpected_crash'`; excluded from headline. |

### Eval-harness failures

- **Telemetry not landed:** wait `settle_sec` (15s) after experiment end; if incident empty, retry once after `2 × settle_sec`; record `eval_results.notes = "retry_after_settle"`.
- **Schema mismatch (major):** hard fail; don't score against a schema we don't understand.
- **Missing label** for `experiment_id`: hard fail.
- **Score panic** (NaN/div-by-zero on empty denominator): record NaN; surface under "computational issues"; never crash run.

### Determinism

Engine is deterministic given the same `(telemetry snapshot, config)`. Sources of non-determinism are controlled:

- Wall-clock — detectors take `now_ts` injected, not `SystemTime::now()`.
- HashMap iteration — `IndexMap` everywhere iteration order is observable.
- Tokio scheduling — adapter results joined by `(adapter, query_id)` sort key.

Non-determinism the engine can't control (container clock skew, scheduler jitter) is *measured* via `eval reproduce` rather than swept under the rug.

### Dogfooding

Engine, runner, and eval-harness emit their own OTel traces and metrics through the same collector. Engine spans include `correlation.elapsed_ms`, `correlation.adapter_latency_ms{adapter}`, `correlation.degradations_count`. Runner spans include `experiment.fault_apply_ms`, `experiment.recovery_ms`, `experiment.health_check_count`. The same Grafana stack the engine queries debugs the engine itself.

### Logging policy

All Rust binaries use `tracing` + `RUST_LOG`. Default level: `info` in services/runner, `warn` in the engine library (loud only on degradations), `info` in CLI/HTTP shells. Engine library never logs above `debug` in normal operation. Degradations travel via `IncidentContext.notes[]`, not stderr — engine is usable as a library without side effects.

---

## 7. Testing Strategy

Six layers. The unusual ones for this project are snapshot/regression and the `eval reproduce` canary.

### Layer 1 — Unit tests (no IO, fast)

- **`correlation-core`** — highest density: graph operations (insert/lookup/dedup/`CausedBy` inference/depth limit/dangling-ref); ranking (each component isolated against fixture graphs; weight combination; tie-break determinism); anomaly detectors (clean baseline, spike, sustained shift, zero variance, insufficient baseline, NaN — same matrix across z-score and EWMA); schema (serde round-trip byte-identical; reject unknown major; accept unknown patch); `MockBackend` fidelity.
- **Adapters** — `wiremock`-based: one test per `BackendError` variant per adapter, so every error path has a fixture.
- **`bank-loadgen`, `experiment-runner`, `eval-harness`** — YAML parser round-trips; recovery-signal state machine; composite math; sweep cell hashing.

Coverage gate per crate: 80% lines / 70% branches via `cargo-llvm-cov`.

### Layer 2 — Integration tests (single crate, in-process)

- Engine end-to-end against `MockBackend` populated from fixtures: `(tempo.json, loki.json, prom.json, expected_incident.json)`. Adding a fixture = adding a test.
- Edge cases from §6 each have a fixture: trace-not-found, empty-window, baseline-too-short, zero-variance, clock-skew.
- Two-mode invocation test: same scenario invoked via both paths; both produce non-empty incidents and agree on top suspect.
- Recovery-signal state machine against a deterministic clock; `MockDriver` lifecycle including failed-revert path.
- Eval harness end-to-end against fixture DBs in tmpdirs: join, per-class breakdown, cache-by-hash, `eval reproduce`.

### Layer 3 — Snapshot / regression (the unusual layer)

Using `insta`:

- JSON snapshots of every fixture scenario's expected incident.
- Markdown snapshots of the same.
- JSON Schema golden file `schema/v1.json`; serializer validates against it.
- CLI golden output (`--help`, `corr trace`, `corr anomaly`) against fixture data.
- Eval report golden against fixture incidents.

`insta` policy: any snapshot diff must be reviewed by a human in PR; CI rejects unreviewed changes. This is the schema-stability gate.

### Layer 4 — Property tests (`proptest`, selectively)

- Schema round-trip via custom `Arbitrary`.
- Graph invariants: any insertion sequence preserves no-dangling-refs, no-duplicate-edges, no-`CausedBy`-cycles, propagation ≤ depth 3.
- Ranking monotonicity: adding error/anomaly evidence never decreases a service's score.
- Sweep determinism: generated `config_hash` set depends only on TOML content.

### Layer 5 — End-to-end (Compose-up, real backends; `--features e2e`)

Three e2e tests, nightly only (not on every commit):

1. **`smoke_full_stack`** — Compose up; fire a transaction; assert trace in Tempo, log in Loki, metric in Prom. Catches OTel wiring breaks.
2. **`engine_against_real_backends`** — same plus `corr trace <id>`; snapshot-compare incident structure.
3. **`one_experiment_full_loop`** — run `payment-storm-001` via runner; settle; invoke engine in both modes; run eval-harness; assert one experiment row (`status='clean'`), two incident rows, one eval result with defined `recall@3`.

### Layer 6 — Determinism canary (nightly)

Picks 3 representative `eval_run_id`s, calls `eval reproduce`, asserts `composite` matches original to ε = 0.001. Anything beyond ε flags non-determinism.

### Test data layout

```
tests/fixtures/
  scenarios/
    payment-storm-001/{tempo.json, loki.json, prom.json, expected_incident.json}
    ...
  edge_cases/{trace_not_found, empty_window, zero_variance, clock_skew}/...
  schemas/incident_context_v1.json
```

Fixture authoring: `tools/gen-fixture` boots Compose, runs an experiment, captures responses to JSON. Used only when authoring/refreshing fixtures, never in CI.

### CI structure

```
.github/workflows/
  unit.yml          ← every push; coverage gate
  snapshot.yml      ← every push; unreviewed diffs fail
  property.yml      ← every push (short); long runs nightly
  e2e.yml           ← nightly + on demand
  reproduce.yml     ← nightly determinism canary
```

Every-push target: < 5 min. Nightly: < 60 min.

### TDD discipline

Edge cases from §6 are written as failing tests first. Test-driven development applies for the engine's correctness-critical paths (engine, anomaly detectors, ranking, recovery-signal state machine). Service CRUD endpoints and CLI wiring do not require strict TDD.

---

## 8. Repo / Crate Layout

Monorepo, Cargo workspace. Image-publishing on release tags is a documented future retrofit.

### Top-level

```
OpenTelemetry-Multi-Source-Correlation-Engine/
├── Cargo.toml                    # workspace
├── README.md
├── LICENSE
├── rust-toolchain.toml           # pinned MSRV
├── .github/workflows/            # unit | snapshot | property | e2e | reproduce
│
├── crates/
│   ├── correlation-core/         # the library (no IO)
│   ├── correlation-tempo/        # TraceQL adapter
│   ├── correlation-loki/         # LogQL adapter
│   ├── correlation-prom/         # PromQL adapter
│   ├── correlation-cli/          # `corr` binary
│   ├── correlation-http/         # axum HTTP shell
│   ├── eval-harness/             # `eval` binary
│   ├── experiment-runner/        # `exp` binary
│   ├── bank-loadgen/             # load generator
│   ├── bank-common/              # shared types: OTel init, health, error types
│   └── services/{auth, accounts, transactions, notifications}/
│
├── compose/
│   ├── docker-compose.yaml       # default: local image builds
│   ├── compose.override.yaml     # optional dev overrides (gitignored example)
│   ├── otel-collector-config.yaml
│   ├── tempo-config.yaml
│   ├── loki-config.yaml
│   ├── prometheus-config.yaml
│   ├── toxiproxy/proxies.json
│   └── grafana/dashboards/       # one default dashboard
│
├── experiments/                  # YAML scenarios (§3)
├── configs/
│   ├── default.toml              # CorrelationConfig
│   ├── scoring.toml              # composite weights
│   ├── coverage_targets.toml     # per-failure-class detectable sets
│   ├── anomaly_invocation.toml   # per-failure-class metric for anomaly mode
│   └── sweep.example.toml
│
├── data/                         # gitignored; sample seeded for tests
│   ├── labels.db
│   ├── incidents.db
│   └── eval_runs.db
│
├── results/                      # gitignored
│   └── <tag>/report.md
│
├── tests/
│   ├── e2e/
│   └── fixtures/
│
├── tools/gen-fixture/
│
└── docs/
    └── design-spec.md
```

### Workspace `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
    "crates/correlation-core", "crates/correlation-tempo",
    "crates/correlation-loki", "crates/correlation-prom",
    "crates/correlation-cli", "crates/correlation-http",
    "crates/eval-harness", "crates/experiment-runner",
    "crates/bank-loadgen", "crates/bank-common",
    "crates/services/auth", "crates/services/accounts",
    "crates/services/transactions", "crates/services/notifications",
    "tools/gen-fixture",
]

[workspace.package]
edition      = "2021"
rust-version = "1.78"
license      = "MIT"

[workspace.dependencies]
tokio                 = { version = "1", features = ["full"] }
axum                  = "0.7"
reqwest               = { version = "0.12", features = ["json", "rustls-tls"] }
serde                 = { version = "1", features = ["derive"] }
serde_json            = "1"
opentelemetry         = "0.24"
opentelemetry_sdk     = { version = "0.24", features = ["rt-tokio"] }
opentelemetry-otlp    = { version = "0.17", features = ["grpc-tonic"] }
tracing               = "0.1"
tracing-subscriber    = { version = "0.3", features = ["env-filter", "json"] }
tracing-opentelemetry = "0.25"
sqlx                  = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
async-trait           = "0.1"
thiserror             = "1"
anyhow                = "1"
uuid                  = { version = "1", features = ["v7", "serde"] }
indexmap              = { version = "2", features = ["serde"] }
prometheus            = "0.13"
toml                  = "0.8"

[workspace.dev-dependencies]
insta                 = { version = "1", features = ["json", "yaml"] }
proptest              = "1"
wiremock              = "0.6"
testcontainers        = "0.21"
```

Workspace-level pinning keeps engine + adapters + services on the same OTel SDK — critical for semantic-convention consistency.

### Service crate template

```
crates/services/auth/
├── Cargo.toml
├── Dockerfile                    # cargo-chef layer → distroless
├── src/
│   ├── main.rs                   # axum + OTel init via bank-common
│   ├── routes/{mod.rs, login.rs, verify.rs}
│   ├── domain/                   # business logic (IO-free, testable)
│   ├── repo/                     # sqlx queries
│   ├── failure_modes.rs          # env-gated injection knobs
│   └── health.rs                 # /health, /ready
└── tests/routes.rs
```

All four services follow this skeleton. `failure_modes.rs` covers application-level fault classes (`error_rate_spike`, `cold_start`); Toxiproxy/Pumba handle the rest.

### `correlation-core` internal structure

```
crates/correlation-core/src/
├── lib.rs              # public surface: Engine, CorrelationConfig,
│                       # IncidentContext, BackendError, TelemetryBackend
├── config.rs           # CorrelationConfig (toml-loadable)
├── backend.rs          # TelemetryBackend trait, BackendError, retry policy
├── graph/{mod.rs, nodes.rs, edges.rs, builder.rs, invariants.rs}
├── anomaly/{mod.rs, zscore.rs, ewma.rs}
├── ranking/{mod.rs, scoring.rs, propagation.rs}
├── schema/{mod.rs, version.rs, renderer_md.rs}
└── time.rs             # clock injection trait + WallClock + TestClock
```

### Binary entry points

- **`correlation-cli` (`corr`):** `trace <id>`, `anomaly --metric ... --service ... --window ...`, `render <incident.json>`.
- **`correlation-http`:** axum process; `POST /correlate/trace`, `POST /correlate/anomaly`, `GET /healthz`. Shared `Engine` across requests.
- **`experiment-runner` (`exp`):** `run <yaml>`, `suite <glob>`. Owns labels DB. `--dry-run` validates YAML + connectivity without firing chaos.
- **`eval-harness` (`eval`):** `run`, `sweep`, `reproduce <id>`, `report --tag ...`.
- **`bank-loadgen`:** invoked by runner; also standalone for sandbox exploration.

### Compose, two profiles

`docker compose up` boots only application + telemetry + chaos planes (the sandbox). `docker compose --profile research up` adds runner + correlation HTTP + harness. This satisfies the "stop the research plane, still have a working observability sandbox" promise.

### Build & dev workflow

- `cargo build` builds everything.
- `cargo test` runs unit + snapshot + (short) property suites.
- `cargo test --features e2e` runs e2e (needs Docker).
- `cargo insta review` for snapshots.
- `cargo-llvm-cov` for coverage gate.
- Dockerfiles use `cargo-chef` for incremental build caching.
- `.dockerignore` excludes `target/`, `data/`, `results/`, `tests/e2e/`.

---

## 9. Locked-in decisions (quick reference)

| Topic | Decision |
|---|---|
| Language | Rust everywhere (services + engine + runner + harness + loadgen) |
| Orchestrator | Docker Compose |
| Telemetry storage | Tempo (traces) + Loki (logs) + Prometheus (metrics) via OTel collector |
| Domain | Fake banking: auth, accounts, transactions, notifications |
| Correlation scope | Trace-anchored + temporal + anomaly-driven + reconstruction |
| Engine architecture | Approach B: evidence graph + deterministic scoring |
| Engine interface | Library + thin CLI + thin HTTP |
| Output | Canonical JSON (`IncidentContext` v1.0.0) + deterministic Markdown renderer; persisted to SQLite |
| Chaos | Toxiproxy (network) + Pumba (container) |
| Experiment runner | Dedicated Rust service; YAML definitions; labels SQLite with `status` enum |
| Recovery definition | Three-signal conjunction (health + load 5xx + prom error rate) |
| Anomaly detectors v1 | z-score + EWMA (deliberate baselines; no ML) |
| Log bucketing | 10s per (service, level) |
| Causal propagation | Transitive over `CausedBy` up to depth 3, β = 0.5 |
| Success metrics | precision/recall@k + four completeness ratios + time-to-context; combined into composite |
| Composite weights | (0.50, 0.10, 0.25, 0.10, −0.05); revisit after first real run |
| Eval invocations | Both trace and anomaly modes per experiment |
| Coverage denominator | Per-failure-class curated `coverage_targets.toml` |
| Degradation channel | `IncidentContext.notes[]`; library never logs above `debug` |
| Determinism | Same `(telemetry, config)` → byte-identical JSON; verified by `eval reproduce` |
| Snapshot policy | `insta` review required in PR (hard gate) |
| e2e cadence | Three e2e tests, nightly only |
| Repo layout | Monorepo + Cargo workspace; local image builds |

## 10. Future work (called out explicitly so it stays out of v1)

- Image publishing to a private registry (one CI job + `compose.eval.yaml` overlay).
- ClickHouse adapter against the `TelemetryBackend` trait.
- ML/GNN ranking on top of `EvidenceGraph` (the original research motivation).
- MAD anomaly detector alongside z-score and EWMA.
- Richer Grafana dashboards.
- Web UI for `IncidentContext` browsing.
- Authentication on the HTTP shell.
- Multi-tenant telemetry.
- Alertmanager webhook → engine anomaly path.
