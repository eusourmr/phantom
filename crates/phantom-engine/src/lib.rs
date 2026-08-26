//! Phantom web engine orchestration boundary.

#![forbid(unsafe_code)]

use phantom_dom::Document;

/// High-level lifecycle state of an engine instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineState {
    Idle,
}

/// Minimal engine shell.
///
/// Parsing, style, layout, rendering, scripting and networking are deliberately
/// absent until their contracts and conformance tests are introduced.
#[derive(Debug, Default)]
pub struct Engine {
    document: Document,
}

impl Engine {
    #[must_use]
    pub fn new() -> Self {
        Self { document: Document::new() }
    }

    #[must_use]
    pub const fn state(&self) -> EngineState {
        EngineState::Idle
    }

    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, EngineState};

    #[test]
    fn new_engine_is_idle_with_one_root_node() {
        let engine = Engine::new();
        assert_eq!(engine.state(), EngineState::Idle);
        assert_eq!(engine.document().len(), 1);
    }
}
