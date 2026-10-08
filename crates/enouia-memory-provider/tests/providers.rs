//! Isolated synthetic Vaults and fake byte transports. No credentials or HTTP.
#[path = "../../enouia-memory-context/tests/support/mod.rs"]
mod support;
use enouia_memory_context::{CompileInput, compiler, session};
use enouia_memory_contract::{
    MemoryErrorCode,
    common::{Sensitivity, TrustedSurface},
    context::*,
    foundation::Cancellation,
    hash::sha256,
    ids::*,
    json::{Revision, SchemaVersion},
    policy::*,
    ports::{SecretBytes, SecretStore},
    provider::{Capability, InvocationState, ProviderCapabilities, TokenCounting},
    record::RecordKind,
    session::ClientSurface,
};
use enouia_memory_provider::{
    Result,
    client::{Client, InvocationJournal, Limits},
    codec::{Api, WireRequest},
    stream::Decoder,
    transport::Transport,
    vault::{CallOptions, Quota, VaultAdapter},
};
use enouia_memory_vault::service::SessionStart;
use serde_json::json;
use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use support::*;

struct NeverCancel;
impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
struct Secrets {
    missing: bool,
}
impl SecretStore for Secrets {
    fn read(&self, _: &str) -> Result<SecretBytes> {
        if self.missing {
            return Err(enouia_memory_contract::MemoryError::new(
                MemoryErrorCode::Unauthenticated,
                enouia_memory_contract::foundation::ComponentId::Provider,
            ));
        }
        Ok(SecretBytes::new(b"synthetic-secret".to_vec()))
    }
}
struct FakeHttp {
    calls: Cell<u64>,
    bytes: Vec<u8>,
    fail: bool,
}
impl Transport for FakeHttp {
    fn exchange(
        &self,
        _: &WireRequest,
        _: &SecretBytes,
        _: &dyn Cancellation,
        _: Duration,
        sink: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        self.calls.set(self.calls.get() + 1);
        for chunk in self.bytes.chunks(3) {
            sink(chunk)?;
        }
        if self.fail {
            return Err(enouia_memory_contract::MemoryError::new(
                MemoryErrorCode::ProviderUnavailable,
                enouia_memory_contract::foundation::ComponentId::Provider,
            ));
        }
        Ok(())
    }
}
fn reply(api: Api) -> Vec<u8> {
    let v = match api {
        Api::OpenAiResponses => {
            json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"合成答复"}]}],"usage":{"input_tokens":100,"output_tokens":4}})
        }
        Api::AnthropicMessages => {
            json!({"type":"message","role":"assistant","content":[{"type":"text","text":"合成答复"}],"stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":4}})
        }
    };
    serde_json::to_vec(&v).unwrap()
}
fn grant(env: &Env, api: Api) -> PolicyId {
    grant_bindings(env, &[(api, "synthetic-model".into())])
}
fn grant_bindings(env: &Env, bindings: &[(Api, String)]) -> PolicyId {
    use enouia_memory_provider::policy;
    let plan = policy::plan_grant(
        &env.vault,
        owner(),
        TrustedSurface::TrustedLocalCli,
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
            max_sensitivity: Sensitivity::Private,
        },
        vec![
            Purpose::Answer,
            Purpose::ContinueSession,
            Purpose::Extraction,
        ],
        bindings,
    )
    .unwrap();
    assert!(!plan.inspect().is_empty());
    assert!(
        policy::confirm(
            &env.vault,
            &owner(),
            TrustedSurface::TrustedLocalCli,
            &plan,
            &sha256(b"wrong")
        )
        .is_err()
    );
    policy::confirm(
        &env.vault,
        &owner(),
        TrustedSurface::TrustedLocalCli,
        &plan,
        &plan.hash(),
    )
    .unwrap();
    plan.policy_id().clone()
}
fn setup(
    env: &Env,
    api: Api,
    sensitivity: Sensitivity,
) -> (CapsuleId, EventId, PolicyId, ProviderCapabilities) {
    setup_policy(env, api, sensitivity, grant(env, api))
}
fn setup_policy(
    env: &Env,
    api: Api,
    sensitivity: Sensitivity,
    policy: PolicyId,
) -> (CapsuleId, EventId, PolicyId, ProviderCapabilities) {
    let (sid, bid) = session::start(
        &env.vault,
        &SessionStart {
            owner: owner(),
            surface: ClientSurface::Test,
            sensitivity,
            policy_id: env.policy(),
        },
        format!("session:{}", api.provider()).as_bytes(),
    )
    .unwrap()
    .id;
    let mut input = CompileInput::local(
        "合成输入",
        owner(),
        RequestId::from_random(env.vault.random_id_bytes()),
    );
    input.session_id = Some(sid);
    input.branch_id = Some(bid);
    let event = session::save_input(
        &env.vault,
        &owner(),
        input.session_id.as_ref().unwrap(),
        input.branch_id.as_ref().unwrap(),
        &input.query,
        &input.request_id,
        format!("input:{}", api.provider()).as_bytes(),
    )
    .unwrap()
    .id;
    let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
    let binding = api.binding("synthetic-model").unwrap();
    let caps = compiler::compile_for_destination(
        &env.vault,
        &index,
        &input,
        &Destination {
            kind: DestinationKind::ExternalProvider,
            provider_binding: Some(binding.clone()),
        },
    )
    .unwrap();
    let capabilities = ProviderCapabilities {
        schema_version: SchemaVersion,
        binding,
        text_input: Capability::Supported,
        image_input: Capability::Unsupported,
        streaming: Capability::Supported,
        tool_calling: Capability::Unsupported,
        cancellation: Capability::Supported,
        token_counting: TokenCounting::Estimated,
        context_window_tokens: Some(128_000),
        max_output_tokens: Some(4096),
        verified_at: Some(env.vault.now().unwrap()),
    };
    (caps.capsule.capsule_id, event, policy, capabilities)
}

