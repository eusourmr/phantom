//! Minimal, invariant-preserving DOM foundations.
//!
//! This is intentionally not an HTML parser. It provides a safe tree model on
//! which the future parser can build.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// Stable node identifier inside one document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u64);

impl NodeId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Supported node categories in the foundation model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Document,
    Element(String),
    Text(String),
    Comment(String),
}

/// Read-only DOM node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    id: NodeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    kind: NodeKind,
}

impl Node {
    #[must_use]
    pub const fn id(&self) -> NodeId { self.id }

    #[must_use]
    pub const fn parent(&self) -> Option<NodeId> { self.parent }

    #[must_use]
    pub fn children(&self) -> &[NodeId] { &self.children }

    #[must_use]
    pub const fn kind(&self) -> &NodeKind { &self.kind }
}

/// Errors raised while preserving document invariants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomError {
    ParentNotFound(NodeId),
    NodeIdExhausted,
}

impl fmt::Display for DomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParentNotFound(id) => write!(formatter, "parent node {} was not found", id.0),
            Self::NodeIdExhausted => formatter.write_str("DOM node identifier space exhausted"),
        }
    }
}

impl Error for DomError {}

/// One DOM document with a single immutable root identity.
#[derive(Clone, Debug)]
pub struct Document {
    root: NodeId,
    next_id: u64,
    nodes: BTreeMap<NodeId, Node>,
}

impl Default for Document {
    fn default() -> Self { Self::new() }
}

impl Document {
    #[must_use]
    pub fn new() -> Self {
        let root = NodeId(0);
        let root_node = Node {
            id: root,
            parent: None,
            children: Vec::new(),
            kind: NodeKind::Document,
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(root, root_node);

        Self { root, next_id: 1, nodes }
    }

    #[must_use]
    pub const fn root(&self) -> NodeId { self.root }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> { self.nodes.get(&id) }

    /// Appends a newly-created node below an existing parent while preserving
    /// parent/child consistency and preventing cycles by construction.
    pub fn append_child(&mut self, parent: NodeId, kind: NodeKind) -> Result<NodeId, DomError> {
        if !self.nodes.contains_key(&parent) {
            return Err(DomError::ParentNotFound(parent));
        }

        let id = NodeId(self.next_id);
        self.next_id = self.next_id.checked_add(1).ok_or(DomError::NodeIdExhausted)?;
        let node = Node { id, parent: Some(parent), children: Vec::new(), kind };
        self.nodes.insert(id, node);

        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(id);
        }

        Ok(id)
    }

    #[must_use]
    pub fn len(&self) -> usize { self.nodes.len() }

    #[must_use]
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::{Document, NodeKind};

    #[test]
    fn document_starts_with_one_root() {
        let document = Document::new();
        assert_eq!(document.len(), 1);
        assert!(!document.is_empty());
        assert_eq!(document.node(document.root()).map(|node| node.parent()), Some(None));
    }

    #[test]
    fn append_child_keeps_links_consistent() -> Result<(), Box<dyn std::error::Error>> {
        let mut document = Document::new();
        let root = document.root();
        let child = document.append_child(root, NodeKind::Element("html".to_owned()))?;

        assert_eq!(document.node(child).map(|node| node.parent()), Some(Some(root)));
        assert_eq!(document.node(root).map(|node| node.children()), Some(&[child][..]));
        Ok(())
    }
}
