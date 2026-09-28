//! Default egress table (PRIVACY_RECOVERY §2), derived-sensitivity rule, and
//! persistent access/egress policies with a default-deny evaluator
//! (ADR-MEM-31). Scopes are independent: memory:read ≠ source:read ≠
//! provider:send. Policies, grants, and revocations are file records, so the
//! authorization state is recoverable from the Vault, not from an index.

use crate::common::{ActorRef, ActorType, Sensitivity};
use crate::context::{Destination, DestinationKind, Purpose};
use crate::error::{MemoryErrorCode, Violation};
use crate::ids::{ApprovalId, PolicyId, PrincipalId, ProjectId};
use crate::json::{Revision, SchemaVersion};
use crate::record::{RecordKind, RecordRef};
use crate::time::Timestamp;
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

/// Derived data inherits the strictest source level unless a matching owner
/// declassification approval exists (the set validator checks its binding);
/// shorter text is not a reason to downgrade.
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
    /// Put resources into a request for a Provider destination.
    #[serde(rename = "provider:send")]
    ProviderSend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus {
    Active,
    Revoked,
}

/// `genesis_default` is the owner-only local policy created with the Vault;
/// every other policy is an explicit owner grant bound to an approval.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOrigin {
    GenesisDefault,
    OwnerGrant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalSelector {
    pub actor_type: ActorType,
    /// `null` matches every principal of that type.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub actor_id: Option<PrincipalId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSelector {
    pub all_projects: bool,
    pub project_ids: Vec<ProjectId>,
    pub record_kinds: Vec<RecordKind>,
    pub max_sensitivity: Sensitivity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationSelector {
    pub kind: DestinationKind,
    /// `null` matches any provider/model of this kind; a set value must equal
    /// the destination binding exactly.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub provider: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub model: Option<String>,
}

impl DestinationSelector {
    pub fn matches(&self, destination: &Destination) -> bool {
        let binding = destination.provider_binding.as_ref();
        self.kind == destination.kind
            && self
                .provider
                .as_ref()
                .is_none_or(|p| binding.is_some_and(|b| &b.provider == p))
            && self
                .model
                .as_ref()
                .is_none_or(|m| binding.is_some_and(|b| &b.model == m))
    }
}

/// Versioned access/egress policy. No extensions: every field affects security.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRecord {
    pub schema_version: SchemaVersion,
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub status: PolicyStatus,
    pub origin: PolicyOrigin,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub approval_id: Option<ApprovalId>,
    pub principals: Vec<PrincipalSelector>,
    pub scopes: Vec<Scope>,
    pub resources: ResourceSelector,
    pub purposes: Vec<Purpose>,
    pub destinations: Vec<DestinationSelector>,
    pub valid_from: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_until: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub revoked_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl PolicyRecord {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if (self.origin == PolicyOrigin::OwnerGrant) != self.approval_id.is_some() {
            out.push(Violation::new("policy.approval", "/approval_id"));
        }
        if (self.status == PolicyStatus::Revoked) != self.revoked_at.is_some() {
            out.push(Violation::new("policy.revocation", "/revoked_at"));
        }
        if self.resources.all_projects == !self.resources.project_ids.is_empty() {
            out.push(Violation::new("policy.project_selector", "/resources"));
        }
        if self.principals.is_empty()
            || self.scopes.is_empty()
            || self.purposes.is_empty()
            || self.resources.record_kinds.is_empty()
        {
            out.push(Violation::new("policy.empty_selector", "/"));
        }
        if self.scopes.contains(&Scope::ProviderSend) && self.destinations.is_empty() {
            out.push(Violation::new("policy.destinations", "/destinations"));
        }
        if self.destinations.iter().any(|d| {
            d.kind != DestinationKind::ExternalProvider
                && (d.provider.is_some() || d.model.is_some())
        }) {
            out.push(Violation::new("policy.destinations", "/destinations"));
        }
        if self.origin == PolicyOrigin::GenesisDefault
            && (self.scopes.contains(&Scope::ProviderSend)
                || self
                    .destinations
                    .iter()
                    .any(|d| d.kind == DestinationKind::ExternalProvider)
                || self
                    .principals
                    .iter()
                    .any(|p| p.actor_type != ActorType::Owner))
        {
            out.push(Violation::new("policy.default_scope", "/"));
        }
        if let Some(until) = &self.valid_until
            && until <= &self.valid_from
        {
            out.push(Violation::new("policy.validity", "/valid_until"));
        }
        if self.created_at > self.updated_at {
            out.push(Violation::new("policy.time_order", "/updated_at"));
        }
        out
    }

    pub fn active_at(&self, at: &Timestamp) -> bool {
        self.status == PolicyStatus::Active
            && self.revoked_at.is_none()
            && &self.valid_from <= at
            && self.valid_until.as_ref().is_none_or(|until| at < until)
    }
}

