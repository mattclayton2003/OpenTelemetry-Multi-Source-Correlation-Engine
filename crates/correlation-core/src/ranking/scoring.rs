use super::propagation::propagate;
use super::ScoredSuspect;
use crate::config::CorrelationConfig;
use crate::graph::builder::EvidenceGraph;
use crate::graph::edges::EdgeKind;
use crate::graph::nodes::Node;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub fn rank_suspects(
    g: &EvidenceGraph,
    cfg: &CorrelationConfig,
    anomaly_start: Option<DateTime<Utc>>,
) -> Vec<ScoredSuspect> {
    let mut services: HashMap<String, ScoredSuspect> = HashMap::new();
    for (_id, n) in g.nodes() {
        if let Node::Service { name } = n {
            services.insert(
                name.clone(),
                ScoredSuspect {
                    service: name.clone(),
                    score: 0.0,
                    direct_error: 0.0,
                    direct_anomaly: 0.0,
                    propagated: 0.0,
                    direct_latency: 0.0,
                    temporal_mult: 1.0,
                    contributors: vec![],
                },
            );
        }
    }
    for e in g.edges() {
        if e.kind != EdgeKind::EmittedBy {
            continue;
        }
        let svc_node = g.get(&e.to);
        let from_node = g.get(&e.from);
        if let (Some(Node::Service { name }), Some(node)) = (svc_node, from_node) {
            let entry = services.get_mut(name).unwrap();
            match node {
                Node::Span {
                    status: crate::backend::SpanStatus::Error,
                    id,
                    duration_ms,
                    ..
                } => {
                    let w = 1.0 + (*duration_ms as f64) / 1000.0;
                    entry.direct_error += w;
                    entry
                        .contributors
                        .push(("span".into(), format!("span:{id}"), w));
                }
                Node::LogBatch {
                    id, level, count, ..
                } if level == "ERROR" => {
                    let w = (*count as f64).sqrt();
                    entry.direct_error += w;
                    entry
                        .contributors
                        .push(("log_batch".into(), format!("lb:{id}"), w));
                }
                Node::MetricAnomaly { id, severity, .. } => {
                    entry.direct_anomaly += *severity * 2.0;
                    entry.contributors.push((
                        "metric_anomaly".into(),
                        format!("anom:{id}"),
                        *severity * 2.0,
                    ));
                }
                _ => {}
            }
        }
    }
    let prop = propagate(g, &services, cfg);
    for (svc, w) in prop {
        if let Some(s) = services.get_mut(&svc) {
            s.propagated += w;
            s.contributors
                .push(("propagated_from".into(), "graph".into(), w));
        }
    }
    // Latency evidence: a non-error span whose *self-time* (its duration minus
    // the time spent in its children) exceeds the threshold is evidence that
    // its service is the one actually doing slow work — not a caller merely
    // blocked waiting on a slow downstream (whose self-time is ~0). This lets
    // the engine identify the culprit of a pure-latency fault, where every span
    // is status=OK and so carries no error evidence.
    {
        use crate::backend::SpanStatus;
        let mut span_dur: HashMap<&str, i64> = HashMap::new();
        let mut spans: Vec<(&str, &str, i64, bool)> = vec![]; // (id, service, dur, is_error)
        for (id, n) in g.nodes() {
            if let Node::Span {
                service,
                duration_ms,
                status,
                ..
            } = n
            {
                span_dur.insert(id.as_str(), *duration_ms);
                spans.push((
                    id.as_str(),
                    service.as_str(),
                    *duration_ms,
                    matches!(status, SpanStatus::Error),
                ));
            }
        }
        let mut child_dur: HashMap<&str, i64> = HashMap::new();
        for e in g.edges() {
            if e.kind == EdgeKind::ParentOf {
                if let Some(d) = span_dur.get(e.to.as_str()) {
                    *child_dur.entry(e.from.as_str()).or_default() += *d;
                }
            }
        }
        for (id, service, dur, is_error) in spans {
            if is_error {
                continue; // already captured as direct_error
            }
            let self_ms = dur - child_dur.get(id).copied().unwrap_or(0);
            if self_ms > cfg.slow_span_self_ms {
                let w = self_ms as f64 / 1000.0;
                if let Some(s) = services.get_mut(service) {
                    s.direct_latency += w;
                    s.contributors.push(("slow_span".into(), id.to_string(), w));
                }
            }
        }
    }
    if let Some(t0) = anomaly_start {
        for (_id, n) in g.nodes() {
            if let Node::MetricAnomaly {
                service,
                window_start,
                ..
            } = n
            {
                if (window_start.signed_duration_since(t0)).num_seconds().abs() < 30 {
                    if let Some(s) = services.get_mut(service) {
                        s.temporal_mult = 1.10;
                    }
                }
            }
        }
    }
    let mut out: Vec<ScoredSuspect> = services
        .into_values()
        .map(|mut s| {
            s.score = (s.direct_error + s.direct_anomaly + s.propagated + s.direct_latency)
                * s.temporal_mult;
            s
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.service.cmp(&b.service))
    });
    out
}
