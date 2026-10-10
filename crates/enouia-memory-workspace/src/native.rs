//! Native text Provider hosting (ADR-MEM-48, MV-7). The host injects a
//! transport and a secret store with [`Workspace::set_native_provider`];
//! without them every external command answers `provider.not_configured` and
//! the page keeps the local Mock. Credentials, models and verified
//! capabilities stay native. The page sees the exact wire body it confirms,
//! never a key.
//!
//! Lifecycle: each open Vault has its own `enabled` flag. Close and lock turn
//! it off before cancelling and joining operations, so a late Provider result
//! cannot publish into a Vault the owner has closed.

use super::*;
use enouia_memory_contract::context::{Destination, DestinationKind, Purpose};
use enouia_memory_contract::ids::{EventId, VaultId};
use enouia_memory_contract::policy::{PolicyOrigin, PolicyRecord, PolicyStatus, ResourceSelector};
use enouia_memory_contract::ports::SecretStore;
use enouia_memory_contract::provider::{FinishReason, ProviderCapabilities};
use enouia_memory_contract::workspace::{ExternalApi, GrantSensitivity};
use enouia_memory_provider::client::{Client, Limits, PreparedCall};
use enouia_memory_provider::codec::Api;
use enouia_memory_provider::policy::{self, PolicyPlan};
use enouia_memory_provider::transport::Transport;
use enouia_memory_provider::vault::{CallOptions, InterruptionPlan, Quota, VaultAdapter};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

/// How long a prepared external call waits for the owner's confirmation.
/// It matches the single-use Egress approval it becomes.
pub const CALL_TTL_MS: i64 = 15 * 60 * 1000;

/// One API the host configured, with capabilities the owner verified for
/// that exact model. Nothing is inferred from a subscription or model name.
#[derive(Clone, Debug)]
pub struct NativeDestination {
    pub api: Api,
    pub capabilities: ProviderCapabilities,
}

/// What the host injects to enable native sending. Runtime passes
/// `HttpsTransport` and the Windows `CredentialStore`; tests pass fakes.
#[derive(Clone)]
pub struct NativeProvider {
    pub transport: Arc<dyn Transport + Send + Sync>,
    pub secrets: Arc<dyn SecretStore + Send + Sync>,
    pub destinations: Vec<NativeDestination>,
    pub quota: Quota,
    pub limits: Limits,
}

pub(crate) enum EgressPlan {
    Policy {
        plan: Box<PolicyPlan>,
        kind: &'static str,
    },
    Interrupt {
        plan: Box<InterruptionPlan>,
        input_event: EventId,
        vault_id: VaultId,
    },
}

pub(crate) struct PendingCall {
    prepared: PreparedCall,
    input_event: EventId,
    expires_ms: i64,
}

/// Native Provider state of one Workspace.
#[derive(Default)]
pub(crate) struct Native {
    provider: Mutex<Option<Arc<NativeProvider>>>,
    plans: Mutex<BTreeMap<String, EgressPlan>>,
    calls: Mutex<BTreeMap<String, PendingCall>>,
    /// Dispatches whose send worker is still running.
    pub(crate) sending: Arc<Mutex<BTreeSet<DispatchId>>>,
}

impl Native {
    /// Forget page-held plans and calls (close, lock, reconfiguration).
    pub(crate) fn forget(&self) {
        self.plans.lock().expect("egress plans").clear();
        self.calls.lock().expect("external calls").clear();
    }
}

fn api_of(api: ExternalApi) -> Api {
    match api {
        ExternalApi::Openai => Api::OpenAiResponses,
        ExternalApi::Anthropic => Api::AnthropicMessages,
    }
}

fn api_name(api: Api) -> &'static str {
    api.provider()
}

fn not_configured() -> Fail {
    let mut error = WorkspaceError::new(
        MemoryErrorCode::ProviderUnavailable,
        &["provider.not_configured"],
    );
    error.retryable = false;
    Fail(error)
}

fn memory(error: enouia_memory_contract::MemoryError, rule: &str) -> Fail {
    Fail(WorkspaceError::from_memory(&error, vec![rule.to_owned()]))
}

