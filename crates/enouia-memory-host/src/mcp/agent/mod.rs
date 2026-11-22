//! ADR-MEM-166: one Agent connection with the initialized read tools and the
//! note, correction-report and forget-request tools its grants allow.
//! It adds no authority: each write tool needs its own writer.
use super::{
    WriteCall, closed, corrections, error, forget_requests, initialized, notes,
    read_session::{ReadSession, Route},
    reply,
};
use crate::automation::{
    application::ApplicationFault,
    qualified_agent::{
        AgentCorrectionReporter, AgentForgetRequester, AgentNoteWriter, QualifiedAgentReader,
    },
};
use enouia_memory_contract::next::AttentionProfile;
use enouia_memory_vault::automation::AutomationFault;
use serde_json::{Value, json};
#[cfg(windows)]
pub mod native;

/// Write delegations for one Agent connection; each is optional.
#[derive(Clone, Copy, Default)]
pub struct AgentWriters<'a, 'host> {
    pub note: Option<&'a AgentNoteWriter<'host>>,
    pub correction: Option<&'a AgentCorrectionReporter<'host>>,
    pub forget: Option<&'a AgentForgetRequester<'host>>,
}
impl AgentWriters<'_, '_> {
    /// Every present writer's connection check, so an uncertain outcome or a
    /// grant change on any of them ends the whole connection.
    pub(crate) fn check_connection(&self) -> Result<(), ApplicationFault> {
        if let Some(w) = self.note {
            w.check_connection()?;
        }
        if let Some(w) = self.correction {
            w.check_connection()?;
        }
        if let Some(w) = self.forget {
            w.check_connection()?;
        }
        Ok(())
    }
    pub(crate) fn revoke(&self) {
        if let Some(w) = self.note {
            w.revoke();
        }
        if let Some(w) = self.correction {
            w.revoke();
        }
        if let Some(w) = self.forget {
            w.revoke();
        }
    }
}

pub struct Server<'a, 'host> {
    reader: &'a QualifiedAgentReader<'host>,
    writers: AgentWriters<'a, 'host>,
    reads: initialized::Server<'a, 'host>,
    session: ReadSession,
    uncertain: bool,
    original: Option<Value>,
}
impl<'a, 'host> Server<'a, 'host> {
    /// Trusted native embedding only; OS peer enrollment belongs to transport.
    /// Every writer must belong to the reader's principal and vault.
    pub fn new(
        reader: &'a QualifiedAgentReader<'host>,
        writers: AgentWriters<'a, 'host>,
        profile: &'a AttentionProfile,
    ) -> Option<Self> {
        let same = |principal: &enouia_memory_contract::common::ActorRef, shares: bool| {
            principal == reader.principal() && shares
        };
        if writers
            .note
            .is_some_and(|w| !same(w.principal(), w.shares_vault(reader)))
            || writers
                .correction
                .is_some_and(|w| !same(w.principal(), w.shares_vault(reader)))
            || writers
                .forget
                .is_some_and(|w| !same(w.principal(), w.shares_vault(reader)))
            || writers.check_connection().is_err()
        {
            return None;
        }
        Some(Self {
            reader,
            writers,
            reads: initialized::Server::new(reader, profile)?,
            session: ReadSession::new("enouia-memory-agent-v1", 4096),
            uncertain: false,
            original: None,
        })
    }
    pub fn closed(&self) -> bool {
        self.session.closed()
    }
    pub fn outcome_unknown(&self) -> bool {
        self.uncertain
    }
    pub fn original_unknown(&self) -> Option<&Value> {
        self.original.as_ref()
    }
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        if let Err(fault) = self.writers.check_connection() {
            self.session.close();
            if matches!(
                fault,
                ApplicationFault::Vault(AutomationFault::OutcomeUnknown)
            ) {
                self.uncertain = true;
                self.original.get_or_insert_with(|| message.clone());
                return None;
            }
        }
        if self.reader.check_connection().is_err() {
            self.session.close();
        }
        let (id, method, params) = match self.session.route(message) {
            Route::Response(response) => return response,
            Route::Ready { id, method, params } => (id, method, params),
        };
        let call = match method.as_str() {
            "tools/list" => {
                if !closed(&params, &["_meta"]) {
                    return Some(error(id, -32602, "invalid_tool_cursor"));
                }
                let mut tools = initialized::tools();
                if self.writers.note.is_some() {
                    tools.push(notes::tool());
                }
                if self.writers.correction.is_some() {
                    tools.push(corrections::tool());
                }
                if self.writers.forget.is_some() {
                    tools.push(forget_requests::tool());
                }
                return Some(reply(id, json!({"tools":tools})));
            }
            "tools/call" => match (&params["name"], self.writers) {
                (name, AgentWriters { note: Some(w), .. }) if name == "memory_note" => {
                    notes::call(w, id, &params)
                }
                (
                    name,
                    AgentWriters {
                        correction: Some(w),
                        ..
                    },
                ) if name == "memory_correction_report" => corrections::call(w, id, &params),
                (
                    name,
                    AgentWriters {
                        forget: Some(w), ..
                    },
                ) if name == "memory_forget_request" => forget_requests::call(w, id, &params),
                _ => {
                    let response = self.reads.call(id, &params);
                    if self.reads.closed() {
                        self.session.close();
                    }
                    return Some(response);
                }
            },
            _ => return Some(error(id, -32601, "method_not_found")),
        };
        match call {
            WriteCall::Reply(response, close) => {
                if close {
                    self.session.close();
                }
                Some(response)
            }
            WriteCall::Unknown => {
                self.uncertain = true;
                self.original = Some(message.clone());
                self.session.close();
                None
            }
        }
    }
}
