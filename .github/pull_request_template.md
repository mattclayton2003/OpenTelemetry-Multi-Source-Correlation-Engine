## Summary
<!-- 1-3 bullets: what changed and why. -->

## References
- Spec: §<N> (`docs/design-spec.md`)

## Pre-merge checklist
- [ ] `cargo test --workspace` green locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] Snapshot diffs reviewed with `cargo insta review`; no `.snap.new` files remain
- [ ] No new `SystemTime::now()` call inside `correlation-core` (determinism)
- [ ] New dependencies justified below
- [ ] If a new scenario YAML was added: `ground_truth` complete and `failure_class` in the spec enum

## Notes for reviewer
<!-- Anything non-obvious: design tradeoff, deferred TODO, snapshot intent, etc. -->
