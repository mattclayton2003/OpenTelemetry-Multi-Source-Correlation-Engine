# OpenTelemetry Multi-Source Correlation Engine

A Rust correlation engine that turns OpenTelemetry **traces, logs, and metrics**
into a single structured **`IncidentContext`** — a ranked, evidence-backed
root-cause document — plus the observability sandbox and reproducible evaluation
harness needed to measure how well it works.

It ships with a distributed banking app (auth · accounts · transactions ·
notifications) wired to Tempo/Loki/Prometheus, a library of labelled chaos
experiments, and an eval harness that scores the engine against ground truth.
Built as a pre-PhD research artifact for AI-Ops correlation work.

- Design spec: [`docs/design-spec.md`](docs/design-spec.md)
- Implementation plan: [`docs/implementation-plan.md`](docs/implementation-plan.md)
- Operations guide: [`docs/operations.md`](docs/operations.md)
- Live demo walkthrough: [`docs/demo-walkthrough.md`](docs/demo-walkthrough.md)

---

## The idea in one paragraph

When a distributed system degrades, the signal is scattered across three
telemetry stores: a slow span in Tempo, an error burst in Loki, a p99 spike in
Prometheus. A human on-call engineer mentally joins them to find the culprit.
This engine does that join mechanically. Given a trigger (a slow/failed **trace**,
or a metric **anomaly**), it pulls the relevant evidence from all three backends,
builds an **evidence graph**, propagates blame along causal edges, and emits a
ranked list of suspect services with the exact evidence behind each score — as a
machine-readable `IncidentContext` that a human, a dashboard, or an LLM can read.

---

## Architecture — three planes in one Compose stack

```
┌─ Application plane ─────────────────────────────────────────────┐
│  auth · accounts · transactions · notifications  (axum, Rust)   │
│  OpenTelemetry SDK on every service · W3C trace-context propag.  │
└───────────────┬─────────────────────────────────────────────────┘
                │ OTLP
┌─ Telemetry plane ──────────▼────────────────────────────────────┐
│  OTel Collector ──┬─► Tempo        (traces)                      │
│   (+ spanmetrics  ├─► Loki         (logs)                        │
│    connector,     ├─► Prometheus   (metrics: p99, calls_total)   │
│    zipkin export) └─► Zipkin       (dependency graph + traces)   │
│  Toxiproxy fronts postgres + SMTP for fault injection            │
└───────────────┬─────────────────────────────────────────────────┘
                │ HTTP query (TraceQL / LogQL / PromQL)
┌─ Research plane ───────────▼────────────────────────────────────┐
│  correlation-engine  (correlation-core: the algorithm)          │
│    ├─ corr            CLI: trace · anomaly · render · explain    │
│    └─ correlation-http  service: /correlate/{trace,anomaly}     │
│  experiment-runner  (exp): runs YAML chaos + writes ground truth │
│  eval-harness       (eval): scores engine output vs labels      │
│  Grafana dashboards · HTML scorecard                            │
└─────────────────────────────────────────────────────────────────┘
```

---

## How the engine works

The engine lives in **`correlation-core`** and is backend-agnostic: it talks to
telemetry only through one trait, so the same algorithm runs against live
Tempo/Loki/Prometheus, an in-memory mock in tests, or any future store.

```rust
#[async_trait]
pub trait TelemetryBackend: Send + Sync {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError>;
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError>;
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError>;
    async fn query_metric_window(&self, q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError>;
}
```

`MultiBackend` fans these four calls out to the three real adapters
(`correlation-tempo`, `correlation-loki`, `correlation-prom`).

The pipeline (`Engine::correlate_trace` / `Engine::correlate_anomaly`):

1. **Trigger → window.** A trace id or `(metric, service, window)` anchors a
   time window, expanded by `window_expansion_sec` to catch lead/lag.
2. **Gather.** Fetch the trace's spans, the logs in-window per service, and the
   metric series around the event — from the three backends.
