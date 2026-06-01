# eval-harness

Evaluation harness for the OTel correlation engine. Joins
`labels.db` (experiment-runner ground truth) with `incidents.db`
(engine output) and scores against ground truth per spec §5.

## Metrics

- `recall@k` for k ∈ {1, 3, 5} — primary faulted service in top-k
- `precision@k` — top-k overlap with {primary} ∪ blast_radius
- 4 completeness ratios: trace, error log, anomaly, span-tree integrity
- `composite` — weighted sum per `configs/scoring.toml`

## Commands

    eval run --suite 'experiments/*.yaml' --tag v0.1
    eval report --tag v0.1
    eval reproduce <eval_run_id>

## Reproducibility

Each eval run persists the full `CorrelationConfig` JSON to
`eval_runs.config_json` so `eval reproduce` can rebuild the engine
identically. Determinism canary verifies recomputed composite matches
stored within ε = 0.001 (see `canary.rs`).

## Migrations

- `migrations_eval/` — eval_runs + eval_results
- `migrations_incidents/` — incidents table (engine output cache)
- labels DB migrations owned by experiment-runner.
