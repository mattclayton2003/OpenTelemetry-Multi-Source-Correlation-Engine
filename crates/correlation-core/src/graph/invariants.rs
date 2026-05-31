use super::builder::EvidenceGraph;
use super::edges::EdgeKind;
use super::nodes::NodeId;
use std::collections::HashSet;

pub fn check_no_dangling(g: &EvidenceGraph) -> Result<(), String> {
    for e in g.edges() {
        if g.get(&e.from).is_none() { return Err(format!("dangling from {}", e.from)); }
        if g.get(&e.to).is_none()   { return Err(format!("dangling to {}",   e.to));   }
    }
    Ok(())
}

pub fn check_no_caused_by_cycles(g: &EvidenceGraph) -> Result<(), String> {
    fn dfs(g: &EvidenceGraph, n: &NodeId, stack: &mut HashSet<NodeId>, visited: &mut HashSet<NodeId>) -> bool {
        if stack.contains(n) { return true; }
        if visited.contains(n) { return false; }
        stack.insert(n.clone()); visited.insert(n.clone());
        for e in g.edges_from(n, EdgeKind::CausedBy) {
            if dfs(g, &e.to, stack, visited) { return true; }
        }
        stack.remove(n);
        false
    }
    let mut visited = HashSet::new();
    for (id, _) in g.nodes() {
        let mut stack = HashSet::new();
        if dfs(g, id, &mut stack, &mut visited) { return Err(format!("cycle through {id}")); }
    }
    Ok(())
}