3. **Build the evidence graph** (`graph/`). Nodes: `Span`, `Service`,
   `LogBatch`, `MetricAnomaly`. Edges: `EmittedBy`, `ParentOf`, `CausedBy`.
   Strict insertion invariants are enforced (and property-tested).
4. **Score direct evidence** (`ranking/scoring.rs`) per service:
   - **error** — error spans / error-level log batches,
   - **anomaly** — a flagged metric anomaly (z-score / EWMA detector),
   - **latency** — a span's *self-time* (`duration − Σ children`) exceeding
     `slow_span_self_ms`. This is the key signal: it blames the **slow worker**,
     not a caller merely *blocked waiting* on it.
5. **Propagate** (`ranking/propagation.rs`). Blame flows backward along causal
   edges, decayed by `causal_propagation_beta` up to `…_max_depth`, so an
   upstream dependency's fault surfaces on the right service.
6. **Rank & assemble.** Services sort by score (one entry each, deterministic
   tie-break). The result is an `IncidentContext` with the suspects, their
   per-signal **evidence breakdown**, the span tree, log batches, metric
   anomalies, a timeline, and free-text notes.

The `IncidentContext` (`schema/`) is the product. `render_md` turns it into a
human/LLM-readable markdown report; `corr explain` hands that grounded document
to the Claude API for a plain-English narrative.

### Anomaly detectors (`anomaly/`)
- **z-score** — robust median/MAD variant, translation- and scale-invariant.
- **EWMA** — exponentially-weighted moving average, deterministic.

Both are property-tested (constant-series never flags, invariances hold).

---

## Repository layout

| Crate | Binary | Responsibility |
|---|---|---|
| `correlation-core` | — | The engine: backend trait, evidence graph, anomaly detectors, ranking, `IncidentContext` schema + `render_md`. |
| `correlation-tempo` / `-loki` / `-prom` | — | Backend adapters: TraceQL / LogQL / PromQL → the trait. |
| `correlation-cli` | `corr` | CLI: `trace`, `anomaly`, `render <json>`, `explain` (LLM). |
| `correlation-http` | — | HTTP service: `POST /correlate/trace`, `/correlate/anomaly`, `/healthz`. |
| `services/{auth,accounts,transactions,notifications}` | per-service | The banking app — axum services, OTel-instrumented, with injectable failure modes. |
| `bank-common` | — | Shared: errors, health, OTel setup (`otel.rs`: trace-context propagation, span kinds), failure modes. |
| `bank-loadgen` | — | Load generation (profiles, runner, stats). |
| `chaos` | — | Fault drivers: Toxiproxy (latency/down toxics) and Pumba (container chaos). |
| `experiment-runner` | `exp` | Runs a YAML experiment (load + faults), records **labelled ground truth** to `labels.db`. |
| `eval-harness` | `eval` | Discovers a representative incident per experiment, runs the engine in both modes, scores vs ground truth, writes incidents + reports. |

---

## Data flow at a glance

```
request → service (OTel span) ─OTLP→ Collector ─→ Tempo / Loki / Prometheus
                                                          │
   corr / correlation-http  ──TraceQL/LogQL/PromQL────────┘
        │
        └─► Engine.correlate_*  →  EvidenceGraph  →  ranking  →  IncidentContext
                                                                    │
                                            ├─ render_md → markdown report
                                            ├─ corr explain → LLM narrative
                                            └─ eval-harness → scores vs labels
```

---

## Quickstart

```sh
# 1. Sandbox only (services + telemetry)
docker compose -f compose/docker-compose.yaml up -d

# 2. Full research stack (adds engine, runner, eval-harness, exporter, Grafana, Zipkin)
docker compose -f compose/docker-compose.yaml --profile research up -d
#    NOTE: after rebuilding any image, add --force-recreate (stale-image gotcha).

# 3. Correlate a trace from the CLI (talks to localhost Tempo/Loki/Prom)
cargo run -p correlation-cli -- trace <trace-id>
cargo run -p correlation-cli -- trace <trace-id> --json   # raw IncidentContext

# 4. Or hit the HTTP engine
curl -s localhost:8500/correlate/trace -H 'content-type: application/json' \
     -d '{"trace_id":"<trace-id>"}' | jq

# 5. Plain-English root cause via the Claude API (needs ANTHROPIC_API_KEY)
export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p correlation-cli --release -- explain --trace-id <trace-id>
cargo run -p correlation-cli --release -- explain --incident docs/sample-incident.json --dry-run
```

