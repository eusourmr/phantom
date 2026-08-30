//! Minimal document object model used by the Phantom rendering engine.
//!
//! The DOM owns its nodes, element attributes, and parent/child
//! relationships. Nodes are referenced through stable identifiers rather
//! than external pointers.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use thiserror::Error;

/// Maximum number of nodes retained by one Phantom document, including root.
pub const MAX_DOM_NODES: usize = 65_536;

/// Stable identifier for a node stored inside a [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// Returns the raw numeric identifier.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Data associated with an HTML element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementData {
    tag_name: String,
    attributes: BTreeMap<String, String>,
}

impl ElementData {
    /// Creates an element without attributes.
    #[must_use]
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self {
            tag_name: tag_name.into(),
            attributes: BTreeMap::new(),
        }
    }

    /// Creates an element with an owned attribute map.
    #[must_use]
    pub fn with_attributes(
        tag_name: impl Into<String>,
        attributes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            tag_name: tag_name.into(),
            attributes,
        }
    }

    /// Returns the element tag name.
    #[must_use]
    pub fn tag_name(&self) -> &str {
        &self.tag_name
    }

    /// Returns all attributes attached to the element.
    #[must_use]
    pub const fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    /// Returns one attribute value by its normalized name.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }
}

/// Data represented by a DOM node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Root document node.
    Document,

    /// HTML element.
    Element(ElementData),

    /// Text content.
    Text(String),

    /// HTML comment content.
    Comment(String),
}

/// One node stored inside a [`Document`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    id: NodeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    kind: NodeKind,
}

impl Node {
    /// Returns this node's stable identifier.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the parent node identifier, if this node has a parent.
    #[must_use]
    pub const fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Returns the identifiers of this node's direct children.
    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Returns the semantic data represented by this node.
    #[must_use]
    pub const fn kind(&self) -> &NodeKind {
        &self.kind
    }
}

/// Errors raised while preserving DOM invariants.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomError {
    /// The requested parent node does not exist in the document.
    #[error("parent node {0:?} was not found")]
    ParentNotFound(NodeId),

    /// The document reached the deterministic per-document node budget.
    #[error("DOM node limit exceeded ({MAX_DOM_NODES})")]
    NodeLimitExceeded,

    /// The numeric node identifier space has been exhausted.
    #[error("DOM node identifier space exhausted")]
    NodeIdExhausted,
}

/// Owned DOM document.
///
/// Nodes are stored in an internal arena and are referenced using [`NodeId`].
#[derive(Debug, Clone)]
pub struct Document {
    root: NodeId,
    next_id: u64,
    nodes: BTreeMap<NodeId, Node>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Creates a new document containing only its root document node.
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

        Self {
            root,
            next_id: 1,
            nodes,
        }
    }

    /// Returns the root node identifier.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Returns a node by identifier.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Iterates over every node in stable identifier order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Creates and attaches a new child below `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`DomError::ParentNotFound`] when `parent` does not exist,
    /// [`DomError::NodeLimitExceeded`] when the deterministic document budget
    /// has been reached, or [`DomError::NodeIdExhausted`] if another numeric
    /// identifier cannot be allocated.
    pub fn append_child(&mut self, parent: NodeId, kind: NodeKind) -> Result<NodeId, DomError> {
        if !self.nodes.contains_key(&parent) {
            return Err(DomError::ParentNotFound(parent));
        }

        if self.nodes.len() >= MAX_DOM_NODES {
            return Err(DomError::NodeLimitExceeded);
        }

        let id = NodeId(self.next_id);

        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DomError::NodeIdExhausted)?;

        let node = Node {
            id,
            parent: Some(parent),
            children: Vec::new(),
            kind,
        };

        self.nodes.insert(id, node);

        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(id);
        }

        Ok(id)
    }

    /// Returns the number of nodes currently stored in the document.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` when the document contains no nodes.
    ///
    /// A normally constructed [`Document`] is never empty because it always
    /// contains its root node.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Document, DomError, ElementData, MAX_DOM_NODES, Node, NodeKind};

    #[test]
    fn new_document_contains_root() {
        let document = Document::new();

        assert_eq!(document.len(), 1);
        assert!(!document.is_empty());
        assert_eq!(document.node(document.root()).map(Node::parent), Some(None));
    }

    #[test]
    fn append_child_preserves_relationship() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();
        let child = document.append_child(root, NodeKind::Element(ElementData::new("html")))?;

        assert_eq!(document.node(child).map(Node::parent), Some(Some(root)));
        assert_eq!(document.node(root).map(Node::children), Some(&[child][..]));

        Ok(())
    }

    #[test]
    fn element_attributes_can_be_read() {
        let mut attributes = std::collections::BTreeMap::new();
        attributes.insert("href".to_owned(), "https://example.com".to_owned());

        let element = ElementData::with_attributes("a", attributes);

        assert_eq!(element.attribute("href"), Some("https://example.com"));
    }

    #[test]
    fn document_rejects_nodes_beyond_security_budget() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();

        for _ in 1..MAX_DOM_NODES {
            document.append_child(root, NodeKind::Text(String::new()))?;
        }

        assert_eq!(
            document.append_child(root, NodeKind::Text(String::new())),
            Err(DomError::NodeLimitExceeded)
        );

        Ok(())
    }
}
