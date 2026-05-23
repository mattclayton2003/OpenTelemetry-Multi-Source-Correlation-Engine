# Ruflo Swarm Orchestration Plan

**Goal:** Execute the [OTel correlation engine implementation plan](./2026-05-23-otel-correlation-engine.md) via a ruflo hive — multiple workers in parallel, queen coordination, automated rogue detection, full autonomous file/git/docker access, no per-action permission prompts.

**Companion document, not a replacement.** The implementation plan defines *what* to build (80 tasks across 8 phases). This document defines *how* the swarm executes those tasks safely without a human in the loop.

**Non-negotiable invariants:**

1. **The plan is the source of truth.** Workers execute existing tasks; they cannot invent new tasks or reorder phases. Plan edits require a Queen-Architect change request and reviewer approval.
2. **`main` is sacred.** Only the Reviewer Queen can merge to `main`. Workers cannot push there.
3. **Tests are the gate.** No commit lands on `main` without all required checks green (`unit`, `snapshot`, `property-short`).
4. **The Sentinel cannot be disabled by workers.** It runs as a separate process watching from outside the worker session.
5. **Determinism rules from spec §6 are enforced as automated checks**, not aspirations.

---

## 1. Topology

```
                       ┌─────────────────────────┐
                       │  Queen-Architect         │
                       │  - owns the backlog       │
                       │  - builds dep graph       │
                       │  - dispatches tasks       │
                       │  - cross-task consistency │
                       └─────┬────────────┬──────┘
                             │            │
                  dispatch   ▼            ▼  review request
                 ┌────────────────┐  ┌────────────────────┐
                 │  Worker pool   │  │  Queen-Reviewer    │
                 │  (3-4 active)  │  │  - merge authority │
                 │  Worker A      │  │  - runs full suite │
                 │  Worker B      │  │  - veto rights     │
                 │  Worker C      │  └────────────────────┘
                 │  Worker D      │
                 └────────┬───────┘
                          │ every tool call
                          ▼
                 ┌────────────────────┐
                 │  Sentinel          │
                 │  - rogue detection │
                 │  - emits kill sigs │
                 │  - quarantine log  │
                 └────────────────────┘
```

**Four roles, four ruflo agent profiles.** No worker is also a queen. No queen writes code.

### 1.1 Queen-Architect

- **Owns:** the backlog (the 80 tasks), the dependency graph, the dispatch queue.
- **Reads:** plan tasks, quarantine log, worker status.
- **Writes:** task assignments, retry annotations on quarantined tasks, dependency edges.
- **Cannot:** modify task content, modify spec or plan files, write code, push to git.
- **Spawn:** singleton.

### 1.2 Queen-Reviewer

- **Owns:** merge authority for `main`.
- **Reads:** completed worker branches, CI status, full PR diff.
- **Writes:** merge-commits to `main`, review comments, reject-with-reason for branches that fail the merge checklist.
- **Cannot:** write code, modify the spec or plan, dispatch tasks.
- **Spawn:** singleton.

### 1.3 Worker

- **Owns:** one task at a time, in an isolated git worktree.
- **Reads:** assigned task content, files declared in the task's `Files:` block, the spec, files in their worktree.
- **Writes:** files in their worktree only; atomic commits on their feature branch; `git push` to their branch only.
- **Cannot:** modify files outside the task's declared `Files:` block (Sentinel kill); touch other workers' worktrees (worktree isolation); push to `main`; modify CI workflows, settings, or hooks (Phase 0 only, and only with Reviewer pre-approval); add dependencies not in `workspace.dependencies` unless the task itself adds them.
- **Spawn:** up to N concurrent (default N=3; tunable per phase).

### 1.4 Sentinel

- **Owns:** rogue detection, kill signal emission.
- **Watches:** every worker's tool calls via ruflo hooks (`hooks_pre-edit`, `hooks_pre-command`, `hooks_post-edit`, `hooks_post-command`).
- **Writes:** quarantine log entries, kill signals to `agent_terminate`.
- **Cannot:** modify code, modify tasks, merge to git.
- **Spawn:** singleton. **Runs outside worker session contexts** so a worker cannot inject prompts to disable it.