### Ports

| Service | URL | Service | URL |
|---|---|---|---|
| Grafana | http://localhost:3001 | Tempo | http://localhost:3200 |
| Zipkin | http://localhost:9411 | Loki | http://localhost:3100 |
| Prometheus | http://localhost:9090 | Engine HTTP | http://localhost:8500 |
| auth / accounts | :8001 / :8002 | transactions / notifications | :8003 / :8004 |
| Toxiproxy API | http://localhost:8474 | eval metrics exporter | http://localhost:9112 |

---

## Experiments & the eval harness

Each experiment is a YAML file in [`experiments/`](experiments/) declaring the
load profile, the fault schedule, and — crucially — the **ground truth**:

```yaml
ground_truth:
  primary_faulted_service: notifications
  expected_blast_radius: [transactions]
  expected_clean_services: [auth]
  failure_class: dependency_latency
```

`experiment-runner` (`exp`) drives the load + faults and writes the labels.
`eval-harness` (`eval`) then, for every experiment and in **both** invocation
modes (trace + anomaly), discovers a representative incident, runs the engine,
and scores the result against the labels:

- **recall@k / precision@k** — is the faulted service ranked in the top-k, and
  how many top-k picks are real (faulted ∪ blast radius)?
- **completeness** — trace / error-log / anomaly / tree-integrity coverage.
- **composite** — the weighted headline score (`configs/scoring.toml`).

```sh
docker compose -f compose/docker-compose.yaml exec eval-harness \
    eval run --suite '/experiments/*.yaml' --tag v0.1
docker compose -f compose/docker-compose.yaml exec eval-harness \
    eval report --tag v0.1
```

Each run writes per-incident RCA markdown to `results/<tag>/incidents/<exp>-<mode>.md`,
the incident JSON to `data/incidents.db`, and scores to `data/eval_runs.db`
(surfaced in the Grafana **Eval** dashboard and `results/scorecard.html`).
Configuration is hashed into every run for reproducibility.

---

## Live demo

```sh
docker compose -f compose/docker-compose.yaml --profile research up -d
./scripts/demo.sh                  # interactive (pauses between acts)
DEMO_NOPAUSE=1 ./scripts/demo.sh   # straight through
```

The demo walks a five-act arc: healthy system → inject an 800 ms SMTP-latency
fault → watch the telemetry react → hand a slow trace to the engine (it
pinpoints the culprit) → an LLM narrates the finding → reproducible eval coda.
Act 5 uses the Claude API if `ANTHROPIC_API_KEY` is set, otherwise it shows a
recorded sample so the demo runs with zero spend. A full beat-by-beat
explanation is in [`docs/demo-walkthrough.md`](docs/demo-walkthrough.md).

---

## Testing & CI

```sh
cargo test --workspace --features correlation-core/test-helpers
cargo test -p correlation-core --test properties   # graph / detector / ranking / schema invariants
cargo test --workspace --features e2e              # requires the stack up
```

PR-gating workflows: `unit.yml` (fmt + clippy `-D warnings` + tests),
`property.yml` (proptests), `snapshot.yml`. `reproduce.yml` / `e2e.yml` are
nightly/dispatch (they need the live stack).

> macOS local note: testcontainers tests need
> `DOCKER_HOST=unix:///Users/<you>/.docker/run/docker.sock` (Docker Desktop
> socket isn't auto-resolved by bollard).

See [`docs/operations.md`](docs/operations.md) for adding scenarios, sweeping
parameters, interpreting reports, and refreshing fixtures.
