use super::*;
use std::fmt::Write;

pub fn render_md(ic: &IncidentContext) -> String {
    let mut s = String::new();
    writeln!(s, "# Incident {}", ic.incident_id).ok();
    match &ic.trigger {
        Trigger::Trace { trace } => writeln!(s, "**Trigger:** trace `{}`", trace.trace_id).ok(),
        Trigger::Anomaly { anomaly } => writeln!(
            s,
            "**Trigger:** anomaly on `{}:{}`",
            anomaly.service, anomaly.metric
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

    writeln!(s, "## Top suspects").ok();
    for sus in &ic.suspects {
        writeln!(
            s,
            "{}. **{}** — score {:.2}",
            sus.rank, sus.service, sus.score
        )
        .ok();
    }
    if ic.suspects.is_empty() {
        writeln!(s, "(none)").ok();
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
