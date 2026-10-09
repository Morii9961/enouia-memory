//! Inspector -> durable admission -> one attempt -> persisted stream/outcome.
use crate::{
    Result,
    codec::{Api, WireRequest, decode, encode},
    egress::SendGuard,
    error,
    stream::Decoder,
    transport::Transport,
};
use enouia_memory_contract::{
    MemoryError, MemoryErrorCode,
    context::{DispatchRecord, DispatchState},
    foundation::Cancellation,
    hash::Sha256Hex,
    ports::SecretStore,
    provider::{
        Capability, ProviderCapabilities, ProviderRequest, ProviderResponse, TokenCounting,
    },
};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub safety_margin_tokens: u64,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 128 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
            safety_margin_tokens: 1024,
            timeout: Duration::from_secs(60),
        }
    }
}

pub struct PreparedCall {
    pub(crate) wire: WireRequest,
    pub(crate) dispatch: DispatchRecord,
    pub(crate) estimated_input_tokens: u64,
    pub(crate) limits: Limits,
}
impl PreparedCall {
    pub fn inspect(&self) -> &WireRequest {
        &self.wire
    }
    pub fn dispatch(&self) -> &DispatchRecord {
        &self.dispatch
    }
    pub fn estimated_input_tokens(&self) -> u64 {
        self.estimated_input_tokens
    }
}

/// Preparation is offline. Capabilities are supplied from explicit account /
/// model verification; no capability or budget is guessed from a model name.
pub fn prepare(
    api: Api,
    request: &ProviderRequest,
    dispatch: &DispatchRecord,
    capabilities: &ProviderCapabilities,
    limits: Limits,
) -> Result<PreparedCall> {
    if !dispatch.validate().is_empty()
        || !request.verify_against(dispatch).is_empty()
        || dispatch.state != DispatchState::Prepared
        || !capabilities.validate().is_empty()
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let wire = encode(api, request)?;
    if capabilities.binding != wire.binding
        || capabilities.text_input != Capability::Supported
        || capabilities.token_counting != TokenCounting::Estimated
        || !capabilities.budget_supported()
        || request.output.streaming && capabilities.streaming != Capability::Supported
    {
        return Err(error(MemoryErrorCode::UnsupportedBudget));
    }
    // Explicit estimate of the COMPLETE wire envelope in UTF-8 bytes. This is
    // deliberately not advertised as an exact tokenizer or a proved bound.
    let estimate = (wire.body.len() as u64)
        .checked_add(limits.safety_margin_tokens)
        .ok_or_else(|| error(MemoryErrorCode::BudgetExceeded))?;
    let needed = estimate.checked_add(request.output.max_output_tokens);
    if wire.body.len() > limits.max_input_bytes
        || limits.max_response_bytes == 0
        || needed.is_none_or(|n| n > capabilities.context_window_tokens.unwrap_or(0))
        || request.output.max_output_tokens > capabilities.max_output_tokens.unwrap_or(0)
        || limits.timeout.is_zero()
        || limits.timeout > Duration::from_secs(300)
    {
        return Err(error(MemoryErrorCode::BudgetExceeded));
    }
    Ok(PreparedCall {
        wire,
        dispatch: dispatch.clone(),
        estimated_input_tokens: estimate,
        limits,
    })
}

/// Host journal implementations MUST durably and atomically claim a dispatch
/// (and its single-use approval) before returning true. false means an earlier
/// attempt exists or the turn is already closed; MUST NOT send HTTP. Persist every chunk
/// before exposing it, and a terminal outcome before returning to the caller.
pub trait InvocationJournal {
    fn claim(&self, call: &PreparedCall) -> Result<bool>;
    fn chunk(&self, dispatch: &DispatchRecord, sequence: u64, text: &str) -> Result<()>;
    fn finish(
        &self,
        dispatch: &DispatchRecord,
        response: Option<&ProviderResponse>,
        failure: Option<MemoryError>,
    ) -> Result<()>;
}

pub struct Client<'a> {
    pub transport: &'a dyn Transport,
    pub secrets: &'a dyn SecretStore,
    pub guard: &'a dyn SendGuard,
    pub journal: &'a dyn InvocationJournal,
}
impl Client<'_> {
    /// approved_wire_hash comes from a trusted native inspection confirmation;
    /// it is separate from the logical digest in the existing EgressApproval.
    /// Network attempts are never automatically retried, including on timeout.
    pub fn send(
        &self,
        call: &PreparedCall,
        approved_wire_hash: &Sha256Hex,
        cancellation: &dyn Cancellation,
    ) -> Result<ProviderResponse> {
        if approved_wire_hash != &call.wire.hash() {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        if cancellation.is_cancelled() {
            return Err(error(MemoryErrorCode::Cancelled));
        }
        self.guard.check(&call.dispatch, false)?;
        let secret = self.secrets.read(match call.wire.api {
            Api::OpenAiResponses => "Enouia.Memory.Provider.openai",
            Api::AnthropicMessages => "Enouia.Memory.Provider.anthropic",
        })?;
        // No state is consumed by a missing credential. Admission failures,
        // audit failures and retry races cannot escape this send boundary.
        if cancellation.is_cancelled() {
            return Err(error(MemoryErrorCode::Cancelled));
        }
        self.guard.check(&call.dispatch, true)?;
        if !self.journal.claim(call)? {
            let mut err = error(MemoryErrorCode::IdempotencyConflict);
            err.retryable = false;
            return Err(err);
        }
        let mut body = vec![];
        let mut total = 0usize;
        let mut sequence = 0;
        let mut decoder = Decoder::new(call.wire.api);
        let result = (|| {
            self.guard.check(&call.dispatch, false)?;
            if cancellation.is_cancelled() {
                return Err(error(MemoryErrorCode::Cancelled));
            }
            self.transport.exchange(
                &call.wire,
                &secret,
                cancellation,
                call.limits.timeout,
                &mut |bytes| {
                    if cancellation.is_cancelled() {
                        return Err(error(MemoryErrorCode::Cancelled));
                    }
                    self.guard.check(&call.dispatch, false)?;
                    total = total
                        .checked_add(bytes.len())
                        .ok_or_else(|| error(MemoryErrorCode::BudgetExceeded))?;
                    if total > call.limits.max_response_bytes {
                        return Err(error(MemoryErrorCode::BudgetExceeded));
                    }
                    if call.wire.streaming {
                        decoder.push(bytes, &mut |text| {
                            sequence += 1;
                            self.journal.chunk(&call.dispatch, sequence, text)
                        })?;
                    } else {
                        body.extend_from_slice(bytes);
                    }
                    Ok(())
                },
            )?;
            if cancellation.is_cancelled() {
                return Err(error(MemoryErrorCode::Cancelled));
            }
            self.guard.check(&call.dispatch, false)?;
            if call.wire.streaming {
                decoder.finish()
            } else {
                decode(call.wire.api, &body)
            }
        })();
        match result {
            Ok(response) => {
                self.journal.finish(&call.dispatch, Some(&response), None)?;
                self.guard.check(&call.dispatch, false)?;
                Ok(response)
            }
            Err(mut err) => {
                // A retryable transport error does not authorize an egress retry.
                err.retryable = false;
                self.journal.finish(&call.dispatch, None, Some(err))?;
                Err(err)
            }
        }
    }
}
