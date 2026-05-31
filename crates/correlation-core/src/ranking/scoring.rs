use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::config::CorrelationConfig;
use crate::graph::builder::EvidenceGraph;
use crate::graph::edges::EdgeKind;
use crate::graph::nodes::Node;
use super::propagation::propagate;
use super::ScoredSuspect;

pub fn rank_suspects(
    g: &EvidenceGraph,
    cfg: &CorrelationConfig,
    anomaly_start: Option<DateTime<Utc>>,
) -> Vec<ScoredSuspect> {
    let mut services: HashMap<String, ScoredSuspect> = HashMap::new();
    for (_id, n) in g.nodes() {
        if let Node::Service { name } = n {
            services.insert(name.clone(), ScoredSuspect {
                service: name.clone(), score: 0.0,
                direct_error: 0.0, direct_anomaly: 0.0,
                propagated: 0.0, temporal_mult: 1.0, contributors: vec![],
            });
        }
    }
    for e in g.edges() {
        if e.kind != EdgeKind::EmittedBy { continue; }
        let svc_node = g.get(&e.to);
        let from_node = g.get(&e.from);
        if let (Some(Node::Service { name }), Some(node)) = (svc_node, from_node) {
            let entry = services.get_mut(name).unwrap();
            match node {
                Node::Span { status: crate::backend::SpanStatus::Error, id, duration_ms, .. } => {
                    let w = 1.0 + (*duration_ms as f64) / 1000.0;
                    entry.direct_error += w;
                    entry.contributors.push(("span".into(), format!("span:{id}"), w));
                }
                Node::LogBatch { id, level, count, .. } if level == "ERROR" => {
                    let w = (*count as f64).sqrt();
                    entry.direct_error += w;
                    entry.contributors.push(("log_batch".into(), format!("lb:{id}"), w));
                }
                Node::MetricAnomaly { id, severity, .. } => {
                    entry.direct_anomaly += *severity * 2.0;
                    entry.contributors.push(("metric_anomaly".into(), format!("anom:{id}"), *severity * 2.0));
                }
                _ => {}
            }
        }
    }
    let prop = propagate(g, &services, cfg);
    for (svc, w) in prop {
        if let Some(s) = services.get_mut(&svc) {
            s.propagated += w;
            s.contributors.push(("propagated_from".into(), "graph".into(), w));
        }
    }
    if let Some(t0) = anomaly_start {
        for (_id, n) in g.nodes() {
            if let Node::MetricAnomaly { service, window_start, .. } = n {
                if (window_start.signed_duration_since(t0)).num_seconds().abs() < 30 {
                    if let Some(s) = services.get_mut(service) { s.temporal_mult = 1.10; }
                }
            }
        }
    }
    let mut out: Vec<ScoredSuspect> = services.into_values().map(|mut s| {
        s.score = (s.direct_error + s.direct_anomaly + s.propagated) * s.temporal_mult;
        s
    }).collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.service.cmp(&b.service)));
    out
}
