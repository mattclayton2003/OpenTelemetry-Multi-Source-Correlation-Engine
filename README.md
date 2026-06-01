# OpenTelemetry Multi-Source Correlation Engine

A Rust observability sandbox plus a correlation engine that produces
structured `IncidentContext` documents from OpenTelemetry traces, logs,
and metrics. Designed as a research artifact for AI-Ops correlation
work — comes with a labeled chaos dataset and a reproducible evaluation
harness.

See the spec at [`docs/superpowers/specs/2026-05-23-otel-correlation-engine-design.md`](docs/superpowers/specs/2026-05-23-otel-correlation-engine-design.md)
and the implementation plan at [`docs/superpowers/plans/2026-05-23-otel-correlation-engine.md`](docs/superpowers/plans/2026-05-23-otel-correlation-engine.md).

## Architecture

Three planes in one Docker Compose stack:

1. **Application** — 4 Rust banking microservices (auth, accounts, transactions, notifications) with OpenTelemetry instrumentation.
2. **Telemetry** — OTel collector routes traces → Tempo, logs → Loki, metrics → Prometheus. Toxiproxy in front of postgres + SMTP for chaos.
3. **Research** — `correlation-engine` (CLI + HTTP), `experiment-runner` (YAML-driven chaos + labeled ground truth), `eval-harness` (scores engine against labels).

## Quickstart

    # sandbox only (services + telemetry)
    docker compose -f compose/docker-compose.yaml up -d

    # full research stack (adds engine + runner + eval-harness + pumba + grafana)
    docker compose -f compose/docker-compose.yaml --profile research --profile chaos up -d

    # run a chaos experiment manually
    docker compose -f compose/docker-compose.yaml exec experiment-runner \
        exp run /experiments/payment-storm-001.yaml

    # score the engine against the labeled dataset
    docker compose -f compose/docker-compose.yaml exec eval-harness \
        eval run --suite '/experiments/*.yaml' --tag v0.1
    docker compose -f compose/docker-compose.yaml exec eval-harness \
        eval report --tag v0.1

    # browse telemetry
    open http://localhost:3001    # Grafana
    open http://localhost:3200    # Tempo
    open http://localhost:9090    # Prometheus
    open http://localhost:3100    # Loki

## Tested via

    cargo test --workspace --features correlation-core/test-helpers
    cargo test --workspace --features e2e   # requires docker compose up first

## Detailed operations

See [`docs/operations.md`](docs/operations.md) for adding scenarios, sweeping
parameters, interpreting reports, and refreshing fixtures.
