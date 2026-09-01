//! Standards-shaped HTML tree-builder foundation for Phantom.
//!
//! `2E-2` consumes the deterministic token stream established in `2E-1` and
//! turns it into Phantom's owned DOM representation. The builder intentionally
//! implements a bounded first subset of HTML insertion modes and recovery.
//!
//! The rendering-facing [`crate::parse`] compatibility path remains unchanged
//! in 2E-2 because raw-text/RCDATA and character-reference semantics are
//! completed in 2E-3 before the final parser cutover.

use std::collections::BTreeMap;

use phantom_dom::{Document, DomError, ElementData, NodeId, NodeKind};
use thiserror::Error;

use crate::{
    MAX_HTML_NESTING_DEPTH, MAX_RETAINED_TEXT_BYTES, MAX_TEXT_NODE_BYTES,
    tokenizer::{SourceSpan, Token, Tokenization, TokenizerError, tokenize},
};

/// Maximum number of retained structural recovery diagnostics.
///
/// Recovery still proceeds after this many entries; the report records that the
/// diagnostic list was truncated so malformed input cannot grow telemetry
/// without a deterministic bound.
pub const MAX_TREE_DIAGNOSTICS: usize = 4_096;

/// Structural recovery category produced by the 2E-2 tree builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeRecoveryCode {
    /// The document omitted an explicit `<html>` element.
    ImplicitHtmlElement,
    /// The document omitted an explicit `<head>` element.
    ImplicitHeadElement,
    /// The document omitted an explicit `<body>` element.
    ImplicitBodyElement,
    /// A new block-like element implicitly closed an open paragraph.
    ImplicitParagraphClose,
    /// A start tag was ignored because the structural element already existed.
    IgnoredStartTag,
    /// An end tag could not match any currently open element.
    IgnoredEndTag,
    /// Closing an element also closed unmatched descendants above it.
    MisnestedEndTag,
    /// A self-closing flag on a non-void HTML element was ignored.
    IgnoredSelfClosingFlag,
    /// A DOCTYPE appeared after the initial insertion mode.
    UnexpectedDoctype,
}

/// One bounded structural-recovery observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeRecovery {
    /// Recovery category.
    pub code: TreeRecoveryCode,
    /// Source range associated with the recovery.
    pub span: SourceSpan,
}

/// Deterministic structural intelligence produced while building a DOM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeBuildReport {
    /// Number of non-EOF tokens consumed by the tree builder.
    pub tokens_processed: usize,
    /// Number of DOM nodes in the completed document, including its root.
    pub nodes_created: usize,
    /// Maximum number of simultaneously open HTML elements.
    pub max_open_elements: usize,
    /// Recoverable tokenizer parse errors inherited from 2E-1.
    pub tokenizer_parse_errors: usize,
    /// Character-reference candidates deferred to 2E-3.
    pub character_reference_candidates: usize,
    /// Total structural recoveries, including entries not retained individually.
    pub recoveries_total: usize,
    /// Bounded ordered structural-recovery diagnostics.
    pub recoveries: Vec<TreeRecovery>,
    /// Whether the recovery diagnostic vector reached its deterministic cap.
    pub recoveries_truncated: bool,
    /// Whether an explicit DOCTYPE token was seen.
    pub doctype_seen: bool,
    /// Whether the observed DOCTYPE requests quirks treatment.
    pub force_quirks: bool,
    /// Whether `<html>` was synthesized.
    pub implicit_html: bool,
    /// Whether `<head>` was synthesized.
    pub implicit_head: bool,
    /// Whether `<body>` was synthesized.
    pub implicit_body: bool,
}

impl TreeBuildReport {
    fn new(tokenization: &Tokenization) -> Self {
        Self {
            tokens_processed: 0,
            nodes_created: 1,
            max_open_elements: 0,
            tokenizer_parse_errors: tokenization.parse_errors.len(),
            character_reference_candidates: tokenization.character_references.len(),
            recoveries_total: 0,
            recoveries: Vec::new(),
            recoveries_truncated: false,
            doctype_seen: false,
            force_quirks: false,
            implicit_html: false,
            implicit_head: false,
            implicit_body: false,
        }
    }
}

