//! Identity sidecar metadata. The Markdown body lives beside it
//! (`records/identity/<id>/<revision>.md`) and is referenced by `content_hash`;
//! both are published by the same commit. Identity text can shape style and
//! relationship context only; it never changes ACLs, egress, or tool rights.

use crate::common::{ActorRef, ActorType, Sensitivity};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{IdentityId, PolicyId, ReviewId};
use crate::json::{Extensions, Revision, SchemaVersion, validate_extensions};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySlug {
    Core,
    RuntimeRules,
    Style,
    Relationship,
    Boundaries,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum IdentityMediaType {
    #[serde(rename = "text/markdown; charset=utf-8")]
    MarkdownUtf8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityMetadata {
    pub schema_version: SchemaVersion,
    pub identity_id: IdentityId,
    pub revision: Revision,
    pub slug: IdentitySlug,
    pub title: String,
    pub content_hash: Sha256Hex,
    pub content_media_type: IdentityMediaType,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub egress_policy_id: Option<PolicyId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub previous_revision: Option<Revision>,
    pub review_id: ReviewId,
    pub approved_by: ActorRef,
    pub approved_at: Timestamp,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub extensions: Extensions,
}

impl IdentityMetadata {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let expected_previous = self.revision.get().checked_sub(1).and_then(Revision::new);
        if self.previous_revision != expected_previous {
            out.push(Violation::new(
                "identity.previous_revision",
                "/previous_revision",
            ));
        }
        if self.approved_by.actor_type != ActorType::Owner {
            out.push(Violation::new("identity.approval_actor", "/approved_by"));
        }
        if self.created_at > self.updated_at || self.approved_at > self.updated_at {
            out.push(Violation::new("identity.time_order", "/created_at"));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}