#[test]
fn both_providers_archive_exact_request_and_replay_without_http() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new(api.provider());
        let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: input,
            enabled: &enabled,
            quota: Quota::default(),
        };
        let prepared = adapter
            .prepare_saved(
                api,
                &capsule,
                &caps,
                Limits::default(),
                CallOptions::text(policy, 128),
            )
            .unwrap();
        let inspected = prepared.inspect().hash();
        let parsed: serde_json::Value = serde_json::from_slice(prepared.inspect().body()).unwrap();
        if api == Api::OpenAiResponses {
            assert_eq!(parsed["store"], false);
            assert_eq!(parsed["truncation"], "disabled");
        }
        let http = FakeHttp {
            calls: Cell::new(0),
            bytes: reply(api),
            fail: false,
        };
        let secrets = Secrets { missing: false };
        let client = Client {
            transport: &http,
            secrets: &secrets,
            guard: &adapter,
            journal: &adapter,
        };
        assert_eq!(
            client
                .send(&prepared, &inspected, &NeverCancel)
                .unwrap()
                .text,
            "合成答复"
        );
        assert_eq!(
            adapter.invocations().unwrap()[0].state,
            InvocationState::Completed
        );
        assert_eq!(
            adapter
                .inspect_saved(&prepared.dispatch().dispatch_id)
                .unwrap()
                .body(),
            prepared.inspect().body()
        );
        assert_eq!(
            adapter
                .saved_response(&prepared.dispatch().dispatch_id)
                .unwrap()
                .unwrap()
                .text,
            "合成答复"
        );
        assert!(client.send(&prepared, &inspected, &NeverCancel).is_err());
        assert_eq!(http.calls.get(), 1);
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}
#[test]
fn private_requires_bound_confirmation_and_wire_mutations_are_refused() {
    let env = Env::new("private");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Private);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: reply(api),
        fail: false,
    };
    let secrets = Secrets { missing: false };
    let client = Client {
        transport: &http,
        secrets: &secrets,
        guard: &adapter,
        journal: &adapter,
    };
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    let hash = call.inspect().hash();
    let approved = adapter
        .approve(
            call,
            &hash,
            TrustedSurface::TrustedLocalCli,
            "1234567890abcdef1234567890abcdef",
        )
        .unwrap();
    assert!(
        client
            .send(&approved, &sha256(b"changed"), &NeverCancel)
            .is_err()
    );
    assert_eq!(http.calls.get(), 0);
    assert!(client.send(&approved, &hash, &NeverCancel).is_ok());
    assert_eq!(http.calls.get(), 1);
}
#[test]
fn missing_keys_unknown_capabilities_lock_and_quotas_do_not_send() {
    let env = Env::new("preflight");
    let api = Api::AnthropicMessages;
    let (capsule, input, policy, mut caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota {
            max_attempts: 0,
            max_reserved_tokens: 0,
            price_limit: None,
        },
    };
    caps.token_counting = TokenCounting::Unknown;
    assert!(
        adapter
            .prepare_saved(
                api,
                &capsule,
                &caps,
                Limits::default(),
                CallOptions::text(policy.clone(), 128)
            )
            .is_err()
    );
    caps.token_counting = TokenCounting::Estimated;
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: reply(api),
        fail: false,
    };
    let missing = Secrets { missing: true };
    let secrets = Secrets { missing: false };
    let mut client = Client {
        transport: &http,
        secrets: &missing,
        guard: &adapter,
        journal: &adapter,
    };
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    assert!(adapter.invocations().unwrap().is_empty());
    client.secrets = &secrets;
    enabled.store(false, Ordering::SeqCst);
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    enabled.store(true, Ordering::SeqCst);
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    assert_eq!(http.calls.get(), 0);
}
#[test]
fn admitted_crash_is_unknown_and_cannot_be_resent() {
    let env = Env::new("crash");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy.clone(), 128),
        )
        .unwrap();
    assert!(adapter.claim(&call).unwrap());
    assert!(!adapter.claim(&call).unwrap());
    let second = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    assert!(!adapter.claim(&second).unwrap());
    assert_eq!(
        adapter.invocations().unwrap()[0].state,
        InvocationState::OutcomeUnknown
    );
    assert!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap()
            .is_none()
    );
}
#[test]
fn fragmented_sse_preserves_utf8_and_rejects_missing_terminal() {
    let body = format!(
        "event: response.output_text.delta\r\ndata: {}\r\n\r\nevent: response.completed\ndata: {}\n\n",
        json!({"type":"response.output_text.delta","delta":"合成答复"}),
        json!({"type":"response.completed","response":serde_json::from_slice::<serde_json::Value>(&reply(Api::OpenAiResponses)).unwrap()})
    );
    let mut decoder = Decoder::new(Api::OpenAiResponses);
    let mut text = String::new();
    for b in body.as_bytes() {
        decoder
            .push(&[*b], &mut |t| {
                text.push_str(t);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(decoder.finish().unwrap().text, text);
    let mut incomplete = Decoder::new(Api::OpenAiResponses);
    incomplete
        .push(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            &mut |_| Ok(()),
        )
        .unwrap();
    assert!(incomplete.finish().is_err());
}

fn streaming_reply(api: Api, complete: bool) -> Vec<u8> {
    let events = match api {
        Api::OpenAiResponses => vec![
            json!({"type":"response.output_text.delta","delta":"合成答复"}),
            json!({"type":"response.completed","response":serde_json::from_slice::<serde_json::Value>(&reply(api)).unwrap()}),
        ],
        Api::AnthropicMessages => vec![
            json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"合成答复"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}),
            json!({"type":"message_stop"}),
        ],
    };
    let end = events.len() - usize::from(!complete);
    events[..end]
        .iter()
        .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
        .collect::<String>()
        .into_bytes()
}
#[test]
fn both_streams_persist_chunks_and_truncation_never_completes() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for complete in [true, false] {
            let env = Env::new("stream");
            let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
            let enabled = AtomicBool::new(true);
            let adapter = VaultAdapter {
                vault: &env.vault,
                owner: owner(),
                input_event: input,
                enabled: &enabled,
                quota: Quota::default(),
            };
            let mut options = CallOptions::text(policy, 128);
            options.streaming = true;
            let call = adapter
                .prepare_saved(api, &capsule, &caps, Limits::default(), options)
                .unwrap();
            let http = FakeHttp {
                calls: Cell::new(0),
                bytes: streaming_reply(api, complete),
                fail: false,
            };
            let secrets = Secrets { missing: false };
            let client = Client {
                transport: &http,
                secrets: &secrets,
                guard: &adapter,
                journal: &adapter,
            };
            assert_eq!(
                client
                    .send(&call, &call.inspect().hash(), &NeverCancel)
                    .is_ok(),
                complete
            );
            assert_eq!(
                adapter.invocations().unwrap()[0].state,
                if complete {
                    InvocationState::Completed
                } else {
                    InvocationState::Failed
                }
            );
            assert!(
                client
                    .send(&call, &call.inspect().hash(), &NeverCancel)
                    .is_err()
            );
            assert_eq!(http.calls.get(), 1);
            let pin = env.vault.pin_current().unwrap();
            assert!(env.vault.verify(&pin).unwrap().is_clean());
        }
    }
}
#[test]
fn network_failure_is_saved_and_does_not_authorize_retry() {
    let env = Env::new("network-failure");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: vec![],
        fail: true,
    };
    let secrets = Secrets { missing: false };
    let client = Client {
        transport: &http,
        secrets: &secrets,
        guard: &adapter,
        journal: &adapter,
    };
    let failure = client
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap_err();
    assert!(!failure.retryable);
    assert_eq!(
        adapter.invocations().unwrap()[0].error_code,
        Some(MemoryErrorCode::ProviderUnavailable)
    );
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    assert_eq!(http.calls.get(), 1);
}
#[test]
fn wire_and_local_terminal_recovery_survive_reopen() {
    let env = Env::new("reopen");
    let api = Api::AnthropicMessages;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input.clone(),
        enabled: &enabled,
        quota: Quota::default(),
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    assert!(adapter.claim(&call).unwrap());
    session::append_output(
        &env.vault,
        &owner(),
        &input,
        enouia_memory_contract::session::EventKind::AssistantCompleted,
        Some("合成答复"),
        format!("provider-terminal:{}", call.dispatch().dispatch_id).as_bytes(),
    )
    .unwrap();
    let root = enouia_memory_vault::verify_data_root(
        env.vault.managed_root().root(),
        &enouia_memory_vault::RootPolicy::default(),
    )
    .unwrap();
    let reopened = enouia_memory_vault::Vault::open(
        &root,
        None,
        env.clock.clone(),
        std::sync::Arc::new(enouia_memory_vault::OsIdSource),
        enouia_memory_vault::VaultOptions::default(),
    )
    .unwrap();
    let restored = VaultAdapter {
        vault: &reopened,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    assert!(
        restored
            .recover_local_outcome(&call.dispatch().dispatch_id)
            .unwrap()
    );
    assert!(
        !restored
            .recover_local_outcome(&call.dispatch().dispatch_id)
            .unwrap()
    );
    assert_eq!(
        restored
            .inspect_saved(&call.dispatch().dispatch_id)
            .unwrap()
            .body(),
        call.inspect().body()
    );
    assert_eq!(
        restored
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap()
            .unwrap()
            .text,
        "合成答复"
    );
    assert!(!restored.claim(&call).unwrap());
}
#[test]
fn price_reservations_refuse_an_over_budget_call_before_http() {
    let env = Env::new("price");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let quota = Quota {
        price_limit: Some(enouia_memory_provider::vault::PriceLimit {
            input_microusd_per_million: 1_000_000,
            output_microusd_per_million: 1_000_000,
            max_reserved_microusd: 0,
        }),
        ..Quota::default()
    };
    let mut adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota,
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy, 128),
        )
        .unwrap();
    // Enough for wire bytes + output, but not the explicit input margin.
    // Both the price ceiling and the token ceiling must account for it.
    let without_margin = call.inspect().body().len() as u64 + 128;
    adapter
        .quota
        .price_limit
        .as_mut()
        .unwrap()
        .max_reserved_microusd = without_margin;
    assert_eq!(
        adapter.claim(&call).unwrap_err().code,
        MemoryErrorCode::BudgetExceeded
    );
    assert!(adapter.invocations().unwrap().is_empty());
    adapter.quota.price_limit = None;
    adapter.quota.max_reserved_tokens = without_margin;
    assert_eq!(
        adapter.claim(&call).unwrap_err().code,
        MemoryErrorCode::BudgetExceeded
    );
    assert!(adapter.invocations().unwrap().is_empty());
}

