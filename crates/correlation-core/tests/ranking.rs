use chrono::Utc;
use correlation_core::config::CorrelationConfig;
use correlation_core::graph::builder::EvidenceGraph;
use correlation_core::graph::edges::{Edge, EdgeKind};
use correlation_core::graph::nodes::Node;
use correlation_core::ranking::scoring::rank_suspects;

#[test]
fn service_with_error_span_outranks_clean_service() {
    let mut g = EvidenceGraph::new();
    let bad = g.add_node(Node::service("bad".into()));
    let _good = g.add_node(Node::service("good".into()));
    let sp = g.add_node(Node::Span {
        id: "s1".into(),
        service: "bad".into(),
        operation: "x".into(),
        status: correlation_core::backend::SpanStatus::Error,
        start: Utc::now(),
        duration_ms: 10,
        parent: None,
        status_message: None,
    });
    g.add_edge(Edge {
        from: sp,
        to: bad.clone(),
        kind: EdgeKind::EmittedBy,
    });
    let cfg = CorrelationConfig::default();
    let suspects = rank_suspects(&g, &cfg, None);
    assert_eq!(suspects[0].service, "bad");
    assert!(suspects[0].score > 0.0);
}

#[test]
fn monotonic_more_evidence_never_lowers_score() {
    use correlation_core::backend::SpanStatus;
    let cfg = CorrelationConfig::default();
    let mut g = EvidenceGraph::new();
    let svc = g.add_node(Node::service("s".into()));
    let s1 = g.add_node(Node::Span {
        id: "a".into(),
        service: "s".into(),
        operation: "x".into(),
        status: SpanStatus::Error,
        start: Utc::now(),
        duration_ms: 10,
        parent: None,
        status_message: None,
    });
    g.add_edge(Edge {
        from: s1,
        to: svc.clone(),
        kind: EdgeKind::EmittedBy,
    });
    let before = rank_suspects(&g, &cfg, None)[0].score;
    let s2 = g.add_node(Node::Span {
        id: "b".into(),
        service: "s".into(),
        operation: "y".into(),
        status: SpanStatus::Error,
        start: Utc::now(),
        duration_ms: 10,
        parent: None,
        status_message: None,
    });
    g.add_edge(Edge {
        from: s2,
        to: svc.clone(),
        kind: EdgeKind::EmittedBy,
    });
    let after = rank_suspects(&g, &cfg, None)[0].score;
    assert!(after >= before);
}
