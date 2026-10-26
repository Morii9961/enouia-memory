//! ADR-MEM-93. Closed revision-bound fields; no authority from deserialization.
use super::*;
use crate::{common::*, error::Violation, ids::*, time::Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
pub(super) fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}
/// Conservative BCP 47 subset, including `und` for unknown. Not language detection.
pub fn valid_content_lang(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    let alpha = |s: &str| s.bytes().all(|b| b.is_ascii_alphabetic());
    let first = parts[0];
    if value.len() > 63 || !(2..=8).contains(&first.len()) || !alpha(first) {
        return false;
    }
    // Deliberate subset: language[-Script][-REGION][-variant...]. No partial
    // extension/private-use sequence is accepted as if it were a variant.
    let mut index = 1;
    if parts.get(index).is_some_and(|s| s.len() == 4 && alpha(s)) {
        index += 1;
    }
    if parts.get(index).is_some_and(|s| {
        (s.len() == 2 && alpha(s)) || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
    }) {
        index += 1;
    }
    let mut variants = BTreeSet::new();
    parts[index..].iter().all(|s| {
        ((5..=8).contains(&s.len()) || (s.len() == 4 && s.as_bytes()[0].is_ascii_digit()))
            && s.bytes().all(|b| b.is_ascii_alphanumeric())
            && variants.insert(s.to_ascii_lowercase())
    })
}
/// Registry membership/offset conversion is deliberately not guessed here.
pub fn valid_timezone(value: &str) -> bool {
    value == "Etc/UTC"
        || (value.len() <= 128
            && value.contains('/')
            && value.split('/').all(|p| {
                !p.is_empty()
                    && p != "."
                    && p != ".."
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
            }))
}
macro_rules! enumeration {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}
enumeration!(RecordPrecision {
    Instant,
    Minute,
    Hour,
    Day,
    Month,
    Year,
    Unknown
});
enumeration!(TimeBound { Within, Span });
enumeration!(TimeBasis {
    Stated,
    Inferred,
    SourceTime,
    Unknown
});
enumeration!(FormationMode { Backfill, Live });
enumeration!(EntityRole {
    Subject,
    Participant,
    Mentioned,
    Place,
    Project,
    Organization
});
enumeration!(SuggestedRelationKind {
    RelatedTo,
    SameEvent,
    ContinuationOf
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordTime {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub from: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub until: Option<Timestamp>,
    pub precision: RecordPrecision,
    pub bound: TimeBound,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub tz: Option<String>,
    pub basis: TimeBasis,
}
impl RecordTime {
    pub fn valid(&self) -> bool {
        let unknown = self.precision == RecordPrecision::Unknown;
        self.tz.as_ref().is_none_or(|s| valid_timezone(s))
            && if unknown {
                self.from.is_none() && self.until.is_none() && self.basis == TimeBasis::Unknown
            } else {
                self.from.is_some()
                    && self.basis != TimeBasis::Unknown
                    && self
                        .until
                        .as_ref()
                        .is_none_or(|end| Some(end) > self.from.as_ref())
                    && (self.precision == RecordPrecision::Instant || self.until.is_some())
                    && (self.bound != TimeBound::Span || self.until.is_some())
            }
    }
    pub fn matches(&self, old: &Option<FormationTime>) -> bool {
        self.valid()
            && match old {
                None => self.precision == RecordPrecision::Unknown,
                Some(t) => self.from.as_ref() == Some(&t.from) && self.until == t.until,
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuotedEvidence {
    pub reference: EvidenceRef,
    pub quote: String,
    pub speaker: SpeakerRole,
    /// Unicode scalar offsets into the exact captured message, [start, end).
    pub start: u32,
    pub end: u32,
}
impl QuotedEvidence {
    pub fn valid_shape(&self) -> bool {
        let mut errors = Vec::new();
        self.reference
            .locator
            .validate("/quote/locator", &mut errors);
        errors.is_empty()
            && !self.quote.trim().is_empty()
            && self.quote.chars().count() <= 500
            && self.start < self.end
            && self.end <= 16 * 1024
            && self.end - self.start == self.quote.chars().count() as u32
            && self.reference.supports == "content"
    }
    pub fn resolves(&self, sources: &[CapturedSource]) -> bool {
        self.valid_shape()
            && sources.iter().any(|s| {
                s.record.source_id == self.reference.source_id
                    && s.record.revision == self.reference.source_revision
                    && self.reference.locator.within(&s.record.locator)
                    && &self.reference.object_hash == s.record.anchor_hash()
                    && self.speaker == s.record.speaker_role
                    && self.reference.evidence_class == s.record.evidence_class
                    && s.text
                        .chars()
                        .skip(self.start as usize)
                        .take((self.end - self.start) as usize)
                        .collect::<String>()
                        == self.quote
            })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityMention {
    /// Separate namespace; never a name or a project/subject identifier.
    pub entity_id: String,
    pub role: EntityRole,
    pub evidence: Vec<QuotedEvidence>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormationOrigin {
    pub prompt_version: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub import_batch_id: Option<ImportId>,
    pub mode: FormationMode,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestedRelation {
    pub target: MemoryRevisionRef,
    pub relation: SuggestedRelationKind,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToneGuess {
    pub valence_milli: i16,
    pub arousal_milli: u16,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormationAnnotations {
    pub memory: MemoryRevisionRef,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub model_version: Option<String>,
    pub prompt_version: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub formation_note: Option<String>,
    pub suggested_tags: Vec<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub tone: Option<ToneGuess>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub importance_guess_milli: Option<u16>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub current_state_note: Option<String>,
    pub suggested_relations: Vec<SuggestedRelation>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecordFields {
    pub title: String,
    pub content_lang: String,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub entities: Vec<EntityMention>,
    pub evidence: Vec<QuotedEvidence>,
    pub formation: FormationOrigin,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub annotations: Option<FormationAnnotations>,
}
impl MemoryRecordFields {
    pub fn same_claim(&self, old: &Self) -> bool {
        self.title == old.title
            && self.content_lang == old.content_lang
            && self.event_time == old.event_time
            && self.valid_time == old.valid_time
            && self.entities == old.entities
            && self.evidence == old.evidence
    }
    pub fn validate_input(&self, input: &AdmissionInput) -> Vec<Violation> {
        let valid = text(&self.title, 60)
            && valid_content_lang(&self.content_lang)
            && input.dimensions.as_ref().is_some_and(|d| {
                self.event_time.matches(&d.event_time) && self.valid_time.matches(&d.valid_time)
            })
            && text(&self.formation.prompt_version, 128)
            && (self.formation.mode != FormationMode::Backfill
                || self.formation.import_batch_id.is_some())
            && self.evidence.len() == input.evidence.len()
            && self
                .evidence
                .iter()
                .zip(&input.evidence)
                .all(|(q, e)| q.valid_shape() && &q.reference == e)
            && self.entities.len() <= 32
            && (input.project_id.is_none()
                || self.entities.iter().any(|e| e.role == EntityRole::Project))
            && self
                .entities
                .iter()
                .map(|e| (&e.entity_id, format!("{:?}", e.role)))
                .collect::<BTreeSet<_>>()
                .len()
                == self.entities.len()
            && self.entities.iter().all(|e| {
                check_prefixed_uuid(&e.entity_id, "ent").is_ok()
                    && !e.evidence.is_empty()
                    && e.evidence.len() <= 8
                    && e.evidence.iter().all(|q| self.evidence.contains(q))
            })
            && self.annotations.as_ref().is_none_or(|a| {
                a.memory == input.result
                    && a.model_version == input.model_version
                    && a.prompt_version == self.formation.prompt_version
                    && a.formation_note.as_ref().is_none_or(|s| text(s, 500))
                    && a.current_state_note.as_ref().is_none_or(|s| text(s, 500))
                    && a.suggested_tags.len() <= 16
                    && a.suggested_tags.iter().all(|s| text(s, 64))
                    && a.suggested_tags.iter().collect::<BTreeSet<_>>().len()
                        == a.suggested_tags.len()
                    && a.suggested_relations.len() <= 32
                    && a.suggested_relations
                        .iter()
                        .all(|r| r.target.memory_id != input.result.memory_id)
                    && a.importance_guess_milli.is_none_or(|n| n <= 1000)
                    && a.tone.as_ref().is_none_or(|t| {
                        (-1000..=1000).contains(&t.valence_milli) && t.arousal_milli <= 1000
                    })
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("ui.record_fields", "/record_fields")]
        }
    }
}
pub(super) fn validate_record_evidence(input: &AdmissionInput, data: &AutomationData) -> bool {
    input.record_fields.as_ref().is_some_and(|r| {
        r.evidence.iter().all(|q| q.resolves(&data.sources))
            && input
                .dimensions
                .as_ref()
                .and_then(|d| d.reverts_to.as_ref())
                .is_none_or(|target| {
                    data.memories.iter().any(|m| {
                        m.target() == *target
                            && m.decision
                                .input
                                .record_fields
                                .as_ref()
                                .is_some_and(|old| r.same_claim(old))
                    })
                })
            && [&r.event_time, &r.valid_time].into_iter().all(|t| {
                t.basis != TimeBasis::SourceTime
                    || r.evidence.iter().any(|q| {
                        data.sources.iter().any(|s| {
                            s.reference().source_id == q.reference.source_id
                                && s.record.revision == q.reference.source_revision
                                && s.record.occurred_at == t.from
                        })
                    })
            })
            && r.entities.iter().all(|e| {
                data.record_catalog.as_ref().is_some_and(|c| {
                    c.entities.iter().rev().any(|record| {
                        record.entity_id == e.entity_id
                            && record.sensitivity <= input.sensitivity
                            && record.access_policy_id == input.access_policy_id
                            && (e.role != EntityRole::Project
                                || record.project_id == input.project_id)
                    })
                })
            })
    })
}
