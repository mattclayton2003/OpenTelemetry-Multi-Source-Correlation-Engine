# correlation-core

The pure correlation library: graph, ranking, anomaly detection,
schema, Markdown renderer. No IO. Backend adapters live in
separate crates (Phase 3).

## Engine paths

- `correlate_trace(trace_id)` — fetch spans, expand window, fetch logs,
  build evidence graph, rank suspects, emit IncidentContext.
- `correlate_anomaly(metric, service, window, value)` — detect via
  z-score, build incident from detected anomaly.

## Testing

Integration tests use `MockBackend` (gated behind `test-helpers` feature):

    cargo test -p correlation-core --features test-helpers

## Schema

See `docs/superpowers/specs/2026-05-23-otel-correlation-engine-design.md`
§4 for the `IncidentContext` schema (v1.0.0).
