# Branch protection on `main`

These rules MUST be set on the GitHub repository before the first phase merges.
Apply via the UI (Settings → Branches → Add rule) or via `gh` CLI as shown.

## Required settings

| Setting | Value |
|---|---|
| Require pull request before merging | enabled |
| Require approvals | 0 (solo) — set to 1 once a collaborator joins |
| Require status checks to pass | enabled |
| Required checks | `unit`, `snapshot`, `property` |
| Require branches to be up to date | enabled |
| Require linear history | **disabled** (we use merge-commits) |
| Allow force pushes | disabled |
| Allow deletions | disabled |
| Allowed merge types | merge-commit only (disable squash + rebase) |

## Apply via gh CLI

    OWNER=<your-org-or-user>
    REPO=OpenTelemetry-Multi-Source-Correlation-Engine

    gh api -X PUT "repos/$OWNER/$REPO/branches/main/protection" \
      --input - <<'JSON'
    {
      "required_status_checks": {
        "strict": true,
        "checks": [
          { "context": "unit"     },
          { "context": "snapshot" },
          { "context": "property" }
        ]
      },
      "enforce_admins": false,
      "required_pull_request_reviews": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews": true
      },
      "restrictions": null,
      "allow_force_pushes": false,
      "allow_deletions": false,
      "required_linear_history": false
    }
    JSON

    gh api -X PATCH "repos/$OWNER/$REPO" \
      -f allow_merge_commit=true \
      -f allow_squash_merge=false \
      -f allow_rebase_merge=false

## Verifying

    gh api "repos/$OWNER/$REPO/branches/main/protection" | jq '{
      required_checks: .required_status_checks.checks,
      strict: .required_status_checks.strict,
      force_pushes: .allow_force_pushes.enabled,
      linear_history: .required_linear_history.enabled
    }'

Expected: required_checks = [unit, snapshot, property], strict = true,
force_pushes = false, linear_history = false.
