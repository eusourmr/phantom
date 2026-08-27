#![forbid(unsafe_code)]

//! Capability-based security primitives for Phantom.
//!
//! This crate defines explicit capability grants used to prevent browser
//! components from silently acquiring access to sensitive resources.

/// Categories of privileged operations controlled by Phantom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityKind {
    /// Permission to read the contents of the current page.
    PageRead,

    /// Permission to initiate or continue browser navigation.
    Navigation,

    /// Permission to read files from the local filesystem.
    FileRead,

    /// Permission to write files to the local filesystem.
    FileWrite,

    /// Permission to communicate over the network.
    Network,

    /// Permission to access the system clipboard.
    Clipboard,

    /// Permission to access location information.
    Location,

    /// Permission to access microphone input.
    Microphone,

    /// Permission to access camera input.
    Camera,

    /// Permission to propose an action that requires explicit user approval.
    ActionProposal,
}

/// Scope to which a capability grant applies.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CapabilityScope {
    /// Restricts the capability to one isolated site instance.
    SiteInstance(u64),

    /// Restricts the capability to a named resource.
    Resource(String),

    /// Grants the capability to an internal Phantom service.
    Service,
}

/// Explicit authorization for one capability within one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    kind: CapabilityKind,
    scope: CapabilityScope,
}

impl Capability {
    /// Returns the category of operation authorized by this capability.
    #[must_use]
    pub const fn kind(&self) -> CapabilityKind {
        self.kind
    }

    /// Returns the scope in which this capability is valid.
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
}

/// Creates explicit capability grants.
///
/// The broker does not infer permissions. A capability exists only after a
/// caller explicitly asks for one.
#[derive(Debug, Default, Clone, Copy)]
pub struct CapabilityBroker;

impl CapabilityBroker {
    /// Creates an empty capability broker.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Creates an explicit capability for the requested kind and scope.
    #[must_use]
    pub fn grant(&self, kind: CapabilityKind, scope: CapabilityScope) -> Capability {
        Capability { kind, scope }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_grants_only_requested_kind_and_scope() {
        let broker = CapabilityBroker::new();
        let capability = broker.grant(CapabilityKind::PageRead, CapabilityScope::SiteInstance(7));

        assert_eq!(capability.kind(), CapabilityKind::PageRead);
        assert_eq!(capability.scope(), &CapabilityScope::SiteInstance(7));
    }
}
