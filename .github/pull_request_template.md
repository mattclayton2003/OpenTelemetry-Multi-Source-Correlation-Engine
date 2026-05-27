## Summary
<!-- 1-3 bullets: what changed and why. Link the phase. -->

## Spec / plan references
- Spec: §<N>
- Plan tasks: <list of Task N.M>

## Pre-merge checklist
- [ ] All new tasks committed atomically (one commit per task)
- [ ] `cargo test --workspace` green locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] Snapshot diffs reviewed with `cargo insta review`; no `.snap.new` files remain
- [ ] No new `SystemTime::now()` call inside `correlation-core` (determinism)
- [ ] New dependencies justified below
- [ ] If a new scenario YAML was added: `ground_truth` complete and `failure_class` in the spec enum
- [ ] Plan task checkboxes ticked in `docs/superpowers/plans/2026-05-23-otel-correlation-engine.md`

## Notes for reviewer
<!-- Anything non-obvious: design tradeoff, deferred TODO, snapshot intent, etc. -->
