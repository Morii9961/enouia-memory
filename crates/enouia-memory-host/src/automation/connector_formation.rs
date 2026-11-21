//! ADR-MEM-157: daily memories distilled by ChatGPT and submitted through the
//! remote connector. Memory only validates and stores: the submitted turns are
//! captured through the owner Runtime capture path, and the captured formation
//! worker forms the submitted statements under its existing checks.
use super::application::{ApplicationFault, ApplicationResult};
use super::runtime_capture::RuntimeCaptureHost;
use enouia_memory_contract::{common::SpeakerRole, next::*};
use enouia_memory_vault::automation::segmented::{
    CaptureAnalysis, CaptureAnalyzer, CaptureStatement, CaptureStopToken, CapturedQueueReceipt,
    NativeRuntimeCapture, PendingCaptureAnalysis, QueuedCapture,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const PROMPT_VERSION: &str = "chatgpt-connector-authored-1";
pub const MAX_STATEMENTS: usize = 32;

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSubmission {
    pub request: RuntimeCaptureRequest,
    /// The model ChatGPT reports for its own analysis, recorded as-is.
    pub analyst_model: String,
    pub statements: Vec<ConnectorStatement>,
}

#[derive(Serialize)]
pub struct ConnectorReceipt {
    pub capture: NativeRuntimeCapture,
    pub queue: Option<CapturedQueueReceipt>,
    pub statements: usize,
}

type Key = (String, String);

/// A `CaptureAnalyzer` that returns exactly what was submitted for a turn. A
/// turn that was never registered in this process (for example after a
/// restart) pauses, so it stays queued until the same request is resubmitted.
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
    /// The Vault and the formation worker repeat their own checks.
    pub fn valid(&self) -> bool {
        let text_ok = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
        self.request.valid()
            && text_ok(&self.analyst_model, 128)
            && self.statements.len() <= MAX_STATEMENTS
            && self.statements.iter().all(|s| {
                text_ok(&s.title, 128)
                    && text_ok(&s.quote, 2048)
                    && s.note.as_ref().is_none_or(|n| text_ok(n, 512))
                    && self.request.turns.iter().any(|t| {
                        t.message_id == s.message_id
                            && t.speaker == SpeakerRole::User
                            && t.text.contains(&s.quote)
                    })
            })
    }
    fn statements_by_turn(&self) -> BTreeMap<Key, Vec<CaptureStatement>> {
        self.request
            .turns
            .iter()
            .map(|t| {
                let statements = self
                    .statements
                    .iter()
                    .filter(|s| s.message_id == t.message_id)
                    .map(|s| CaptureStatement {
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

/// Preflight writes nothing. Otherwise the statements are registered for the
/// running worker, then the turns are captured and enqueued; a replayed request
/// returns the original capture and does not enqueue twice.
pub fn submit(
    host: &RuntimeCaptureHost,
    analyzer: &SubmittedAnalyzer,
    submission: &ConnectorSubmission,
    preflight: bool,
) -> ApplicationResult<ConnectorReceipt> {
    if !submission.valid() {
        return Err(ApplicationFault::Invalid);
    }
    if preflight {
        return Ok(ConnectorReceipt {
            capture: host.preflight(&submission.request)?,
            queue: None,
            statements: submission.statements.len(),
        });
    }
    analyzer
        .0
        .lock()
        .map_err(|_| ApplicationFault::Locked)?
        .extend(submission.statements_by_turn());
    let capture = host.capture(&submission.request)?;
    let queue = (!capture.replayed)
        .then(|| host.enqueue(&capture))
        .transpose()?;
    Ok(ConnectorReceipt {
        capture,
        queue,
        statements: submission.statements.len(),
    })
}