fn finish_word(finish: FinishReason) -> &'static str {
    match finish {
        FinishReason::Completed => "completed",
        FinishReason::Length => "length",
        FinishReason::Cancelled => "cancelled",
        FinishReason::Failed => "failed",
    }
}

fn plan_id(ids: &(dyn IdSource + Send + Sync)) -> String {
    format!("pln_{}", hex(&ids.random_16()))
}

/// The plan bytes are canonical JSON; the page shows exactly what it hashes.
fn inspection_text(bytes: &[u8]) -> R<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| fail(MemoryErrorCode::StorageFailed, "provider.inspection"))
}

impl Workspace {
    /// Enable (or with `None`, disable) native sending. The host calls this
    /// from native code only. Refused while a send is still running: the
    /// host cancels those operations and waits for them first.
    pub fn set_native_provider(
        &self,
        provider: Option<NativeProvider>,
    ) -> Result<(), WorkspaceError> {
        self.with_lifecycle(true, || {
            if !self.native.sending.lock().expect("sending").is_empty() {
                let mut error =
                    WorkspaceError::new(MemoryErrorCode::Busy, &["provider.call_running"]);
                error.retryable = true;
                return Err(Fail(error));
            }
            if let Some(provider) = &provider {
                let mut seen = BTreeSet::new();
                for d in &provider.destinations {
                    let expected = d
                        .api
                        .binding(&d.capabilities.binding.model)
                        .map_err(|e| memory(e, "provider.destination"))?;
                    if !seen.insert(api_name(d.api))
                        || d.capabilities.binding != expected
                        || !d.capabilities.validate().is_empty()
                    {
                        return Err(fail(
                            MemoryErrorCode::InvalidRequest,
                            "provider.destination",
                        ));
                    }
                }
            }
            self.native.forget();
            *self.native.provider.lock().expect("provider") = provider.map(Arc::new);
            Ok(())
        })
        .map_err(|f| f.0)
    }

    fn native_provider(&self) -> R<Arc<NativeProvider>> {
        self.native
            .provider
            .lock()
            .expect("provider")
            .clone()
            .ok_or_else(not_configured)
    }

    fn destination(provider: &NativeProvider, api: Api) -> R<NativeDestination> {
        provider
            .destinations
            .iter()
            .find(|d| d.api == api)
            .cloned()
            .ok_or_else(|| {
                let mut error = WorkspaceError::new(
                    MemoryErrorCode::ProviderUnavailable,
                    &["provider.api_not_configured"],
                );
                error.retryable = false;
                Fail(error)
            })
    }

    pub(crate) fn provider_component(&self) -> Value {
        match &*self.native.provider.lock().expect("provider") {
            None => {
                json!({"component": "provider", "state": "unavailable", "mode": "local_mock_only"})
            }
            Some(p) => json!({
                "component": "provider", "state": "healthy", "mode": "native_explicit",
                "apis": p.destinations.iter().map(|d| api_name(d.api)).collect::<Vec<_>>(),
            }),
        }
    }

    pub(crate) fn provider_status(&self) -> R<Value> {
        let provider = self.native.provider.lock().expect("provider").clone();
        let destinations: Vec<Value> = provider
            .iter()
            .flat_map(|p| p.destinations.iter())
            .map(|d| {
                let c = &d.capabilities;
                json!({
                    "api": api_name(d.api), "model": c.binding.model,
                    "streaming": c.streaming, "contextWindowTokens": c.context_window_tokens,
                    "maxOutputTokens": c.max_output_tokens, "verifiedAt": c.verified_at,
                })
            })
            .collect();
        let grants = match self.open() {
            Err(_) => Value::Null,
            Ok(open) => {
                let pin = open.vault.pin_current()?;
                let mut grants: Vec<PolicyRecord> =
                    latest_records(&open.vault, &pin, RecordKind::Policy)?;
                grants.retain(|p| p.origin == PolicyOrigin::OwnerGrant);
                grants.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                Value::Array(
                    grants
                        .iter()
                        .map(|p| {
                            json!({
                                "policyId": p.policy_id,
                                "active": p.status == PolicyStatus::Active,
                                "maxSensitivity": p.resources.max_sensitivity,
                                "purposes": p.purposes,
                                "destinations": p.destinations.iter().map(|d| json!({
                                    "provider": d.provider, "model": d.model,
                                })).collect::<Vec<_>>(),
                                "validFrom": p.valid_from, "revokedAt": p.revoked_at,
                            })
                        })
                        .collect(),
                )
            }
        };
        Ok(json!({
            "configured": provider.is_some(),
            "destinations": destinations,
            "grants": grants,
        }))
    }

