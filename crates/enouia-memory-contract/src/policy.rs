//! Frozen default egress table (PRIVACY_RECOVERY §2) and derived-sensitivity
//! rules. Scopes are independent: memory:read ≠ source:read ≠ provider:send.

use crate::common::Sensitivity;
use crate::context::DestinationKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgressRule {
    /// May be placed in automatic context for this destination.
    Allowed,
    /// Needs a current, provider-specific standing egress policy.
    RequiresStandingGrant,
    /// Needs the owner to confirm the exact content for this request.
    RequiresPerRequestConfirmation,
    /// Never sent automatically.
    Denied,
}

pub const fn egress_rule(sensitivity: Sensitivity, destination: DestinationKind) -> EgressRule {
    match (destination.is_local(), sensitivity) {
        (_, Sensitivity::HighlySensitive) => EgressRule::Denied,
        (true, _) => EgressRule::Allowed,
        (false, Sensitivity::Public | Sensitivity::Normal) => EgressRule::RequiresStandingGrant,
        (false, Sensitivity::Private) => EgressRule::RequiresPerRequestConfirmation,
    }
}

/// Derived data inherits the strictest source level unless an owner
/// declassification review exists; shorter text is not a reason to downgrade.
pub fn derived_sensitivity_ok(
    derived: Sensitivity,
    sources: impl IntoIterator<Item = Sensitivity>,
    declassified: bool,
) -> bool {
    declassified || sources.into_iter().all(|source| derived >= source)
}

/// Permission scopes for domain operations. No scope implies another.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "memory:read")]
    MemoryRead,
    #[serde(rename = "source:read")]
    SourceRead,
    #[serde(rename = "memory:propose")]
    MemoryPropose,
    #[serde(rename = "session:propose")]
    SessionPropose,
    #[serde(rename = "context:read")]
    ContextRead,
    #[serde(rename = "owner:review")]
    OwnerReview,
    #[serde(rename = "owner:identity")]
    OwnerIdentity,
    #[serde(rename = "operation:read")]
    OperationRead,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highly_sensitive_never_leaves_automatically() {
        for destination in [
            DestinationKind::LocalMock,
            DestinationKind::LocalModel,
            DestinationKind::ExternalProvider,
        ] {
            assert_eq!(
                egress_rule(Sensitivity::HighlySensitive, destination),
                EgressRule::Denied
            );
        }
        assert_eq!(
            egress_rule(Sensitivity::Private, DestinationKind::ExternalProvider),
            EgressRule::RequiresPerRequestConfirmation
        );
    }

    #[test]
    fn derived_data_cannot_silently_downgrade() {
        assert!(!derived_sensitivity_ok(
            Sensitivity::Normal,
            [Sensitivity::Private],
            false
        ));
        assert!(derived_sensitivity_ok(
            Sensitivity::Normal,
            [Sensitivity::Private],
            true
        ));
    }
}
