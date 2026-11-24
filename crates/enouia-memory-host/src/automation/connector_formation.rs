//! ADR-MEM-157/158: daily memories distilled by ChatGPT and submitted through
//! the remote connector. Memory only validates and stores: the submitted turns
//! are captured under a Morii capture window, and the captured formation step
//! forms exactly the submitted statements under its existing checks. Uses the
//! platform-neutral Vault API, so it runs wherever the remote service runs.
use super::application::{ApplicationFault, ApplicationResult};
use enouia_memory_contract::{
    common::{ActorRef, ActorType, Sensitivity, SourceRevisionRef, SpeakerRole},
    ids::{CommitId, MemoryId, OperationId, PolicyId, RequestId},
    json::Revision,
    next::*,
    time::Timestamp,
};
use enouia_memory_vault::automation::segmented::{
    CaptureAnalysis, CaptureAnalyzer, CaptureStatement, CaptureStopToken, CapturedFormation,
    CapturedQueueReceipt, FormationSeed, MAX_CAPTURE_WINDOW_DAYS, NativeRuntimeCapture,
    PendingCaptureAnalysis, ProducerJobStatus, QueuedCapture, RuntimeCaptureEntity,
    RuntimeCaptureScope, SegmentedAutomationVault, StatementEntity, SystemReadScope,
    formation_seed,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const PROMPT_VERSION: &str = "chatgpt-connector-authored-1";
pub const MAX_STATEMENTS: usize = 32;
/// ADR-MEM-171: named people, places and other entities per statement.
pub const MAX_STATEMENT_ENTITIES: usize = 8;
/// ADR-MEM-174: memories one ChatGPT theme may synthesize.
pub const MAX_THEME_MEMBERS: usize = 32;
pub const THEME_PROMPT_VERSION: &str = "chatgpt-connector-theme-1";

/// Sixteen stable bytes for a derived identifier.
fn seed16(value: &serde_json::Value) -> [u8; 16] {
    let digest = enouia_memory_contract::hash::sha256(
        &enouia_memory_contract::json::canonical_bytes(value).expect("closed seed"),
    );
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest.as_str()[2 * i..2 * i + 2], 16).unwrap_or(0);
    }
    bytes
}

/// ADR-MEM-174: a theme ChatGPT synthesized over daily memories it read.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorTheme {
    pub request_id: RequestId,
    /// The model ChatGPT reports for its own synthesis, recorded as-is.
    pub analyst_model: String,
    pub title: String,
    /// The theme in ChatGPT's own words: an inference, not a statement by Morii.
    pub summary: String,
    /// The current memories it draws on, 2 to 32, in the connector domain.
    pub members: Vec<MemoryId>,
    #[serde(default)]
    pub note: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct ThemeReceipt {
    /// The formed theme, or `None` on preflight.
    pub memory: Option<MemoryRevisionRef>,
    pub replayed: bool,
    pub preflight: bool,
}
impl ConnectorTheme {
    /// ADR-MEM-181: every self-check this theme fails, worded so ChatGPT can
    /// correct it. Empty means the theme may reach the Vault.
    pub fn problems(&self) -> Vec<String> {
        let text_ok = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
        let mut out = Vec::new();
        for (field, value, max) in [
            ("analyst_model", &self.analyst_model, 128),
            ("title", &self.title, 60),
            ("summary", &self.summary, 2000),
        ] {
            if !text_ok(value, max) {
                out.push(format!("{field} must be 1 to {max} characters"));
            }
        }
        if self.note.as_ref().is_some_and(|n| !text_ok(n, 512)) {
            out.push("note must be 1 to 512 characters, or null".into());
        }
        if !(2..=MAX_THEME_MEMBERS).contains(&self.members.len()) {
            out.push(format!(
                "members must list 2 to {MAX_THEME_MEMBERS} memory IDs"
            ));
        }
        if self
            .members
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.members.len()
        {
            out.push("members must not repeat a memory ID".into());
        }
        out
    }
    pub fn valid(&self) -> bool {
        self.problems().is_empty()
    }
}
pub const CAPTURE_PLATFORM: &str = "enouia-runtime";
/// Formation steps one submission may drive before answering.
const MAX_STEPS: usize = 64;
/// Capture permits stay short (the Vault allows at most five minutes).
const PERMIT_MS: i64 = 240_000;

/// One memory ChatGPT distilled from a user turn of the same submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorStatement {
    pub message_id: String,
    pub quote: String,
    pub title: String,
    pub interpretation: Interpretation,
    pub kind: FormationKind,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    #[serde(default)]
    pub note: Option<String>,
    /// ADR-MEM-171: who or what the statement names. Each becomes its own
    /// entity record (no name-based merge), evidenced by the statement quote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<ConnectorEntity>,
}

/// One entity a statement names, as ChatGPT reports it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorEntity {
    pub name: String,
    pub entity_type: EntityType,
    pub reality: EntityReality,
    pub role: EntityRole,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSubmission {
    pub request: RuntimeCaptureRequest,
    /// The model ChatGPT reports for its own analysis, recorded as-is.
    pub analyst_model: String,
    pub statements: Vec<ConnectorStatement>,
}

