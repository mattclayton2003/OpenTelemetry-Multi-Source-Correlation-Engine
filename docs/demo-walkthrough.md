# Demo walkthrough — what's happening at each point

A beat-by-beat companion to [`scripts/demo.sh`](../scripts/demo.sh). Each section
covers **what runs**, **what's happening under the hood**, and **what to say /
point at**. The script narrates itself as it goes; this is the deeper "why" for
the presenter.

```sh
docker compose -f compose/docker-compose.yaml --profile research up -d
./scripts/demo.sh                  # interactive — pauses between acts
DEMO_NOPAUSE=1 ./scripts/demo.sh   # straight through (rehearsal)
DEMO_NOOPEN=1  ./scripts/demo.sh   # print URLs but don't open the browser
```

The whole point of the arc: **scattered telemetry → one machine-made,
evidence-backed root cause → a plain-English narrative**, all reproducible.

---

## Preflight (before act 1)

**What runs:** health-checks transactions/notifications/engine; creates two
accounts (`a1`, `a2`); starts a background loop POSTing `/transactions` so the
dashboards and traces are live; sleeps 8s to let telemetry flow.

**Under the hood:** every `/transactions` call fans out to accounts (debit/credit)
and notifications (send receipt) — each hop is an OpenTelemetry span, trace
context propagated W3C-style, exported via OTLP to the collector, which routes
traces→Tempo, logs→Loki, and (via the spanmetrics connector) metrics→Prometheus.

**Say:** "A steady stream of real bank transactions is flowing. Every request is
traced end-to-end across four services. This is a healthy baseline."

---

## Act 1/5 — Healthy system

**What runs:** prints `notifications` and `transactions` p99 (queried live from
Prometheus' spanmetrics); opens the **Zipkin dependency graph**.

**Under the hood:** `p99()` runs a PromQL `histogram_quantile(0.99, …
duration_milliseconds_bucket …)` against the metrics the collector derives from
spans. The dependency graph is built by Zipkin from span `SERVER`/`CLIENT` kinds.

**Say:** "p99 is a few milliseconds everywhere. The dependency graph shows the
real call topology — transactions calls accounts and notifications. Remember this
shape; we're about to break one edge of it." **Point at:** the green, low-latency
baseline.

---

## Act 2/5 — Inject the fault

**What runs:** `POST` to the Toxiproxy API adds an **800 ms latency toxic**
(`smtp-latency`, ±150 ms jitter) on the SMTP path notifications depends on; opens
the Grafana **Services** dashboard; then prints notifications p99 climbing live
for ~32s.

**Under the hood:** Toxiproxy sits between notifications and its mail server. The
toxic delays every SMTP response by 800 ms, so notifications' `/notify` handler
now blocks for ~800 ms per request. Nothing in the *code* changed — this is a
real network-level fault, exactly what a slow downstream dependency looks like.

**Say:** "I've injected an 800ms latency fault on the dependency notifications
talks to — no code change, just a slow network path. Watch the p99 climb in real
time: 5ms… 600… 800… 860." **Point at:** the `notifications /notify` line spiking
on the Grafana p99 panel.

---

## Act 3/5 — Find a slow trace

**What runs:** polls Tempo (`TraceQL: { service.name="notifications" &&
duration>500ms }`) until it finds a slow trace; opens that trace in Zipkin.

**Under the hood:** the fault is now producing slow-but-successful traces (no
errors — just latency). The query targets duration so it catches the impacted
request even though faster traces are more recent.

**Say:** "Here's one slow trace. Open it: the long span is
`notifications:/notify`. Notice `transactions:create` is *also* long — but only
because it's sitting and **waiting** on notifications. To the eye, two services
look slow. Which one is actually at fault?" **Point at:** the nested spans — the
caller's bar fully contains the dependency's bar.

> This is the crux of the whole demo: distinguishing the **slow worker** from the
> **blocked caller**. Hold the question open going into act 4.

---

## Act 4/5 — The engine answers

**What runs:** `POST /correlate/trace` to the HTTP engine with that trace id;
prints the ranked suspects with their evidence breakdown and elapsed time.

**Under the hood:** the engine fetches the trace from Tempo, builds the evidence
graph, and computes each span's **self-time** (`duration − Σ children`).
notifications' `/notify` span has ~735 ms of *self-time* — time spent in itself,
not waiting on a child. transactions' span is long but its self-time is near
zero (its duration is almost entirely the child call to notifications). So the
latency evidence lands on **notifications**, and transactions/accounts score ~0.

**Say:** "No human looked at this. The engine ranks **notifications #1 on
self-time latency evidence** — it correctly blames the slow worker, not the
caller blocked waiting on it. That's the question from act 3, answered
mechanically in about half a second." **Point at:** `#1 notifications … latency`
vs the zero scores below it.

---

## Act 5/5 — Plain-English root cause (the LLM)

**What runs:** captures the act-4 incident document and passes the *same* grounded
JSON to `corr explain`, which sends it to the Claude API for a root-cause /
blast-radius / remediation narrative.

- **With `ANTHROPIC_API_KEY` set + the `corr` CLI built:** live narration of this
  run's incident.
- **Without a key:** shows a recorded sample
  ([`docs/sample-corr-explain.md`](sample-corr-explain.md), grounded in
  [`docs/sample-incident.json`](sample-incident.json)) so the demo runs with zero
  spend.

**Under the hood:** `corr explain` is a thin Claude API call (`POST /v1/messages`,
model `claude-opus-4-8`, adaptive thinking). The system prompt instructs the
model to cite only the evidence already in the document — the engine did the
correlation; the LLM only translates it into prose. No new judgement, no invented
services.

**Say:** "The engine's output is already an LLM-ready grounded document. Hand it
to a model and you get the on-call-ready writeup: root cause (notifications,
self-time latency), blast radius (transactions stalled behind it, accounts
clean), and a next step (instrument the SMTP call). From raw telemetry to a
sentence a human can act on."

---

## Coda — Reproducible evaluation

**What runs:** `eval report --tag suite-baseline` inside the eval-harness
container; opens the Grafana **Eval** dashboard and the HTML scorecard.

**Under the hood:** this isn't the live incident — it's the standing benchmark.
The eval harness runs the engine against every labelled chaos experiment in both
trace and anomaly modes and scores it (recall@k, precision@k, completeness,
composite) against ground truth. The config is hashed into every run so results
are reproducible.

**Say:** "This is the research artifact. The same engine, scored against a
labelled chaos dataset. Trace mode is strong; anomaly mode is where the current
work is. Every number here is reproducible from a config hash." **Point at:**
trace-vs-anomaly composite, and any misses.

---

## Cleanup (always)

An `EXIT`/`INT`/`TERM` trap removes the latency toxic, stops the background load
generator, and deletes the temp incident file — even on Ctrl-C. The system is
left exactly as it was found.

---

## Common questions

- **"Why does the caller look slow if it isn't at fault?"** Its span *duration*
  includes the time blocked on the dependency. The engine looks at **self-time**,
  not duration, which is why it doesn't get fooled. (Act 3 → Act 4.)
- **"Is the fault real or simulated?"** Real — a network-level latency injection
  via Toxiproxy, not a code branch. The services are unmodified.
- **"Does the LLM decide the root cause?"** No. The engine decides; the LLM only
  narrates the engine's evidence. Run with `--dry-run` to see the exact grounded
  prompt.
- **"How do I make act 5 narrate live?"** `export ANTHROPIC_API_KEY=…` and build
  the CLI (`cargo build -p correlation-cli`). Otherwise it shows the recorded
  sample.