#[test]
fn shared_owner_approved_evidence_is_archived_for_both_and_purged() {
    use enouia_memory_contract::{
        candidate::{ProposalKind, ProposedType},
        commit::{DeleteMode, DeleteScope},
        memory::CanonicalMemory,
    };
    use enouia_memory_govern::{
        Canonical, Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed,
        canonical_memories, confirm, plan, propose,
    };
    let mut env = Env::new("common-evidence");
    // Ordinary approval never silently authorizes external use.
    let memory_id = env.remember("shared", "shared_fact", "合成输入：共同证据内容", None);
    let memory: CanonicalMemory = canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .into_iter()
    .find(|m| m.memory_id == memory_id)
    .unwrap();
    assert!(memory.egress_policy_id.is_none());
    let policy = grant_bindings(
        &env,
        &[
            (Api::OpenAiResponses, "synthetic-model".into()),
            (Api::AnthropicMessages, "synthetic-model".into()),
        ],
    );
    let mut proposal = Proposal::create(
        ProposedType::Fact,
        &memory.content,
        serde_json::Map::new(),
        memory
            .evidence
            .iter()
            .map(|e| EvidenceSpec::content(e.source_id.clone(), e.source_revision))
            .collect(),
    );
    proposal.kind = ProposalKind::Revise;
    proposal.target_memory_id = Some(memory_id.clone());
    proposal.expected_revision = Some(memory.revision);
    let Proposed::Stored(candidate) = propose(
        &env.vault,
        &proposal,
        &Origin::owner(owner()),
        b"bind-shared",
    )
    .unwrap() else {
        panic!("stored");
    };
    let shown = plan(
        &env.vault,
        &[Decision::AcceptWithEgress {
            candidate_id: candidate.id,
            revision: Revision::new(1).unwrap(),
            policy_id: policy.clone(),
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    assert!(shown.diff.to_string().contains(policy.as_str()));
    confirm(
        &env.vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
    let enabled = AtomicBool::new(true);
    let mut wires = vec![];
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let (capsule, input, _, caps) =
            setup_policy(&env, api, Sensitivity::Normal, policy.clone());
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: input,
            enabled: &enabled,
            quota: Quota::default(),
        };
        let call = adapter
            .prepare_saved(
                api,
                &capsule,
                &caps,
                Limits::default(),
                CallOptions::text(policy.clone(), 128),
            )
            .unwrap();
        assert!(String::from_utf8_lossy(call.inspect().body()).contains("共同证据内容"));
        let hash = call.inspect().hash();
        let call = adapter
            .approve(
                call,
                &hash,
                TrustedSurface::TrustedLocalCli,
                &format!("1234567890abcdef1234567890abcdef-{}", api.provider()),
            )
            .unwrap();
        let http = FakeHttp {
            calls: Cell::new(0),
            bytes: reply(api),
            fail: false,
        };
        Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        }
        .send(&call, &hash, &NeverCancel)
        .unwrap();
        wires.push(hash);
        assert_eq!(http.calls.get(), 1);
        let terminal = adapter.invocations().unwrap()[0]
            .terminal_event_id
            .clone()
            .unwrap();
        let pin = env.vault.pin_current().unwrap();
        let event = env
            .vault
            .read_parsed(
                &pin,
                &enouia_memory_contract::record::RecordRef::new(
                    RecordKind::SessionEvent,
                    terminal.as_str(),
                    Revision::new(1).unwrap(),
                ),
            )
            .unwrap();
        let enouia_memory_contract::record::AnyRecord::SessionEvent(event) = event else {
            panic!("event");
        };
        assert_eq!(event.sensitivity, Sensitivity::Private);
    }
    // Forget/purge the approved evidence and all dependent provider payloads.
    let current = canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .into_iter()
    .find(|m| m.memory_id == memory_id)
    .unwrap();
    let deletion = enouia_memory_govern::delete::delete_proposal(
        &current,
        DeleteMode::Purge,
        DeleteScope::AllRevisions,
    );
    let Proposed::Stored(candidate) = propose(
        &env.vault,
        &deletion,
        &Origin::owner(owner()),
        b"delete-shared",
    )
    .unwrap() else {
        panic!("stored");
    };
    let shown = plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: candidate.id,
            revision: Revision::new(1).unwrap(),
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    confirm(
        &env.vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
    enouia_memory_govern::delete::complete_purge(
        &env.vault,
        &shown.ids[0].delete_id,
        &owner(),
        b"purge-shared",
    )
    .unwrap();
    let pin = env.vault.pin_current().unwrap();
    for hash in wires {
        assert!(env.vault.read_object(&pin, &hash).is_err());
    }
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn revocation_invalidates_a_prepared_call_without_http() {
    let env = Env::new("revoke");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let call = adapter
        .prepare_saved(
            api,
            &capsule,
            &caps,
            Limits::default(),
            CallOptions::text(policy.clone(), 128),
        )
        .unwrap();
    let shown = enouia_memory_provider::policy::plan_revoke(
        &env.vault,
        owner(),
        TrustedSurface::TrustedLocalCli,
        &policy,
    )
    .unwrap();
    enouia_memory_provider::policy::confirm(
        &env.vault,
        &owner(),
        TrustedSurface::TrustedLocalCli,
        &shown,
        &shown.hash(),
    )
    .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: reply(api),
        fail: false,
    };
    assert!(
        Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter
        }
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .is_err()
    );
    assert_eq!(http.calls.get(), 0);
    assert!(adapter.invocations().unwrap().is_empty());
    // Even a publisher that already passed an earlier check cannot add text
    // against an obsolete policy epoch after revocation.
    assert!(
        session::append_output_with_guard(
            &env.vault,
            &owner(),
            &adapter.input_event,
            enouia_memory_contract::session::EventKind::AssistantChunk,
            Some("过期片段"),
            b"stale-publication",
            Some(session::OutputGuard {
                policy_epoch: call.dispatch().egress.policy_epoch,
                deletion_epoch: call.dispatch().egress.deletion_epoch,
                sensitivity: Sensitivity::Normal,
            }),
        )
        .is_err()
    );
}

#[test]
fn length_responses_remain_incomplete_and_recoverable() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("length");
        let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: input.clone(),
            enabled: &enabled,
            quota: Quota::default(),
        };
        let call = adapter
            .prepare_saved(
                api,
                &capsule,
                &caps,
                Limits::default(),
                CallOptions::text(policy, 128),
            )
            .unwrap();
        let mut response: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
        if api == Api::OpenAiResponses {
            response["status"] = json!("incomplete");
            response["incomplete_details"] = json!({"reason":"max_output_tokens"});
        } else {
            response["stop_reason"] = json!("max_tokens");
        }
        let http = FakeHttp {
            calls: Cell::new(0),
            bytes: serde_json::to_vec(&response).unwrap(),
            fail: false,
        };
        let response = Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        }
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap();
        assert_eq!(
            response.finish,
            enouia_memory_contract::provider::FinishReason::Length
        );
        assert_eq!(
            adapter.invocations().unwrap()[0].state,
            InvocationState::Length
        );
        assert_eq!(
            adapter
                .saved_response(&call.dispatch().dispatch_id)
                .unwrap()
                .unwrap()
                .text,
            "合成答复"
        );
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}