/// Fixed by Morii's service configuration, never by the submitting client.
/// Owner capture accepts only the `enouia-runtime` platform, so the
/// connector's conversations are told apart by `account_scope`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorConfig {
    pub account_scope: String,
    pub default_timezone: String,
    pub max_sensitivity: Sensitivity,
    pub allowed_forms: Vec<FormationKind>,
    pub window_days: u32,
}

#[derive(Serialize)]
pub struct ConnectorReceipt {
    pub capture: Option<NativeRuntimeCapture>,
    pub queue: Option<CapturedQueueReceipt>,
    pub jobs: Vec<ProducerJobStatus>,
    /// True when preflight found no open window; one would be opened on submit.
    pub window_needed: bool,
    /// ADR-MEM-187: a replay after its window ended; nothing was captured
    /// again, and the receipt is read from what was recorded.
    pub window_ended: bool,
    /// ADR-MEM-183: the current memories formed from this submission's
    /// captured turns, so ChatGPT can cite them without searching.
    pub memories: Vec<FormedMemory>,
    /// ADR-MEM-184: the captured source of each turn, in turn order, so a
    /// later correction or forget request can cite where Morii said it.
    pub turns: Vec<TurnSource>,
}

#[derive(Debug, Serialize)]
pub struct TurnSource {
    pub message_id: String,
    pub source: SourceRevisionRef,
}

#[derive(Debug, Serialize)]
pub struct FormedMemory {
    pub memory_id: MemoryId,
    pub revision: Revision,
    pub title: String,
}

type Key = (String, String);

/// A `CaptureAnalyzer` that returns exactly what was submitted for a turn. A
/// turn never registered in this process (for example after a restart) pauses,
/// so it stays queued until the same request is resubmitted.
#[derive(Clone, Default)]
pub struct SubmittedAnalyzer(Arc<Mutex<BTreeMap<Key, Vec<CaptureStatement>>>>);

impl CaptureAnalyzer for SubmittedAnalyzer {
    fn start(
        &mut self,
        _: &QueuedCapture,
        source: &CapturedSource,
        _: &[AgentNoteRecord],
        _: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        let key = (
            source
                .record
                .original_conversation_id
                .clone()
                .unwrap_or_default(),
            source
                .record
                .original_message_id
                .clone()
                .unwrap_or_default(),
        );
        let Ok(mut submitted) = self.0.lock() else {
            return Err(CaptureAnalysis::Paused);
        };
        Err(match submitted.remove(&key) {
            Some(statements) if statements.is_empty() => CaptureAnalysis::NoMemory,
            Some(statements) => CaptureAnalysis::Statements(statements),
            None => CaptureAnalysis::Paused,
        })
    }
}

