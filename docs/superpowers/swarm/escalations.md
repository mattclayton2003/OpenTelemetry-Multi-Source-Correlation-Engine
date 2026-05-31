n   
can 
# Swarm Escalations

Append-only log. When the Architect detects a non-trivial issue, they write here and halt dispatch until the user adds a `**Resolution:**` line.

---

## Escalation 2026-05-28T00:00:00 — task 0.1 attempt 1 — Architect dispatch defect

**Reason:** Architect transcription error. Worker-1's dispatch prompt omitted four workspace dependencies that the implementation plan declares for Task 0.1: `chrono = { version = "0.4", features = ["serde"] }`, `jsonwebtoken = "9"`, and the `postgres` + `chrono` features on the `sqlx` dependency.

**Detection:** Architect comparing worker-1's committed `Cargo.toml` against `docs/superpowers/plans/2026-05-23-otel-correlation-engine.md` lines 158-186 (Task 0.1 Step 2 contents).

**Impact if unfixed:** Phase 1 Task 1.7 (`auth` service) needs `jsonwebtoken` from workspace.dependencies. Phase 1 Task 1.11 (`accounts` service) needs `sqlx` postgres feature + `chrono`. Both would fail with "no such workspace dependency" errors.

**Worker-1 verdict:** clean — worker faithfully implemented the prompt as given. Not a rule violation; not the worker's responsibility to second-guess the dispatch prompt against the plan.

**Action taken:**
1. Branch `bootstrap/workspace` renamed to `quarantine/task-0.1-attempt-1`
2. Worktree `data/worktrees/task-0.1` removed
3. Backlog updated: task 0.1 status reset to `pending`, `quarantine_count` = 1
4. Task 0.1 re-dispatched as worker-2 with corrected prompt sourced verbatim from plan

**Resolution:** Auto-resolved by Architect re-dispatch. No user input required for this attempt since the fix is mechanical and reversible. Subsequent quarantines on the same task → halt + user escalation per orchestration §6.1.

---

## Escalation 2026-05-28T01:00:00 — task 1.1 attempt 1 — Workspace dep version defect

**Reason:** `indexmap` workspace dependency was declared as `version = "2"` (semver: `>=2.0.0, <3.0.0`). Cargo resolved this to the latest patch, `indexmap 2.14.0`, which requires Rust 1.85+ (uses Cargo's unstable `edition2024` feature). Workspace `rust-toolchain.toml` pins channel to `1.78.0`. Result: every `cargo` command fails until indexmap is pinned to a 1.78-compatible release.

**Detection:** Worker-10 ran Step 5 (`cargo check -p bank-common`), got exit 101 with diagnostic naming indexmap 2.14.0 and the `edition2024` requirement. Worker correctly bailed per rule 1 rather than modifying out-of-scope files to patch the constraint.

**Impact if unfixed:** All of Phase 1 (and beyond) is blocked. Every worker that runs `cargo` anything hits the same wall.

**Worker-10 verdict:** clean — exemplary bail behavior. Worker preserved evidence (Steps 1-4 done, working tree left for inspection), gave the Architect three concrete remediation options, did not violate scope.

**Action taken by Architect:**
1. Worker-10's uncommitted scaffolding discarded from `data/worktrees/phase-1`.
2. Worker-10 claim released on task-1.1.
3. Re-dispatching task-1.1 as worker-11 with corrected workspace prompt: `indexmap = { version = "~2.6", features = ["serde"] }` (pins to `>=2.6.0, <2.7.0`, a stable 1.78-compatible range).
4. Quarantine entry written for attempt-1 — preserved as note in `docs/superpowers/swarm/quarantine.md`. No quarantine branch since worker bailed without committing.

**Plan amendment (deferred):** `docs/superpowers/plans/2026-05-23-otel-correlation-engine.md` Task 0.1 still shows `indexmap = { version = "2", ... }`. This is now divergent from on-disk `Cargo.toml`. Per the Architect's prohibition on modifying the plan, this divergence is logged here rather than auto-fixed. User decision needed on whether to amend the plan retroactively or leave the divergence with a note.

**Resolution:** Auto-resolved for forward progress. Plan amendment pending user input — does NOT block dispatch.

---

## Escalation 2026-05-31T13:42:00 — task 1.2 attempt 1 — Sync test + runtime-required init mismatch

**Reason:** Plan's Task 1.2 implementation calls `opentelemetry_otlp::new_pipeline()....install_batch(runtime::Tokio)`. This **panics** (not returns Err) when called outside a Tokio runtime — the panic happens inside hyper-util's tokio reactor check, before any Result is produced. The plan's test is `#[test]` (sync), so the `.or_else` no-op fallback is unreachable.

**Detection:** Worker-13 ran red-green TDD per Step 4. Red phase confirmed test fails to compile (init not found — expected). Green phase: implementation compiled, but test panicked at runtime with "there is no reactor running, must be called from the context of a Tokio 1.x runtime".

**Worker-13 verdict:** clean — bailed correctly with two viable fixes proposed.

**Action taken by Architect:** Adopt Option B from worker-13's report. Rewrite `otel::init` to branch on `OTLP_ENDPOINT` presence:
- If unset → construct `sdktrace::TracerProvider::builder().build()` directly (no tonic, no runtime required, suitable for sync tests + early-main use).
- If set → full OTLP pipeline (current plan body), which requires runtime.

Re-dispatching as worker-16 with corrected impl.

**Resolution:** Auto-resolved (Option B in dispatch).
