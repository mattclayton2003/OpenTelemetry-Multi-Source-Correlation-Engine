use correlation_core::graph::nodes::Node;
use correlation_core::graph::edges::{Edge, EdgeKind};
use correlation_core::graph::builder::EvidenceGraph;

#[test]
fn graph_dedups_nodes_and_edges() {
    let mut g = EvidenceGraph::new();
    let svc = Node::service("auth".into());
    let id1 = g.add_node(svc.clone());
    let id2 = g.add_node(svc.clone());
    assert_eq!(id1, id2);
    assert_eq!(g.node_count(), 1);
    let e = Edge { from: id1.clone(), to: id1.clone(), kind: EdgeKind::EmittedBy };
    g.add_edge(e.clone());
    g.add_edge(e);
    assert_eq!(g.edge_count(), 1);
}

#[test]
fn graph_rejects_dangling_edge_in_strict_mode() {
    let mut g = EvidenceGraph::new();
    let e = Edge { from: "svc:foo".into(), to: "svc:bar".into(), kind: EdgeKind::EmittedBy };
    let res = g.add_edge_strict(e);
    assert!(res.is_err());
}