impl ConnectorSubmission {
    /// Checks the submission against itself and the forms this service
    /// allows before anything reaches the Vault. The Vault and the formation
    /// step repeat their own checks.
    pub fn valid(&self, allowed: &[FormationKind]) -> bool {
        self.problems(allowed).is_empty()
    }
    /// ADR-MEM-181: every self-check this submission fails, worded so ChatGPT
    /// can correct it. Empty means the submission may reach the Vault.
    pub fn problems(&self, allowed: &[FormationKind]) -> Vec<String> {
        let text_ok = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
        let mut out = Vec::new();
        let turns = &self.request.turns;
        if !(1..=8).contains(&turns.len()) {
            out.push("request.turns must hold 1 to 8 turns".into());
        }
        if turns
            .iter()
            .map(|t| &t.message_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != turns.len()
        {
            out.push("each turn needs its own message_id".into());
        }
        for t in turns {
            if t.text.trim().is_empty() || t.text.len() > 16 * 1024 {
                out.push(format!(
                    "turn {}: text must be non-empty and at most 16 KiB",
                    t.message_id
                ));
            }
        }
        if out.is_empty() && !self.request.valid() {
            out.push("request is malformed: check conversation_id (1-256 characters), title, branch and sequence, parent_message_id, content_lang and attachments".into());
        }
        if !text_ok(&self.analyst_model, 128) {
            out.push("analyst_model must be 1 to 128 characters".into());
        }
        if self.statements.len() > MAX_STATEMENTS {
            out.push(format!(
                "at most {MAX_STATEMENTS} statements per submission"
            ));
        }
        for (n, s) in self.statements.iter().enumerate() {
            let at = format!("statement {n} ({})", s.message_id);
            if !text_ok(&s.title, 128) {
                out.push(format!("{at}: title must be 1 to 128 characters"));
            }
            if !text_ok(&s.quote, 2048) {
                out.push(format!("{at}: quote must be 1 to 2048 characters"));
            }
            if s.note.as_ref().is_some_and(|n| !text_ok(n, 512)) {
                out.push(format!("{at}: note must be 1 to 512 characters, or null"));
            }
            if s.entities.len() > MAX_STATEMENT_ENTITIES
                || s.entities.iter().any(|e| !text_ok(&e.name, 128))
            {
                out.push(format!(
                    "{at}: at most {MAX_STATEMENT_ENTITIES} entities, each named in 1 to 128 characters"
                ));
            }
            if !allowed.contains(&s.kind) {
                out.push(format!(
                    "{at}: kind is not allowed here; use one of {allowed:?}"
                ));
            }
            match turns.iter().find(|t| t.message_id == s.message_id) {
                None => out.push(format!(
                    "{at}: no turn in this submission has this message_id"
                )),
                Some(t) if t.speaker != SpeakerRole::User => {
                    out.push(format!("{at}: quote Morii's own (user) turns only"))
                }
                Some(t) if !t.text.contains(&s.quote) => out.push(format!(
                    "{at}: the quote does not appear verbatim in that turn's text"
                )),
                Some(_) => {}
            }
        }
        out
    }
    /// A fresh, deterministic entity ID per statement and position, so a
    /// resubmitted request names the entities its first capture recorded.
    fn entity_id(&self, statement: usize, entity: usize) -> String {
        let bytes = seed16(&serde_json::json!([
            "connector-entity-1",
            self.request.request_id,
            statement,
            entity
        ]));
        format!("ent_{}", enouia_memory_contract::ids::format_uuid_v4(bytes))
    }
    fn capture_entities(&self) -> Vec<RuntimeCaptureEntity> {
        self.statements
            .iter()
            .enumerate()
            .flat_map(|(n, s)| {
                s.entities
                    .iter()
                    .enumerate()
                    .map(move |(k, e)| RuntimeCaptureEntity {
                        message_id: s.message_id.clone(),
                        quote: s.quote.clone(),
                        entity_id: self.entity_id(n, k),
                        entity_type: e.entity_type,
                        reality: e.reality,
                        display_name: e.name.clone(),
                    })
            })
            .collect()
    }
    fn statements_by_turn(&self) -> BTreeMap<Key, Vec<CaptureStatement>> {
        self.request
            .turns
            .iter()
            .map(|t| {
                let statements = self
                    .statements
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.message_id == t.message_id)
                    .map(|(n, s)| CaptureStatement {
                        quote: s.quote.clone(),
                        title: s.title.clone(),
                        interpretation: s.interpretation,
                        dimensions: FormationDimensions {
                            kind: s.kind,
                            occurrence: OccurrenceAssertion::Unknown,
                            state: None,
                            change: FormationChange::Initial,
                            conditions: vec![],
                            event_time: None,
                            valid_time: None,
                            counter_sources: vec![],
                            derived_from: vec![],
                            reverts_to: None,
                        },
                        event_time: s.event_time.clone(),
                        valid_time: s.valid_time.clone(),
                        note: s.note.clone(),
                        prompt_version: PROMPT_VERSION.into(),
                        model_version: Some(self.analyst_model.clone()),
                        entities: s
                            .entities
                            .iter()
                            .enumerate()
                            .map(|(k, e)| StatementEntity {
                                entity_id: self.entity_id(n, k),
                                role: e.role,
                            })
                            .collect(),
                    })
                    .collect();
                (
                    (self.request.conversation_id.clone(), t.message_id.clone()),
                    statements,
                )
            })
            .collect()
    }
}

/// Morii's connector writer for one leased Vault. Not reachable from a client
/// without the remote service's own authorization (ADR-MEM-158).
pub struct ConnectorWriter {
    vault: Arc<SegmentedAutomationVault>,
    owner: ActorRef,
    producer: ActorRef,
    config: ConnectorConfig,
    analyzer: SubmittedAnalyzer,
}

