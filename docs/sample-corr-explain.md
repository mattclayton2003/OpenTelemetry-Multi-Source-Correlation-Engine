<!--
Recorded sample of `corr explain` output, shown by scripts/demo.sh (act 5) when
no ANTHROPIC_API_KEY is set so the demo can run end-to-end without billing.

Provenance: this narration was produced from docs/sample-incident.json — a real
IncidentContext captured from the running stack under the same 800ms SMTP-latency
fault the demo injects (incident 019eb0f1-d7c7-7090-b6f8-b64a73a101a6, model
claude-opus-4-8). Regenerate it live with:

  corr explain --incident docs/sample-incident.json
-->

**Root cause.** The slow service is **notifications**. The engine ranks it #1
(score 0.73) purely on self-time latency evidence: its `POST /notify` span (and
the `handler` span beneath it) each ran for **735 ms**, and that time is spent
*inside* notifications, not waiting on a downstream child it called. No error
spans and no error logs are present — this is a pure latency fault, not a
failure. The single contributor is the slow span `9GA8cA62CIc=`.

**Blast radius.** Affected: **notifications** is the origin of the slowdown.
The caller, **transactions**, looks slow too — its `create` and `POST
/transactions` spans are ~738 ms and its `POST /notify` span is 736 ms — but
that is *induced*: transactions is simply blocked waiting on the notifications
call it made (the 736 ms `/notify` client span wraps the 735 ms notifications
server span). **accounts** is clean: its `POST /accounts/a1/adjust` and
`/a2/adjust` spans completed in 0–1 ms with no errors. So the impact is one slow
worker (notifications) plus one caller stalled behind it (transactions), with
accounts and the rest of the request path unaffected.

**Recommended next step.** Look at what `notifications:POST /notify` does
in-process. It has no child span in this trace, so the 735 ms is either CPU/lock
time in the handler or an **un-instrumented downstream dependency** — most likely
the outbound mail/SMTP send. Check the notifications service's SMTP path and its
connection to the mail server (latency, pool exhaustion, or a slow upstream);
add a client span around the SMTP call so the next incident attributes the wait
directly instead of leaving it as notifications self-time.
