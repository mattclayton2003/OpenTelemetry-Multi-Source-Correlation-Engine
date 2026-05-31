use std::collections::HashMap;
use crate::config::CorrelationConfig;
use crate::graph::builder::EvidenceGraph;
use crate::graph::edges::EdgeKind;
use crate::graph::nodes::Node;
use super::ScoredSuspect;

pub fn propagate(
    g: &EvidenceGraph,
    direct: &HashMap<String, ScoredSuspect>,
    cfg: &CorrelationConfig,
) -> HashMap<String, f64> {
    let beta = cfg.causal_propagation_beta;
    let max_depth = cfg.causal_propagation_max_depth;
    let mut acc: HashMap<String, f64> = HashMap::new();
    for (id, n) in g.nodes() {
        if let Node::Span { service, .. } = n {
            let direct_w = direct.get(service).map(|s| s.direct_error + s.direct_anomaly).unwrap_or(0.0);
            if direct_w == 0.0 { continue; }
            let mut current = id.clone();
            let mut depth = 0u8;
            let mut factor = beta;
            while depth < max_depth {
                let next = g.edges_from(&current, EdgeKind::CausedBy).next().map(|e| e.to.clone());
                if let Some(target_span) = next {
                    if let Some(Node::Span { service: tgt_service, .. }) = g.get(&target_span) {
                        *acc.entry(tgt_service.clone()).or_default() += direct_w * factor;
                    }
                    current = target_span;
                    depth += 1;
                    factor *= beta;
                } else { break; }
            }
        }
    }
    acc
}
