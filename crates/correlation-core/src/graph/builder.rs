use super::edges::{Edge, EdgeKind};
use super::nodes::{Node, NodeId};
use indexmap::{IndexMap, IndexSet};

#[derive(Default)]
pub struct EvidenceGraph {
    nodes: IndexMap<NodeId, Node>,
    edges: IndexSet<Edge>,
}

impl EvidenceGraph {
    pub fn new() -> Self { Self::default() }
    pub fn add_node(&mut self, n: Node) -> NodeId {
        let id = n.id();
        self.nodes.entry(id.clone()).or_insert(n);
        id
    }
    pub fn add_edge(&mut self, e: Edge) -> bool { self.edges.insert(e) }
    pub fn add_edge_strict(&mut self, e: Edge) -> Result<bool, String> {
        if !self.nodes.contains_key(&e.from) { return Err(format!("dangling from: {}", e.from)); }
        if !self.nodes.contains_key(&e.to)   { return Err(format!("dangling to: {}",   e.to));   }
        Ok(self.edges.insert(e))
    }
    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }
    pub fn nodes(&self) -> impl Iterator<Item=(&NodeId, &Node)> { self.nodes.iter() }
    pub fn edges(&self) -> impl Iterator<Item=&Edge> { self.edges.iter() }
    pub fn get(&self, id: &NodeId) -> Option<&Node> { self.nodes.get(id) }
    pub fn edges_to<'a>(&'a self, target: &'a NodeId, kind: EdgeKind) -> impl Iterator<Item=&'a Edge> {
        self.edges.iter().filter(move |e| e.to == *target && e.kind == kind)
    }
    pub fn edges_from<'a>(&'a self, src: &'a NodeId, kind: EdgeKind) -> impl Iterator<Item=&'a Edge> {
        self.edges.iter().filter(move |e| e.from == *src && e.kind == kind)
    }
}
