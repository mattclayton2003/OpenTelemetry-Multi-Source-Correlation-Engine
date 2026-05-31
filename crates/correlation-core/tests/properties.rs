use correlation_core::graph::builder::EvidenceGraph;
use correlation_core::graph::nodes::Node;
use correlation_core::graph::edges::{Edge, EdgeKind};
use correlation_core::graph::invariants::{check_no_dangling, check_no_caused_by_cycles};
use proptest::prelude::*;
use rand::SeedableRng;

proptest! {
    #[test]
    fn graph_strict_insertions_preserve_invariants(seed in 0u64..1000) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut g = EvidenceGraph::new();
        let svcs = ["a","b","c","d"];
        for s in &svcs { g.add_node(Node::service((*s).to_string())); }
        for _ in 0..20 {
            let from = svcs[rand::Rng::gen_range(&mut rng, 0..svcs.len())];
            let to   = svcs[rand::Rng::gen_range(&mut rng, 0..svcs.len())];
            let _ = g.add_edge_strict(Edge {
                from: format!("svc:{from}"), to: format!("svc:{to}"),
                kind: EdgeKind::EmittedBy,
            });
        }
        prop_assert!(check_no_dangling(&g).is_ok());
        prop_assert!(check_no_caused_by_cycles(&g).is_ok());
    }
}