---

## 2. Rogue detection rules

Each rule has a **detection predicate**, a **severity** (warn / quarantine / kill-immediate), and a **response**. The Sentinel evaluates every worker action against this list.

| # | Predicate | Severity | Response |
|---|---|---|---|
| 1 | Worker edits a file not in the assigned task's `Files:` block | **kill-immediate** | terminate worker, quarantine branch, return task to backlog with note |
| 2 | Worker invokes `git push origin main` or `git push --force` (any target) | **kill-immediate** | terminate, quarantine, audit log |
| 3 | Worker invokes `rm -rf`, `git reset --hard`, `git clean -fd`, `git checkout .` outside its worktree | **kill-immediate** | same |
| 4 | Worker modifies `.github/workflows/*`, `.github/dependabot.yml`, or `Cargo.lock` outside the explicit Phase 0 + 8 CI tasks | **quarantine** | terminate, branch quarantined, Reviewer audits |
| 5 | Worker adds a dependency to `Cargo.toml` not already in `workspace.dependencies` without the task explicitly authorizing it | **quarantine** | same |
| 6 | Worker writes `SystemTime::now()` inside any file under `crates/correlation-core/src/` | **quarantine** | spec §6 determinism violation |
| 7 | Worker introduces `unsafe { ... }` blocks not present in the task spec | **quarantine** | requires Reviewer audit |
| 8 | Worker exceeds task time budget (default 30 min for any task) | **kill** | terminate, branch preserved, retry with fresh worker |
| 9 | Worker exceeds task token budget (default 100k tokens) | **kill** | same |
| 10 | Worker commits a file containing patterns matching common secret formats (AWS key, JWT signing secrets, `.env`-style assignments with HEX/base64 values > 32 chars) | **kill-immediate** | terminate, branch shredded (not quarantined), audit log |
| 11 | Worker's branch fails `cargo build` or `cargo test` after declaring task complete | **warn → retry once → quarantine** | up to one auto-retry on the same worker; if it still fails, quarantine and rotate |
| 12 | Worker pushes a commit with no `Co-Authored-By:` trailer or a non-conventional commit prefix | **warn** | log; allow merge if reviewer approves; do not block |
| 13 | Worker writes a file outside the project root (`/tmp`, `~/`, etc) without it being declared as a temp file | **kill-immediate** | terminate, quarantine |
| 14 | Worker invokes any of: `curl <external-host>`, `wget`, `nc`, `ssh` to a non-localhost target | **quarantine** | exception list: `crates.io`, `index.crates.io`, `static.crates.io`, `github.com`, `*.dl.fedoraproject.org`-style mirrors used by Docker base images |
| 15 | Worker modifies the spec doc, plan doc, branch-protection doc, or this orchestration doc | **kill-immediate** | terminate, quarantine — only Queen-Architect (with user approval) edits plan/spec docs |
| 16 | Worker invokes `mcp__ruflo__agent_terminate` or any swarm-level admin tool | **kill-immediate** | terminate, audit log — workers cannot kill other workers or themselves bypassing Sentinel |
| 17 | Worker's diff against `main` (or its phase branch) exceeds 5x the task's expected size (judged by `Files:` count and step content) | **quarantine** | scope-drift signal; Reviewer audits |
| 18 | Worker creates a new file not in the task's `Files:` block | **kill-immediate** | special case of #1 with clearer messaging |

Rules 1, 13, 15, 16 are the **structural** rules — they protect the swarm itself. Rules 2, 3, 10 protect the repo. Rules 4–9, 17, 18 protect the spec. Rules 11–12, 14 are quality gates.

### 2.1 What "kill-immediate" means

