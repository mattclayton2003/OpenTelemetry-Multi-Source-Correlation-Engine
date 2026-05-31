use serde::{Deserialize, Serialize};
use super::nodes::NodeId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeKind { ParentOf, EmittedBy, CoOccurs, CausedBy }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Edge { pub from: NodeId, pub to: NodeId, pub kind: EdgeKind }
