# Operations Guide

Practical guide for working with the OTel correlation engine after
`v0.1.0`. Assumes you've run `docker compose --profile research up -d`
once and the stack is healthy.

## Adding a new chaos scenario

1. Copy `experiments/payment-storm-001.yaml` as a template.
2. Set a unique `id`, write a one-line `description`.
3. Set `failure_class` to one of:
   `dependency_latency`, `dependency_outage`, `db_connection_exhaustion`,
   `cpu_saturation`, `memory_pressure`, `container_restart`,
   `network_partition`, `error_rate_spike`, `cold_start`.
4. Fill in `load.profile` (which endpoints to drive, at what RPS).
5. Fill in `faults` (when to apply, what fault, target proxy/container).
6. Fill in `ground_truth` (`primary_faulted_service`,
   `expected_blast_radius`, `expected_clean_services`).
7. If you added a new `failure_class`, update:
   - `configs/coverage_targets.toml` (which metrics are detectable)
   - `configs/anomaly_invocation.toml` (which metric to feed the
     anomaly path)
8. Run a single experiment:

       docker compose exec experiment-runner exp run /experiments/your-scenario.yaml

## Sweeping engine parameters

Use a `sweep.toml` modeled on `configs/sweep.example.toml`:

    [anomaly]
    z_score_k = [2.5, 3.0, 3.5]
    ewma_alpha = [0.2, 0.3, 0.5]

    [ranking]
    causal_propagation_beta = [0.3, 0.5, 0.7]

    [window]
    expansion_sec = [15, 30, 60]

(Sweep CLI orchestration is currently a follow-up; v1 runs one config
per `eval run` invocation.)

## Interpreting an eval report

`results/<tag>/report.md` has:

- **Headline**: average recall@k, precision@3, composite, elapsed_ms.
- **By invocation mode**: trace vs anomaly path scored separately so
  you can see which path the engine handles better.
- **Misses (recall@3 = 0)**: scenarios where the primary faulted
  service wasn't in the top 3 suspects. Start here when iterating.

Per the spec §5 composite formula, the default weighting is:
50% recall@3, 10% precision@3, 25% completeness_mean, 10% inverse
time, -5% clean-service false positives.

## Reproducibility

Every eval run persists the full `CorrelationConfig` JSON into
`eval_runs.config_json` and the SHA256 hash into `eval_runs.config_hash`.
The determinism canary (`eval reproduce <eval_run_id>`) rebuilds the
engine from the stored config and verifies recomputed composite scores
match stored within ε = 0.001. CI runs this nightly
(`.github/workflows/reproduce.yml`).

## Refreshing test fixtures

`crates/correlation-core/tests/fixtures/scenarios/*` are minimal hand-
crafted JSON fixtures. To regenerate against the live stack:

1. Bring up `docker compose --profile research up -d`.
2. Trigger a scenario, let telemetry land.
3. Capture Tempo/Loki/Prom responses to the fixture JSON files.
4. Re-run snapshot tests with `INSTA_UPDATE=always cargo test -p correlation-core`.
5. Review snapshots with `cargo insta review`.

(Programmatic fixture regeneration is `tools/gen-fixture/` —
documented but not yet implemented as of `v0.1.0`.)

## Branch protection + CI gates

`main` requires `unit`, `snapshot`, `property` checks green for merge.
e2e and reproduce are nightly and informational. Branch protection
config is in [`docs/operations/branch-protection.md`](operations/branch-protection.md).
