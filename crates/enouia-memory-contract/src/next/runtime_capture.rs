//! Native Runtime capture data. Deserialization does not register a producer.
use super::*;
use crate::{
    common::{ActorRef, SpeakerRole},
    hash::Sha256Hex,
    ids::*,
    json::nullable,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const OWNER_RUNTIME_CAPTURE_CAPABILITY: &str = "memory-runtime-owner-capture-native-1";

pub const RUNTIME_CAPTURE_CAPABILITY: &str = "memory-runtime-capture-native-1";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTimezoneOrigin {
    Source,
    OwnerDefault,
}
/// Source audit data only. Never use extension content as an identity grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCaptureProvenance {
    pub schema_version: crate::json::Revision,
    pub producer: ActorRef,
    pub owner_config_hash: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub turn_hash: Sha256Hex,
    pub resolved_timezone: String,
    pub timezone_origin: RuntimeTimezoneOrigin,
    pub attachments: Vec<RuntimeAttachmentMetadata>,
    pub attachment_bytes_captured: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuntimeCaptureKind {
    #[serde(rename = "runtime_capture_request")]
    Capture,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuntimeCaptureCapability {
    #[serde(rename = "memory-runtime-capture-native-1")]
    V1,
    #[serde(rename = "memory-runtime-owner-capture-native-1")]
    OwnerV1,
}
impl RuntimeCaptureCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => RUNTIME_CAPTURE_CAPABILITY,
            Self::OwnerV1 => OWNER_RUNTIME_CAPTURE_CAPABILITY,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAttachmentMetadata {
    pub original_id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub media_type: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub byte_len: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub content_hash: Option<Sha256Hex>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCaptureTurn {
    pub message_id: String,
    #[serde(deserialize_with = "nullable")]
    pub parent_message_id: Option<String>,
    pub branch_id: String,
    pub sequence: u32,
    pub speaker: SpeakerRole,
    pub text: String,
    #[serde(deserialize_with = "nullable")]
    pub original_time: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub original_timezone: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub occurred_at: Option<Timestamp>,
    pub content_lang: String,
    #[serde(deserialize_with = "nullable")]
    pub assistant_model: Option<String>,
    pub attachments: Vec<RuntimeAttachmentMetadata>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCaptureRequest {
    pub schema_version: NextVersion,
    pub kind: RuntimeCaptureKind,
    pub capability: RuntimeCaptureCapability,
    pub request_id: RequestId,
    pub conversation_id: String,
    #[serde(deserialize_with = "nullable")]
    pub title: Option<String>,
    pub turns: Vec<RuntimeCaptureTurn>,
}
impl RuntimeCaptureRequest {
    pub fn valid(&self) -> bool {
        let text = |s: &str, n| record_fields::text(s, n);
        text(&self.conversation_id, 256)
            && self.title.as_ref().is_none_or(|s| text(s, 256))
            && (1..=8).contains(&self.turns.len())
            && self
                .turns
                .iter()
                .map(|t| (&t.branch_id, t.sequence))
                .collect::<BTreeSet<_>>()
                .len()
                == self.turns.len()
            && self
                .turns
                .iter()
                .map(|t| &t.message_id)
                .collect::<BTreeSet<_>>()
                .len()
                == self.turns.len()
            && self.turns.iter().all(|t| {
                text(&t.message_id, 256)
                    && text(&t.branch_id, 256)
                    && t.sequence > 0
                    && t.parent_message_id
                        .as_ref()
                        .is_none_or(|s| text(s, 256) && s != &t.message_id)
                    && !t.text.trim().is_empty()
                    && t.text.len() <= 16 * 1024
                    && t.original_time.as_ref().is_none_or(|s| text(s, 256))
                    && t.original_timezone
                        .as_ref()
                        .is_none_or(|s| valid_timezone(s))
                    && valid_content_lang(&t.content_lang)
                    && t.assistant_model
                        .as_ref()
                        .is_none_or(|s| t.speaker == SpeakerRole::Assistant && text(s, 128))
                    && t.attachments.len() <= 16
                    && t.attachments
                        .iter()
                        .map(|a| &a.original_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == t.attachments.len()
                    && t.attachments.iter().all(|a| {
                        text(&a.original_id, 256)
                            && a.name.as_ref().is_none_or(|s| text(s, 256))
                            && a.media_type.as_ref().is_none_or(|s| text(s, 128))
                            && a.byte_len
                                .is_none_or(|n| n <= crate::json::MAX_SAFE_INTEGER)
                    })
            })
    }
}
