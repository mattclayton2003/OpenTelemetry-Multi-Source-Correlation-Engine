use correlation_core::graph::nodes::{Node, NodeId};

#[test]
fn node_ids_are_stable_and_unique() {
    let svc = Node::service("auth".into());
    let svc2 = Node::service("auth".into());
    let other = Node::service("accounts".into());
    assert_eq!(svc.id(), svc2.id());
    assert_ne!(svc.id(), other.id());
    let _: NodeId = svc.id();
}
