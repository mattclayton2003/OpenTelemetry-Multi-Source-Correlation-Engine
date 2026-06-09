use chrono::Utc;
use correlation_core::backend::SpanStatus;
use correlation_core::config::CorrelationConfig;
use correlation_core::graph::builder::EvidenceGraph;
use correlation_core::graph::edges::{Edge, EdgeKind};
use correlation_core::graph::nodes::Node;
use correlation_core::ranking::scoring::rank_suspects;

/// A latency fault produces slow but successful (status=OK) spans. The service
/// doing the slow work (high self-time) must be blamed; a caller merely blocked
/// waiting on it (self-time ~0) must not be.
#[test]
fn latency_evidence_blames_the_slow_worker_not_the_blocked_caller() {
    let mut g = EvidenceGraph::new();
    let svc_slow = g.add_node(Node::service("slow".into()));
    let svc_caller = g.add_node(Node::service("caller".into()));
    // caller span: 800ms total, but almost all of it is spent in its child.
    let caller_span = g.add_node(Node::Span {
        id: "c1".into(),
        service: "caller".into(),
        operation: "call".into(),
        status: SpanStatus::Ok,
        start: Utc::now(),
        duration_ms: 800,
        parent: None,
        status_message: None,
    });
    // slow span: 790ms of self-time (no children) — the actual culprit.
    let slow_span = g.add_node(Node::Span {
        id: "s1".into(),
        service: "slow".into(),
        operation: "work".into(),
        status: SpanStatus::Ok,
        start: Utc::now(),
        duration_ms: 790,
        parent: Some("c1".into()),
        status_message: None,
    });
    g.add_edge(Edge {
        from: caller_span,
        to: svc_caller,
        kind: EdgeKind::EmittedBy,
    });
    g.add_edge(Edge {
        from: slow_span,
        to: svc_slow,
        kind: EdgeKind::EmittedBy,
    });
    // caller -> slow (the caller is the parent of the slow span).
    g.add_edge(Edge {
        from: "span:c1".into(),
        to: "span:s1".into(),
        kind: EdgeKind::ParentOf,
    });

    let suspects = rank_suspects(&g, &CorrelationConfig::default(), None);
    assert_eq!(suspects[0].service, "slow", "slow worker should rank first");
    assert!(suspects[0].direct_latency > 0.0);
    let caller = suspects.iter().find(|s| s.service == "caller").unwrap();
    assert_eq!(
        caller.direct_latency, 0.0,
        "caller's self-time is below threshold; it should not be blamed"
    );
}

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
