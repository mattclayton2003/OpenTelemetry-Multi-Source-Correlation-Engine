# OpenTelemetry Multi-Source Correlation Engine

A Rust observability sandbox plus a correlation engine that produces
structured incident context documents from OpenTelemetry data.

See [`docs/superpowers/specs/2026-05-23-otel-correlation-engine-design.md`](docs/superpowers/specs/2026-05-23-otel-correlation-engine-design.md)
for the design and
[`docs/superpowers/plans/2026-05-23-otel-correlation-engine.md`](docs/superpowers/plans/2026-05-23-otel-correlation-engine.md)
for the implementation plan.

## Quickstart

    docker compose up                       # sandbox only
    docker compose --profile research up    # + correlation engine + runner
