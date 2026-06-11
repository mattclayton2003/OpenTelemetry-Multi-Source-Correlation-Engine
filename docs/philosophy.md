# OpenTelemetry Multi-Source Correlation Engine — Philosophy & Vision

> **Mechanize the on-call join.** Turn scattered traces, logs, and metrics into one ranked, evidence-backed root cause — and measure how often it's right.

---

## The problem

When a distributed system degrades, the truth is never in one place. It's split across three separate telemetry stores:

- a slow span buried in the **traces**,
- an error burst in the **logs**,
- a latency or throughput spike in the **metrics**.

Today a human on-call engineer holds all three on screen at 3 a.m. and mentally joins them under pressure to guess the culprit. That join is the real work of an incident — and it is slow, stressful, and irreproducible. Two engineers reach two different conclusions; the same engineer reaches a different one a week later. The signal exists. The synthesis is the bottleneck.

---

## The idea

That synthesis can be **mechanized.**

Given a trigger — a slow or failed trace, or a metric anomaly — the engine pulls the relevant evidence from all three sources, assembles it into an evidence graph, propagates blame along causal edges, and emits a single ranked list of suspect services. Each suspect arrives with the exact evidence behind its score.

The output is one structured document: a ranked, evidence-backed root cause. Not a dashboard to interpret, not three tabs to reconcile — one artifact a human, a dashboard, or a language model can read directly.

---

## How it thinks

**An evidence graph, not a heuristic pile.** The engine builds a graph per incident. Nodes are the things that happened — services, spans, batched log evidence, metric anomalies. Edges are the relationships between them — who emitted what, which span is the parent of which, what plausibly caused what. Blame is scored on that structure, so the reasoning is inspectable rather than buried in a formula.

**The key insight: self-time, not wall-clock.** A naive system blames whatever looks slow. But a caller's span is long *only because it is blocked waiting on a slow dependency.* The engine measures each span's **self-time** — its own work, excluding the time it spends waiting on its children — and blames the actual slow worker, not the caller stuck behind it. This is the difference between correctly naming the downstream service that's genuinely slow and wrongly indicting the upstream one that's merely waiting. It is the single idea that most separates a useful answer from a misleading one.

**Multi-source by design.** Traces, logs, and metrics each see only part of the picture. Traces show structure and timing; logs show what failed and why; metrics show when behavior left its baseline. The engine fuses all three. And it works from either of two entry points — a specific trace, or a metric anomaly — because incidents announce themselves both ways.

---

## The end-to-end loop

The project is not just the engine. It's a closed, measurable loop from raw telemetry to a scored root cause:

```
instrumented distributed app
        │  (traces · logs · metrics)
        ▼
   telemetry collection + stores
        │
        ▼
   correlation engine  ──►  ranked, evidence-backed IncidentContext
        │                          │
        │                          ├──►  human / dashboard / LLM  (consume + narrate)
        ▼                          │
   evaluation harness  ◄───────────┘
        │  (score vs. labelled ground truth)
        ▼
   findings drive the next improvement
```

A distributed system produces telemetry; the engine turns it into an evidence-backed answer; a human, dashboard, or model consumes that answer; the harness scores it against known ground truth; the scores point at the next thing to fix. The loop closes.

---

## Why it's research, not a demo

Anyone can build a correlation tool that looks convincing on a single hand-picked incident. The distinguishing stance of this project is rigor: it doesn't just build a correlation engine, it **quantifies how good the engine is.**

- **A labelled chaos dataset.** A library of experiments injects real faults — added latency, outages, resource pressure — into the system, each with labelled ground truth: which service is actually at fault, the expected blast radius, and which services are genuinely clean.
- **A reproducible evaluation harness.** The harness runs the engine against every experiment, in both entry-point modes, and scores it on precision and recall at rank, on completeness of the evidence it gathered, and on a single composite headline score.
- **Reproducible by construction.** Every evaluation run hashes its configuration, so any score can be reproduced from a known engine plus a known scoring setup. Improvements are *measured*, not asserted. A number that goes up has to survive being re-run.

This is what makes it a research artifact rather than a demo: it's held to a standard where claims are backed by reproducible measurement.

---

## The engine decides; the model narrates

Explainability is a first principle, not an afterthought. The output is never a black-box verdict. Every suspect carries an itemized breakdown — which error spans, which anomaly, which slow self-time, and how blame propagated to land it where it did. The document is auditable by a human and machine-readable for a dashboard or a language model.

That last part matters for the division of labor. The engine does the correlation and decides the ranking **deterministically.** A language model can then translate that grounded document into plain-English root cause, blast radius, and remediation. But the model only narrates the evidence the engine already gathered — **it does not invent the conclusion.** The engine is the decider; the model is the narrator. Grounding the narration in the engine's evidence is what keeps the plain-English story honest.

---

## Where it's going

**Honest current state.** Trace-based correlation is strong — handed a slow or failed trace, the engine reliably names the right worker. Metric-anomaly-based correlation now reaches the same recall: triggered by an anomaly alone, the engine lands the faulted service in the top-3 on every scenario in the labelled chaos suite (recall@3 = 1.0). The remaining frontier is precision and richer anomaly signals — making an anomaly trigger as sharp an entry point as a trace.

**The longer arc.** The deliberate baselines in this engine — straightforward anomaly detection, deterministic scoring — are not the destination. They are the *rigorous baseline that future work must beat.* The longer-term research direction is learned approaches: graph neural networks and related methods operating over the same evidence graph the engine already builds.

This is why the eval substrate is the real long-term asset. The labelled chaos dataset and the reproducible harness are exactly what learned methods need: ground truth to train against and an honest yardstick to be measured by. The deterministic engine is both the baseline to beat and the scaffold the next generation of research is built on.

---

## Principles

- **Evidence, not vibes.** Every ranking carries the evidence that produced it.
- **Blame the worker, not the blocked caller.** Self-time over wall-clock.
- **Fuse the sources.** Traces, logs, and metrics each see only part of it.
- **Measure, don't claim.** Improvements are scored against labelled ground truth.
- **Reproducible by construction.** Hash the configuration; any score can be re-run.
- **The model narrates; the engine decides.** Grounded narration, never invented conclusions.
