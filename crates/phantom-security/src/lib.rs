//! Capability-based security primitives for Phantom.

#![forbid(unsafe_code)]

/// Privileged resource classes that can be granted to a component.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CapabilityKind {
    PageRead,
    Navigation,
    FileRead,
    FileWrite,
    Network,
    Clipboard,
    Location,
    Microphone,
    Camera,
    ActionProposal,
}

/// Scope attached to a granted capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityScope {
    SiteInstance(u64),
    Resource(String),
    Service,
}

/// Authorization token with private construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capability {
    kind: CapabilityKind,
    scope: CapabilityScope,
}

impl Capability {
    #[must_use]
    pub const fn kind(&self) -> CapabilityKind {
        self.kind
    }

    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
}

/// Initial authority boundary responsible for issuing capabilities.
#[derive(Debug, Default)]
pub struct CapabilityBroker;

impl CapabilityBroker {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Grants a capability after policy evaluation by the caller.
    #[must_use]
    pub fn grant(&self, kind: CapabilityKind, scope: CapabilityScope) -> Capability {
        Capability { kind, scope }
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityBroker, CapabilityKind, CapabilityScope};

    #[test]
    fn broker_grants_only_requested_kind_and_scope() {
        let broker = CapabilityBroker::new();
        let capability = broker.grant(
            CapabilityKind::PageRead,
            CapabilityScope::SiteInstance(7),
        );

        assert_eq!(capability.kind(), CapabilityKind::PageRead);
        assert_eq!(capability.scope(), &CapabilityScope::SiteInstance(7));
    }
}