#[test]
fn cancelling_a_stream_keeps_partial_chunks_and_does_not_resend() {
    struct Cancel(Cell<bool>);
    impl Cancellation for Cancel {
        fn is_cancelled(&self) -> bool {
            self.0.get()
        }
    }
    struct StopHttp<'a> {
        cancel: &'a Cancel,
    }
    impl Transport for StopHttp<'_> {
        fn exchange(
            &self,
            _: &WireRequest,
            _: &SecretBytes,
            _: &dyn Cancellation,
            _: Duration,
            sink: &mut dyn FnMut(&[u8]) -> Result<()>,
        ) -> Result<()> {
            sink(&streaming_reply(Api::OpenAiResponses, false))?;
            self.cancel.0.set(true);
            Err(enouia_memory_contract::MemoryError::new(
                MemoryErrorCode::Cancelled,
                enouia_memory_contract::foundation::ComponentId::Provider,
            ))
        }
    }
    let env = Env::new("cancel-stream");
    let api = Api::OpenAiResponses;
    let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: input,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let mut options = CallOptions::text(policy, 128);
    options.streaming = true;
    let call = adapter
        .prepare_saved(api, &capsule, &caps, Limits::default(), options)
        .unwrap();
    let cancel = Cancel(Cell::new(false));
    let error = Client {
        transport: &StopHttp { cancel: &cancel },
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    }
    .send(&call, &call.inspect().hash(), &cancel)
    .unwrap_err();
    assert_eq!(error.code, MemoryErrorCode::Cancelled);
    assert!(!error.retryable);
    assert_eq!(
        adapter.invocations().unwrap()[0].state,
        InvocationState::Cancelled
    );
    cancel.0.set(false);
    assert!(!adapter.claim(&call).unwrap());
    assert!(
        env.vault
            .verify(&env.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
}

fn extraction_options(
    source: SourceId,
    api: Api,
) -> enouia_memory_provider::extraction::ExtractionOptions {
    use enouia_memory_provider::extraction::{ExtractionOptions, SourceSlice};
    ExtractionOptions {
        api,
        model: "synthetic-model".into(),
        subject_id: SubjectId::parse("sub_00000001-0000-4000-8000-000000000001").unwrap(),
        sources: vec![SourceSlice {
            source: enouia_memory_contract::common::SourceRevisionRef {
                source_id: source,
                source_revision: Revision::new(1).unwrap(),
            },
            start: 0,
            end: "偏好合成红色，并倾向合成蓝色。".len() as u64,
        }],
        max_candidates: 2,
        max_pending: 500,
        max_reserved_tokens: 100_000,
        max_reserved_cost_microusd: None,
    }
}
fn extraction_source(env: &Env) -> SourceId {
    env.vault
        .record_manual_assertion(
            &enouia_memory_vault::service::ManualAssertionInput {
                text: "偏好合成红色，并倾向合成蓝色。".into(),
                operator: owner(),
                trusted_surface: TrustedSurface::TrustedLocalCli,
                confirmation: enouia_memory_contract::source::ConfirmationMethod::TypedConfirmation,
                sensitivity: Sensitivity::Normal,
                time_precision: enouia_memory_contract::common::TimePrecision::Second,
                access_policy_id: env.policy(),
            },
            b"extraction-source",
        )
        .unwrap()
        .id
}
fn extraction_reply(api: Api, invalid: bool) -> Vec<u8> {
    let claims = json!({"candidates":[
        {"kind":"fact","content":"合成用户偏好红色","quote":"偏好合成红色","source_index":0,"claim_key":"synthetic_red"},
        {"kind":"preference","content":"合成用户倾向蓝色","quote":if invalid {"不存在的来源引用"} else {"倾向合成蓝色"},"source_index":0,"scope":"synthetic_color","strength":"tentative"}]});
    let mut wire: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
    if api == Api::OpenAiResponses {
        wire["output"][0]["content"][0]["text"] = json!(claims.to_string());
    } else {
        wire["content"][0]["text"] = json!(claims.to_string());
    }
    serde_json::to_vec(&wire).unwrap()
}
fn extraction_capabilities(env: &Env, api: Api) -> ProviderCapabilities {
    ProviderCapabilities {
        schema_version: SchemaVersion,
        binding: api.binding("synthetic-model").unwrap(),
        text_input: Capability::Supported,
        image_input: Capability::Unsupported,
        streaming: Capability::Supported,
        tool_calling: Capability::Unsupported,
        cancellation: Capability::Supported,
        token_counting: TokenCounting::Estimated,
        context_window_tokens: Some(128_000),
        max_output_tokens: Some(4096),
        verified_at: Some(env.vault.now().unwrap()),
    }
}
#[test]
fn extraction_is_explicit_resumable_and_never_accepts_memories() {
    use enouia_memory_contract::extraction::ExtractionState;
    use enouia_memory_provider::extraction;
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("extraction-resume");
        let source = extraction_source(&env);
        let options = extraction_options(source, api);
        let policy = grant(&env, api);
        let (sid, run) = extraction::create(
            &env.vault,
            &owner(),
            &env.policy(),
            &options,
            b"extract-run",
        )
        .unwrap();
        assert_eq!(
            extraction::create(
                &env.vault,
                &owner(),
                &env.policy(),
                &options,
                b"extract-run"
            )
            .unwrap(),
            (sid.clone(), run.clone())
        );
        let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: job.input_event_id,
            enabled: &enabled,
            quota: Quota::default(),
        };
        let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
        let call = extraction::prepare(
            &adapter,
            &index,
            &run,
            &extraction_capabilities(&env, api),
            Limits::default(),
            CallOptions::text(policy, 1024),
        )
        .unwrap();
        assert!(
            call.dispatch()
                .resource_refs()
                .iter()
                .any(|r| r.record_kind == RecordKind::Source)
        );
        let http = FakeHttp {
            calls: Cell::new(0),
            bytes: extraction_reply(api, false),
            fail: false,
        };
        Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        }
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap();
        assert_eq!(extraction::apply_next(&adapter, &run).unwrap().cursor, 1);
        enabled.store(false, Ordering::SeqCst);
        assert_eq!(
            extraction::apply_next(&adapter, &run).unwrap_err().code,
            MemoryErrorCode::PermissionDenied
        );
        enabled.store(true, Ordering::SeqCst);
        extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
        assert!(extraction::apply_next(&adapter, &run).is_err());
        extraction::set_paused(&env.vault, &owner(), &sid, &run, false).unwrap();
        let completed = extraction::apply_next(&adapter, &run).unwrap();
        assert_eq!(completed.state, ExtractionState::Completed);
        assert_eq!(completed.cursor, 2);
        assert_eq!(extraction::apply_next(&adapter, &run).unwrap(), completed);
        let pin = env.vault.pin_current().unwrap();
        let candidates = enouia_memory_govern::pending_candidates(&env.vault, &pin).unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|c| c.origin_kind
            == enouia_memory_contract::candidate::OriginKind::ModelExtraction
            && c.extraction_run_id.as_ref() == Some(&run)
            && c.sensitivity == Sensitivity::Normal));
        assert!(
            enouia_memory_govern::canonical_memories(
                &env.vault,
                &pin,
                enouia_memory_govern::Canonical::Active
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(http.calls.get(), 1);
        assert!(env.vault.verify(&pin).unwrap().is_clean());
    }
}
#[test]
fn extraction_validates_the_whole_response_before_any_candidate() {
    use enouia_memory_provider::extraction;
    let env = Env::new("extraction-invalid");
    let api = Api::OpenAiResponses;
    let options = extraction_options(extraction_source(&env), api);
    let policy = grant(&env, api);
    let (sid, run) = extraction::create(
        &env.vault,
        &owner(),
        &env.policy(),
        &options,
        b"invalid-run",
    )
    .unwrap();
    let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: job.input_event_id,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
    let call = extraction::prepare(
        &adapter,
        &index,
        &run,
        &extraction_capabilities(&env, api),
        Limits::default(),
        CallOptions::text(policy, 1024),
    )
    .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: extraction_reply(api, true),
        fail: false,
    };
    Client {
        transport: &http,
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    }
    .send(&call, &call.inspect().hash(), &NeverCancel)
    .unwrap();
    assert_eq!(
        extraction::apply_next(&adapter, &run).unwrap_err().code,
        MemoryErrorCode::BrokenProvenance
    );
    assert_eq!(
        extraction::job(&env.vault, &owner(), &sid, &run)
            .unwrap()
            .cursor,
        0
    );
    assert!(
        enouia_memory_govern::pending_candidates(&env.vault, &env.vault.pin_current().unwrap())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn reviewed_extraction_is_suppressed_when_the_evidence_is_unchanged() {
    use enouia_memory_provider::extraction;
    let env = Env::new("extraction-rejected");
    let api = Api::OpenAiResponses;
    let options = extraction_options(extraction_source(&env), api);
    let policy = grant(&env, api);
    let enabled = AtomicBool::new(true);
    for pass in 0..2 {
        let (sid, run) = extraction::create(
            &env.vault,
            &owner(),
            &env.policy(),
            &options,
            format!("run-{pass}").as_bytes(),
        )
        .unwrap();
        let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: job.input_event_id,
            enabled: &enabled,
            quota: Quota::default(),
        };
        let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
        let call = extraction::prepare(
            &adapter,
            &index,
            &run,
            &extraction_capabilities(&env, api),
            Limits::default(),
            CallOptions::text(policy.clone(), 1024),
        )
        .unwrap();
        Client {
            transport: &FakeHttp {
                calls: Cell::new(0),
                bytes: extraction_reply(api, false),
                fail: false,
            },
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        }
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap();
        extraction::apply_next(&adapter, &run).unwrap();
        let done = extraction::apply_next(&adapter, &run).unwrap();
        if pass == 0 {
            let decisions = done
                .candidates
                .iter()
                .map(|id| enouia_memory_govern::Decision::Reject {
                    candidate_id: id.clone().unwrap(),
                    revision: Revision::new(1).unwrap(),
                    reason_code: None,
                })
                .collect::<Vec<_>>();
            let shown = enouia_memory_govern::plan(
                &env.vault,
                &decisions,
                &owner(),
                TrustedSurface::TrustedLocalCli,
            )
            .unwrap();
            enouia_memory_govern::confirm(
                &env.vault,
                &shown,
                &enouia_memory_govern::OwnerConfirmation {
                    owner: owner(),
                    surface: TrustedSurface::TrustedLocalCli,
                },
            )
            .unwrap();
        } else {
            assert!(done.candidates.iter().all(Option::is_none));
        }
    }
    let pin = env.vault.pin_current().unwrap();
    assert!(
        enouia_memory_govern::pending_candidates(&env.vault, &pin)
            .unwrap()
            .is_empty()
    );
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}
#[test]
fn extraction_changed_keys_offsets_and_pending_backlog_fail_closed() {
    use enouia_memory_provider::extraction;
    let env = Env::new("extraction-backlog");
    let api = Api::AnthropicMessages;
    let mut options = extraction_options(extraction_source(&env), api);
    options.max_pending = 1;
    let policy = grant(&env, api);
    let (sid, run) =
        extraction::create(&env.vault, &owner(), &env.policy(), &options, b"backlog").unwrap();
    options.max_candidates = 1;
    assert!(extraction::create(&env.vault, &owner(), &env.policy(), &options, b"backlog").is_err());
    options.sources[0].start = 1;
    assert!(
        extraction::create(&env.vault, &owner(), &env.policy(), &options, b"bad-utf8").is_err()
    );
    let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: job.input_event_id,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
    let call = extraction::prepare(
        &adapter,
        &index,
        &run,
        &extraction_capabilities(&env, api),
        Limits::default(),
        CallOptions::text(policy, 1024),
    )
    .unwrap();
    assert!(extraction::apply_next(&adapter, &run).is_err());
    Client {
        transport: &FakeHttp {
            calls: Cell::new(0),
            bytes: extraction_reply(api, false),
            fail: false,
        },
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    }
    .send(&call, &call.inspect().hash(), &NeverCancel)
    .unwrap();
    extraction::apply_next(&adapter, &run).unwrap();
    assert_eq!(
        extraction::apply_next(&adapter, &run).unwrap_err().code,
        MemoryErrorCode::BudgetExceeded
    );
    assert_eq!(
        extraction::job(&env.vault, &owner(), &sid, &run)
            .unwrap()
            .cursor,
        1
    );
}

