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

    // PHANTOM_2J2_LIVE_DOM_BRIDGE_I
    /// Sets or replaces one normalized element attribute.
    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(name.into(), value.into());
    }

    /// Removes one element attribute and returns its prior value.
    pub fn remove_attribute(&mut self, name: &str) -> Option<String> {
        self.attributes.remove(name)
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

    /// The requested node does not exist in the document.
    #[error("node {0:?} was not found")]
    NodeNotFound(NodeId),

    /// The requested operation requires an element node.
    #[error("node {0:?} is not an element")]
    NodeNotElement(NodeId),

    /// Requested parent/child relationship would violate tree invariants.
    #[error("invalid DOM hierarchy: parent {parent:?}, child {child:?}")]
    HierarchyRequest {
        /// Requested parent.
        parent: NodeId,
        /// Requested child.
        child: NodeId,
    },

    /// The document root cannot be detached or reparented.
    #[error("document root cannot be detached or reparented")]
    CannotDetachRoot,

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

    /// Sets or replaces one attribute on an element node.
    ///
    /// # Errors
    ///
    /// Returns [`DomError::NodeNotFound`] for an unknown node or
    /// [`DomError::NodeNotElement`] when the node is not an element.
    pub fn set_element_attribute(
        &mut self,
        id: NodeId,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), DomError> {
        let Some(node) = self.nodes.get_mut(&id) else {
            return Err(DomError::NodeNotFound(id));
        };
        let NodeKind::Element(element) = &mut node.kind else {
            return Err(DomError::NodeNotElement(id));
        };

        element.set_attribute(name, value);
        Ok(())
    }

    /// Removes one attribute from an element node.
    ///
    /// # Errors
    ///
    /// Returns [`DomError::NodeNotFound`] for an unknown node or
    /// [`DomError::NodeNotElement`] when the node is not an element.
    pub fn remove_element_attribute(
        &mut self,
        id: NodeId,
        name: &str,
    ) -> Result<Option<String>, DomError> {
        let Some(node) = self.nodes.get_mut(&id) else {
            return Err(DomError::NodeNotFound(id));
        };
        let NodeKind::Element(element) = &mut node.kind else {
            return Err(DomError::NodeNotElement(id));
        };

        Ok(element.remove_attribute(name))
    }

    /// Replaces an element's descendants with one text node, or no children
    /// when `value` is empty.
    ///
    /// The element's own [`NodeId`] is preserved, while removed descendants
    /// become invalid. Newly inserted text receives a fresh stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`DomError::NodeNotFound`] for an unknown node,
    /// [`DomError::NodeNotElement`] when the target is not an element, or an
    /// allocation error while inserting the replacement text node.
    pub fn set_text_content(&mut self, id: NodeId, value: &str) -> Result<(), DomError> {
        let Some(node) = self.nodes.get(&id) else {
            return Err(DomError::NodeNotFound(id));
        };
        if !matches!(node.kind(), NodeKind::Element(_)) {
            return Err(DomError::NodeNotElement(id));
        }

        let mut stack = node.children().to_vec();
        let mut descendants = Vec::new();

        while let Some(current) = stack.pop() {
            let Some(current_node) = self.nodes.get(&current) else {
                continue;
            };
            stack.extend(current_node.children().iter().copied());
            descendants.push(current);
        }

        for descendant in descendants {
            self.nodes.remove(&descendant);
        }

        if let Some(parent) = self.nodes.get_mut(&id) {
            parent.children.clear();
        }

        if !value.is_empty() {
            self.append_child(id, NodeKind::Text(value.to_owned()))?;
        }

        Ok(())
    }

    /// Creates one detached element with a fresh stable identifier.
    ///
    /// Detached nodes do not participate in layout or paint until attached.
    ///
    /// # Errors
    ///
    /// Returns the same deterministic allocation failures as [`Self::append_child`].
    pub fn create_detached_element(
        &mut self,
        tag_name: impl Into<String>,
    ) -> Result<NodeId, DomError> {
        if self.nodes.len() >= MAX_DOM_NODES {
            return Err(DomError::NodeLimitExceeded);
        }

        let id = NodeId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DomError::NodeIdExhausted)?;

        self.nodes.insert(
            id,
            Node {
                id,
                parent: None,
                children: Vec::new(),
                kind: NodeKind::Element(ElementData::new(tag_name)),
            },
        );

        Ok(id)
    }

    /// Appends or moves an existing node below `parent`.
    ///
    /// Existing child identity and descendants are preserved. Cycles and root
    /// reparenting are rejected deterministically.
    ///
    /// # Errors
    ///
    /// Returns [`DomError`] for unknown nodes or invalid hierarchy changes.
    pub fn append_existing_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), DomError> {
        if !self.nodes.contains_key(&parent) {
            return Err(DomError::ParentNotFound(parent));
        }
        if !self.nodes.contains_key(&child) {
            return Err(DomError::NodeNotFound(child));
        }
        if child == self.root {
            return Err(DomError::CannotDetachRoot);
        }

        let mut current = Some(parent);
        while let Some(node_id) = current {
            if node_id == child {
                return Err(DomError::HierarchyRequest { parent, child });
            }
            current = self.nodes.get(&node_id).and_then(|node| node.parent);
        }

        let old_parent = self.nodes.get(&child).and_then(|node| node.parent);
        if let Some(old_parent) = old_parent
            && let Some(node) = self.nodes.get_mut(&old_parent)
        {
            node.children.retain(|candidate| *candidate != child);
        }

        if let Some(child_node) = self.nodes.get_mut(&child) {
            child_node.parent = Some(parent);
        }

        if let Some(parent_node) = self.nodes.get_mut(&parent)
            && !parent_node.children.contains(&child)
        {
            parent_node.children.push(child);
        }

        Ok(())
    }

    /// Detaches one node from its parent while preserving the node/subtree.
    ///
    /// # Errors
    ///
    /// Returns [`DomError`] for an unknown node or when asked to detach root.
    pub fn detach_node(&mut self, id: NodeId) -> Result<(), DomError> {
        if id == self.root {
            return Err(DomError::CannotDetachRoot);
        }

        let Some(parent) = self.nodes.get(&id).map(|node| node.parent) else {
            return Err(DomError::NodeNotFound(id));
        };

        if let Some(parent) = parent
            && let Some(parent_node) = self.nodes.get_mut(&parent)
        {
            parent_node.children.retain(|candidate| *candidate != id);
        }

        if let Some(node) = self.nodes.get_mut(&id) {
            node.parent = None;
        }

        Ok(())
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
    fn bounded_dom_mutation_preserves_element_identity() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();
        let element = document.append_child(root, NodeKind::Element(ElementData::new("p")))?;
        document.append_child(element, NodeKind::Text("before".to_owned()))?;

        document.set_element_attribute(element, "id", "hero")?;
        document.set_text_content(element, "after")?;

        let node = document
            .node(element)
            .ok_or(DomError::NodeNotFound(element))?;
        let NodeKind::Element(data) = node.kind() else {
            return Err(DomError::NodeNotElement(element));
        };

        assert_eq!(data.attribute("id"), Some("hero"));
        assert_eq!(node.children().len(), 1);
        Ok(())
    }

    #[test]
    fn removing_element_attribute_is_bounded() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();
        let element = document.append_child(root, NodeKind::Element(ElementData::new("p")))?;
        document.set_element_attribute(element, "data-test", "yes")?;

        assert_eq!(
            document.remove_element_attribute(element, "data-test")?,
            Some("yes".to_owned())
        );
        Ok(())
    }

    #[test]
    fn detached_element_can_be_attached_moved_and_detached() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();
        let first = document.append_child(root, NodeKind::Element(ElementData::new("main")))?;
        let second = document.append_child(root, NodeKind::Element(ElementData::new("aside")))?;
        let child = document.create_detached_element("p")?;

        assert_eq!(document.node(child).map(Node::parent), Some(None));

        document.append_existing_child(first, child)?;
        assert_eq!(document.node(child).map(Node::parent), Some(Some(first)));

        document.append_existing_child(second, child)?;
        assert_eq!(document.node(child).map(Node::parent), Some(Some(second)));

        document.detach_node(child)?;
        assert_eq!(document.node(child).map(Node::parent), Some(None));
        Ok(())
    }

    #[test]
    fn reparenting_rejects_cycles() -> Result<(), DomError> {
        let mut document = Document::new();
        let root = document.root();
        let parent = document.append_child(root, NodeKind::Element(ElementData::new("div")))?;
        let child = document.append_child(parent, NodeKind::Element(ElementData::new("span")))?;

        assert!(matches!(
            document.append_existing_child(child, parent),
            Err(DomError::HierarchyRequest { .. })
        ));
        Ok(())
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
