//! Core types and invariants shared across Phantom components.

#![forbid(unsafe_code)]

use core::fmt;

/// Stable identifier used to correlate work across component boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(u128);

impl RequestId {
    /// Creates a request identifier from a caller-provided value.
    #[must_use]
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    /// Returns the primitive representation for serialization adapters.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

/// Build-time information exposed without mutable global state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildInfo {
    /// Semantic project version.
    pub version: &'static str,
    /// Product name.
    pub product: &'static str,
}

impl BuildInfo {
    /// Returns build information for the Phantom workspace.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            product: "Phantom",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildInfo, RequestId};

    #[test]
    fn request_id_has_deterministic_display() {
        let id = RequestId::from_u128(0x2a);
        assert_eq!(id.to_string(), "0000000000000000000000000000002a");
    }

    #[test]
    fn build_info_identifies_phantom() {
        assert_eq!(BuildInfo::current().product, "Phantom");
    }
}
