use super::*;
use std::fmt::Write;

pub fn render_md(ic: &IncidentContext) -> String {
    let mut s = String::new();
    writeln!(s, "# Incident {}", ic.incident_id).ok();
    match &ic.trigger {
        Trigger::Trace { trace } => writeln!(s, "**Trigger:** trace `{}`", trace.trace_id).ok(),
        Trigger::Anomaly { anomaly } => writeln!(s, "**Trigger:** anomaly on `{}:{}`", anomaly.service, anomaly.metric).ok(),
    };
    writeln!(s, "**Window:** {} → {} ({})",
             ic.window.start, ic.window.end, if ic.window.expanded { "expanded" } else { "raw" }).ok();
    writeln!(s, "**Engine:** {}  ·  config {}  ·  elapsed {}ms\n",
             ic.engine_version, &ic.config_hash[..ic.config_hash.len().min(16)], ic.elapsed_ms).ok();

    writeln!(s, "## Top suspects").ok();
    for sus in &ic.suspects {
        writeln!(s, "{}. **{}** — score {:.2}", sus.rank, sus.service, sus.score).ok();
    }
    if ic.suspects.is_empty() { writeln!(s, "(none)").ok(); }

    writeln!(s, "\n## Notes").ok();
    if ic.notes.is_empty() { writeln!(s, "(none)").ok(); }
    for n in &ic.notes { writeln!(s, "- {n}").ok(); }
    s
}
