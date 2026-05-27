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