/// The resource a request would touch, resolved by the server from pinned
/// records, never from caller-supplied arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceContext {
    pub record: RecordRef,
    pub project_id: Option<ProjectId>,
    pub sensitivity: Sensitivity,
}

#[derive(Clone, Debug)]
pub struct AccessContext<'a> {
    /// From the transport's authentication context.
    pub principal: &'a ActorRef,
    pub scope: Scope,
    pub purpose: Option<Purpose>,
    /// The full destination (kind and exact Provider binding) for egress.
    pub destination: Option<&'a Destination>,
    pub resource: &'a ResourceContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyDecision {
    /// Names the exact policy revision that allowed the request.
    Allow {
        policy_id: PolicyId,
        revision: Revision,
    },
    /// Always rendered to callers as a non-disclosing code.
    Deny { code: MemoryErrorCode },
}

impl PolicyDecision {
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

fn policy_matches(policy: &PolicyRecord, context: &AccessContext<'_>) -> bool {
    let principal = context.principal;
    let resource = context.resource;
    let principal_ok = policy.principals.iter().any(|selector| {
        selector.actor_type == principal.actor_type
            && selector
                .actor_id
                .as_ref()
                .is_none_or(|id| id == &principal.actor_id)
    });
    let project_ok = policy.resources.all_projects
        || resource
            .project_id
            .as_ref()
            .is_some_and(|p| policy.resources.project_ids.contains(p));
    let destination_ok = context
        .destination
        .is_none_or(|d| policy.destinations.iter().any(|s| s.matches(d)));
    principal_ok
        && policy.scopes.contains(&context.scope)
        && context.purpose.is_none_or(|p| policy.purposes.contains(&p))
        && policy
            .resources
            .record_kinds
            .contains(&resource.record.record_kind)
        && project_ok
        && resource.sensitivity <= policy.resources.max_sensitivity
        && destination_ok
}

/// Default-deny evaluation over the latest revision of each policy. The
/// frozen egress table still applies on top: highly sensitive content is never
/// sent automatically, whatever a policy says. Among allowing policies the
/// lowest policy ID wins, so the named policy is deterministic.
pub fn evaluate(
    policies: &[&PolicyRecord],
    context: &AccessContext<'_>,
    at: &Timestamp,
) -> PolicyDecision {
    let deny = PolicyDecision::Deny {
        code: MemoryErrorCode::PermissionDenied,
    };
    if context.scope == Scope::ProviderSend {
        let Some(destination) = context.destination else {
            return deny;
        };
        if egress_rule(context.resource.sensitivity, destination.kind) == EgressRule::Denied {
            return deny;
        }
    }
    let mut latest: std::collections::BTreeMap<&PolicyId, &PolicyRecord> =
        std::collections::BTreeMap::new();
    for policy in policies {
        let entry = latest.entry(&policy.policy_id).or_insert(policy);
        if policy.revision > entry.revision {
            *entry = policy;
        }
    }
    latest
        .into_values()
        .find(|policy| policy.active_at(at) && policy_matches(policy, context))
        .map_or(deny, |policy| PolicyDecision::Allow {
            policy_id: policy.policy_id.clone(),
            revision: policy.revision,
        })
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