1. Sentinel calls `mcp__ruflo__agent_terminate` with the worker's agent ID.
2. Sentinel calls `mcp__ruflo__hooks_worker-cancel` to flush any in-flight tool calls.
3. The worker's git worktree is *not* deleted — it's renamed to `quarantine/<task-id>-attempt-<N>` for forensic review.
4. The branch is renamed to `quarantine/<task-id>-attempt-<N>`.
5. The Queen-Architect is notified; the task returns to the backlog with the quarantine reference and a note explaining which rule was violated.
6. Quarantine count for the task increments. If a task hits **2 quarantines**, it escalates: the Queen-Architect emits an escalation event and refuses to dispatch it again until a human reviews.

### 2.2 What "quarantine" (without immediate kill) means

Same as kill-immediate, but the worker is allowed to finish its current tool call before being terminated. Used when the violation might be benign (e.g., the worker tried to add a dependency that turns out to be already declared in `workspace.dependencies` — the post-check catches it but the call itself wasn't dangerous in flight).

---

## 3. Verification gates ("test through")

The user said: *"no agent goes rogue, if so kill it and we test through."* The "test through" half: every worker change must survive a multi-tier verification cascade before merging.

### Tier 1 — In-worker (pre-commit)

Before the worker can commit, it must run:

```bash
cargo build -p <crate>              # under 60s on cached workspace
cargo test  -p <crate>              # all tests in the touched crate
cargo clippy -p <crate> -- -D warnings
cargo fmt --all -- --check
```

Sentinel rule 11 enforces: if any of these fail and the worker has already declared the task complete, the worker is warned, given one auto-retry, then quarantined.

### Tier 2 — On-branch (CI)

Once the worker pushes its commit to its phase branch, the existing CI workflows run:

- `unit` (Task 0.2 from impl plan): fmt + clippy + workspace test
- `snapshot` (Task 0.7): `cargo insta test --workspace --unreferenced=reject`
- `property` (Task 0.8): proptest with `PROPTEST_CASES=64`

All three must be green before the Queen-Reviewer considers the branch mergeable.

### Tier 3 — Pre-merge (Reviewer)

The Reviewer Queen runs an additional cross-cutting check before merging:

- `cargo test --workspace --all-targets` against the phase branch HEAD (catches workspace-level breakage that single-crate tests missed)
- Diff scan against rule 17 (scope creep)
- Diff scan against rules 4–7 (CI / Cargo.lock / determinism / unsafe)
- Reads the PR body checklist (from `.github/pull_request_template.md` — Task 0.4) and confirms boxes ticked
- Confirms task checkboxes are ticked in `docs/superpowers/plans/2026-05-23-otel-correlation-engine.md`

### Tier 4 — Phase checkpoint (gated)

A phase merges to `main` only when:

- All tasks for that phase have green branches
- The phase's checkpoint task explicitly ran and passed (e.g., Task 1.26 checkpoint validates the smoke e2e test ran)
- For Phases 2, 3, 7: snapshot-test status is green AND no `.snap.new` files remain in the tree
- For Phase 8 specifically: nightly canaries (`e2e`, `reproduce`) have run successfully at least once before merging

Reviewer Queen has veto on each tier. If something looks suspect, she calls a halt — work freezes until a human reviews the quarantine log.

### Tier 5 — Cross-phase invariant (Architect)

After every merge to `main`, the Queen-Architect runs a cross-phase consistency check:

- `IncidentContext` schema version still matches `crates/correlation-core/src/schema/version.rs`
- All starter scenario YAMLs still parse (Task 6.6)
- The plan task count totals match (catches accidental task deletion or duplication)
- Spec `failure_class` enum matches scenarios' `failure_class` values

A cross-phase failure does NOT auto-revert (we have main protection), but it pauses dispatch until resolved.

---

## 4. Dispatch and dependency graph

The Queen-Architect maintains a directed acyclic graph of tasks:

- Each task is a node.
- A task is **blocked by** another task if it touches files the other creates, or if its task body references "from Task N.M" or is within a phase checkpoint chain.
- A task is **ready** when all its blockers are merged to `main`.
- Multiple ready tasks can dispatch in parallel only if their `Files:` blocks are disjoint.

### 4.1 Dispatch policy

- Queen-Architect computes the ready set on every merge to `main` (or every 60s, whichever comes first).
- Up to `N` workers may be active; default `N=3`, configurable per phase.
- For Phase 1 (which has 26 tasks), the recommended N is 4 because the service crates are highly independent.
- For Phase 2 (engine core) the recommended N is 2 because tasks chain heavily.
- The Architect avoids dispatching two tasks that touch the same file within the same 5-minute window even if they're technically ready, to reduce merge-conflict probability.

### 4.2 Worker lifecycle

```
backlog → ready → claimed → working → tested → pushed → merged
                                    ↘ quarantined → backlog (with note) or escalated
```

Worker steps explicitly:

1. **Claim** — `mcp__ruflo__claims_claim` on the task id. Architect assigns the worker.
2. **Worktree** — Worker calls `EnterWorktree` to create an isolated working copy on a fresh branch off the phase branch.
3. **Read** — Worker reads the task content, the spec sections referenced, and the files in the task's `Files:` block.
4. **Implement** — Worker executes the task's steps in order. Sentinel watches every Edit/Write/Bash call.
5. **Test** — Worker runs Tier 1 checks.
6. **Commit** — Worker runs `git add <task-files-only> && git commit` with the task's commit message template.
7. **Push** — Worker pushes to the phase branch with `--force-with-lease` (its own branch only).
8. **Release** — Worker calls `mcp__ruflo__claims_release`.
9. **Terminate** — Worker exits. A new worker takes the next ready task; we do not reuse worker sessions, to keep context clean.

If any step fails: worker reports back to Architect, branch quarantined, task returned to backlog.

---

## 5. Autonomous permissions allowlist

The user said: *"allow them full access no need to prompt me for permissions."* This is the explicit allowlist of what workers can do without confirmation. Anything not in this list still requires explicit user approval (delivered to the Queen-Architect as a halt-and-ask event).

### Allowed without prompt

| Category | Action |
|---|---|
| **Filesystem** | Read/Write files inside the project root |
| | Create directories inside the project root |
| | Delete files the worker itself created in this task |
| **Cargo** | `cargo build`, `cargo test`, `cargo check`, `cargo clippy`, `cargo fmt`, `cargo insta test`, `cargo insta review --accept` |
| | Adding a dep to `Cargo.toml` ONLY if the task explicitly lists the dependency change |
| **Git (worktree only)** | `git add`, `git commit`, `git branch`, `git checkout` |
| | `git push origin <phase-branch>` with `--force-with-lease` on the worker's own branch |
| | `git fetch`, `git rebase` onto phase branch HEAD |
| **Docker** | `docker compose -f compose/docker-compose.yaml [build|up|down|exec|logs]` for tasks that legitimately need a stack (Phases 1, 6, 8) |
| | `docker buildx build` for service Dockerfile validation |
| **Network** | HTTPS to `crates.io`, `index.crates.io`, `static.crates.io`, `github.com`, Docker Hub mirrors, `pkg-config` mirrors |
| | HTTP to localhost / Docker network targets only |
| **GitHub** | `gh pr create --draft` on the worker's branch |
| | `gh pr view`, `gh pr checks`, `gh api repos/.../pulls/...` (read-only) |

### Requires Reviewer approval (not user)

| Category | Action |
|---|---|
| **Git** | Merge to `main` (Reviewer only) |
| | `gh pr ready` (mark draft as ready) |
| **CI** | Modifying `.github/workflows/*` outside Phase 0/8 |
| | Modifying branch protection (which is documented, not configured in-repo) |

### Requires user halt-and-ask (Architect halts dispatch)

| Category | Action |
|---|---|
| **Force-destructive** | Anything matched by Sentinel rules 2, 3, 10, 13, 15, 16 (these are blocked by the Sentinel and escalate) |
| **External** | Any network call to a non-allowlisted host |
| **Push to main** | Always — Reviewer Queen does this only after explicit user sign-off for the first time per phase. Subsequent merges within the same phase are pre-approved by the same sign-off. |
| **Cost-significant** | A worker that has used > 200k tokens cumulatively across retries on the same task |

The user can adjust this allowlist by editing this document — Sentinel reloads its policy whenever the file changes (it watches `docs/superpowers/plans/2026-05-23-swarm-orchestration.md` for modifications via `hooks_session-start`).

---

## 6. Failure handling and escalation

### 6.1 Worker failure modes

| Mode | Detection | Response |
|---|---|---|
| Compilation error after worker claims complete | Tier 1 cargo build fails | one auto-retry on same worker; if it still fails, quarantine + rotate to fresh worker |
| Test failure | Tier 1 cargo test fails | same |
| Snapshot diff unreviewed | Tier 2 snapshot CI fails | Worker must run `cargo insta review --accept` only for snapshots their task explicitly creates; for other diffs, escalate |
| Clippy warning | Tier 1 clippy fails | Worker fixes inline; no escalation unless warning is in a file outside the task scope (then rule 1 fires) |
| Time budget exceeded | Sentinel rule 8 | kill + retry once on fresh worker; second time → escalate |
| Token budget exceeded | Sentinel rule 9 | same |
| Same task quarantined twice | Architect counter | **escalate to user** — pause dispatch on this task; Architect emits a halt event with quarantine logs of both attempts |
| Reviewer rejects branch | Tier 3 check fails | Architect re-dispatches with a note containing the Reviewer's rejection reason |
| Phase checkpoint fails | Tier 4 check fails | **halt dispatch on this phase**; Architect emits escalation event with full diagnostic |
| Cross-phase invariant fails | Tier 5 check fails | halt **all** dispatch; only previously-merged work is safe |

### 6.2 Escalation event format

When the Architect emits an escalation event, it writes a structured entry to `docs/superpowers/swarm/escalations.md`:

```markdown
## Escalation YYYY-MM-DDTHH:MM:SS — task <task-id>

**Reason:** <Sentinel rule N> | <test failure> | <2x quarantine> | <invariant failure>

**Quarantine attempts:**
1. <branch ref> — <rule violated or test failure>
2. <branch ref> — <rule violated or test failure>

**Diff summary:** <stat-style summary of both quarantine branches>

**Logs:** <path to worker session log under data/swarm-logs/>

**Architect recommendation:** <a) revise task content; b) abandon task; c) split task; d) human intervention>
```

The user reads this file when prompted by the Architect's halt. The Architect resumes dispatch only after the user appends a `**Resolution:** <action>` line and clears the halt with `mcp__ruflo__autopilot_resume`.

### 6.3 Reviewer disagreement

The Reviewer can refuse to merge even a fully-green branch (e.g., scope creep flagged by rule 17 that didn't fire as kill). When this happens:

1. Reviewer writes a structured rejection: `docs/superpowers/swarm/rejections.md` with task id, branch ref, and reason.
2. Architect re-dispatches the task with the rejection note prepended to the worker's prompt context.
3. If the same Reviewer rejection reason appears twice on the same task → escalate as if 2x quarantine.

---

## 7. Memory and shared state

ruflo's `memory_*` and `agentdb_*` tools persist swarm state. Schema:

### `swarm:backlog`
- Key: `task:<task-id>` (e.g., `task:1.7`)
- Value: `{ status: pending|claimed|working|tested|merged|quarantined|escalated, claimant: <worker-id> | null, quarantine_count: 0..N, last_attempt_branch: <ref> | null, last_failure_reason: <string> | null }`

### `swarm:dep-graph`
- Key: `dep:<task-id>`
- Value: `{ blocked_by: [task-id, ...], blocks: [task-id, ...], files_touched: [path, ...] }`

### `swarm:active-workers`
- Key: `worker:<worker-id>`
- Value: `{ task_id, worktree_path, branch, started_at, tokens_used, budget_remaining }`

### `swarm:quarantine-log`
- Append-only entries persisted via `memory_store` with index `swarm/quarantine`.
- Per-entry: `{ task_id, attempt_n, rule_violated, evidence: [<tool-call-summary>, ...], branch_ref, worker_id, ts }`

### `swarm:sentinel-policy-hash`
- SHA-256 of this orchestration document.
- Sentinel re-reads policy when the hash changes, so editing this file is the supported way to adjust rules.

The Reviewer reads from `swarm:backlog` and `swarm:quarantine-log`. The Architect writes to `swarm:backlog`, `swarm:dep-graph`, `swarm:active-workers`. The Sentinel writes to `swarm:quarantine-log` and reads `swarm:sentinel-policy-hash`. Workers read their own row in `swarm:active-workers`; they cannot read other workers' rows.

---

## 8. Bootstrap procedure

The exact ruflo tool sequence to start the swarm from a fresh checkout. Run from project root.

### 8.1 Initialize

1. **Spawn the swarm container:**
   `mcp__ruflo__swarm_init` with topology hint `hive-mind`, max-workers 4.
2. **Initialize hive-mind queens:**
   `mcp__ruflo__hive-mind_init` with roles `["architect", "reviewer"]`.
3. **Spawn Architect:**
   `mcp__ruflo__hive-mind_spawn` role `architect`, system prompt loaded from `docs/superpowers/swarm/prompts/architect.md` (created in §10).
4. **Spawn Reviewer:**
   `mcp__ruflo__hive-mind_spawn` role `reviewer`, prompt from `docs/superpowers/swarm/prompts/reviewer.md`.
5. **Spawn Sentinel:**
   `mcp__ruflo__agent_spawn` role `sentinel`, prompt from `docs/superpowers/swarm/prompts/sentinel.md`. Sentinel registers hooks via `mcp__ruflo__hooks_pre-edit`, `hooks_pre-command`, `hooks_post-edit`, `hooks_post-command` listening on **all** worker sessions.
6. **Load policy:**
   Sentinel computes SHA-256 of this document, writes to `swarm:sentinel-policy-hash`.

### 8.2 Seed the backlog

7. **Architect parses the plan:**
   Reads `docs/superpowers/plans/2026-05-23-otel-correlation-engine.md`, extracts each `### Task N.M:` heading, reads its `Files:` block, builds the dependency graph by file overlap.
8. **Architect populates `swarm:backlog`** with status=`pending` for every task.
9. **Architect emits initial ready set** — only Phase 0 tasks at first (the bootstrap phase is single-threaded since it sets up the CI gates the rest of the swarm depends on).

### 8.3 Run

10. **Enable autopilot:**
    `mcp__ruflo__autopilot_enable` with config `{ max_workers: 1 }` while Phase 0 runs (single worker for bootstrap), then bump to `{ max_workers: 4 }` after Phase 0 merges.
11. **Workers self-claim** via `mcp__ruflo__claims_claim`. The first available worker claims the lowest-numbered ready task.
12. **Architect monitors** via `mcp__ruflo__swarm_status` and `mcp__ruflo__system_metrics` every 60s.
13. **Reviewer polls** for branches with all required CI green: queries via `gh pr checks` and `gh pr view`.

### 8.4 Per-phase ceremonial

After each phase checkpoint merges, the Architect:

14. Runs the Tier 5 cross-phase invariants.
15. Re-tunes `max_workers` for the next phase (default policy in §4.1).
16. Emits a phase-summary entry in `docs/superpowers/swarm/phase-summary.md` with: tasks completed, quarantines, escalations, total wall-clock.

### 8.5 Shutdown

When the final Phase 8 checkpoint merges and `v0.1.0` is tagged:

17. Architect dispatches no further tasks.
18. Reviewer confirms all tasks in `swarm:backlog` show status `merged`.
19. `mcp__ruflo__autopilot_disable`.
20. `mcp__ruflo__hive-mind_shutdown`.
21. `mcp__ruflo__swarm_shutdown`.

---

## 9. Observability

The user can see what the swarm is doing without interrupting it.

### 9.1 Live dashboard

`docs/superpowers/swarm/dashboard.md` — regenerated by the Architect every 60s. Contents:

- Active workers and their current task + elapsed time + tokens used
- Ready queue (next 5 tasks)
- Recent merges to `main` (last 10)
- Quarantine count per phase
- Escalation count (lifetime)
- CI status of every phase branch

### 9.2 Quarantine log

`docs/superpowers/swarm/quarantine.md` — append-only, every quarantine entry. Human-readable: rule violated, evidence snippet, branch ref, worker id, timestamp.

### 9.3 Escalation log

`docs/superpowers/swarm/escalations.md` (§6.2). Halts dispatch when written. Cleared by user adding `**Resolution:**` lines.

### 9.4 Swarm telemetry

Optional: feed the swarm's own activity into the OTel collector once Phase 1 lands. Every queen, worker, and sentinel emits spans for major operations. This makes the swarm dogfood the very stack it's building — a useful sanity check for Phase 1's wiring.

### 9.5 Tail commands the user can run

```bash
# What is happening right now?
cat docs/superpowers/swarm/dashboard.md

# Did anything go wrong since I checked last?
tail docs/superpowers/swarm/quarantine.md
tail docs/superpowers/swarm/escalations.md

# Pull a stuck worker's session log
ls -lt data/swarm-logs/ | head
cat data/swarm-logs/<worker-id>.log
```

`data/swarm-logs/` is gitignored — these are runtime artifacts, not commit material.

---

## 10. Agent prompts (to be written before bootstrap)

Each agent role has a system prompt that primes its behavior. These files live at `docs/superpowers/swarm/prompts/`:

- `architect.md` — "You are the Queen-Architect of a swarm building <plan>. You own the backlog, build the dep graph, dispatch tasks. You never write code. You read CI status and the quarantine log on every cycle. Tools you must use: claims_*, task_*, system_status, memory_*, hooks_intelligence_*. Stop conditions: …"
- `reviewer.md` — "You are the Queen-Reviewer. You have exclusive merge authority on `main`. You run Tier 3 + Tier 5 checks (see §3, §3.5). You never write code. You can veto with reason. Tools: gh pr *, cargo test, memory_retrieve, etc."
- `sentinel.md` — "You are the Sentinel. You watch every worker tool call via hooks. You enforce the 18 rules in §2. You emit kill signals. You do not write code. You do not modify spec/plan. Your policy is the SHA-256 of `docs/superpowers/plans/2026-05-23-swarm-orchestration.md`. Reload when the SHA changes. Tools: hooks_*, agent_terminate, memory_store."
- `worker.md` — "You are a Worker. You implement exactly one task from the plan. You work in your isolated worktree. You read the task content and the spec. You commit at the end of every task. You push to your phase branch only. You release your claim. You exit. Tools: Read, Edit, Write, Bash, claims_*, EnterWorktree."

These prompts are crafted to be deliberately brief — the *constraints* are this orchestration document, which agents reference. Long prompts get out of sync; pointers to a versioned doc don't.

---

## 11. Stop conditions

The swarm halts dispatch automatically when:

- An escalation log entry is appended (until user adds `Resolution:`)
- The Sentinel policy hash mismatches against the orchestration doc and the doc fails to parse
- A Tier 4 phase checkpoint fails
- A Tier 5 cross-phase invariant fails
- `system_health` reports unhealthy (e.g., out-of-disk on the host)
- The user creates `data/HALT` (an empty file — checked on every Architect cycle)

When dispatch is halted:

- Active workers complete their current step, then suspend (do not commit).
- Reviewer freezes merges.
- Sentinel continues to enforce rules (workers cannot escape during halt).

When dispatch resumes:

- Architect verifies all halt conditions cleared.
- `data/HALT` removed if present.
- Suspended workers resume.

---

## 12. Phased rollout

Trust the swarm with one phase first. Earn scope as it proves itself.

### 12.1 Pilot — Phase 0 only

- Single worker (`max_workers = 1`).
- Full sentinel enforcement.
- Reviewer manually inspects every commit before approving merge to `main` for the first time.
- Goal: validate the bootstrap procedure (§8), confirm CI gates work, confirm the Sentinel intercepts rule violations (deliberately trigger a couple of test rule violations during pilot and verify quarantine flow works).

### 12.2 Expansion — Phase 1

- Bump `max_workers = 4` (services are independent).
- Reviewer's per-commit inspection drops to spot-check; trusts CI.
- Architect's dispatch policy starts honoring the dep graph instead of strict order.

### 12.3 Steady state — Phases 2-8

- `max_workers` tuned per phase per §4.1.
- Reviewer's intervention is now exception-based, not per-commit.
- User checks the dashboard once a day or when notified by escalation.

### 12.4 Exit criteria

The pilot ends successfully (and the swarm earns expansion) when:

- Phase 0 merged to `main`
- All required CI checks were green at merge time
- At least one deliberately-triggered rule violation was caught and quarantined correctly
- No false-positive kills occurred during normal operation
- Zero "Resolution:" lines were needed in escalations.md

If any of these fail, expand more cautiously: replay with logs, refine the rule with the user, re-pilot.

---

## 13. Open design points

Three honest unknowns. These resolve during the pilot:

1. **How well does the Sentinel actually intercept Edit/Write calls in worker sessions?** ruflo's hook coverage is the load-bearing assumption. If `hooks_pre-edit` doesn't actually fire reliably across spawned agents, rules 1, 13, 18 silently fail. Pilot Day 1: deliberately spawn a worker, have it try to edit a file outside its task scope, confirm the Sentinel kills it before the write lands. If the write lands first, this whole plan needs a redesign — the Sentinel becomes a post-hoc auditor rather than a preventer, and we tighten Tier 2/3 checks accordingly.

2. **Worktree isolation under ruflo.** The plan assumes each worker gets a truly isolated worktree (via `superpowers:using-git-worktrees`). If two workers somehow end up in the same tree, rule 17 (size drift) will catch it eventually, but we'd prefer not to find out that way. Pilot Day 1: spawn two workers on disjoint tasks, confirm `git rev-parse --show-toplevel` differs across their sessions.

3. **Token budget enforcement granularity.** ruflo's per-agent token accounting may or may not be fine-grained enough for rule 9 to fire pre-violation. If it only reports post-hoc, we accept some over-budget waste in exchange for the behavior never being silent.

These get a fast-feedback resolution in the pilot; the orchestration plan adjusts.

---

## 14. Quick reference

| Question | Answer |
|---|---|
| Who can merge to `main`? | Reviewer Queen only |
| Who can write code? | Workers only |
| Who can disable the Sentinel? | Nobody; it watches from outside |
| What happens when a worker drifts scope? | Kill, quarantine branch, return task to backlog |
| What happens after 2 quarantines on the same task? | Halt dispatch, escalate to user via `escalations.md` |
| Where does the user see what's happening? | `docs/superpowers/swarm/dashboard.md` |
| How does the user halt the swarm? | `touch data/HALT` |
| How does the user adjust policy? | Edit this file; Sentinel re-loads on hash change |
| Who runs full test suite before merge? | Reviewer; CI runs unit/snapshot/property automatically |
| Can workers add new dependencies? | Only if the task explicitly declares them |
| What's the recommended starting concurrency? | 1 worker for Phase 0, 4 for Phase 1, 2 for Phase 2, then per §4.1 |
