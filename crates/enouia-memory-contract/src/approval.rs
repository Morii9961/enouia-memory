//! Owner approvals that are not candidate reviews (ADR-MEM-30).
//!
//! A ReviewRecord accepts a *memory proposal*. Consent to send content to an
//! external model, to lower a record's sensitivity, or to grant a policy is a
//! different decision and must never be inferred from a memory review. Each
//! ApprovalRecord therefore carries an explicit binding to exactly what was
//! approved, is issued by the owner on a trusted surface with a single-use
//! nonce, and is checked against that binding by the set validator.

use crate::common::{ActorRef, ActorType, Sensitivity, SourceRevisionRef, TrustedSurface};
use crate::context::{Destination, DestinationKind};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{ApprovalId, CapsuleId, PolicyId, RequestId};
use crate::json::{Revision, SchemaVersion};
use crate::record::RecordRef;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Egress approvals are short-lived: at most 15 minutes from issue to expiry.
pub const EGRESS_APPROVAL_MAX_TTL_MS: i64 = 15 * 60 * 1000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovalBinding {
    /// Send exactly this rendered request (payload digest) with exactly these
    /// resource revisions to exactly this destination, under this policy.
    Egress {
        request_id: RequestId,
        capsule_id: CapsuleId,
        payload_hash: Sha256Hex,
        destination: Destination,
        resources: Vec<RecordRef>,
        policy_id: PolicyId,
        policy_epoch: u64,
    },
    /// Lower one record revision from the strictest source level to a lower
    /// level, for exactly this content and these sources.
    Declassification {
        target: RecordRef,
        from_sensitivity: Sensitivity,
        to_sensitivity: Sensitivity,
        source_refs: Vec<SourceRevisionRef>,
        final_content_hash: Sha256Hex,
    },
    /// Grant exactly this policy revision (digest of its canonical bytes).
    PolicyGrant {
        policy_id: PolicyId,
        policy_revision: Revision,
        grant_hash: Sha256Hex,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRecord {
    pub schema_version: SchemaVersion,
    pub approval_id: ApprovalId,
    pub approved_by: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub approval_nonce: String,
    pub approved_diff_hash: Sha256Hex,
    pub issued_at: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub expires_at: Option<Timestamp>,
    pub binding: ApprovalBinding,
}

impl ApprovalRecord {
    /// True when `at` lies within `[issued_at, expires_at)`.
    pub fn valid_at(&self, at: &Timestamp) -> bool {
        &self.issued_at <= at && self.expires_at.as_ref().is_none_or(|end| at < end)
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.approved_by.actor_type != ActorType::Owner {
            out.push(Violation::new("approval.owner_required", "/approved_by"));
        }
        if !crate::candidate::is_nonce(&self.approval_nonce) {
            out.push(Violation::new("approval.nonce_format", "/approval_nonce"));
        }
        if let Some(expires) = &self.expires_at
            && expires <= &self.issued_at
        {
            out.push(Violation::new("approval.validity", "/expires_at"));
        }
        match &self.binding {
            ApprovalBinding::Egress {
                destination,
                resources,
                ..
            } => {
                let ttl = self
                    .expires_at
                    .as_ref()
                    .map(|e| e.unix_ms() - self.issued_at.unix_ms());
                if ttl.is_none_or(|ttl| ttl > EGRESS_APPROVAL_MAX_TTL_MS) {
                    out.push(Violation::new("approval.egress_ttl", "/expires_at"));
                }
                if destination.kind != DestinationKind::ExternalProvider
                    || destination.provider_binding.is_none()
                {
                    out.push(Violation::new(
                        "approval.egress_destination",
                        "/binding/destination",
                    ));
                }
                let unique: BTreeSet<&RecordRef> = resources.iter().collect();
                if resources.is_empty() || unique.len() != resources.len() {
                    out.push(Violation::new(
                        "approval.egress_resources",
                        "/binding/resources",
                    ));
                }
                for (index, reference) in resources.iter().enumerate() {
                    reference.validate(&format!("/binding/resources/{index}"), &mut out);
                }
            }
            ApprovalBinding::Declassification {
                target,
                from_sensitivity,
                to_sensitivity,
                source_refs,
                ..
            } => {
                if to_sensitivity >= from_sensitivity {
                    out.push(Violation::new(
                        "approval.declassification_direction",
                        "/binding/to_sensitivity",
                    ));
                }
                if source_refs.is_empty() {
                    out.push(Violation::new(
                        "approval.declassification_sources",
                        "/binding/source_refs",
                    ));
                }
                target.validate("/binding/target", &mut out);
            }
            ApprovalBinding::PolicyGrant { .. } => {}
        }
        out
    }
}
