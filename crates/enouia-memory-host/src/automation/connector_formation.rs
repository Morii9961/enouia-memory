//! ADR-MEM-157/158: daily memories distilled by ChatGPT and submitted through
//! the remote connector. Memory only validates and stores: the submitted turns
//! are captured under a Morii capture window, and the captured formation step
//! forms exactly the submitted statements under its existing checks. Uses the
//! platform-neutral Vault API, so it runs wherever the remote service runs.
use super::application::{ApplicationFault, ApplicationResult};
use enouia_memory_contract::{
    common::{ActorRef, ActorType, Sensitivity, SpeakerRole},
    next::*,
    time::Timestamp,
};
use enouia_memory_vault::automation::segmented::{
    CaptureAnalysis, CaptureAnalyzer, CaptureStatement, CaptureStopToken, CapturedFormation,
    CapturedQueueReceipt, MAX_CAPTURE_WINDOW_DAYS, NativeRuntimeCapture, PendingCaptureAnalysis,
    ProducerJobStatus, QueuedCapture, RuntimeCaptureEntity, RuntimeCaptureScope,
    SegmentedAutomationVault, StatementEntity,
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
    /// Checks the submission against itself before anything reaches the Vault.
    /// The Vault and the formation step repeat their own checks.
    pub fn valid(&self) -> bool {
        let text_ok = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
        self.request.valid()
            && text_ok(&self.analyst_model, 128)
            && self.statements.len() <= MAX_STATEMENTS
            && self.statements.iter().all(|s| {
                text_ok(&s.title, 128)
                    && text_ok(&s.quote, 2048)
                    && s.note.as_ref().is_none_or(|n| text_ok(n, 512))
                    && s.entities.len() <= MAX_STATEMENT_ENTITIES
                    && s.entities.iter().all(|e| text_ok(&e.name, 128))
                    && self.request.turns.iter().any(|t| {
                        t.message_id == s.message_id
                            && t.speaker == SpeakerRole::User
                            && t.text.contains(&s.quote)
                    })
            })
    }
    /// A fresh, deterministic entity ID per statement and position, so a
    /// resubmitted request names the entities its first capture recorded.
    fn entity_id(&self, statement: usize, entity: usize) -> String {
        let digest = enouia_memory_contract::hash::sha256(
            &enouia_memory_contract::json::canonical_bytes(&serde_json::json!([
                "connector-entity-1",
                self.request.request_id,
                statement,
                entity
            ]))
            .expect("closed entity seed"),
        );
        let mut bytes = [0; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&digest.as_str()[2 * i..2 * i + 2], 16).unwrap_or(0);
        }
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
        if !submission.valid() {
            return Err(ApplicationFault::Invalid);
        }
        let request = &submission.request;
        let turns = request.turns.len();
        let replay = self.vault.capture_policy_for_request(
            &self.owner,
            &self.producer,
            &request.request_id,
        )?;
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
                        });
                    }
                    None => {
                        self.vault
                            .open_capture_window(
                                &self.owner,
                                self.config.window_days,
                                self.config.max_sensitivity,
                                self.config.allowed_forms.clone(),
                            )?
                            .policy
                    }
                }
            }
        };
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
        Ok(ConnectorReceipt {
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