    fn hold_plan(&self, plan: EgressPlan, kind: &str, bytes: &[u8], hash: Value) -> R<Value> {
        let id = plan_id(self.config.ids.as_ref());
        let inspection = inspection_text(bytes)?;
        self.native
            .plans
            .lock()
            .expect("egress plans")
            .insert(id.clone(), plan);
        Ok(json!({"planId": id, "kind": kind, "inspection": inspection, "diffHash": hash}))
    }

    pub(crate) fn egress_grant_plan(&self, a: &wire::GrantPlanArgs) -> R<Value> {
        let provider = self.native_provider()?;
        let open = self.open()?;
        let mut bindings = Vec::new();
        for api in &a.apis {
            let d = Self::destination(&provider, api_of(*api))?;
            bindings.push((d.api, d.capabilities.binding.model.clone()));
        }
        let plan = policy::plan_grant(
            &open.vault,
            open.owner.clone(),
            SURFACE,
            ResourceSelector {
                all_projects: true,
                project_ids: vec![],
                record_kinds: vec![
                    RecordKind::Memory,
                    RecordKind::Source,
                    RecordKind::Identity,
                    RecordKind::Session,
                    RecordKind::SessionEvent,
                    RecordKind::Checkpoint,
                ],
                max_sensitivity: match a.max_sensitivity {
                    GrantSensitivity::Normal => Sensitivity::Normal,
                    GrantSensitivity::Private => Sensitivity::Private,
                },
            },
            vec![Purpose::Answer, Purpose::ContinueSession],
            &bindings,
        )
        .map_err(|e| memory(e, "provider.grant_plan"))?;
        let (bytes, hash) = (plan.inspect().to_vec(), plan.hash());
        self.hold_plan(
            EgressPlan::Policy {
                plan: Box::new(plan),
                kind: "grant",
            },
            "grant",
            &bytes,
            json!(hash),
        )
    }

    pub(crate) fn egress_revoke_plan(&self, a: &wire::PolicyArgs) -> R<Value> {
        let open = self.open()?;
        let plan = policy::plan_revoke(&open.vault, open.owner.clone(), SURFACE, &a.policy_id)
            .map_err(|e| memory(e, "provider.revoke_plan"))?;
        let (bytes, hash) = (plan.inspect().to_vec(), plan.hash());
        self.hold_plan(
            EgressPlan::Policy {
                plan: Box::new(plan),
                kind: "revoke",
            },
            "revoke",
            &bytes,
            json!(hash),
        )
    }