#[test]
fn extraction_recovers_a_published_candidate_before_cursor_even_at_backlog_limit() {
    use enouia_memory_provider::extraction;
    use enouia_memory_vault::{
        Vault, VaultOptions,
        fault::{FaultAction, FaultPoint, Faults},
    };
    let env = Env::new("extraction-cursor-crash");
    let api = Api::OpenAiResponses;
    let mut options = extraction_options(extraction_source(&env), api);
    options.max_pending = 1;
    let policy = grant(&env, api);
    let (sid, run) = extraction::create(
        &env.vault,
        &owner(),
        &env.policy(),
        &options,
        b"cursor-crash",
    )
    .unwrap();
    let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: job.input_event_id.clone(),
        enabled: &enabled,
        quota: Quota::default(),
    };
    let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
    let call = extraction::prepare(
        &adapter,
        &index,
        &run,
        &extraction_capabilities(&env, api),
        Limits::default(),
        CallOptions::text(policy, 1024),
    )
    .unwrap();
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: extraction_reply(api, false),
        fail: false,
    };
    Client {
        transport: &http,
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    }
    .send(&call, &call.inspect().hash(), &NeverCancel)
    .unwrap();
    let root = enouia_memory_vault::verify_data_root(
        env.vault.managed_root().root(),
        &enouia_memory_vault::RootPolicy::default(),
    )
    .unwrap();
    let faults = Faults::armed();
    let interrupted = Vault::open(
        &root,
        None,
        env.clock.clone(),
        std::sync::Arc::new(enouia_memory_vault::OsIdSource),
        VaultOptions {
            faults: faults.clone(),
            ..VaultOptions::default()
        },
    )
    .unwrap();
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    let worker = VaultAdapter {
        vault: &interrupted,
        owner: owner(),
        input_event: job.input_event_id.clone(),
        enabled: &enabled,
        quota: Quota::default(),
    };
    assert!(extraction::apply_next(&worker, &run).is_err());
    let restored = Vault::open(
        &root,
        None,
        env.clock.clone(),
        std::sync::Arc::new(enouia_memory_vault::OsIdSource),
        VaultOptions::default(),
    )
    .unwrap();
    let pin = restored.pin_current().unwrap();
    let published = enouia_memory_govern::pending_candidates(&restored, &pin).unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(
        extraction::job(&restored, &owner(), &sid, &run)
            .unwrap()
            .cursor,
        0
    );
    let recovered = VaultAdapter {
        vault: &restored,
        owner: owner(),
        input_event: job.input_event_id,
        enabled: &enabled,
        quota: Quota::default(),
    };
    let progress = extraction::apply_next(&recovered, &run).unwrap();
    assert_eq!(progress.cursor, 1);
    assert_eq!(
        progress.candidates,
        vec![Some(published[0].candidate_id.clone())]
    );
    assert_eq!(
        enouia_memory_govern::pending_candidates(&restored, &restored.pin_current().unwrap())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        extraction::apply_next(&recovered, &run).unwrap_err().code,
        MemoryErrorCode::BudgetExceeded
    );
    assert_eq!(http.calls.get(), 1);
    assert!(
        restored
            .verify(&restored.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
}

#[test]
fn deleting_selected_evidence_purges_extraction_input_before_dispatch() {
    use enouia_memory_contract::{
        commit::{DeleteMode, DeleteScope},
        record::{AnyRecord, RecordRef},
    };
    use enouia_memory_govern::{Canonical, Decision, Origin, OwnerConfirmation, Proposed};
    use enouia_memory_provider::extraction;
    let mut env = Env::new("extraction-delete");
    let text = "偏好合成红色，并倾向合成蓝色。";
    let memory = env.remember("extract", "synthetic_color", text, None);
    let current = enouia_memory_govern::canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .into_iter()
    .find(|m| m.memory_id == memory)
    .unwrap();
    let options = extraction_options(current.evidence[0].source_id.clone(), Api::OpenAiResponses);
    let (sid, run) = extraction::create(
        &env.vault,
        &owner(),
        &env.policy(),
        &options,
        b"erase-extract",
    )
    .unwrap();
    let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
    let pin = env.vault.pin_current().unwrap();
    let AnyRecord::SessionEvent(input) = env
        .vault
        .read_parsed(
            &pin,
            &RecordRef::new(
                RecordKind::SessionEvent,
                job.input_event_id.as_str(),
                Revision::new(1).unwrap(),
            ),
        )
        .unwrap()
    else {
        panic!("input")
    };
    let body = input.content_ref.unwrap().object_hash;
    let deletion = enouia_memory_govern::delete::delete_proposal(
        &current,
        DeleteMode::Purge,
        DeleteScope::WithDependents,
    );
    let Proposed::Stored(candidate) = enouia_memory_govern::propose(
        &env.vault,
        &deletion,
        &Origin::owner(owner()),
        b"erase-proposal",
    )
    .unwrap() else {
        panic!("candidate")
    };
    let shown = enouia_memory_govern::plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: candidate.id,
            revision: Revision::new(1).unwrap(),
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    enouia_memory_govern::confirm(
        &env.vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
    enouia_memory_govern::delete::complete_purge(
        &env.vault,
        &shown.ids[0].delete_id,
        &owner(),
        b"erase-complete",
    )
    .unwrap();
    let pin = env.vault.pin_current().unwrap();
    assert!(env.vault.read_object(&pin, &body).is_err());
    assert!(extraction::job(&env.vault, &owner(), &sid, &run).is_err());
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn extraction_admission_keeps_saved_token_cost_and_pause_limits() {
    use enouia_memory_provider::{extraction, vault::PriceLimit};
    for scenario in ["tokens", "missing-rate", "cost", "paused"] {
        let env = Env::new("extraction-durable-budget");
        let api = Api::OpenAiResponses;
        let mut options = extraction_options(extraction_source(&env), api);
        if scenario == "tokens" {
            options.max_reserved_tokens = 1;
        }
        if matches!(scenario, "missing-rate" | "cost") {
            options.max_reserved_cost_microusd = Some(1);
        }
        let policy = grant(&env, api);
        let (sid, run) =
            extraction::create(&env.vault, &owner(), &env.policy(), &options, b"budget-job")
                .unwrap();
        let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
        assert_eq!(job.max_reserved_tokens, options.max_reserved_tokens);
        assert_eq!(
            job.max_reserved_cost_microusd,
            options.max_reserved_cost_microusd
        );
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: job.input_event_id,
            enabled: &enabled,
            quota: Quota {
                price_limit: if scenario == "cost" {
                    Some(PriceLimit {
                        input_microusd_per_million: 1_000_000,
                        output_microusd_per_million: 1_000_000,
                        max_reserved_microusd: 100_000,
                    })
                } else {
                    None
                },
                ..Quota::default()
            },
        };
        let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
        let call = extraction::prepare(
            &adapter,
            &index,
            &run,
            &extraction_capabilities(&env, api),
            Limits::default(),
            CallOptions::text(policy, 1024),
        )
        .unwrap();
        if scenario == "paused" {
            extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
        }
        let http = FakeHttp {
            calls: Cell::new(0),
            bytes: extraction_reply(api, false),
            fail: false,
        };
        let failure = Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        }
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap_err();
        assert_eq!(
            failure.code,
            match scenario {
                "missing-rate" => MemoryErrorCode::UnsupportedBudget,
                "paused" => MemoryErrorCode::Cancelled,
                _ => MemoryErrorCode::BudgetExceeded,
            },
            "{scenario}"
        );
        assert_eq!(http.calls.get(), 0);
        assert!(adapter.invocations().unwrap().is_empty());
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}
