#![forbid(unsafe_code)]

//! Capability-based security primitives and passive security events for Phantom.
//!
//! Security policy remains deterministic. [`SecurityEvent`] is a typed,
//! observation-only contract for later Guardian integration; creating an event
//! grants no capability and performs no action.

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

/// Subsystem that emitted a passive security observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecuritySurface {
    /// HTML/DOM parsing and admission controls.
    Parser,
    /// CSS parsing, cascade, and numeric controls.
    Style,
    /// Layout resource/work bounds.
    Layout,
    /// Network-origin and resource fetching controls.
    Network,
    /// Explicit capability and permission controls.
    Permission,
    /// Build/release dependency and provenance controls.
    SupplyChain,
}

/// Stable machine-readable category for a passive security event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityEventCode {
    /// A parser or DOM budget rejected work.
    ParserBudgetExceeded,
    /// CSS parsing or cascade work reached a deterministic limit.
    StyleBudgetExceeded,
    /// A layout safety bound rejected or sanitized work.
    LayoutBudgetExceeded,
    /// Mixed active content was rejected.
    MixedContentBlocked,
    /// Automatic access to a private/local network target was rejected.
    PrivateNetworkBlocked,
    /// Automatic subresource work reached its aggregate document budget.
    ResourceBudgetExceeded,
    /// A capability or permission request was denied by deterministic policy.
    PermissionDenied,
    /// A build or release supply-chain gate failed.
    SupplyChainGateFailed,
}

/// Severity used for local prioritization of passive security events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecuritySeverity {
    /// Informational observation with no active compromise signal.
    Info,
    /// A security-relevant action was constrained or rejected.
    Warning,
    /// A high-impact gate or policy boundary rejected the operation.
    High,
}

/// Typed passive security observation for the future Phantom Guardian seam.
///
/// This type intentionally contains no callback, capability, command, network
/// handle, or filesystem handle. It records a bounded stable code plus optional
/// local context. Emitting an event cannot authorize or execute an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityEvent {
    surface: SecuritySurface,
    code: SecurityEventCode,
    severity: SecuritySeverity,
    context: Option<String>,
}

impl SecurityEvent {
    /// Creates a passive event without free-form context.
    #[must_use]
    pub const fn new(
        surface: SecuritySurface,
        code: SecurityEventCode,
        severity: SecuritySeverity,
    ) -> Self {
        Self {
            surface,
            code,
            severity,
            context: None,
        }
    }

    /// Attaches bounded caller-provided local context to an event.
    ///
    /// Context is retained only when at most 512 UTF-8 bytes are supplied.
    /// Oversized context is discarded rather than truncated at an arbitrary
    /// character boundary.
    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        let context = context.into();
        if context.len() <= 512 {
            self.context = Some(context);
        }
        self
    }

    /// Returns the subsystem that emitted the observation.
    #[must_use]
    pub const fn surface(&self) -> SecuritySurface {
        self.surface
    }

    /// Returns the stable machine-readable event category.
    #[must_use]
    pub const fn code(&self) -> SecurityEventCode {
        self.code
    }

    /// Returns the event severity.
    #[must_use]
    pub const fn severity(&self) -> SecuritySeverity {
        self.severity
    }

    /// Returns optional bounded local context.
    #[must_use]
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
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

    #[test]
    fn security_event_is_passive_typed_data() {
        let event = SecurityEvent::new(
            SecuritySurface::Parser,
            SecurityEventCode::ParserBudgetExceeded,
            SecuritySeverity::Warning,
        )
        .with_context("depth");

        assert_eq!(event.surface(), SecuritySurface::Parser);
        assert_eq!(event.code(), SecurityEventCode::ParserBudgetExceeded);
        assert_eq!(event.severity(), SecuritySeverity::Warning);
        assert_eq!(event.context(), Some("depth"));
    }

    #[test]
    fn oversized_event_context_is_discarded() {
        let event = SecurityEvent::new(
            SecuritySurface::SupplyChain,
            SecurityEventCode::SupplyChainGateFailed,
            SecuritySeverity::High,
        )
        .with_context("x".repeat(513));

        assert_eq!(event.context(), None);
    }
}
