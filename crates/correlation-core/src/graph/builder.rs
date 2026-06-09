use super::edges::{Edge, EdgeKind};
use super::nodes::{Node, NodeId};
use indexmap::{IndexMap, IndexSet};

#[derive(Default)]
pub struct EvidenceGraph {
    nodes: IndexMap<NodeId, Node>,
    edges: IndexSet<Edge>,
}

impl EvidenceGraph {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_node(&mut self, n: Node) -> NodeId {
        let id = n.id();
        self.nodes.entry(id.clone()).or_insert(n);
        id
    }
    pub fn add_edge(&mut self, e: Edge) -> bool {
        self.edges.insert(e)
    }
    pub fn add_edge_strict(&mut self, e: Edge) -> Result<bool, String> {
        if !self.nodes.contains_key(&e.from) {
            return Err(format!("dangling from: {}", e.from));
        }
        if !self.nodes.contains_key(&e.to) {
            return Err(format!("dangling to: {}", e.to));
        }
        Ok(self.edges.insert(e))
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    pub fn nodes(&self) -> impl Iterator<Item = (&NodeId, &Node)> {
        self.nodes.iter()
    }
    pub fn edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges.iter()
    }
    pub fn get(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }
    pub fn edges_to<'a>(
        &'a self,
        target: &'a NodeId,
        kind: EdgeKind,
    ) -> impl Iterator<Item = &'a Edge> {
        self.edges
            .iter()
            .filter(move |e| e.to == *target && e.kind == kind)
    }
    pub fn edges_from<'a>(
        &'a self,
        src: &'a NodeId,
        kind: EdgeKind,
    ) -> impl Iterator<Item = &'a Edge> {
        self.edges
            .iter()
            .filter(move |e| e.from == *src && e.kind == kind)
    }
}

use crate::backend::{LogRecord, Span, SpanStatus};
use crate::config::CorrelationConfig;
use chrono::{DateTime, Utc};

/// One detected anomaly, as passed into the graph builder:
/// (service, metric, window_start, window_end, severity, detector, baseline_mean, observed_peak).
pub type AnomalyInput = (
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
    f64,
    &'static str,
    f64,
    f64,
);

pub fn build_from(
    spans: &[Span],
    logs: &[LogRecord],
    anomalies: &[AnomalyInput],
    cfg: &CorrelationConfig,
) -> EvidenceGraph {
    let mut g = EvidenceGraph::new();
    for sp in spans {
        let svc_id = g.add_node(Node::service(sp.service.clone()));
        let span_id = g.add_node(Node::Span {
            id: sp.span_id.clone(),
            service: sp.service.clone(),
            operation: sp.operation.clone(),
            status: sp.status.clone(),
            start: sp.start,
            duration_ms: sp.duration_ms,
            parent: sp.parent_id.clone(),
            status_message: sp.status_message.clone(),
        });
        g.add_edge(Edge {
            from: span_id.clone(),
            to: svc_id,
            kind: EdgeKind::EmittedBy,
        });
        if let Some(parent_span_id) = &sp.parent_id {
            let pid = format!("span:{parent_span_id}");
            g.add_edge(Edge {
                from: pid,
                to: span_id.clone(),
                kind: EdgeKind::ParentOf,
            });
        }
    }
    // CausedBy: ERROR span → its parent
    for sp in spans {
        if matches!(sp.status, SpanStatus::Error) {
            if let Some(parent) = &sp.parent_id {
                g.add_edge(Edge {
                    from: format!("span:{}", sp.span_id),
                    to: format!("span:{parent}"),
                    kind: EdgeKind::CausedBy,
                });
            }
        }
    }
    // Log batches: group by (service, bucket, level)
    use std::collections::BTreeMap;
    type Key = (String, String, i64);
    let bucket = cfg.log_bucket_sec;
    let mut groups: BTreeMap<Key, Vec<&LogRecord>> = BTreeMap::new();
    for l in logs {
        let bkt = (l.ts.timestamp() / bucket) * bucket;
        groups
            .entry((l.service.clone(), l.level.clone(), bkt))
            .or_default()
            .push(l);
    }
    for (counter, ((service, level, bkt), items)) in groups.into_iter().enumerate() {
        let id = format!("lb_{counter}");
        let bucket_start = DateTime::<Utc>::from_timestamp(bkt, 0).unwrap();
        let samples: Vec<String> = items.iter().take(3).map(|l| l.message.clone()).collect();
        let lb_id = g.add_node(Node::LogBatch {
            id: id.clone(),
            service: service.clone(),
            level,
            bucket_start,
            count: items.len(),
            samples,
        });
        let svc_id = g.add_node(Node::service(service));
        g.add_edge(Edge {
            from: lb_id,
            to: svc_id,
            kind: EdgeKind::EmittedBy,
        });
    }
    for (acounter, (service, metric, ws, we, severity, detector, baseline_mean, observed_peak)) in
        anomalies.iter().enumerate()
    {
        let id = format!("anom_{acounter}");
        let n = g.add_node(Node::MetricAnomaly {
            id,
            service: service.clone(),
            metric: metric.clone(),
            window_start: *ws,
            window_end: *we,
            severity: *severity,
            detector: detector.to_string(),
            baseline_mean: *baseline_mean,
            observed_peak: *observed_peak,
        });
        let svc_id = g.add_node(Node::service(service.clone()));
        g.add_edge(Edge {
            from: n,
            to: svc_id,
            kind: EdgeKind::EmittedBy,
        });
    }
    g
}
