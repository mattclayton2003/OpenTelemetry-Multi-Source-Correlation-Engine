use super::*;
use std::fmt::Write;

pub fn render_md(ic: &IncidentContext) -> String {
    let mut s = String::new();
    writeln!(s, "# Incident {}", ic.incident_id).ok();
    match &ic.trigger {
        Trigger::Trace { trace } => writeln!(s, "**Trigger:** trace `{}`", trace.trace_id).ok(),
        Trigger::Anomaly { anomaly } => writeln!(
            s,
            "**Trigger:** anomaly on `{}:{}` (observed {:.2}, baseline {:.2}±{:.2}, z={:.2}, {})",
            anomaly.service,
            anomaly.metric,
            anomaly.observed_value,
            anomaly.baseline_mean,
            anomaly.baseline_stddev,
            anomaly.z_score,
            anomaly.detector
        )
        .ok(),
    };
    writeln!(
        s,
        "**Window:** {} → {} ({})",
        ic.window.start,
        ic.window.end,
        if ic.window.expanded {
            "expanded"
        } else {
            "raw"
        }
    )
    .ok();
    // Take the first 16 *characters* (not bytes) so a multi-byte UTF-8 hash
    // can't panic on a char-boundary slice.
    let config_short: String = ic.config_hash.chars().take(16).collect();
    writeln!(
        s,
        "**Engine:** {}  ·  config {}  ·  elapsed {}ms\n",
        ic.engine_version, config_short, ic.elapsed_ms
    )
    .ok();

    // --- Top suspects, with the evidence the ranker actually used ----------
    // This is the heart of the document: an LLM (or a human) should be able to
    // read *why* each service was ranked where it was, not just its score.
    writeln!(s, "## Top suspects").ok();
    for sus in &ic.suspects {
        writeln!(
            s,
            "{}. **{}** — score {:.2}",
            sus.rank, sus.service, sus.score
        )
        .ok();
        let eb = &sus.evidence_breakdown;
        let mut parts: Vec<String> = Vec::new();
        if eb.direct_latency_weight != 0.0 {
            parts.push(format!("latency {:.2}", eb.direct_latency_weight));
        }
        if eb.direct_error_weight != 0.0 {
            parts.push(format!("error {:.2}", eb.direct_error_weight));
        }
        if eb.direct_anomaly_weight != 0.0 {
            parts.push(format!("anomaly {:.2}", eb.direct_anomaly_weight));
        }
        if eb.propagated_weight != 0.0 {
            parts.push(format!("propagated {:.2}", eb.propagated_weight));
        }
        if !parts.is_empty() {
            writeln!(s, "   - evidence: {}", parts.join(", ")).ok();
        }
        if eb.temporal_tightness_multiplier != 1.0 {
            writeln!(
                s,
                "   - temporal tightness ×{:.2}",
                eb.temporal_tightness_multiplier
            )
            .ok();
        }
        for c in &eb.contributors {
            writeln!(s, "   - {} `{}` (+{:.2})", c.kind, c.r#ref, c.weight).ok();
        }
    }
    if ic.suspects.is_empty() {
        writeln!(s, "(none)").ok();
    }

    // --- Affected services -------------------------------------------------
    if !ic.services.is_empty() {
        writeln!(s, "\n## Affected services").ok();
        writeln!(s, "| service | spans | error spans | logs | error logs |").ok();
        writeln!(s, "|---|---|---|---|---|").ok();
        for svc in &ic.services {
            writeln!(
                s,
                "| {} | {} | {} | {} | {} |",
                svc.name, svc.span_count, svc.error_span_count, svc.log_count, svc.error_log_count
            )
            .ok();
        }
    }

    // --- Notable spans -----------------------------------------------------
    // Surface the spans that carry the signal: error spans first, then the
    // slowest, capped so the document stays compact for an LLM context window.
    if !ic.spans.is_empty() {
        let mut ranked: Vec<&SpanRef> = ic.spans.iter().collect();
        ranked.sort_by(|a, b| {
            let a_err = a.status.eq_ignore_ascii_case("error");
            let b_err = b.status.eq_ignore_ascii_case("error");
            b_err.cmp(&a_err).then(b.duration_ms.cmp(&a.duration_ms))
        });
        writeln!(s, "\n## Notable spans").ok();
        for sp in ranked.iter().take(12) {
            let msg = sp
                .status_message
                .as_deref()
                .filter(|m| !m.is_empty())
                .map(|m| format!(" — {m}"))
                .unwrap_or_default();
            writeln!(
                s,
                "- `{}:{}` — {}ms — {}{}",
                sp.service, sp.operation, sp.duration_ms, sp.status, msg
            )
            .ok();
        }
        if ic.spans.len() > 12 {
            writeln!(s, "- … {} more spans", ic.spans.len() - 12).ok();
        }
    }

    // --- Metric anomalies --------------------------------------------------
    if !ic.metric_anomalies.is_empty() {
        writeln!(s, "\n## Metric anomalies").ok();
        for m in &ic.metric_anomalies {
            writeln!(
                s,
                "- `{}:{}` — peak {:.2} vs baseline {:.2} (severity {:.2}, {})",
                m.service, m.metric, m.observed_peak, m.baseline_mean, m.severity, m.detector
            )
            .ok();
        }
    }

    // --- Log batches -------------------------------------------------------
    if !ic.log_batches.is_empty() {
        writeln!(s, "\n## Log batches").ok();
        for lb in &ic.log_batches {
            let sample = lb
                .sample_messages
                .first()
                .map(|m| format!(" — e.g. {m:?}"))
                .unwrap_or_default();
            writeln!(s, "- `{}` {} ×{}{}", lb.service, lb.level, lb.count, sample).ok();
        }
    }

    // --- Timeline ----------------------------------------------------------
    if !ic.timeline.is_empty() {
        writeln!(s, "\n## Timeline").ok();
        for ev in &ic.timeline {
            writeln!(s, "- {} {} `{}`", ev.ts, ev.kind, ev.r#ref).ok();
        }
    }

    writeln!(s, "\n## Notes").ok();
    if ic.notes.is_empty() {
        writeln!(s, "(none)").ok();
    }
    for n in &ic.notes {
        writeln!(s, "- {n}").ok();
    }
    s
}