impl ConnectorWriter {
    /// The embedding native service authenticates Morii; `producer` is the
    /// connector's own Client principal.
    pub fn new(
        vault: Arc<SegmentedAutomationVault>,
        owner: ActorRef,
        producer: ActorRef,
        config: ConnectorConfig,
    ) -> ApplicationResult<Self> {
        if owner != vault.descriptor().created_by
            || owner.actor_type != ActorType::Owner
            || producer.actor_type != ActorType::Client
            || !(1..=MAX_CAPTURE_WINDOW_DAYS).contains(&config.window_days)
            || config.allowed_forms.is_empty()
        {
            return Err(ApplicationFault::Invalid);
        }
        Ok(Self {
            vault,
            owner,
            producer,
            config,
            analyzer: SubmittedAnalyzer::default(),
        })
    }
    pub fn config(&self) -> &ConnectorConfig {
        &self.config
    }
    /// ADR-MEM-183: current memories formed under `policy` from these turns.
    fn formed(
        &self,
        policy: &AutomationPolicy,
        turns: &[TurnSource],
    ) -> ApplicationResult<Vec<FormedMemory>> {
        let sources: std::collections::BTreeSet<_> =
            turns.iter().map(|t| &t.source.source_id).collect();
        Ok(self.vault.read_scoped(
            &self.vault.descriptor().system_actor,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| {
                d.memories
                    .iter()
                    .filter(|m| {
                        let input = &m.memory.decision.input;
                        input.policy.policy_id == policy.policy_id
                            && input
                                .input_refs
                                .iter()
                                .any(|r| sources.contains(&r.source_id))
                    })
                    .map(|m| FormedMemory {
                        memory_id: m.memory.memory.memory_id.clone(),
                        revision: m.memory.memory.revision,
                        title: m
                            .memory
                            .decision
                            .input
                            .record_fields
                            .as_ref()
                            .map(|f| f.title.clone())
                            .unwrap_or_default(),
                    })
                    .collect()
            },
        )?)
    }
    /// ADR-MEM-187: the recorded source of each turn of a captured request,
    /// in turn order, found by conversation and message under `policy`.
    fn recorded_turns(
        &self,
        request: &RuntimeCaptureRequest,
        policy: &AutomationPolicy,
    ) -> ApplicationResult<Vec<TurnSource>> {
        Ok(self.vault.read_scoped(
            &self.vault.descriptor().system_actor,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| {
                request
                    .turns
                    .iter()
                    .filter_map(|t| {
                        d.sources
                            .values()
                            .find(|s| {
                                policy.source_ids.contains(&s.record.source_id)
                                    && s.record.original_conversation_id.as_deref()
                                        == Some(request.conversation_id.as_str())
                                    && s.record.original_message_id.as_deref()
                                        == Some(t.message_id.as_str())
                            })
                            .map(|s| TurnSource {
                                message_id: t.message_id.clone(),
                                source: s.reference(),
                            })
                    })
                    .collect()
            },
        )?)
    }
    /// ADR-MEM-186: message IDs of `request` that this connector already
    /// captured in the same conversation, its original or any segment.
    /// ADR-MEM-191: each comes with whether the recorded text is the same
    /// (`true`, a resend) or differs (`false`, the ID reused for a new message).
    pub fn resent_turns(
        &self,
        request: &RuntimeCaptureRequest,
    ) -> ApplicationResult<Vec<(String, bool)>> {
        let recorded = self.recorded_messages(&request.conversation_id)?;
        Ok(request
            .turns
            .iter()
            .filter_map(|t| {
                recorded
                    .get(&t.message_id)
                    .map(|text| (t.message_id.clone(), *text == t.text))
            })
            .collect())
    }
    /// Message ID to text of every turn this connector captured in the
    /// conversation, its original or any segment.
    fn recorded_messages(
        &self,
        conversation_id: &str,
    ) -> ApplicationResult<std::collections::BTreeMap<String, String>> {
        let base = conversation_id;
        let segment = format!("{base}#");
        Ok(self.vault.read_scoped(
            &self.vault.descriptor().system_actor,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| {
                let mut found = std::collections::BTreeMap::new();
                for c in d.conversations.values().filter(|c| {
                    c.platform == CAPTURE_PLATFORM
                        && c.account_scope == self.config.account_scope
                        && c.original_conversation_id.as_deref().is_some_and(|id| {
                            id == base
                                || id
                                    .strip_prefix(&segment)
                                    .is_some_and(|p| p.starts_with("pol_"))
                        })
                }) {
                    for t in &c.turns {
                        if let Some(s) = d.sources.get(&t.source.source_id)
                            && let Some(m) = &s.record.original_message_id
                        {
                            found.insert(m.clone(), s.text.clone());
                        }
                    }
                }
                found
            },
        )?)
    }
    /// ADR-MEM-181/186: every reason this submission would be refused before
    /// the Vault: its own checks, then turns already submitted.
    /// ADR-MEM-193: also a request_id reused for different content, and
    /// parents that are not turns of this conversation.
    pub fn submission_problems(&self, submission: &ConnectorSubmission) -> Vec<String> {
        let mut out = submission.problems(&self.config.allowed_forms);
        let replay = self
            .vault
            .capture_policy_for_request(&self.owner, &self.producer, &submission.request.request_id)
            .ok()
            .flatten();
        let changed = || {
            let recorded = self
                .recorded_messages(&submission.request.conversation_id)
                .unwrap_or_default();
            submission
                .request
                .turns
                .iter()
                .any(|t| recorded.get(&t.message_id) != Some(&t.text))
        };
        if replay.is_some() && out.is_empty() && changed() {
            out.push("this request_id was already used for a different submission; resubmit that submission unchanged to replay it, or use a new request_id for new turns".into());
        }
        if replay.is_none() {
            let request = &submission.request;
            let recorded = self
                .recorded_messages(&request.conversation_id)
                .unwrap_or_default();
            let mut earlier = std::collections::BTreeSet::new();
            for t in &request.turns {
                if let Some(p) = &t.parent_message_id
                    && !earlier.contains(p)
                    && !recorded.contains_key(p)
                {
                    out.push(format!(
                        "turn {} names parent_message_id {p}, which is not a turn of this conversation; leave parent_message_id out",
                        t.message_id
                    ));
                }
                earlier.insert(&t.message_id);
            }
            for (m, same) in self.resent_turns(request).unwrap_or_default() {
                out.push(if same {
                    format!(
                        "turn {m} was already submitted in this conversation; send only new turns (resubmitting the same request_id replays)"
                    )
                } else {
                    format!(
                        "message_id {m} already names a different message in this conversation; give this new message an ID never used in this conversation before, for example the date and its position"
                    )
                });
            }
        }
        out
    }
    /// ADR-MEM-185: capture permits cover one window, and a conversation's
    /// earlier turns must be in it. When this conversation already has turns
    /// under another window, the submission continues it here as a segment
    /// named `<conversation_id>#<window policy>`; a parent outside the segment
    /// is dropped. The rule depends only on recorded state and the window, so
    /// a replay rebuilds the same request.
    ///
    /// ADR-MEM-192: a new request whose sequences overlap the target
    /// conversation's recorded turns continues after them: its sequences
    /// shift past the last recorded one, and its first turn, if parentless,
    /// follows the last recorded turn. A replay takes each turn's sequence
    /// and parent from what was recorded for it.
    fn segment(
        &self,
        submission: &ConnectorSubmission,
        policy: &AutomationPolicy,
        replay: bool,
    ) -> ApplicationResult<ConnectorSubmission> {
        let original = submission.request.conversation_id.as_str();
        let suffix = format!("#{}", policy.policy_id);
        let segment_id: String = original
            .chars()
            .take(256 - suffix.chars().count())
            .chain(suffix.chars())
            .collect();
        let (elsewhere, known, recorded) = self.vault.read_scoped(
            &self.vault.descriptor().system_actor,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| {
                let mut elsewhere = false;
                let mut known = std::collections::BTreeSet::new();
                // Per conversation: (message ID, sequence, parent message ID).
                let mut recorded: std::collections::BTreeMap<
                    String,
                    Vec<(String, u32, Option<String>)>,
                > = Default::default();
                let message = |source: &enouia_memory_contract::ids::SourceId| {
                    d.sources
                        .get(source)
                        .and_then(|s| s.record.original_message_id.clone())
                };
                for c in d.conversations.values().filter(|c| {
                    c.platform == CAPTURE_PLATFORM && c.account_scope == self.config.account_scope
                }) {
                    if let Some(id) = c
                        .original_conversation_id
                        .as_deref()
                        .filter(|id| *id == original || *id == segment_id)
                    {
                        recorded.entry(id.to_owned()).or_default().extend(
                            c.turns.iter().filter_map(|t| {
                                Some((
                                    message(&t.source.source_id)?,
                                    t.sequence,
                                    t.parent_source.as_ref().and_then(|p| message(&p.source_id)),
                                ))
                            }),
                        );
                    }
                    match c.original_conversation_id.as_deref() {
                        Some(id) if id == original => {
                            elsewhere |= c
                                .turns
                                .iter()
                                .any(|t| !policy.source_ids.contains(&t.source.source_id));
                        }
                        Some(id) if id == segment_id => {
                            known.extend(c.turns.iter().filter_map(|t| {
                                d.sources
                                    .get(&t.source.source_id)
                                    .and_then(|s| s.record.original_message_id.clone())
                            }));
                        }
                        _ => {}
                    }
                }
                (elsewhere, known, recorded)
            },
        )?;
        let mut out = submission.clone();
        if elsewhere {
            let own: std::collections::BTreeSet<_> = out
                .request
                .turns
                .iter()
                .map(|t| t.message_id.clone())
                .collect();
            out.request.conversation_id = segment_id;
            for t in &mut out.request.turns {
                if t.parent_message_id
                    .as_ref()
                    .is_some_and(|p| !own.contains(p) && !known.contains(p))
                {
                    t.parent_message_id = None;
                }
            }
        }
        let target = recorded
            .get(&out.request.conversation_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let turns = &mut out.request.turns;
        if replay {
            for t in turns.iter_mut() {
                if let Some((_, sequence, parent)) = target.iter().find(|r| r.0 == t.message_id) {
                    t.sequence = *sequence;
                    t.parent_message_id = parent.clone();
                }
            }
        } else if let (Some(last), Some(first)) = (
            target.iter().max_by_key(|r| r.1),
            turns.iter().map(|t| t.sequence).min(),
        ) && first <= last.1
        {
            let shift = last.1 + 1 - first;
            for t in turns.iter_mut() {
                if t.sequence == first && t.parent_message_id.is_none() {
                    t.parent_message_id = Some(last.0.clone());
                }
                t.sequence += shift;
            }
        }
        Ok(out)
    }
    /// ADR-MEM-184: the current target of `memory` and the current revision
    /// of `source`, for a correction or forget request. `None` when either is
    /// unknown here; the request's own permit decides what it may cite.
    pub fn request_refs(
        &self,
        memory: &MemoryId,
        source: &enouia_memory_contract::ids::SourceId,
    ) -> ApplicationResult<Option<(TargetRef, SourceRevisionRef)>> {
        Ok(self.vault.read_scoped(
            &self.vault.descriptor().system_actor,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| {
                let target = d
                    .memories
                    .iter()
                    .find(|m| &m.memory.memory.memory_id == memory)
                    .map(|m| m.target.clone())?;
                let source = d.sources.get(source).map(|s| s.reference())?;
                Some((target, source))
            },
        )?)
    }
    /// ADR-MEM-174: the connector's own access and egress domain, stable for
    /// this Vault and account scope, so its windows can be synthesized together.
    pub fn domain(&self) -> (PolicyId, PolicyId) {
        let id = |part: &str| {
            PolicyId::from_random(seed16(&serde_json::json!([
                "connector-domain-1",
                self.vault.descriptor().vault_id,
                self.config.account_scope,
                part
            ])))
        };
        (id("access"), id("egress"))
    }
    /// ADR-MEM-174: forms a theme over current daily memories of this
    /// connector's domain, under a fresh synthesis policy that lists exactly
    /// their sources. Preflight writes nothing. A resubmitted request replays.
    /// ADR-MEM-203: why each member cannot join this connector's theme.
    pub fn theme_problems(&self, theme: &ConnectorTheme) -> Vec<String> {
        let domain = self.domain();
        let subject = self.vault.descriptor().owner_subject_id.clone();
        self.vault
            .read_scoped(
                &self.vault.descriptor().system_actor,
                self.config.max_sensitivity,
                SystemReadScope::MoriiFullHistory,
                |d| {
                    theme
                        .members
                        .iter()
                        .filter_map(|id| {
                            let Some(m) = d.memories.iter().find(|m| &m.memory.memory.memory_id == id) else {
                                return Some(format!("member {id} is not a current memory visible to this connector"));
                            };
                            let i = &m.memory.decision.input;
                            if (i.access_policy_id.clone(), i.egress_policy_id.clone()) != domain
                                || Some(&i.subject_id) != subject.as_ref()
                                || i.project_id.is_some()
                            {
                                return Some(format!("member {id} was not submitted through this connector; a theme joins only memories this connector submitted"));
                            }
                            i.record_fields
                                .as_ref()
                                .and_then(|f| f.evidence.first())
                                .is_none()
                                .then(|| format!("member {id} has no quoted evidence"))
                        })
                        .collect()
                },
            )
            .unwrap_or_default()
    }
    pub fn submit_theme(
        &self,
        theme: &ConnectorTheme,
        preflight: bool,
        now_ms: i64,
    ) -> ApplicationResult<ThemeReceipt> {
        if !theme.valid() {
            return Err(ApplicationFault::Invalid);
        }
        let descriptor = self.vault.descriptor();
        let system = descriptor.system_actor.clone();
        let subject = descriptor
            .owner_subject_id
            .clone()
            .ok_or(ApplicationFault::Invalid)?;
        let domain = self.domain();
        let policy_id = PolicyId::from_random(seed16(&serde_json::json!([
            "connector-theme-policy-1",
            theme.request_id
        ])));
        // `prepare` assigns the real result; replays are found by policy.
        let result = MemoryRevisionRef {
            memory_id: MemoryId::from_random(seed16(&serde_json::json!([
                "connector-theme-memory-1",
                theme.request_id
            ]))),
            revision: Revision::new(1).unwrap(),
        };
        struct Member {
            target: TargetRef,
            sensitivity: Sensitivity,
            refs: Vec<SourceRevisionRef>,
            evidence: QuotedEvidence,
            source: CapturedSource,
        }
        type Read = Result<Vec<Member>, MemoryRevisionRef>;
        let read = self.vault.read_scoped(
            &system,
            self.config.max_sensitivity,
            SystemReadScope::MoriiFullHistory,
            |d| -> ApplicationResult<Read> {
                // The theme formed under this request's synthesis policy, if any.
                if let Some(m) = d
                    .memories
                    .iter()
                    .find(|m| m.memory.decision.input.policy.policy_id == policy_id)
                {
                    return Ok(Err(m.memory.memory.clone()));
                }
                theme
                    .members
                    .iter()
                    .map(|id| {
                        let m = d
                            .memories
                            .iter()
                            .find(|m| &m.memory.memory.memory_id == id)
                            .ok_or(ApplicationFault::Invalid)?;
                        let i = &m.memory.decision.input;
                        // ADR-MEM-203: a member without record fields (an
                        // imported memory, say) is refused, never a panic.
                        let evidence = i
                            .record_fields
                            .as_ref()
                            .and_then(|f| f.evidence.first())
                            .ok_or(ApplicationFault::Invalid)?;
                        if (i.access_policy_id.clone(), i.egress_policy_id.clone()) != domain
                            || i.subject_id != subject
                            || i.project_id.is_some()
                        {
                            return Err(ApplicationFault::Invalid);
                        }
                        let source = d
                            .sources
                            .get(&evidence.reference.source_id)
                            .ok_or(ApplicationFault::Invalid)?;
                        Ok(Member {
                            target: m.target.clone(),
                            sensitivity: i.sensitivity,
                            refs: i.input_refs.clone(),
                            evidence: evidence.clone(),
                            source: (*source).clone(),
                        })
                    })
                    .collect::<ApplicationResult<Vec<_>>>()
                    .map(Ok)
            },
        )??;
        let members = match read {
            Err(formed) => {
                return Ok(ThemeReceipt {
                    memory: Some(formed),
                    replayed: true,
                    preflight,
                });
            }
            Ok(members) => members,
        };
        let refs: std::collections::BTreeSet<SourceRevisionRef> = members
            .iter()
            .flat_map(|m| m.refs.iter().cloned())
            .collect();
        let source_ids: std::collections::BTreeSet<_> =
            refs.iter().map(|r| r.source_id.clone()).collect();
        if source_ids.len() > MAX_SOURCES {
            return Err(ApplicationFault::Invalid);
        }
        if preflight {
            return Ok(ThemeReceipt {
                memory: None,
                replayed: false,
                preflight: true,
            });
        }
        let at =
            |offset: i64| Timestamp::from_unix_ms(now_ms + offset).ok_or(ApplicationFault::Invalid);
        let existing = self
            .vault
            .owner_read_status(&self.owner)?
            .policies
            .into_iter()
            .find(|p| p.policy_id == policy_id);
        let policy = match existing {
            Some(policy) => policy,
            None => {
                let policy = AutomationPolicy {
                    schema_version: NextVersion,
                    kind: PolicyKind::AutomationPolicy,
                    policy_id,
                    revision: Revision::new(1).unwrap(),
                    rule_version: RuleVersion::V3,
                    system_actor: system.clone(),
                    vault_id: descriptor.vault_id.clone(),
                    subject_id: subject,
                    project_id: None,
                    source_ids: source_ids.into_iter().collect(),
                    allow_create: true,
                    allow_revise_created: false,
                    target_ids: vec![],
                    allowed_forms: vec![FormationKind::Theme],
                    max_sensitivity: self.config.max_sensitivity,
                    access_policy_id: domain.0.clone(),
                    egress_policy_id: domain.1.clone(),
                    active: true,
                    valid_from: at(0)?,
                    valid_until: at(86_400_000)?,
                };
                self.vault.set_policy(&self.owner, policy.clone())?;
                policy
            }
        };
        use enouia_memory_contract::ports::IdSource;
        let ids = enouia_memory_vault::OsIdSource;
        let mut input = formation_seed(FormationSeed {
            policy: &policy,
            source: &members[0].source,
            evidence: members[0].evidence.clone(),
            snapshot_commit: CommitId::from_random(ids.random_16()),
            operation_id: OperationId::from_random(ids.random_16()),
            result: result.clone(),
            title: &theme.title,
            interpretation: Interpretation::Inference,
            dimensions: FormationDimensions {
                kind: FormationKind::Theme,
                occurrence: OccurrenceAssertion::Unknown,
                state: None,
                change: FormationChange::Synthesis,
                conditions: vec![],
                event_time: None,
                valid_time: None,
                counter_sources: vec![],
                derived_from: members.iter().map(|m| m.target.clone()).collect(),
                reverts_to: None,
            },
            event_time: unknown_time(),
            valid_time: unknown_time(),
            entities: vec![],
            prompt_version: THEME_PROMPT_VERSION,
            model_version: Some(theme.analyst_model.clone()),
            import_batch_id: None,
            mode: FormationMode::Live,
            annotations: Some(FormationAnnotations {
                memory: result.clone(),
                model_version: Some(theme.analyst_model.clone()),
                prompt_version: THEME_PROMPT_VERSION.into(),
                formation_note: theme.note.clone(),
                suggested_tags: vec![],
                tone: None,
                importance_guess_milli: None,
                current_state_note: None,
                suggested_relations: vec![],
            }),
        })
        .ok_or(ApplicationFault::Invalid)?;
        input.content = theme.summary.clone();
        input.input_refs = refs.into_iter().collect();
        input.evidence = members
            .iter()
            .map(|m| m.evidence.reference.clone())
            .collect();
        input.sensitivity = members
            .iter()
            .map(|m| m.sensitivity)
            .max()
            .unwrap_or(Sensitivity::Public);
        let fields = input.record_fields.as_mut().unwrap();
        fields.title = theme.title.clone();
        fields.evidence = members.iter().map(|m| m.evidence.clone()).collect();
        let input = self.vault.prepare(&system, input)?;
        let response = serde_json::to_string(&self.vault.evaluate(&system, &input)?)
            .map_err(|_| ApplicationFault::Invalid)?;
        let binding = GenerationBinding {
            operation_id: input.operation_id.clone(),
            input_hash: digest(&input),
            response_hash: enouia_memory_contract::hash::sha256(response.as_bytes()),
        };
        let receipt = self
            .vault
            .apply_batch(&system, vec![input], vec![response], vec![binding])?
            .remove(0);
        Ok(ThemeReceipt {
            memory: Some(receipt.result),
            replayed: false,
            preflight: false,
        })
    }
    /// Preflight writes nothing. Otherwise: choose or open a capture window,
    /// capture, enqueue, then drive formation for this submission and report
    /// its jobs. A resubmitted request replays under its original window.
    pub fn submit(
        &self,
        submission: &ConnectorSubmission,
        preflight: bool,
        now_ms: i64,
    ) -> ApplicationResult<ConnectorReceipt> {
        let at =
            |offset: i64| Timestamp::from_unix_ms(now_ms + offset).ok_or(ApplicationFault::Invalid);
        if !submission.valid(&self.config.allowed_forms) {
            return Err(ApplicationFault::Invalid);
        }
        let request = &submission.request;
        let turns = request.turns.len();
        let domain = self.domain();
        let replay = self.vault.capture_policy_for_request(
            &self.owner,
            &self.producer,
            &request.request_id,
        )?;
        // ADR-MEM-186: a new request must not resend turns this conversation
        // already holds; the same request_id replays instead.
        if replay.is_none() && !self.resent_turns(request)?.is_empty() {
            return Err(ApplicationFault::Invalid);
        }
        let replay_found = replay.is_some();
        let policy = match replay {
            Some(policy) => policy,
            None => {
                let needed_until = at(2 * PERMIT_MS)?;
                let open = self
                    .vault
                    .capture_windows(&self.owner)?
                    .into_iter()
                    .find(|w| {
                        w.unused_source_ids.len() >= turns
                            && w.policy.access_policy_id == domain.0
                            && w.policy.egress_policy_id == domain.1
                            && w.policy.valid_until > needed_until
                            && w.policy.max_sensitivity == self.config.max_sensitivity
                            && w.policy.allowed_forms == self.config.allowed_forms
                    })
                    .map(|w| w.policy);
                match open {
                    Some(policy) => policy,
                    None if preflight => {
                        return Ok(ConnectorReceipt {
                            capture: None,
                            queue: None,
                            jobs: vec![],
                            window_needed: true,
                            window_ended: false,
                            memories: vec![],
                            turns: vec![],
                        });
                    }
                    None => {
                        self.vault
                            .open_capture_window_in(
                                &self.owner,
                                self.config.window_days,
                                self.config.max_sensitivity,
                                self.config.allowed_forms.clone(),
                                domain.clone(),
                            )?
                            .policy
                    }
                }
            }
        };
        // ADR-MEM-185: a conversation already captured under another window
        // continues here as its own segment.
        let segmented = self.segment(submission, &policy, replay_found)?;
        let submission = &segmented;
        let request = &submission.request;
        // ADR-MEM-187: a replay whose window has ended cannot get a capture
        // permit; answer from what that capture recorded.
        if replay_found && (!policy.active || policy.valid_until <= at(0)?) {
            let turns = self.recorded_turns(request, &policy)?;
            return Ok(ConnectorReceipt {
                capture: None,
                queue: None,
                jobs: vec![],
                window_needed: false,
                window_ended: true,
                memories: self.formed(&policy, &turns)?,
                turns,
            });
        }
        let permit = self.vault.authorize_owner_runtime_capture(
            &self.owner,
            self.producer.clone(),
            RuntimeCaptureScope {
                policy_id: policy.policy_id.clone(),
                source_ids: policy.source_ids.clone(),
                max_sensitivity: self.config.max_sensitivity,
                platform: CAPTURE_PLATFORM.into(),
                account_scope: self.config.account_scope.clone(),
                default_timezone: self.config.default_timezone.clone(),
                valid_until: at(PERMIT_MS)?,
            },
        )?;
        if preflight {
            let capture = self.vault.capture_runtime_with_entities(
                &permit,
                request,
                &submission.capture_entities(),
                true,
            );
            permit.revoke();
            return Ok(ConnectorReceipt {
                capture: Some(capture?),
                queue: None,
                jobs: vec![],
                window_needed: false,
                window_ended: false,
                memories: vec![],
                turns: vec![],
            });
        }
        self.analyzer
            .0
            .lock()
            .map_err(|_| ApplicationFault::Locked)?
            .extend(submission.statements_by_turn());
        let capture = self.vault.capture_runtime_with_entities(
            &permit,
            request,
            &submission.capture_entities(),
            false,
        );
        let queue = capture.as_ref().ok().map(|c| {
            self.vault
                .enqueue_captured(&self.vault.descriptor().system_actor, c, permit.policy())
        });
        permit.revoke();
        let capture = capture?;
        let queue = queue.transpose()?;
        let mut formation =
            CapturedFormation::open(self.vault.clone(), &self.vault.descriptor().system_actor)?;
        let mut analyzer = self.analyzer.clone();
        for _ in 0..MAX_STEPS {
            if !formation.step(&mut analyzer)? {
                break;
            }
        }
        let keys = queue
            .as_ref()
            .map(|q| q.job_keys.clone())
            .unwrap_or_default();
        let turn_sources: Vec<TurnSource> = request
            .turns
            .iter()
            .zip(&capture.sources)
            .map(|(t, s)| TurnSource {
                message_id: t.message_id.clone(),
                source: s.clone(),
            })
            .collect();
        let memories = self.formed(&policy, &turn_sources)?;
        Ok(ConnectorReceipt {
            memories,
            window_ended: false,
            turns: turn_sources,
            jobs: formation
                .status()
                .into_iter()
                .filter(|j| keys.contains(&j.job_key))
                .map(|j| ProducerJobStatus {
                    job_key: j.job_key,
                    state: j.state,
                    receipts: j.receipts.len(),
                    refused: j.refused,
                })
                .collect(),
            capture: Some(capture),
            queue,
            window_needed: false,
        })
    }
}

fn unknown_time() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