    fn adapter<'a>(&self, open: &'a Open, input: EventId, quota: Quota) -> VaultAdapter<'a> {
        VaultAdapter {
            vault: &open.vault,
            owner: open.owner.clone(),
            input_event: input,
            enabled: &open.enabled,
            quota,
        }
    }

    pub(crate) fn external_interrupt_plan(&self, a: &wire::InterruptArgs) -> R<Value> {
        let open = self.open()?;
        if self
            .native
            .sending
            .lock()
            .expect("sending")
            .contains(&a.dispatch_id)
        {
            return Err(fail(MemoryErrorCode::Busy, "provider.call_running"));
        }
        // Interruption reads only the saved ledger; it needs no transport,
        // so it stays available after the host removed its native setup.
        let quota = self
            .native
            .provider
            .lock()
            .expect("provider")
            .as_ref()
            .map(|p| p.quota)
            .unwrap_or_default();
        let plan = self
            .adapter(&open, a.input_event_id.clone(), quota)
            .plan_interruption(&a.dispatch_id, SURFACE)
            .map_err(|e| memory(e, "provider.interrupt_plan"))?;
        let (bytes, hash) = (plan.inspect().to_vec(), plan.hash());
        self.hold_plan(
            EgressPlan::Interrupt {
                plan: Box::new(plan),
                input_event: a.input_event_id.clone(),
                vault_id: open.vault.vault_id().clone(),
            },
            "interrupt",
            &bytes,
            json!(hash),
        )
    }

    /// Confirm a held grant, revocation or interruption plan by its exact
    /// hash. A confirmed plan answers its saved result again.
    pub(crate) fn provider_confirm(&self, a: &wire::ConfirmArgs) -> R<Value> {
        if let Some((hash, result)) = self.confirmed.lock().expect("confirmed").get(&a.plan_id) {
            if hash == &a.diff_hash {
                return Ok(result.clone());
            }
            return Err(fail(MemoryErrorCode::PermissionDenied, "review.stale_plan"));
        }
        let open = self.open()?;
        let plan = self
            .native
            .plans
            .lock()
            .expect("egress plans")
            .remove(&a.plan_id)
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "provider.plan_unknown"))?;
        let outcome = match &plan {
            EgressPlan::Policy { plan, kind } => {
                policy::confirm(&open.vault, &open.owner, SURFACE, plan, &a.diff_hash)
                    .map(|_| json!({"kind": kind, "policyId": plan.policy_id(), "committed": true}))
            }
            EgressPlan::Interrupt {
                plan,
                input_event,
                vault_id,
            } => {
                if vault_id != open.vault.vault_id() {
                    return Err(fail(
                        MemoryErrorCode::PermissionDenied,
                        "provider.plan_vault",
                    ));
                }
                self.adapter(&open, input_event.clone(), Quota::default())
                    .confirm_interruption(plan, SURFACE, &a.diff_hash)
                    .map(|event| json!({"kind": "interrupt", "eventId": event, "committed": true}))
            }
        };
        match outcome {
            Ok(result) => {
                self.confirmed
                    .lock()
                    .expect("confirmed")
                    .insert(a.plan_id.clone(), (a.diff_hash.clone(), result.clone()));
                Ok(result)
            }
            Err(error) => {
                // A wrong hash leaves the plan for the exact confirmation.
                if error.code == MemoryErrorCode::PermissionDenied {
                    self.native
                        .plans
                        .lock()
                        .expect("egress plans")
                        .insert(a.plan_id.clone(), plan);
                }
                Err(memory(error, "provider.confirm"))
            }
        }
    }

    /// Save the input first, compile for the exact destination and build the
    /// exact HTTP body. Nothing is sent and no credential is read.
    pub(crate) fn external_prepare(&self, a: &wire::ExternalPrepareArgs, key: &str) -> R<Value> {
        let provider = self.native_provider()?;
        let api = api_of(a.api);
        let destination = Self::destination(&provider, api)?;
        let open = self.open()?;
        let request = request_id_for(key, "external_prepare");
        let input = session::save_input(
            &open.vault,
            &open.owner,
            &a.session_id,
            &a.branch_id,
            &a.text,
            &request,
            format!("external-input\n{key}").as_bytes(),
        )?
        .id;
        let mut compile_input = CompileInput::local(&a.text, open.owner.clone(), request);
        compile_input.output_tokens = a.output_tokens;
        compile_input.session_id = Some(a.session_id.clone());
        compile_input.branch_id = Some(a.branch_id.clone());
        let compiled = self.with_index(&open, |index| {
            Ok(enouia_memory_context::compiler::compile_for_destination(
                &open.vault,
                index,
                &compile_input,
                &Destination {
                    kind: DestinationKind::ExternalProvider,
                    provider_binding: Some(destination.capabilities.binding.clone()),
                },
            )?)
        })?;
        let prepared = self
            .adapter(&open, input.clone(), provider.quota)
            .prepare_saved(
                api,
                &compiled.capsule.capsule_id,
                &destination.capabilities,
                provider.limits.clone(),
                CallOptions {
                    egress_policy: a.policy_id.clone(),
                    approval: None,
                    streaming: a.streaming,
                    output_tokens: a.output_tokens,
                },
            )
            .map_err(|e| memory(e, "provider.prepare"))?;
        let wire = prepared.inspect();
        let body = String::from_utf8(wire.body().to_vec())
            .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "provider.wire_text"))?;
        let call = plan_id(self.config.ids.as_ref());
        let result = json!({
            "callId": call, "inputEventId": input,
            "capsuleId": compiled.capsule.capsule_id,
            "inspectionId": compiled.inspection.inspection_id,
            "dispatchId": prepared.dispatch().dispatch_id,
            "api": api_name(api), "model": destination.capabilities.binding.model,
            "endpoint": wire.endpoint(), "streaming": a.streaming,
            "body": body, "wireHash": wire.hash(),
            "estimatedInputTokens": prepared.estimated_input_tokens(),
            "outputTokens": a.output_tokens,
            "expiresInMs": CALL_TTL_MS,
        });
        let now = self.now_ms();
        let mut calls = self.native.calls.lock().expect("external calls");
        calls.retain(|_, c| c.expires_ms > now);
        calls.insert(
            call,
            PendingCall {
                prepared,
                input_event: input,
                expires_ms: now + CALL_TTL_MS,
            },
        );
        Ok(result)
    }

    /// The owner confirmed the exact wire body: record the single-use
    /// approval, then send once on a worker thread. The page observes the
    /// operation and the Session; a retry never resends.
    pub(crate) fn external_send(&self, a: &wire::ExternalSendArgs, key: &str) -> R<Done> {
        let fingerprint = format!("external_send\n{}\n{}", a.call_id, a.wire_hash);
        if let Some((seen, operation)) = self.started.lock().expect("started").get(key) {
            if seen == &fingerprint {
                return Ok(Done {
                    result: json!({"operationId": operation, "kind": "external_send"}),
                    operation_id: Some(operation.clone()),
                });
            }
            return Err(fail(
                MemoryErrorCode::IdempotencyConflict,
                "workspace.key_reuse",
            ));
        }
        let provider = self.native_provider()?;
        let open = self.open()?;
        let pending = {
            let mut calls = self.native.calls.lock().expect("external calls");
            let call = calls
                .remove(&a.call_id)
                .ok_or_else(|| fail(MemoryErrorCode::NotFound, "provider.call_unknown"))?;
            if call.expires_ms <= self.now_ms() {
                return Err(fail(
                    MemoryErrorCode::PermissionDenied,
                    "provider.call_expired",
                ));
            }
            if call.prepared.inspect().hash() != a.wire_hash {
                calls.insert(a.call_id.clone(), call);
                return Err(fail(MemoryErrorCode::PermissionDenied, "review.stale_plan"));
            }
            call
        };
        let nonce = hex(&self.config.ids.random_16());
        let prepared = self
            .adapter(&open, pending.input_event.clone(), provider.quota)
            .approve(pending.prepared, &a.wire_hash, SURFACE, &nonce)
            .map_err(|e| memory(e, "provider.approve"))?;
        let dispatch = prepared.dispatch().dispatch_id.clone();
        self.native
            .sending
            .lock()
            .expect("sending")
            .insert(dispatch.clone());
        let sending = self.native.sending.clone();
        let input = pending.input_event;
        let hash = a.wire_hash.clone();
        let done = self.spawn("external_send", move |ticket| {
            struct Clear(Arc<Mutex<BTreeSet<DispatchId>>>, DispatchId);
            impl Drop for Clear {
                fn drop(&mut self) {
                    self.0
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&self.1);
                }
            }
            let _clear = Clear(sending, dispatch.clone());
            let adapter = VaultAdapter {
                vault: &open.vault,
                owner: open.owner.clone(),
                input_event: input,
                enabled: &open.enabled,
                quota: provider.quota,
            };
            let client = Client {
                transport: provider.transport.as_ref(),
                secrets: provider.secrets.as_ref(),
                guard: &adapter,
                journal: &adapter,
            };
            let response = client
                .send(&prepared, &hash, ticket)
                .map_err(|e| memory(e, "provider.send"))?;
            Ok(json!({
                "dispatchId": dispatch, "finish": finish_word(response.finish),
                "inputTokens": response.input_tokens, "outputTokens": response.output_tokens,
            }))
        });
        if let Some(operation) = &done.operation_id {
            self.started
                .lock()
                .expect("started")
                .insert(key.to_owned(), (fingerprint, operation.clone()));
        }
        Ok(done)
    }
}

/// An open Vault's native switch starts on; close turns it off first.
pub(crate) fn enabled_flag() -> AtomicBool {
    AtomicBool::new(true)
}

pub(crate) fn disable(flag: &AtomicBool) {
    flag.store(false, Ordering::SeqCst);
}