/// Successful 2E-2 token-to-DOM construction.
#[derive(Debug)]
pub struct TreeBuildOutput {
    document: Document,
    report: TreeBuildReport,
}

impl TreeBuildOutput {
    /// Returns the built Phantom DOM.
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    /// Returns deterministic structural intelligence for the build.
    #[must_use]
    pub const fn report(&self) -> &TreeBuildReport {
        &self.report
    }

    /// Consumes the output and returns its document and report.
    #[must_use]
    pub fn into_parts(self) -> (Document, TreeBuildReport) {
        (self.document, self.report)
    }
}

/// Fatal 2E-2 tree-builder failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TreeBuilderError {
    /// The DOM rejected a requested mutation.
    #[error("DOM error: {0}")]
    Dom(#[from] DomError),

    /// Open-element nesting exceeded the inherited 2D-6 depth budget.
    #[error("HTML nesting exceeds {MAX_HTML_NESTING_DEPTH} elements")]
    NestingTooDeep,

    /// One retained character-data token exceeded the inherited text budget.
    #[error("HTML text node exceeds {MAX_TEXT_NODE_BYTES} bytes")]
    TextNodeTooLarge,

    /// Aggregate retained character data exceeded the inherited document budget.
    #[error("HTML retained text exceeds {MAX_RETAINED_TEXT_BYTES} bytes")]
    RetainedTextTooLarge,
}

/// Fatal error across tokenization and tree building.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TreePipelineError {
    /// 2E-1 tokenization failed its deterministic admission contract.
    #[error("HTML tokenization failed: {0}")]
    Tokenizer(#[from] TokenizerError),

    /// 2E-2 DOM construction failed its deterministic admission contract.
    #[error("HTML tree building failed: {0}")]
    TreeBuilder(#[from] TreeBuilderError),
}

/// Builds a Phantom DOM from one already-tokenized document.
///
/// # Errors
///
/// Returns [`TreeBuilderError`] when DOM, nesting, or retained-text limits are
/// exceeded.
pub fn build(tokenization: &Tokenization) -> Result<TreeBuildOutput, TreeBuilderError> {
    TreeBuilder::new(tokenization).run(&tokenization.tokens)
}

/// Runs the connected 2E-1 -> 2E-2 structural pipeline.
///
/// This API is intentionally available before [`crate::parse`] is migrated so
/// the new parser architecture can be validated independently against the
/// established rendering path.
///
/// # Errors
///
/// Returns [`TreePipelineError`] if tokenization or tree construction exceeds a
/// deterministic security budget.
pub fn tokenize_and_build(source: &str) -> Result<TreeBuildOutput, TreePipelineError> {
    let tokenization = tokenize(source)?;
    Ok(build(&tokenization)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    AfterBody,
}

struct TreeBuilder {
    document: Document,
    open_elements: Vec<NodeId>,
    html_element: Option<NodeId>,
    head_element: Option<NodeId>,
    body_element: Option<NodeId>,
    mode: InsertionMode,
    retained_text_bytes: usize,
    report: TreeBuildReport,
}

impl TreeBuilder {
    fn new(tokenization: &Tokenization) -> Self {
        Self {
            document: Document::new(),
            open_elements: Vec::new(),
            html_element: None,
            head_element: None,
            body_element: None,
            mode: InsertionMode::Initial,
            retained_text_bytes: 0,
            report: TreeBuildReport::new(tokenization),
        }
    }

    fn run(mut self, tokens: &[Token]) -> Result<TreeBuildOutput, TreeBuilderError> {
        for token in tokens {
            if matches!(token, Token::Eof { .. }) {
                self.finish(token_span(token))?;
                break;
            }

            self.report.tokens_processed = self.report.tokens_processed.saturating_add(1);
            self.process(token)?;
        }

        self.finish(SourceSpan::new(0, 0))?;
        self.report.nodes_created = self.document.len();

        Ok(TreeBuildOutput {
            document: self.document,
            report: self.report,
        })
    }

    fn process(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match self.mode {
            InsertionMode::Initial => self.process_initial(token),
            InsertionMode::BeforeHtml => self.process_before_html(token),
            InsertionMode::BeforeHead => self.process_before_head(token),
            InsertionMode::InHead => self.process_in_head(token),
            InsertionMode::AfterHead => self.process_after_head(token),
            InsertionMode::InBody => self.process_in_body(token),
            InsertionMode::AfterBody => self.process_after_body(token),
        }
    }

    fn process_initial(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Doctype(doctype) => {
                self.report.doctype_seen = true;
                self.report.force_quirks |= doctype.force_quirks;
                self.mode = InsertionMode::BeforeHtml;
                Ok(())
            }
            Token::Comment(comment) => {
                let root = self.document.root();
                self.document
                    .append_child(root, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::Character(character) if character.data.trim().is_empty() => Ok(()),
            _ => {
                self.mode = InsertionMode::BeforeHtml;
                self.process_before_html(token)
            }
        }
    }

    fn process_before_html(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Comment(comment) => {
                let root = self.document.root();
                self.document
                    .append_child(root, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::Character(character) if character.data.trim().is_empty() => Ok(()),
            Token::StartTag(tag) if tag.name == "html" => {
                self.create_html(Some(tag), false, tag.span)?;
                self.mode = InsertionMode::BeforeHead;
                Ok(())
            }
            _ => {
                self.create_html(None, true, token_span(token))?;
                self.mode = InsertionMode::BeforeHead;
                self.process_before_head(token)
            }
        }
    }

    fn process_before_head(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Character(character) if character.data.trim().is_empty() => Ok(()),
            Token::Comment(comment) => {
                let parent = self.current_parent();
                self.document
                    .append_child(parent, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::StartTag(tag) if tag.name == "head" => {
                self.create_head(Some(tag), false, tag.span)?;
                self.mode = InsertionMode::InHead;
                Ok(())
            }
            _ => {
                self.create_head(None, true, token_span(token))?;
                self.close_through("head");
                self.mode = InsertionMode::AfterHead;
                self.process_after_head(token)
            }
        }
    }

    fn process_in_head(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Character(character) if character.data.trim().is_empty() => {
                self.append_text(&character.data)
            }
            Token::Comment(comment) => {
                let parent = self.current_parent();
                self.document
                    .append_child(parent, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::Doctype(doctype) => {
                self.record_recovery(TreeRecoveryCode::UnexpectedDoctype, doctype.span);
                Ok(())
            }
            Token::StartTag(tag) if tag.name == "html" || tag.name == "head" => {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, tag.span);
                Ok(())
            }
            Token::StartTag(tag) if is_head_void_element(&tag.name) => {
                self.append_element(tag, false)?;
                Ok(())
            }
            Token::StartTag(tag) if is_head_container_element(&tag.name) => {
                self.append_element(tag, !is_void_element(&tag.name))?;
                Ok(())
            }
            Token::EndTag(tag) if tag.name == "head" => {
                self.close_through("head");
                self.mode = InsertionMode::AfterHead;
                Ok(())
            }
            Token::EndTag(tag) if is_head_container_element(&tag.name) => {
                if !self.close_through(&tag.name) {
                    self.record_recovery(TreeRecoveryCode::IgnoredEndTag, tag.span);
                }
                Ok(())
            }
            Token::EndTag(tag) if matches!(tag.name.as_str(), "body" | "html" | "br") => {
                self.close_through("head");
                self.mode = InsertionMode::AfterHead;
                self.process_after_head(token)
            }
            _ => {
                self.close_through("head");
                self.mode = InsertionMode::AfterHead;
                self.process_after_head(token)
            }
        }
    }

    fn process_after_head(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Character(character) if character.data.trim().is_empty() => Ok(()),
            Token::Comment(comment) => {
                let parent = self.html_element.unwrap_or(self.document.root());
                self.document
                    .append_child(parent, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::Doctype(doctype) => {
                self.record_recovery(TreeRecoveryCode::UnexpectedDoctype, doctype.span);
                Ok(())
            }
            Token::StartTag(tag) if tag.name == "html" || tag.name == "head" => {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, tag.span);
                Ok(())
            }
            Token::StartTag(tag) if tag.name == "body" => {
                self.create_body(Some(tag), false, tag.span)?;
                self.mode = InsertionMode::InBody;
                Ok(())
            }
            _ => {
                self.create_body(None, true, token_span(token))?;
                self.mode = InsertionMode::InBody;
                self.process_in_body(token)
            }
        }
    }

    fn process_in_body(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Character(character) => self.append_text(&character.data),
            Token::Comment(comment) => {
                let parent = self.current_parent();
                self.document
                    .append_child(parent, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::Doctype(doctype) => {
                self.record_recovery(TreeRecoveryCode::UnexpectedDoctype, doctype.span);
                Ok(())
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "html" | "head" | "body") => {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, tag.span);
                Ok(())
            }
            Token::StartTag(tag) if tag.name == "p" => {
                self.close_paragraph_if_open(tag.span);
                self.append_element(tag, true)?;
                Ok(())
            }
            Token::StartTag(tag) => {
                if closes_paragraph_on_start(&tag.name) {
                    self.close_paragraph_if_open(tag.span);
                }

                if is_void_element(&tag.name) {
                    self.append_element(tag, false)?;
                } else {
                    if tag.self_closing {
                        self.record_recovery(TreeRecoveryCode::IgnoredSelfClosingFlag, tag.span);
                    }
                    self.append_element(tag, true)?;
                }
                Ok(())
            }
            Token::EndTag(tag) if tag.name == "body" => {
                if self.close_through("body") {
                    self.mode = InsertionMode::AfterBody;
                } else {
                    self.record_recovery(TreeRecoveryCode::IgnoredEndTag, tag.span);
                }
                Ok(())
            }
            Token::EndTag(tag) if tag.name == "html" => {
                if self.has_open("body") {
                    self.close_through("body");
                    self.mode = InsertionMode::AfterBody;
                } else {
                    self.record_recovery(TreeRecoveryCode::IgnoredEndTag, tag.span);
                }
                Ok(())
            }
            Token::EndTag(tag) => {
                let popped = self.close_count_through(&tag.name);
                match popped {
                    0 => self.record_recovery(TreeRecoveryCode::IgnoredEndTag, tag.span),
                    1 => {}
                    _ => self.record_recovery(TreeRecoveryCode::MisnestedEndTag, tag.span),
                }
                Ok(())
            }
            Token::Eof { .. } => Ok(()),
        }
    }

    fn process_after_body(&mut self, token: &Token) -> Result<(), TreeBuilderError> {
        match token {
            Token::Character(character) if character.data.trim().is_empty() => {
                self.ensure_body_open()?;
                self.append_text(&character.data)?;
                self.close_through("body");
                Ok(())
            }
            Token::Comment(comment) => {
                let parent = self.html_element.unwrap_or(self.document.root());
                self.document
                    .append_child(parent, NodeKind::Comment(comment.data.clone()))?;
                Ok(())
            }
            Token::EndTag(tag) if tag.name == "html" => {
                self.close_through("html");
                Ok(())
            }
            Token::Eof { .. } => Ok(()),
            _ => {
                self.ensure_body_open()?;
                self.mode = InsertionMode::InBody;
                self.process_in_body(token)
            }
        }
    }

    fn create_html(
        &mut self,
        tag: Option<&crate::tokenizer::TagToken>,
        implicit: bool,
        span: SourceSpan,
    ) -> Result<NodeId, TreeBuilderError> {
        if let Some(existing) = self.html_element {
            if !implicit {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, span);
            }
            return Ok(existing);
        }

        let attributes = tag.map_or_else(BTreeMap::new, attributes_from_tag);
        let root = self.document.root();
        let node = self.document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("html", attributes)),
        )?;
        self.html_element = Some(node);
        self.push_open(node)?;

        if implicit {
            self.report.implicit_html = true;
            self.record_recovery(TreeRecoveryCode::ImplicitHtmlElement, span);
        }

        Ok(node)
    }

    fn create_head(
        &mut self,
        tag: Option<&crate::tokenizer::TagToken>,
        implicit: bool,
        span: SourceSpan,
    ) -> Result<NodeId, TreeBuilderError> {
        if let Some(existing) = self.head_element {
            if !implicit {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, span);
            }
            return Ok(existing);
        }

        if self.html_element.is_none() {
            self.create_html(None, true, span)?;
        }

        self.close_to_html();
        let parent = self.html_element.unwrap_or(self.document.root());
        let attributes = tag.map_or_else(BTreeMap::new, attributes_from_tag);
        let node = self.document.append_child(
            parent,
            NodeKind::Element(ElementData::with_attributes("head", attributes)),
        )?;
        self.head_element = Some(node);
        self.push_open(node)?;

        if implicit {
            self.report.implicit_head = true;
            self.record_recovery(TreeRecoveryCode::ImplicitHeadElement, span);
        }

        Ok(node)
    }

    fn create_body(
        &mut self,
        tag: Option<&crate::tokenizer::TagToken>,
        implicit: bool,
        span: SourceSpan,
    ) -> Result<NodeId, TreeBuilderError> {
        if let Some(existing) = self.body_element {
            if !implicit {
                self.record_recovery(TreeRecoveryCode::IgnoredStartTag, span);
            }
            if !self.has_open("body") {
                self.close_to_html();
                self.push_open(existing)?;
            }
            return Ok(existing);
        }

        if self.html_element.is_none() {
            self.create_html(None, true, span)?;
        }

        if self.head_element.is_none() {
            self.create_head(None, true, span)?;
        }

        self.close_to_html();
        let parent = self.html_element.unwrap_or(self.document.root());
        let attributes = tag.map_or_else(BTreeMap::new, attributes_from_tag);
        let node = self.document.append_child(
            parent,
            NodeKind::Element(ElementData::with_attributes("body", attributes)),
        )?;
        self.body_element = Some(node);
        self.push_open(node)?;

        if implicit {
            self.report.implicit_body = true;
            self.record_recovery(TreeRecoveryCode::ImplicitBodyElement, span);
        }

        Ok(node)
    }

    fn append_element(
        &mut self,
        tag: &crate::tokenizer::TagToken,
        push: bool,
    ) -> Result<NodeId, TreeBuilderError> {
        let parent = self.current_parent();
        let element = ElementData::with_attributes(tag.name.clone(), attributes_from_tag(tag));
        let node = self
            .document
            .append_child(parent, NodeKind::Element(element))?;

        if push {
            self.push_open(node)?;
        }

        Ok(node)
    }

    fn append_text(&mut self, text: &str) -> Result<(), TreeBuilderError> {
        if text.is_empty() {
            return Ok(());
        }

        if text.len() > MAX_TEXT_NODE_BYTES {
            return Err(TreeBuilderError::TextNodeTooLarge);
        }

        self.retained_text_bytes = self.retained_text_bytes.saturating_add(text.len());
        if self.retained_text_bytes > MAX_RETAINED_TEXT_BYTES {
            return Err(TreeBuilderError::RetainedTextTooLarge);
        }

        let parent = self.current_parent();
        self.document
            .append_child(parent, NodeKind::Text(text.to_owned()))?;
        Ok(())
    }

    fn push_open(&mut self, node: NodeId) -> Result<(), TreeBuilderError> {
        if self.open_elements.len() >= MAX_HTML_NESTING_DEPTH {
            return Err(TreeBuilderError::NestingTooDeep);
        }

        self.open_elements.push(node);
        self.report.max_open_elements = self.report.max_open_elements.max(self.open_elements.len());
        Ok(())
    }

    fn current_parent(&self) -> NodeId {
        self.open_elements
            .last()
            .copied()
            .or(self.body_element)
            .or(self.html_element)
            .unwrap_or(self.document.root())
    }

    fn tag_name(&self, node: NodeId) -> Option<&str> {
        self.document.node(node).and_then(|node| match node.kind() {
            NodeKind::Element(element) => Some(element.tag_name()),
            NodeKind::Document | NodeKind::Text(_) | NodeKind::Comment(_) => None,
        })
    }

    fn has_open(&self, tag_name: &str) -> bool {
        self.open_elements
            .iter()
            .rev()
            .any(|node| self.tag_name(*node) == Some(tag_name))
    }

    fn close_through(&mut self, tag_name: &str) -> bool {
        self.close_count_through(tag_name) > 0
    }

    fn close_count_through(&mut self, tag_name: &str) -> usize {
        let Some(index) = self
            .open_elements
            .iter()
            .rposition(|node| self.tag_name(*node) == Some(tag_name))
        else {
            return 0;
        };

        let popped = self.open_elements.len().saturating_sub(index);
        self.open_elements.truncate(index);
        popped
    }

    fn close_to_html(&mut self) {
        let Some(html) = self.html_element else {
            self.open_elements.clear();
            return;
        };

        self.open_elements.clear();
        self.open_elements.push(html);
    }

    fn close_paragraph_if_open(&mut self, span: SourceSpan) {
        if self.has_open("p") {
            let popped = self.close_count_through("p");
            if popped > 0 {
                self.record_recovery(TreeRecoveryCode::ImplicitParagraphClose, span);
            }
        }
    }

    fn ensure_body_open(&mut self) -> Result<(), TreeBuilderError> {
        let span = SourceSpan::new(0, 0);
        let implicit = self.body_element.is_none();
        let body = self.create_body(None, implicit, span)?;
        if !self.has_open("body") {
            self.close_to_html();
            self.push_open(body)?;
        }
        Ok(())
    }

    fn finish(&mut self, span: SourceSpan) -> Result<(), TreeBuilderError> {
        if self.html_element.is_none() {
            self.create_html(None, true, span)?;
        }

        if self.head_element.is_none() {
            self.create_head(None, true, span)?;
        }

        if self.body_element.is_none() {
            self.create_body(None, true, span)?;
        }

        self.report.nodes_created = self.document.len();
        Ok(())
    }

    fn record_recovery(&mut self, code: TreeRecoveryCode, span: SourceSpan) {
        self.report.recoveries_total = self.report.recoveries_total.saturating_add(1);

        if self.report.recoveries.len() < MAX_TREE_DIAGNOSTICS {
            self.report.recoveries.push(TreeRecovery { code, span });
        } else {
            self.report.recoveries_truncated = true;
        }
    }
}

fn attributes_from_tag(tag: &crate::tokenizer::TagToken) -> BTreeMap<String, String> {
    tag.attributes
        .iter()
        .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
        .collect()
}

fn token_span(token: &Token) -> SourceSpan {
    match token {
        Token::Character(character) => character.span,
        Token::StartTag(tag) | Token::EndTag(tag) => tag.span,
        Token::Comment(comment) => comment.span,
        Token::Doctype(doctype) => doctype.span,
        Token::Eof { position } => SourceSpan::new(*position, *position),
    }
}

fn is_head_void_element(tag_name: &str) -> bool {
    matches!(tag_name, "base" | "link" | "meta")
}

fn is_head_container_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "title" | "style" | "script" | "noscript" | "template"
    )
}

fn is_void_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn closes_paragraph_on_start(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "div"
            | "dl"
            | "fieldset"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}
