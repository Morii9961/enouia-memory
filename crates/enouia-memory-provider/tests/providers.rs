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

#[test]
fn standing_policy_confirmation_is_bound_to_its_local_vault_even_for_identical_copies() {
    use enouia_memory_provider::policy;
    for revoke in [false, true] {
        let original = Env::new("policy-plan-original");
        let copy = Env::new("policy-plan-identical-copy");
        let plan = if revoke {
            let original_policy = grant(&original, Api::OpenAiResponses);
            assert_eq!(original_policy, grant(&copy, Api::OpenAiResponses));
            policy::plan_revoke(
                &original.vault,
                owner(),
                TrustedSurface::TrustedLocalCli,
                &original_policy,
            )
            .unwrap()
        } else {
            policy::plan_grant(
                &original.vault,
                owner(),
                TrustedSurface::TrustedLocalCli,
                ResourceSelector {
                    all_projects: true,
                    project_ids: vec![],
                    record_kinds: vec![RecordKind::Memory, RecordKind::SessionEvent],
                    max_sensitivity: Sensitivity::Normal,
                },
                vec![Purpose::Answer],
                &[(Api::OpenAiResponses, "synthetic-model".into())],
            )
            .unwrap()
        };
        // Copies/restores can retain all logical IDs and the exact history.
        // This synthetic pair intentionally reproduces that condition.
        assert_eq!(
            original.vault.descriptor().vault_id,
            copy.vault.descriptor().vault_id
        );
        let original_pin = original.vault.pin_current().unwrap();
        let copy_pin = copy.vault.pin_current().unwrap();
        assert_eq!(original_pin, copy_pin);
        assert_ne!(
            original.vault.managed_root().root(),
            copy.vault.managed_root().root()
        );
        let inspected = plan.hash();
        assert_eq!(
            policy::confirm(
                &copy.vault,
                &owner(),
                TrustedSurface::TrustedLocalCli,
                &plan,
                &inspected,
            )
            .unwrap_err()
            .code,
            MemoryErrorCode::PermissionDenied
        );
        assert_eq!(copy.vault.pin_current().unwrap(), copy_pin);
        assert_eq!(original.vault.pin_current().unwrap(), original_pin);
        // Refusal against a copy must not consume the original confirmation.
        policy::confirm(
            &original.vault,
            &owner(),
            TrustedSurface::TrustedLocalCli,
            &plan,
            &inspected,
        )
        .unwrap();
        let confirmed_pin = original.vault.pin_current().unwrap();
        policy::confirm(
            &original.vault,
            &owner(),
            TrustedSurface::TrustedLocalCli,
            &plan,
            &inspected,
        )
        .unwrap();
        assert_eq!(original.vault.pin_current().unwrap(), confirmed_pin);
        assert!(original.vault.verify(&confirmed_pin).unwrap().is_clean());
        assert!(copy.vault.verify(&copy_pin).unwrap().is_clean());
    }
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
fn disabled_adapter_blocks_archives_recovery_and_late_publication() {
    let env = Env::new("disabled-reads");
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
    let response = enouia_memory_contract::provider::ProviderResponse {
        text: "合成迟到答复".into(),
        finish: enouia_memory_contract::provider::FinishReason::Completed,
        tool_requests: vec![],
        input_tokens: Some(100),
        output_tokens: Some(4),
    };
    enabled.store(false, Ordering::SeqCst);
    let pin = env.vault.pin_current().unwrap();
    assert_eq!(
        adapter
            .prepare_saved(
                api,
                &capsule,
                &caps,
                Limits::default(),
                CallOptions::text(policy, 128)
            )
            .err()
            .unwrap()
            .code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(
        adapter.invocations().unwrap_err().code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(
        adapter
            .inspect_saved(&call.dispatch().dispatch_id)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(
        adapter
            .recover_local_outcome(&call.dispatch().dispatch_id)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(
        adapter
            .finish(call.dispatch(), Some(&response), None)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    assert_eq!(env.vault.pin_current().unwrap().commit_id, pin.commit_id);
    enabled.store(true, Ordering::SeqCst);
    assert_eq!(
        adapter.invocations().unwrap()[0].state,
        InvocationState::OutcomeUnknown
    );
    assert!(
        !adapter
            .recover_local_outcome(&call.dispatch().dispatch_id)
            .unwrap()
    );
    assert!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        adapter
            .inspect_saved(&call.dispatch().dispatch_id)
            .unwrap()
            .body(),
        call.inspect().body()
    );
    adapter
        .finish(call.dispatch(), Some(&response), None)
        .unwrap();
    enabled.store(false, Ordering::SeqCst);
    assert_eq!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    enabled.store(true, Ordering::SeqCst);
    assert_eq!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap()
            .unwrap()
            .text,
        response.text
    );
}
#[test]
fn provider_usage_keeps_total_input_without_double_counting_cache_breakdowns() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let mut body: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
        if api == Api::OpenAiResponses {
            body["usage"]["input_tokens_details"] =
                json!({"cached_tokens":90,"cache_write_tokens":5});
        } else {
            body["usage"]["cache_creation_input_tokens"] = json!(20);
            body["usage"]["cache_read_input_tokens"] = json!(30);
            body["usage"]["cache_creation"] =
                json!({"ephemeral_5m_input_tokens":5,"ephemeral_1h_input_tokens":15});
        }
        let response =
            enouia_memory_provider::codec::decode(api, &serde_json::to_vec(&body).unwrap())
                .unwrap();
        assert_eq!(
            response.input_tokens,
            Some(if api == Api::OpenAiResponses {
                100
            } else {
                150
            })
        );
    }
    let events = [
        json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":0}}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":110,"cache_creation_input_tokens":25,"output_tokens":4}}),
        json!({"type":"message_delta","delta":{},"usage":{"cache_read_input_tokens":null,"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ];
    let bytes = events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    let mut decoder = Decoder::new(Api::AnthropicMessages);
    decoder.push(bytes.as_bytes(), &mut |_| Ok(())).unwrap();
    let response = decoder.finish().unwrap();
    assert_eq!(response.input_tokens, Some(165));
    assert_eq!(response.output_tokens, Some(5));
}
#[test]
fn malformed_usage_fails_before_completed_publication_and_unknown_is_not_zero() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let mut absent: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
        absent.as_object_mut().unwrap().remove("usage");
        let unknown =
            enouia_memory_provider::codec::decode(api, &serde_json::to_vec(&absent).unwrap())
                .unwrap();
        assert_eq!(unknown.input_tokens, None);
        assert_eq!(unknown.output_tokens, None);
        let stream = String::from_utf8(streaming_reply(api, true))
            .unwrap()
            .lines()
            .map(|line| {
                if let Some(data) = line.strip_prefix("data: ") {
                    let mut event: serde_json::Value = serde_json::from_str(data).unwrap();
                    if event["type"] == "response.completed" {
                        event["response"]["usage"]["output_tokens"] = json!(-1);
                    } else if event["type"] == "message_delta" {
                        event["usage"]["output_tokens"] = json!(-1);
                    }
                    format!("data: {event}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let mut decoder = Decoder::new(api);
        assert!(decoder.push(stream.as_bytes(), &mut |_| Ok(())).is_err());
        assert!(decoder.finish().is_err());
        for invalid in [
            json!(-1),
            json!("100"),
            json!(1.5),
            json!(true),
            json!(9_007_199_254_740_992u64),
        ] {
            let env = Env::new("invalid-usage");
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
            let mut body: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
            body["usage"]["input_tokens"] = invalid;
            let http = FakeHttp {
                calls: Cell::new(0),
                bytes: serde_json::to_vec(&body).unwrap(),
                fail: false,
            };
            assert_eq!(
                Client {
                    transport: &http,
                    secrets: &Secrets { missing: false },
                    guard: &adapter,
                    journal: &adapter
                }
                .send(&call, &call.inspect().hash(), &NeverCancel)
                .unwrap_err()
                .code,
                MemoryErrorCode::ProviderUnavailable
            );
            let row = &adapter.invocations().unwrap()[0];
            assert_eq!(row.state, InvocationState::Failed);
            assert_eq!(row.input_tokens, None);
            assert!(
                adapter
                    .saved_response(&call.dispatch().dispatch_id)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(http.calls.get(), 1);
            assert!(
                env.vault
                    .verify(&env.vault.pin_current().unwrap())
                    .unwrap()
                    .is_clean()
            );
        }
    }
    for usage in [
        json!(false),
        json!([]),
        json!({"input_tokens":100,"output_tokens":-1}),
        json!({"input_tokens":100,"cache_read_input_tokens":"30","output_tokens":4}),
        json!({"input_tokens":9_007_199_254_740_991u64,"cache_creation_input_tokens":1,"output_tokens":4}),
    ] {
        let mut body: serde_json::Value =
            serde_json::from_slice(&reply(Api::AnthropicMessages)).unwrap();
        body["usage"] = usage;
        assert!(
            enouia_memory_provider::codec::decode(
                Api::AnthropicMessages,
                &serde_json::to_vec(&body).unwrap()
            )
            .is_err()
        );
    }
}
#[test]
fn malformed_stream_metadata_fails_immediately_without_panic_or_later_overwrite() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let start = json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}});
    let valid = json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}});
    let stop = json!({"type":"message_stop"});
    let encode = |events: Vec<serde_json::Value>| {
        events
            .iter()
            .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
            .collect::<String>()
            .into_bytes()
    };
    let malformed = vec![
        json!([]),
        json!(false),
        json!("invalid"),
        json!({"input_tokens":-1}),
        json!({"output_tokens":"4"}),
        json!({"cache_read_input_tokens":true}),
        json!({"input_tokens":9007199254740991u64,"cache_creation_input_tokens":1}),
    ];
    let mut sequences = vec![];
    for usage in &malformed {
        let mut invalid = start.clone();
        invalid["message"]["usage"] = usage.clone();
        sequences.push(vec![invalid, valid.clone(), stop.clone()]);
        let mut invalid = valid.clone();
        invalid["usage"] = usage.clone();
        sequences.push(vec![
            start.clone(),
            valid.clone(),
            invalid,
            valid.clone(),
            stop.clone(),
        ]);
    }
    // Wrong known container/stop types cannot be ignored as optional nulls.
    for delta in [
        json!(false),
        json!([]),
        json!("invalid"),
        json!(null),
        json!({"stop_reason":false}),
        json!({"stop_reason":42}),
        json!({"stop_reason":[]}),
    ] {
        let mut invalid = valid.clone();
        invalid["delta"] = delta;
        sequences.push(vec![
            start.clone(),
            valid.clone(),
            invalid,
            valid.clone(),
            stop.clone(),
        ]);
    }
    // A individually valid update must also fit the merged running total.
    let mut near_limit = start.clone();
    near_limit["message"]["usage"]["input_tokens"] = json!(9007199254740991u64);
    sequences.push(vec![near_limit, json!({"type":"message_delta","delta":{},"usage":{"cache_read_input_tokens":1}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"cache_read_input_tokens":0}}), stop.clone()]);
    for sequence in sequences {
        let mut decoder = Decoder::new(Api::AnthropicMessages);
        let result = catch_unwind(AssertUnwindSafe(|| {
            decoder.push(&encode(sequence), &mut |_| Ok(()))
        }));
        assert!(result.is_ok(), "malformed external metadata must not panic");
        assert!(
            result.unwrap().is_err(),
            "later valid metadata must not hide earlier invalid data"
        );
        assert!(
            decoder
                .push(&encode(vec![valid.clone(), stop.clone()]), &mut |_| Ok(()))
                .is_err()
        );
        assert!(decoder.finish().is_err());
    }
    for usage in [json!(null), json!({})] {
        let mut unknown = start.clone();
        unknown["message"]["usage"] = usage;
        let mut decoder = Decoder::new(Api::AnthropicMessages);
        decoder
            .push(
                &encode(vec![unknown, valid.clone(), stop.clone()]),
                &mut |_| Ok(()),
            )
            .unwrap();
        let result = decoder.finish().unwrap();
        assert_eq!(result.input_tokens, None);
        assert_eq!(result.output_tokens, Some(4));
    }
    let env = Env::new("invalid-stream-usage");
    let api = Api::AnthropicMessages;
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
    let mut invalid = start;
    invalid["message"]["usage"] = json!([]);
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: encode(vec![invalid, valid, stop]),
        fail: false,
    };
    let client = Client {
        transport: &http,
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    };
    assert_eq!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .unwrap_err()
            .code,
        MemoryErrorCode::ProviderUnavailable
    );
    assert_eq!(
        adapter.invocations().unwrap()[0].state,
        InvocationState::Failed
    );
    assert!(
        adapter
            .saved_response(&call.dispatch().dispatch_id)
            .unwrap()
            .is_none()
    );
    assert!(
        client
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .is_err()
    );
    assert_eq!(http.calls.get(), 1);
    assert!(
        env.vault
            .verify(&env.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
}

#[test]
fn later_admission_uses_reported_usage_overruns_and_never_refunds_reservations() {
    use enouia_memory_contract::record::{AnyRecord, RecordRef};
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for reported_input in [50_000u64, 1] {
            let env = Env::new("usage-quota");
            let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
            let enabled = AtomicBool::new(true);
            let first = VaultAdapter {
                vault: &env.vault,
                owner: owner(),
                input_event: input.clone(),
                enabled: &enabled,
                quota: Quota::default(),
            };
            let call = first
                .prepare_saved(
                    api,
                    &capsule,
                    &caps,
                    Limits::default(),
                    CallOptions::text(policy.clone(), 128),
                )
                .unwrap();
            let mut response: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
            response["usage"]["input_tokens"] = json!(reported_input);
            let http = FakeHttp {
                calls: Cell::new(0),
                bytes: serde_json::to_vec(&response).unwrap(),
                fail: false,
            };
            Client {
                transport: &http,
                secrets: &Secrets { missing: false },
                guard: &first,
                journal: &first,
            }
            .send(&call, &call.inspect().hash(), &NeverCancel)
            .unwrap();
            let reserved = first.invocations().unwrap()[0].reserved_tokens;
            let pin = env.vault.pin_current().unwrap();
            let AnyRecord::SessionEvent(previous) = env
                .vault
                .read_parsed(
                    &pin,
                    &RecordRef::new(
                        RecordKind::SessionEvent,
                        input.as_str(),
                        Revision::new(1).unwrap(),
                    ),
                )
                .unwrap()
            else {
                panic!("input")
            };
            let mut compile = CompileInput::local(
                "第二个合成输入",
                owner(),
                RequestId::from_random(env.vault.random_id_bytes()),
            );
            compile.session_id = Some(previous.session_id.clone());
            compile.branch_id = Some(previous.branch_id.clone());
            let next_input = session::save_input(
                &env.vault,
                &owner(),
                &previous.session_id,
                &previous.branch_id,
                &compile.query,
                &compile.request_id,
                b"second-budget-input",
            )
            .unwrap()
            .id;
            let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
            let capsule = compiler::compile_for_destination(
                &env.vault,
                &index,
                &compile,
                &Destination {
                    kind: DestinationKind::ExternalProvider,
                    provider_binding: Some(caps.binding.clone()),
                },
            )
            .unwrap();
            let mut next = VaultAdapter {
                vault: &env.vault,
                owner: owner(),
                input_event: next_input,
                enabled: &enabled,
                quota: Quota::default(),
            };
            let prepared = next
                .prepare_saved(
                    api,
                    &capsule.capsule.capsule_id,
                    &caps,
                    Limits::default(),
                    CallOptions::text(policy, 128),
                )
                .unwrap();
            let next_reserved =
                prepared.estimated_input_tokens() + prepared.dispatch().output.max_output_tokens;
            let reported_total = reported_input + 4;
            next.quota.max_reserved_tokens = next_reserved
                + if reported_input == 1 {
                    assert!(reported_total < reserved);
                    reported_total // Actual use never refunds the original floor.
                } else {
                    assert!(reported_total > reserved);
                    reserved // An underestimated reservation cannot hide overrun.
                };
            let pin = env.vault.pin_current().unwrap();
            assert_eq!(
                Client {
                    transport: &http,
                    secrets: &Secrets { missing: false },
                    guard: &next,
                    journal: &next
                }
                .send(&prepared, &prepared.inspect().hash(), &NeverCancel)
                .unwrap_err()
                .code,
                MemoryErrorCode::BudgetExceeded
            );
            assert_eq!(http.calls.get(), 1);
            assert_eq!(env.vault.pin_current().unwrap().commit_id, pin.commit_id);
            assert_eq!(next.invocations().unwrap().len(), 1);
            assert!(env.vault.verify(&pin).unwrap().is_clean());
        }
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
fn already_closed_input_never_admits_or_sends_a_prepared_provider_call() {
    use enouia_memory_contract::session::EventKind;
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for kind in [
            EventKind::AssistantCompleted,
            EventKind::TurnCancelled,
            EventKind::TurnFailed,
        ] {
            let env = Env::new("closed-provider-turn");
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
            session::append_output(
                &env.vault,
                &owner(),
                &input,
                kind,
                if kind == EventKind::AssistantCompleted {
                    Some("合成本地答复")
                } else {
                    None
                },
                b"owner-closed-turn",
            )
            .unwrap();
            let pin = env.vault.pin_current().unwrap();
            assert!(
                !adapter.claim(&call).unwrap(),
                "a closed turn is not a new send permission"
            );
            let http = FakeHttp {
                calls: Cell::new(0),
                bytes: reply(api),
                fail: false,
            };
            let client = Client {
                transport: &http,
                secrets: &Secrets { missing: false },
                guard: &adapter,
                journal: &adapter,
            };
            assert_eq!(
                client
                    .send(&call, &call.inspect().hash(), &NeverCancel)
                    .unwrap_err()
                    .code,
                MemoryErrorCode::IdempotencyConflict
            );
            assert_eq!(http.calls.get(), 0);
            assert!(adapter.invocations().unwrap().is_empty());
            assert_eq!(env.vault.pin_current().unwrap(), pin);
            assert!(env.vault.verify(&pin).unwrap().is_clean());
        }
    }
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
fn sse_rejects_out_of_order_stops_and_errors_cannot_be_finished_or_resumed() {
    let start = json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}});
    let block =
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}});
    let block_end = json!({"type":"content_block_stop","index":0});
    let delta = json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":0}});
    let conflicting = json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":1}});
    let stop = json!({"type":"message_stop"});
    for events in [
        vec![delta.clone(), start.clone(), stop.clone()],
        vec![
            start.clone(),
            block.clone(),
            delta.clone(),
            block_end.clone(),
            stop.clone(),
        ],
        vec![start.clone(), delta.clone(), conflicting, stop.clone()],
        vec![start, delta, block, block_end, stop],
    ] {
        let bytes = events
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        let mut decoder = Decoder::new(Api::AnthropicMessages);
        assert_eq!(
            decoder
                .push(bytes.as_bytes(), &mut |_| Ok(()))
                .unwrap_err()
                .code,
            MemoryErrorCode::ProviderUnavailable
        );
        assert!(
            decoder
                .push(b"data: {\"type\":\"message_stop\"}\n\n", &mut |_| Ok(()))
                .is_err()
        );
        assert!(decoder.finish().is_err());
    }
    // A valid terminal followed by a malformed/error event cannot later be
    // exposed as successful by a caller that tries finish after push failed.
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let mut decoder = Decoder::new(api);
        decoder
            .push(&streaming_reply(api, true), &mut |_| Ok(()))
            .unwrap();
        assert!(
            decoder
                .push(b"data: {\"type\":\"error\"}\n\n", &mut |_| Ok(()))
                .is_err()
        );
        assert!(decoder.finish().is_err());
    }
}
#[test]
fn anthropic_multiple_top_level_deltas_keep_stop_reason_and_cumulative_usage() {
    let events = [
        json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}}),
        json!({"type":"message_delta","delta":{"stop_reason":null},"usage":{"output_tokens":1}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":110,"output_tokens":4}}),
        json!({"type":"message_delta","delta":{"stop_reason":null},"usage":{"output_tokens":5}}),
        json!({"type":"message_delta","delta":{},"usage":{"input_tokens":null,"output_tokens":6}}),
        json!({"type":"message_stop"}),
    ];
    let bytes = events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    let mut decoder = Decoder::new(Api::AnthropicMessages);
    decoder.push(bytes.as_bytes(), &mut |_| Ok(())).unwrap();
    let response = decoder.finish().unwrap();
    assert_eq!(
        response.finish,
        enouia_memory_contract::provider::FinishReason::Completed
    );
    assert_eq!(response.input_tokens, Some(110));
    assert_eq!(response.output_tokens, Some(6));
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
fn both_streams_preserve_whitespace_and_ignore_empty_deltas() {
    use enouia_memory_contract::{provider::FinishReason, record::RecordRef, session::EventKind};
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for length in [false, true] {
            for only_whitespace in [false, true] {
                let env = Env::new("stream-whitespace");
                let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
                let pin = env.vault.pin_current().unwrap();
                let enouia_memory_contract::record::AnyRecord::SessionEvent(user) = env
                    .vault
                    .read_parsed(
                        &pin,
                        &RecordRef::new(
                            RecordKind::SessionEvent,
                            input.as_str(),
                            Revision::new(1).unwrap(),
                        ),
                    )
                    .unwrap()
                else {
                    panic!("input")
                };
                // A blank owner input is still rejected without publication.
                assert!(
                    session::save_input(
                        &env.vault,
                        &owner(),
                        &user.session_id,
                        &user.branch_id,
                        " \n\t",
                        &RequestId::from_random(env.vault.random_id_bytes()),
                        b"blank-input"
                    )
                    .is_err()
                );
                assert_eq!(env.vault.pin_current().unwrap(), pin);
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
                let segments = if only_whitespace {
                    vec![" ", "", "\n", "\t", "\r\n"]
                } else {
                    vec![" ", "合成", "", "\n", "\t答复", "\r\n"]
                };
                let text = segments.concat();
                let mut events = match api {
                    Api::OpenAiResponses => vec![],
                    Api::AnthropicMessages => vec![
                        json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}}),
                        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":segments[0]}}),
                    ],
                };
                for segment in &segments[usize::from(api == Api::AnthropicMessages)..] {
                    events.push(match api {
                        Api::OpenAiResponses => json!({"type":"response.output_text.delta","delta":segment}),
                        Api::AnthropicMessages => json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":segment}}),
                    });
                }
                match api {
                    Api::OpenAiResponses => {
                        let mut response: serde_json::Value =
                            serde_json::from_slice(&reply(api)).unwrap();
                        response["output"][0]["content"][0]["text"] = json!(text);
                        if length {
                            response["status"] = json!("incomplete");
                            response["incomplete_details"] = json!({"reason":"max_output_tokens"});
                        }
                        events.push(json!({"type":if length {"response.incomplete"} else {"response.completed"},"response":response}));
                    }
                    Api::AnthropicMessages => {
                        events.extend([
                            json!({"type":"content_block_stop","index":0}),
                            json!({"type":"message_delta","delta":{"stop_reason":if length {"max_tokens"} else {"end_turn"}},"usage":{"output_tokens":4}}),
                            json!({"type":"message_stop"}),
                        ]);
                    }
                }
                let http = FakeHttp {
                    calls: Cell::new(0),
                    bytes: events
                        .iter()
                        .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
                        .collect::<String>()
                        .into_bytes(),
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
                assert_eq!(response.text, text);
                assert_eq!(
                    response.finish,
                    if length {
                        FinishReason::Length
                    } else {
                        FinishReason::Completed
                    }
                );
                let saved = adapter
                    .saved_response(&call.dispatch().dispatch_id)
                    .unwrap()
                    .unwrap();
                assert_eq!(saved.text, text);
                assert_eq!(saved.finish, response.finish);
                let pin = env.vault.pin_current().unwrap();
                let chunks = session::events(&env.vault, &pin, &user.session_id, &user.branch_id)
                    .unwrap()
                    .into_iter()
                    .filter(|e| e.kind == EventKind::AssistantChunk)
                    .map(|e| session::text(&env.vault, &pin, &e).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    chunks,
                    segments
                        .iter()
                        .filter(|s| !s.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                );
                assert_eq!(http.calls.get(), 1);
                assert!(env.vault.verify(&pin).unwrap().is_clean());
            }
        }
    }
}

#[test]
fn empty_text_terminal_results_are_persisted_with_usage() {
    use enouia_memory_contract::{provider::FinishReason, session::EventKind};
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for streaming in [false, true] {
            for length in [false, true] {
                let env = Env::new("empty-provider-output");
                let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
                let enabled = AtomicBool::new(true);
                let adapter = VaultAdapter {
                    vault: &env.vault,
                    owner: owner(),
                    input_event: input.clone(),
                    enabled: &enabled,
                    quota: Quota::default(),
                };
                let mut options = CallOptions::text(policy, 128);
                options.streaming = streaming;
                let call = adapter
                    .prepare_saved(api, &capsule, &caps, Limits::default(), options)
                    .unwrap();
                let before = env.vault.pin_current().unwrap();
                assert!(
                    session::append_output(
                        &env.vault,
                        &owner(),
                        &input,
                        EventKind::AssistantChunk,
                        Some(""),
                        b"empty-chunk"
                    )
                    .is_err()
                );
                assert_eq!(env.vault.pin_current().unwrap(), before);
                let mut response: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
                match api {
                    Api::OpenAiResponses => {
                        response["output"] = json!([]);
                        if length {
                            response["status"] = json!("incomplete");
                            response["incomplete_details"] = json!({"reason":"max_output_tokens"});
                        }
                    }
                    Api::AnthropicMessages => {
                        response["content"] = json!([]);
                        response["stop_reason"] =
                            json!(if length { "max_tokens" } else { "end_turn" });
                    }
                }
                let body = if streaming {
                    let events = match api {
                        Api::OpenAiResponses => vec![
                            json!({"type":"response.output_text.delta","delta":""}),
                            json!({"type":if length { "response.incomplete" } else { "response.completed" },"response":response}),
                        ],
                        Api::AnthropicMessages => vec![
                            json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":100,"output_tokens":0}}}),
                            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":""}}),
                            json!({"type":"content_block_stop","index":0}),
                            json!({"type":"message_delta","delta":{"stop_reason":if length { "max_tokens" } else { "end_turn" }},"usage":{"output_tokens":4}}),
                            json!({"type":"message_stop"}),
                        ],
                    };
                    events
                        .iter()
                        .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
                        .collect::<String>()
                        .into_bytes()
                } else {
                    serde_json::to_vec(&response).unwrap()
                };
                let http = FakeHttp {
                    calls: Cell::new(0),
                    bytes: body,
                    fail: false,
                };
                let result = Client {
                    transport: &http,
                    secrets: &Secrets { missing: false },
                    guard: &adapter,
                    journal: &adapter,
                }
                .send(&call, &call.inspect().hash(), &NeverCancel)
                .unwrap();
                assert_eq!(result.text, "");
                assert_eq!(
                    result.finish,
                    if length {
                        FinishReason::Length
                    } else {
                        FinishReason::Completed
                    }
                );
                let saved = adapter
                    .saved_response(&call.dispatch().dispatch_id)
                    .unwrap()
                    .unwrap();
                assert_eq!(saved.text, "");
                assert_eq!(saved.finish, result.finish);
                assert_eq!(saved.input_tokens, Some(100));
                assert_eq!(saved.output_tokens, Some(4));
                // One admission and one terminal, no invented/empty partial chunk.
                let pin = env.vault.pin_current().unwrap();
                assert_eq!(pin.sequence, before.sequence + 2);
                assert!(
                    !adapter
                        .recover_local_outcome(&call.dispatch().dispatch_id)
                        .unwrap()
                );
                assert!(!adapter.claim(&call).unwrap());
                assert_eq!(http.calls.get(), 1);
                assert!(env.vault.verify(&pin).unwrap().is_clean());
            }
        }
    }
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
fn terminal_event_and_exact_outcome_publish_together_across_reopen() {
    use enouia_memory_contract::provider::{FinishReason, ProviderResponse};
    use enouia_memory_vault::{
        Vault, VaultOptions,
        fault::{FaultAction, FaultPoint, Faults},
    };
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for finish in [FinishReason::Completed, FinishReason::Length] {
            for point in [FaultPoint::BeforeCurrent, FaultPoint::AfterCurrent] {
                let env = Env::new("atomic-provider-terminal");
                let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
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
                let enabled = AtomicBool::new(true);
                let adapter = VaultAdapter {
                    vault: &interrupted,
                    owner: owner(),
                    input_event: input.clone(),
                    enabled: &enabled,
                    quota: Quota::default(),
                };
                let mut options = CallOptions::text(policy, 128);
                options.streaming = true;
                let call = adapter
                    .prepare_saved(api, &capsule, &caps, Limits::default(), options)
                    .unwrap();
                assert!(adapter.claim(&call).unwrap());
                adapter.chunk(call.dispatch(), 1, "合成答复").unwrap();
                let before = interrupted.pin_current().unwrap();
                let response = ProviderResponse {
                    text: "合成答复".into(),
                    finish,
                    tool_requests: vec![],
                    input_tokens: Some(100),
                    output_tokens: Some(4),
                };
                faults.arm(point, 1, FaultAction::Fail(112));
                assert!(
                    adapter
                        .finish(call.dispatch(), Some(&response), None)
                        .is_err()
                );
                let restored = Vault::open(
                    &root,
                    None,
                    env.clock.clone(),
                    std::sync::Arc::new(enouia_memory_vault::OsIdSource),
                    VaultOptions::default(),
                )
                .unwrap();
                let recovered = VaultAdapter {
                    vault: &restored,
                    owner: owner(),
                    input_event: input,
                    enabled: &enabled,
                    quota: Quota::default(),
                };
                let after = restored.pin_current().unwrap();
                let row = recovered.invocations().unwrap().remove(0);
                if point == FaultPoint::BeforeCurrent {
                    assert_eq!(after, before);
                    assert_eq!(row.state, InvocationState::OutcomeUnknown);
                    assert!(row.terminal_event_id.is_none());
                    assert!(
                        recovered
                            .saved_response(&call.dispatch().dispatch_id)
                            .unwrap()
                            .is_none()
                    );
                } else {
                    assert_eq!(after.sequence, before.sequence + 1);
                    assert_eq!(
                        row.state,
                        if finish == FinishReason::Completed {
                            InvocationState::Completed
                        } else {
                            InvocationState::Length
                        }
                    );
                    assert!(row.terminal_event_id.is_some());
                    assert_eq!(row.input_tokens, Some(100));
                    assert_eq!(row.output_tokens, Some(4));
                    let saved = recovered
                        .saved_response(&call.dispatch().dispatch_id)
                        .unwrap()
                        .unwrap();
                    assert_eq!(saved.finish, finish);
                    assert_eq!(saved.text, response.text);
                    assert_eq!(saved.input_tokens, response.input_tokens);
                    assert_eq!(saved.output_tokens, response.output_tokens);
                    recovered
                        .finish(call.dispatch(), Some(&response), None)
                        .unwrap();
                    assert_eq!(restored.pin_current().unwrap(), after);
                    let changed = ProviderResponse {
                        input_tokens: Some(101),
                        ..response
                    };
                    assert_eq!(
                        recovered
                            .finish(call.dispatch(), Some(&changed), None)
                            .unwrap_err()
                            .code,
                        MemoryErrorCode::IdempotencyConflict
                    );
                    assert_eq!(restored.pin_current().unwrap(), after);
                }
                assert!(
                    !recovered
                        .recover_local_outcome(&call.dispatch().dispatch_id)
                        .unwrap()
                );
                assert!(!recovered.claim(&call).unwrap());
                assert!(restored.verify(&after).unwrap().is_clean());
            }
        }
    }
}

#[test]
fn terminal_metadata_cannot_bind_a_different_admission_or_invalid_state() {
    use enouia_memory_contract::session::EventKind;
    let env = Env::new("terminal-metadata");
    let api = Api::OpenAiResponses;
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
    let pin = env.vault.pin_current().unwrap();
    let valid = session::InvocationCompletion {
        dispatch_id: call.dispatch().dispatch_id.clone(),
        state: InvocationState::Completed,
        input_tokens: Some(100),
        output_tokens: Some(4),
        error_code: None,
    };
    let wrong_dispatch = session::InvocationCompletion {
        dispatch_id: DispatchId::from_random(env.vault.random_id_bytes()),
        ..valid.clone()
    };
    let wrong_kind = session::InvocationCompletion {
        state: InvocationState::Length,
        ..valid.clone()
    };
    let invalid_error = session::InvocationCompletion {
        error_code: Some(MemoryErrorCode::ProviderUnavailable),
        ..valid.clone()
    };
    for (index, metadata) in [wrong_dispatch, wrong_kind, invalid_error]
        .iter()
        .enumerate()
    {
        assert!(
            session::append_invocation_output(
                &env.vault,
                &owner(),
                &input,
                EventKind::AssistantCompleted,
                Some("合成答复"),
                format!("invalid-terminal:{index}").as_bytes(),
                None,
                metadata
            )
            .is_err()
        );
        assert_eq!(env.vault.pin_current().unwrap(), pin);
        assert_eq!(
            adapter.invocations().unwrap()[0].state,
            InvocationState::OutcomeUnknown
        );
    }
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn owner_can_interrupt_an_unknown_call_locally_then_start_a_new_explicit_turn() {
    use enouia_memory_contract::{
        record::{AnyRecord, RecordRef},
        session::EventKind,
    };
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("owner-interrupt");
        let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: input.clone(),
            enabled: &enabled,
            quota: Quota::default(),
        };
        let mut options = CallOptions::text(policy.clone(), 128);
        options.streaming = true;
        let call = adapter
            .prepare_saved(api, &capsule, &caps, Limits::default(), options)
            .unwrap();
        assert!(adapter.claim(&call).unwrap());
        adapter.chunk(call.dispatch(), 1, "合成片段").unwrap();
        let root = enouia_memory_vault::verify_data_root(
            env.vault.managed_root().root(),
            &enouia_memory_vault::RootPolicy::default(),
        )
        .unwrap();
        let restored = enouia_memory_vault::Vault::open(
            &root,
            None,
            env.clock.clone(),
            std::sync::Arc::new(enouia_memory_vault::OsIdSource),
            Default::default(),
        )
        .unwrap();
        let recovered = VaultAdapter {
            vault: &restored,
            owner: owner(),
            input_event: input.clone(),
            enabled: &enabled,
            quota: Quota::default(),
        };
        let before = restored.pin_current().unwrap();
        let plan = recovered
            .plan_interruption(
                &call.dispatch().dispatch_id,
                TrustedSurface::TrustedLocalCli,
            )
            .unwrap();
        let view: serde_json::Value = serde_json::from_slice(plan.inspect()).unwrap();
        assert_eq!(view["remote_outcome"], "unknown");
        assert_eq!(view["old_input_resend_allowed"], false);
        assert_eq!(view["partial_text_retained"], true);
        assert!(!String::from_utf8_lossy(plan.inspect()).contains("合成片段"));
        assert_eq!(restored.pin_current().unwrap(), before);
        assert_eq!(
            recovered
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &sha256(b"wrong"))
                .unwrap_err()
                .code,
            MemoryErrorCode::PermissionDenied
        );
        assert_eq!(
            recovered
                .confirm_interruption(&plan, TrustedSurface::TrustedWindowsApp, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::PermissionDenied
        );
        enabled.store(false, Ordering::SeqCst);
        assert_eq!(
            recovered
                .plan_interruption(
                    &call.dispatch().dispatch_id,
                    TrustedSurface::TrustedLocalCli
                )
                .err()
                .unwrap()
                .code,
            MemoryErrorCode::VaultLocked
        );
        assert_eq!(
            recovered
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::VaultLocked
        );
        enabled.store(true, Ordering::SeqCst);
        let wrong_owner = VaultAdapter {
            vault: &restored,
            owner: agent(),
            input_event: input.clone(),
            enabled: &enabled,
            quota: Quota::default(),
        };
        assert_eq!(
            wrong_owner
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::PermissionDenied
        );
        let other = Env::new("different-local-root");
        let wrong_root = VaultAdapter {
            vault: &other.vault,
            owner: owner(),
            input_event: input.clone(),
            enabled: &enabled,
            quota: Quota::default(),
        };
        assert_eq!(
            wrong_root
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::PermissionDenied
        );
        env.clock.set(T0 + 600_001);
        assert_eq!(
            recovered
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::PermissionDenied
        );
        assert_eq!(restored.pin_current().unwrap(), before);
        let fresh = recovered
            .plan_interruption(
                &call.dispatch().dispatch_id,
                TrustedSurface::TrustedLocalCli,
            )
            .unwrap();
        let terminal = recovered
            .confirm_interruption(&fresh, TrustedSurface::TrustedLocalCli, &fresh.hash())
            .unwrap();
        let after = restored.pin_current().unwrap();
        assert_eq!(after.sequence, before.sequence + 1);
        assert_eq!(
            recovered
                .confirm_interruption(&fresh, TrustedSurface::TrustedLocalCli, &fresh.hash())
                .unwrap(),
            terminal
        );
        assert_eq!(restored.pin_current().unwrap(), after);
        let row = recovered.invocations().unwrap().remove(0);
        assert_eq!(row.state, InvocationState::Cancelled);
        assert_eq!(row.error_code, Some(MemoryErrorCode::Cancelled));
        assert_eq!(row.input_tokens, None);
        assert_eq!(row.output_tokens, None);
        assert_eq!(row.terminal_event_id, Some(terminal.clone()));
        assert_eq!(row.reserved_tokens, call.estimated_input_tokens() + 128);
        assert!(!recovered.claim(&call).unwrap());
        assert!(
            !recovered
                .recover_local_outcome(&call.dispatch().dispatch_id)
                .unwrap()
        );
        assert!(
            recovered
                .saved_response(&call.dispatch().dispatch_id)
                .unwrap()
                .is_none()
        );
        let AnyRecord::SessionEvent(user) = restored
            .read_parsed(
                &after,
                &RecordRef::new(
                    RecordKind::SessionEvent,
                    input.as_str(),
                    Revision::new(1).unwrap(),
                ),
            )
            .unwrap()
        else {
            panic!("input")
        };
        let events = session::events(&restored, &after, &user.session_id, &user.branch_id).unwrap();
        assert!(events.iter().any(|e| e.kind == EventKind::AssistantChunk
            && session::text(&restored, &after, e).unwrap() == "合成片段"));
        assert_eq!(events.last().unwrap().kind, EventKind::TurnCancelled);
        let mut compile = CompileInput::local(
            "新的明确合成输入",
            owner(),
            RequestId::from_random(restored.random_id_bytes()),
        );
        compile.session_id = Some(user.session_id.clone());
        compile.branch_id = Some(user.branch_id.clone());
        let next_input = session::save_input(
            &restored,
            &owner(),
            &user.session_id,
            &user.branch_id,
            &compile.query,
            &compile.request_id,
            b"after-local-interruption",
        )
        .unwrap()
        .id;
        let (index, _) = enouia_memory_index::Index::rebuild(&restored).unwrap();
        let capsule = compiler::compile_for_destination(
            &restored,
            &index,
            &compile,
            &Destination {
                kind: DestinationKind::ExternalProvider,
                provider_binding: Some(caps.binding.clone()),
            },
        )
        .unwrap();
        let mut next = VaultAdapter {
            vault: &restored,
            owner: owner(),
            input_event: next_input,
            enabled: &enabled,
            quota: Quota::default(),
        };
        let prepared = next
            .prepare_saved(
                api,
                &capsule.capsule.capsule_id,
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
        next.quota.max_reserved_tokens =
            prepared.estimated_input_tokens() + 128 + row.reserved_tokens - 1;
        assert_eq!(
            Client {
                transport: &http,
                secrets: &Secrets { missing: false },
                guard: &next,
                journal: &next
            }
            .send(&prepared, &prepared.inspect().hash(), &NeverCancel)
            .unwrap_err()
            .code,
            MemoryErrorCode::BudgetExceeded
        );
        assert_eq!(http.calls.get(), 0);
        next.quota = Quota::default();
        Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &next,
            journal: &next,
        }
        .send(&prepared, &prepared.inspect().hash(), &NeverCancel)
        .unwrap();
        assert_eq!(http.calls.get(), 1);
        assert!(
            restored
                .verify(&restored.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}

#[test]
fn interruption_cannot_replace_a_saved_legacy_response_or_known_result() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("interrupt-legacy-result");
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
        let plan = adapter
            .plan_interruption(
                &call.dispatch().dispatch_id,
                TrustedSurface::TrustedLocalCli,
            )
            .unwrap();
        session::append_output(
            &env.vault,
            &owner(),
            &input,
            enouia_memory_contract::session::EventKind::AssistantCompleted,
            Some("合成已保存答复"),
            format!("provider-terminal:{}", call.dispatch().dispatch_id).as_bytes(),
        )
        .unwrap();
        let pin = env.vault.pin_current().unwrap();
        assert!(
            adapter
                .plan_interruption(
                    &call.dispatch().dispatch_id,
                    TrustedSurface::TrustedLocalCli
                )
                .is_err()
        );
        assert_eq!(
            adapter
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .unwrap_err()
                .code,
            MemoryErrorCode::IdempotencyConflict
        );
        assert_eq!(env.vault.pin_current().unwrap(), pin);
        assert!(
            adapter
                .recover_local_outcome(&call.dispatch().dispatch_id)
                .unwrap()
        );
        assert_eq!(
            adapter.invocations().unwrap()[0].state,
            InvocationState::Completed
        );
        assert!(
            adapter
                .confirm_interruption(&plan, TrustedSurface::TrustedLocalCli, &plan.hash())
                .is_err()
        );
        assert_eq!(
            adapter
                .saved_response(&call.dispatch().dispatch_id)
                .unwrap()
                .unwrap()
                .text,
            "合成已保存答复"
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
fn extraction_preview_preserves_exact_quotes_and_current_owner_edits_without_writes() {
    use enouia_memory_provider::extraction::{self, ExtractionApplication};
    let env = Env::new("extraction-preview");
    let api = Api::AnthropicMessages;
    let source = extraction_source(&env);
    let mut options = extraction_options(source.clone(), api);
    // Select a proper UTF-8 subrange, so citation offsets cannot be relative
    // to the selected snippet or confused with character offsets.
    options.sources[0].start = "偏好".len() as u64;
    options.sources[0].end -= "。".len() as u64;
    let policy = grant(&env, api);
    let (sid, run) = extraction::create(
        &env.vault,
        &owner(),
        &env.policy(),
        &options,
        b"preview-run",
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
    assert_eq!(
        extraction::preview(&adapter, &run).unwrap_err().code,
        MemoryErrorCode::NotFound
    );
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
    let mut wire: serde_json::Value =
        serde_json::from_slice(&extraction_reply(api, false)).unwrap();
    let mut claims: serde_json::Value =
        serde_json::from_str(wire["content"][0]["text"].as_str().unwrap()).unwrap();
    claims["candidates"][0]["quote"] = json!("合成红色");
    wire["content"][0]["text"] = json!(claims.to_string());
    let http = FakeHttp {
        calls: Cell::new(0),
        bytes: serde_json::to_vec(&wire).unwrap(),
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
    let pin = env.vault.pin_current().unwrap();
    let shown = extraction::preview(&adapter, &run).unwrap();
    assert_eq!(env.vault.pin_current().unwrap().commit_id, pin.commit_id);
    assert_eq!(shown.dispatch_id, call.dispatch().dispatch_id);
    assert_eq!(shown.response_hash, sha256(claims.to_string().as_bytes()));
    assert_eq!(shown.items.len(), 2);
    let text = "偏好合成红色，并倾向合成蓝色。";
    for item in &shown.items {
        let citation = &item.citation;
        assert_eq!(
            &text[citation.start as usize..citation.end as usize],
            citation.quote
        );
        assert_eq!(citation.start, text.find(&citation.quote).unwrap() as u64);
        assert_eq!(citation.source.source_id, source);
        assert_eq!(citation.selected_start, options.sources[0].start);
        assert_eq!(citation.selected_end, options.sources[0].end);
        assert_eq!(
            citation.selected_text,
            text[options.sources[0].start as usize..options.sources[0].end as usize]
        );
        assert_eq!(
            citation.evidence_class,
            enouia_memory_contract::common::EvidenceClass::UserStatement
        );
        assert_eq!(
            citation.speaker_role,
            enouia_memory_contract::common::SpeakerRole::User
        );
        assert_eq!(citation.sensitivity, Sensitivity::Normal);
        assert_eq!(
            citation.time_precision,
            enouia_memory_contract::common::TimePrecision::Second
        );
        assert!(citation.occurred_at.is_some());
        assert_eq!(item.application, ExtractionApplication::AwaitingCursor);
        assert!(item.current_candidate.is_none());
    }
    let applied = extraction::apply_next(&adapter, &run).unwrap();
    let id = applied.candidates[0].as_ref().unwrap();
    enouia_memory_govern::propose::edit_candidate(
        &env.vault,
        id,
        Revision::new(1).unwrap(),
        &enouia_memory_govern::propose::CandidateEdit {
            content: Some("主人修订的合成红色陈述".into()),
            details: None,
            evidence: None,
            reason: None,
        },
        &owner(),
        b"preview-owner-edit",
    )
    .unwrap();
    extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
    let pin = env.vault.pin_current().unwrap();
    let edited = extraction::preview(&adapter, &run).unwrap();
    assert_eq!(env.vault.pin_current().unwrap().commit_id, pin.commit_id);
    assert_eq!(
        edited.items[0].proposal.content,
        shown.items[0].proposal.content
    );
    let candidate = edited.items[0].current_candidate.as_ref().unwrap();
    assert_eq!(candidate.revision.get(), 2);
    assert_eq!(candidate.proposed_content, "主人修订的合成红色陈述");
    assert_eq!(
        edited.items[0].application,
        ExtractionApplication::CandidateRecorded
    );
    assert_eq!(
        edited.items[1].application,
        ExtractionApplication::AwaitingCursor
    );
    assert_eq!(
        edited.job.state,
        enouia_memory_contract::extraction::ExtractionState::Paused
    );
    enabled.store(false, Ordering::SeqCst);
    assert_eq!(
        extraction::preview(&adapter, &run).unwrap_err().code,
        MemoryErrorCode::VaultLocked
    );
    enabled.store(true, Ordering::SeqCst);
    let wrong_owner = VaultAdapter {
        owner: agent(),
        ..adapter
    };
    assert_eq!(
        extraction::preview(&wrong_owner, &run).unwrap_err().code,
        MemoryErrorCode::PermissionDenied
    );
    assert_eq!(http.calls.get(), 1);
}
#[test]
fn extraction_v2_keeps_unknown_time_branch_and_negation_context() {
    use enouia_memory_contract::{
        commit::OperationKind,
        common::TimePrecision,
        json::{Knowable, canonical_bytes},
        ports::{CommitRequest, IdempotencyScope, StagedRecord},
        record::{AnyRecord, RecordRef},
    };
    use enouia_memory_provider::extraction;
    let env = Env::new("extraction-context");
    let api = Api::OpenAiResponses;
    let id = extraction_source(&env);
    let pin = env.vault.pin_current().unwrap();
    let AnyRecord::Source(mut source) = env
        .vault
        .read_parsed(
            &pin,
            &RecordRef::new(RecordKind::Source, id.as_str(), Revision::new(1).unwrap()),
        )
        .unwrap()
    else {
        panic!("source")
    };
    // A synthetic source revision with unknown occurrence/branch and explicit
    // qualifiers. Its archival timestamp must not become an occurrence time.
    let text = "并不偏好合成红色；只有在合成条件成立时才倾向合成蓝色。";
    source.revision = Revision::new(2).unwrap();
    source.manual_assertion.as_mut().unwrap().input_text = text.into();
    source.content_hash = sha256(text.as_bytes());
    source.occurred_at = None;
    source.time_precision = TimePrecision::Unknown;
    source.branch_id = Knowable::Unknown;
    let payload = canonical_bytes(&serde_json::to_value(&source).unwrap()).unwrap();
    env.vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(env.vault.random_id_bytes()),
            expected_commit_id: Some(pin.commit_id),
            principal: owner(),
            operation_kind: OperationKind::Import,
            idempotency: IdempotencyScope {
                principal_id: owner().actor_id,
                operation_kind: OperationKind::Import,
                key_hash: sha256(b"context-source-revision"),
            },
            request_payload_hash: sha256(&payload),
            expected_revisions: vec![(
                RecordKind::Source,
                id.to_string(),
                Some(Revision::new(1).unwrap()),
            )],
            records: vec![StagedRecord {
                record_kind: RecordKind::Source,
                record_id: id.to_string(),
                revision: source.revision,
                bytes: payload,
            }],
            objects: vec![],
        })
        .unwrap();
    let mut options = extraction_options(id, api);
    options.sources[0].source.source_revision = source.revision;
    options.sources[0].end = text.len() as u64;
    let policy = grant(&env, api);
    let (sid, run) = extraction::create(
        &env.vault,
        &owner(),
        &env.policy(),
        &options,
        b"context-run",
    )
    .unwrap();
    let job = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
    assert_eq!(job.prompt_version, "extract-text-2");
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
    let wire: serde_json::Value = serde_json::from_slice(call.inspect().body()).unwrap();
    let prompt: serde_json::Value =
        serde_json::from_str(wire["input"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(prompt["prompt_version"], "extract-text-2");
    assert_eq!(prompt["sources"][0]["text"], text);
    assert_eq!(
        prompt["sources"][0]["source_context"]["occurred_at"],
        serde_json::Value::Null
    );
    assert_eq!(
        prompt["sources"][0]["source_context"]["captured_at"],
        json!(source.captured_at)
    );
    assert_eq!(
        prompt["sources"][0]["source_context"]["time_precision"],
        "unknown"
    );
    assert_eq!(
        prompt["sources"][0]["source_context"]["branch_id"],
        "unknown"
    );
    assert_eq!(prompt["sources"][0]["selection"]["end"], text.len());
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
    let shown = extraction::preview(&adapter, &run).unwrap();
    for item in &shown.items {
        assert_eq!(item.citation.selected_text, text);
        assert_eq!(item.citation.occurred_at, None);
        assert_eq!(item.citation.time_precision, TimePrecision::Unknown);
        assert_eq!(item.citation.branch_id, Knowable::Unknown);
        assert_eq!(item.citation.captured_at, source.captured_at);
    }
    // Containment does not assert model correctness; the selected negation
    // remains available to the owner and no canonical record is accepted.
    assert!(
        enouia_memory_govern::canonical_memories(
            &env.vault,
            &env.vault.pin_current().unwrap(),
            enouia_memory_govern::Canonical::AllStatuses
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(http.calls.get(), 1);
    assert!(
        env.vault
            .verify(&env.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
}
#[test]
fn extraction_does_not_promote_agent_consent_claims_to_user_preferences() {
    use enouia_memory_contract::{
        candidate::CandidateStatus,
        commit::OperationKind,
        common::{EvidenceClass, Locator, SpeakerRole},
        json::canonical_bytes,
        ports::{CommitRequest, IdempotencyScope, StagedRecord},
        record::{AnyRecord, RecordRef},
        source::{AgentSubmission, SourceKind},
    };
    use enouia_memory_provider::extraction;
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("extraction-agent-consent");
        let manual = extraction_source(&env);
        let pin = env.vault.pin_current().unwrap();
        let AnyRecord::Source(mut source) = env
            .vault
            .read_parsed(
                &pin,
                &RecordRef::new(
                    RecordKind::Source,
                    manual.as_str(),
                    Revision::new(1).unwrap(),
                ),
            )
            .unwrap()
        else {
            panic!("source")
        };
        let text = source.manual_assertion.take().unwrap().input_text;
        source.source_id = SourceId::from_random(env.vault.random_id_bytes());
        source.source_kind = SourceKind::AgentSubmission;
        source.speaker_role = SpeakerRole::Assistant;
        source.evidence_class = EvidenceClass::ModelClaim;
        source.locator = Locator::ByteRange {
            start: 0,
            end: text.len() as u64,
        };
        source.agent_submission = Some(AgentSubmission {
            submitting_principal: agent(),
            submitted_text: text,
            claimed_user_consent: true,
        });
        let payload = canonical_bytes(&serde_json::to_value(&source).unwrap()).unwrap();
        env.vault
            .commit(CommitRequest {
                commit_id: CommitId::from_random(env.vault.random_id_bytes()),
                expected_commit_id: Some(pin.commit_id),
                principal: owner(),
                operation_kind: OperationKind::Import,
                idempotency: IdempotencyScope {
                    principal_id: owner().actor_id,
                    operation_kind: OperationKind::Import,
                    key_hash: sha256(b"agent-source"),
                },
                request_payload_hash: sha256(&payload),
                expected_revisions: vec![(RecordKind::Source, source.source_id.to_string(), None)],
                records: vec![StagedRecord {
                    record_kind: RecordKind::Source,
                    record_id: source.source_id.to_string(),
                    revision: source.revision,
                    bytes: payload,
                }],
                objects: vec![],
            })
            .unwrap();
        let options = extraction_options(source.source_id, api);
        let policy = grant(&env, api);
        let enabled = AtomicBool::new(true);
        for only_fact in [false, true] {
            let (sid, run) = extraction::create(
                &env.vault,
                &owner(),
                &env.policy(),
                &options,
                format!("agent-extract-{only_fact}").as_bytes(),
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
            let mut wire: serde_json::Value =
                serde_json::from_slice(&extraction_reply(api, false)).unwrap();
            let body = if api == Api::OpenAiResponses {
                &mut wire["output"][0]["content"][0]["text"]
            } else {
                &mut wire["content"][0]["text"]
            };
            if only_fact {
                let mut claims: serde_json::Value =
                    serde_json::from_str(body.as_str().unwrap()).unwrap();
                claims["candidates"].as_array_mut().unwrap().truncate(1);
                *body = json!(claims.to_string());
            }
            let http = FakeHttp {
                calls: Cell::new(0),
                bytes: serde_json::to_vec(&wire).unwrap(),
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
            let pin = env.vault.pin_current().unwrap();
            if !only_fact {
                assert_eq!(
                    extraction::preview(&adapter, &run).unwrap_err().code,
                    MemoryErrorCode::InvalidRequest
                );
                assert_eq!(
                    extraction::apply_next(&adapter, &run).unwrap_err().code,
                    MemoryErrorCode::InvalidRequest
                );
                assert_eq!(env.vault.pin_current().unwrap().commit_id, pin.commit_id);
                assert!(
                    enouia_memory_govern::pending_candidates(&env.vault, &pin)
                        .unwrap()
                        .is_empty()
                );
                assert_eq!(
                    extraction::job(&env.vault, &owner(), &sid, &run)
                        .unwrap()
                        .cursor,
                    0
                );
            } else {
                let done = extraction::apply_next(&adapter, &run).unwrap();
                assert_eq!(
                    done.state,
                    enouia_memory_contract::extraction::ExtractionState::Completed
                );
                let shown = extraction::preview(&adapter, &run).unwrap();
                let candidate = shown.items[0].current_candidate.as_ref().unwrap();
                assert_eq!(candidate.status, CandidateStatus::Pending);
                assert_eq!(
                    candidate.evidence[0].evidence_class,
                    EvidenceClass::ModelClaim
                );
                assert_eq!(
                    candidate.proposed_details.as_ref().unwrap()["epistemic_status"],
                    "uncertain"
                );
                assert_eq!(shown.items[0].citation.speaker_role, SpeakerRole::Assistant);
            }
            assert_eq!(http.calls.get(), 1);
        }
        assert!(
            enouia_memory_govern::canonical_memories(
                &env.vault,
                &env.vault.pin_current().unwrap(),
                enouia_memory_govern::Canonical::AllStatuses
            )
            .unwrap()
            .is_empty()
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
        extraction::preview(&adapter, &run).unwrap_err().code,
        MemoryErrorCode::BrokenProvenance
    );
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
        let shown = extraction::preview(&adapter, &run).unwrap();
        if pass == 0 {
            assert!(
                shown
                    .items
                    .iter()
                    .all(|i| i.current_candidate.as_ref().unwrap().status
                        == enouia_memory_contract::candidate::CandidateStatus::Rejected)
            );
        } else {
            assert!(shown.items.iter().all(|i| i.application
                == extraction::ExtractionApplication::Suppressed
                && i.current_candidate.is_none()));
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
#[test]
fn extraction_pause_controls_pin_the_job_before_source_validation() {
    use enouia_memory_contract::{extraction::ExtractionState, foundation::Clock};
    use enouia_memory_provider::extraction;
    use std::sync::{Arc, Mutex, mpsc};

    struct PauseClock {
        gate: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    }
    impl Clock for PauseClock {
        fn now_unix_ms(&self) -> i64 {
            let gate = self.gate.lock().unwrap().take();
            if let Some((entered, release)) = gate {
                entered.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(30)).unwrap();
            }
            T0
        }
    }

    for (initially_paused, applications) in [(false, 1), (false, 2), (true, 1), (true, 2)] {
        let env = Env::new("extraction-pause-completion-race");
        let api = Api::OpenAiResponses;
        let source = extraction_source(&env);
        let policy = grant(&env, api);
        let (sid, run) = extraction::create(
            &env.vault,
            &owner(),
            &env.policy(),
            &extraction_options(source, api),
            b"pause-race-run",
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
        if initially_paused {
            extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
        }
        let clock = Arc::new(PauseClock {
            gate: Mutex::new(None),
        });
        let root = enouia_memory_vault::verify_data_root(
            env.vault.managed_root().root(),
            &enouia_memory_vault::RootPolicy::default(),
        )
        .unwrap();
        let delayed = enouia_memory_vault::Vault::open(
            &root,
            None,
            clock.clone(),
            Arc::new(enouia_memory_vault::OsIdSource),
            enouia_memory_vault::VaultOptions::default(),
        )
        .unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        *clock.gate.lock().unwrap() = Some((entered_tx, release_rx));
        let (delayed_result, completed, completed_pin) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                extraction::set_paused(&delayed, &owner(), &sid, &run, !initially_paused)
            });
            // The delayed operation has loaded the old job and is validating
            // source permissions. Cursor progress below must win permanently.
            entered_rx.recv_timeout(Duration::from_secs(30)).unwrap();
            let finished = (|| -> Result<_> {
                if initially_paused {
                    extraction::set_paused(&env.vault, &owner(), &sid, &run, false)?;
                }
                let mut completed = extraction::apply_next(&adapter, &run)?;
                if applications == 2 {
                    completed = extraction::apply_next(&adapter, &run)?;
                }
                Ok((completed, env.vault.pin_current().unwrap()))
            })();
            release_tx.send(()).unwrap();
            let delayed_result = worker.join().unwrap();
            let (completed, completed_pin) = finished.unwrap();
            (delayed_result, completed, completed_pin)
        });
        assert_eq!(
            completed.state,
            if applications == 2 {
                ExtractionState::Completed
            } else {
                ExtractionState::Ready
            }
        );
        assert_eq!(completed.cursor, applications);
        assert_eq!(
            delayed_result.unwrap_err().code,
            MemoryErrorCode::RevisionConflict
        );
        assert_eq!(env.vault.pin_current().unwrap(), completed_pin);
        assert_eq!(
            extraction::job(&env.vault, &owner(), &sid, &run).unwrap(),
            completed
        );
        if applications == 2 {
            assert_eq!(extraction::apply_next(&adapter, &run).unwrap(), completed);
            assert_eq!(env.vault.pin_current().unwrap(), completed_pin);
        } else {
            // A fresh control after refreshing the job still works normally.
            extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
            extraction::set_paused(&env.vault, &owner(), &sid, &run, false).unwrap();
            assert_eq!(
                extraction::job(&env.vault, &owner(), &sid, &run).unwrap(),
                completed
            );
        }
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
fn openai_terminal_event_and_response_status_must_agree_before_publication() {
    use enouia_memory_contract::{provider::FinishReason, session::EventKind};
    for completed in [true, false] {
        let api = Api::OpenAiResponses;
        let mut response: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
        if !completed {
            response["status"] = json!("incomplete");
            response["incomplete_details"] = json!({"reason":"max_output_tokens"});
        }
        let response_bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            enouia_memory_provider::codec::decode(api, &response_bytes)
                .unwrap()
                .finish,
            if completed {
                FinishReason::Completed
            } else {
                FinishReason::Length
            }
        );
        let delta = json!({"type":"response.output_text.delta","delta":"合成答复"});
        let wrong_terminal = json!({
            "type":if completed {"response.incomplete"} else {"response.completed"},
            "response":response,
        });
        let bytes = format!("data: {delta}\n\ndata: {wrong_terminal}\n\n").into_bytes();
        let mut decoder = Decoder::new(api);
        let mut partial = String::new();
        assert_eq!(
            decoder
                .push(&bytes, &mut |text| {
                    partial.push_str(text);
                    Ok(())
                })
                .unwrap_err()
                .code,
            MemoryErrorCode::ProviderUnavailable
        );
        assert_eq!(partial, "合成答复");
        let correct_terminal = json!({
            "type":if completed {"response.completed"} else {"response.incomplete"},
            "response":response,
        });
        assert!(
            decoder
                .push(
                    format!("data: {correct_terminal}\n\n").as_bytes(),
                    &mut |_| Ok(())
                )
                .is_err()
        );
        assert!(decoder.finish().is_err());

        let env = Env::new("openai-terminal-status");
        let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
        let enabled = AtomicBool::new(true);
        let adapter = VaultAdapter {
            vault: &env.vault,
            owner: owner(),
            input_event: input.clone(),
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
            bytes,
            fail: false,
        };
        let client = Client {
            transport: &http,
            secrets: &Secrets { missing: false },
            guard: &adapter,
            journal: &adapter,
        };
        assert_eq!(
            client
                .send(&call, &call.inspect().hash(), &NeverCancel)
                .unwrap_err()
                .code,
            MemoryErrorCode::ProviderUnavailable
        );
        let row = adapter.invocations().unwrap().remove(0);
        assert_eq!(row.state, InvocationState::Failed);
        assert_eq!(row.error_code, Some(MemoryErrorCode::ProviderUnavailable));
        assert!(adapter.saved_response(&row.dispatch_id).unwrap().is_none());
        let pin = env.vault.pin_current().unwrap();
        let entry = env
            .vault
            .record_entry(&pin, RecordKind::SessionEvent, input.as_str())
            .unwrap()
            .unwrap();
        let event: enouia_memory_contract::session::SessionEvent =
            enouia_memory_contract::record::parse_record(
                &env.vault
                    .read_record(
                        &pin,
                        &enouia_memory_contract::record::RecordRef::new(
                            RecordKind::SessionEvent,
                            input.as_str(),
                            entry.revision,
                        ),
                    )
                    .unwrap(),
            )
            .unwrap();
        let events =
            session::events(&env.vault, &pin, &event.session_id, &event.branch_id).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == EventKind::AssistantCompleted)
                .count(),
            0
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == EventKind::TurnFailed)
                .count(),
            1
        );
        let chunks: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::AssistantChunk)
            .collect();
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            session::text(&env.vault, &pin, chunks[0]).unwrap(),
            "合成答复"
        );
        assert!(
            client
                .send(&call, &call.inspect().hash(), &NeverCancel)
                .is_err()
        );
        assert_eq!(http.calls.get(), 1);
        assert_eq!(env.vault.pin_current().unwrap(), pin);
        assert!(env.vault.verify(&pin).unwrap().is_clean());
    }
}

#[test]
fn native_http_context_refuses_before_secret_read_or_admission() {
    struct CountSecrets(Cell<u64>);
    impl SecretStore for CountSecrets {
        fn read(&self, _: &str) -> Result<SecretBytes> {
            self.0.set(self.0.get() + 1);
            // Empty synthetic credentials independently prevent HTTP even
            // if a regression gets past the execution-context check.
            Ok(SecretBytes::new(vec![]))
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("native-http-context");
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
        let secrets = CountSecrets(Cell::new(0));
        let native = enouia_memory_provider::transport::HttpsTransport;
        let client = Client {
            transport: &native,
            secrets: &secrets,
            guard: &adapter,
            journal: &adapter,
        };
        let before = env.vault.pin_current().unwrap();
        runtime.block_on(async {
            let failure = client
                .send(&call, &call.inspect().hash(), &NeverCancel)
                .unwrap_err();
            assert_eq!(failure.code, MemoryErrorCode::InvalidRequest);
            assert!(!failure.retryable);
            // Direct transport use must have the same guard before creating
            // an inner runtime; there is no credential store or live secret.
            let mut delivered = false;
            assert_eq!(
                native
                    .exchange(
                        call.inspect(),
                        &SecretBytes::new(vec![]),
                        &NeverCancel,
                        Duration::from_secs(1),
                        &mut |_| {
                            delivered = true;
                            Ok(())
                        }
                    )
                    .unwrap_err()
                    .code,
                MemoryErrorCode::InvalidRequest
            );
            assert!(!delivered);
            tokio::task::yield_now().await;
        });
        assert_eq!(secrets.0.get(), 0);
        assert_eq!(env.vault.pin_current().unwrap(), before);
        assert!(adapter.invocations().unwrap().is_empty());
        // An entered synchronous context is refused conservatively too.
        // Native HTTPS requires its own thread outside any Tokio context.
        {
            let _entered = runtime.enter();
            assert_eq!(
                client
                    .send(&call, &call.inspect().hash(), &NeverCancel)
                    .unwrap_err()
                    .code,
                MemoryErrorCode::InvalidRequest
            );
        }
        assert_eq!(secrets.0.get(), 0);
        assert_eq!(env.vault.pin_current().unwrap(), before);
        // The exact approval/admission remains unconsumed. Fake transports
        // retain their default preflight and work in this synthetic runtime.
        let fake = FakeHttp {
            calls: Cell::new(0),
            bytes: reply(api),
            fail: false,
        };
        let fake_client = Client {
            transport: &fake,
            ..client
        };
        runtime.block_on(async {
            assert_eq!(
                fake_client
                    .send(&call, &call.inspect().hash(), &NeverCancel)
                    .unwrap()
                    .text,
                "合成答复"
            );
        });
        assert_eq!(fake.calls.get(), 1);
        assert_eq!(secrets.0.get(), 1);
        assert_eq!(adapter.invocations().unwrap().len(), 1);
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}
#[test]
fn malformed_native_credentials_do_not_consume_private_approval_or_admission() {
    struct FixedSecret(Vec<u8>);
    impl SecretStore for FixedSecret {
        fn read(&self, _: &str) -> Result<SecretBytes> {
            Ok(SecretBytes::new(self.0.clone()))
        }
    }
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("credential-shape-preflight");
        let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Private);
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
        let call = adapter
            .approve(
                prepared,
                &inspected,
                TrustedSurface::TrustedLocalCli,
                "credential-shape-confirmation",
            )
            .unwrap();
        let before = env.vault.pin_current().unwrap();
        let native = enouia_memory_provider::transport::HttpsTransport;
        // Every shape is already refused by native HTTP before builder.send,
        // independently of whether Client validates it before admission.
        for secret in [
            vec![],
            vec![0xff],
            b"synthetic\r\ncredential".to_vec(),
            vec![0],
            vec![0x7f],
            vec![b'x'; 4097],
        ] {
            let store = FixedSecret(secret.clone());
            let client = Client {
                transport: &native,
                secrets: &store,
                guard: &adapter,
                journal: &adapter,
            };
            let failure = client.send(&call, &inspected, &NeverCancel).unwrap_err();
            assert_eq!(failure.code, MemoryErrorCode::Unauthenticated);
            assert!(!failure.retryable);
            assert!(adapter.invocations().unwrap().is_empty());
            assert_eq!(env.vault.pin_current().unwrap(), before);
            let mut delivered = false;
            assert_eq!(
                native
                    .exchange(
                        call.inspect(),
                        &SecretBytes::new(secret),
                        &NeverCancel,
                        Duration::from_secs(1),
                        &mut |_| {
                            delivered = true;
                            Ok(())
                        }
                    )
                    .unwrap_err()
                    .code,
                MemoryErrorCode::Unauthenticated
            );
            assert!(!delivered);
            assert_eq!(env.vault.pin_current().unwrap(), before);
        }
        // A custom transport can support opaque/binary secret bytes. Its
        // default local validation must not inherit native HTTP's text rules.
        let binary_store = FixedSecret(vec![0xff]);
        let fake = FakeHttp {
            calls: Cell::new(0),
            bytes: reply(api),
            fail: false,
        };
        let fake_client = Client {
            transport: &fake,
            secrets: &binary_store,
            guard: &adapter,
            journal: &adapter,
        };
        assert_eq!(
            fake_client
                .send(&call, &inspected, &NeverCancel)
                .unwrap()
                .text,
            "合成答复"
        );
        assert_eq!(fake.calls.get(), 1);
        let row = adapter.invocations().unwrap().remove(0);
        assert_eq!(row.state, InvocationState::Completed);
        assert_eq!(row.dispatch_id, call.dispatch().dispatch_id);
        assert!(!adapter.claim(&call).unwrap());
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}

#[test]
fn failed_or_cancelled_streams_keep_validated_usage_for_later_token_admission() {
    struct Stop(Cell<bool>);
    impl Cancellation for Stop {
        fn is_cancelled(&self) -> bool {
            self.0.get()
        }
    }
    struct PrefixHttp<'a> {
        bytes: Vec<u8>,
        mode: &'static str,
        stop: &'a Stop,
    }
    impl Transport for PrefixHttp<'_> {
        fn exchange(
            &self,
            _: &WireRequest,
            _: &SecretBytes,
            _: &dyn Cancellation,
            _: Duration,
            sink: &mut dyn FnMut(&[u8]) -> Result<()>,
        ) -> Result<()> {
            for chunk in self.bytes.chunks(3) {
                sink(chunk)?;
            }
            if self.mode == "cancel" {
                self.stop.0.set(true);
            }
            if self.mode == "eof" {
                return Err(enouia_memory_contract::MemoryError::new(
                    MemoryErrorCode::ProviderUnavailable,
                    enouia_memory_contract::foundation::ComponentId::Provider,
                ));
            }
            Ok(())
        }
    }
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        for mode in ["eof", "cancel", "malformed"] {
            let env = Env::new("failed-stream-usage");
            let (capsule, input, policy, caps) = setup(&env, api, Sensitivity::Normal);
            let enabled = AtomicBool::new(true);
            let adapter = VaultAdapter {
                vault: &env.vault,
                owner: owner(),
                input_event: input.clone(),
                enabled: &enabled,
                quota: Quota::default(),
            };
            let mut options = CallOptions::text(policy.clone(), 128);
            options.streaming = true;
            let call = adapter
                .prepare_saved(api, &capsule, &caps, Limits::default(), options)
                .unwrap();
            let mut events = match api {
                Api::OpenAiResponses => {
                    let mut terminal: serde_json::Value =
                        serde_json::from_slice(&reply(api)).unwrap();
                    terminal["usage"] = json!({"input_tokens":5000,"output_tokens":1000});
                    vec![
                        json!({"type":"response.output_text.delta","delta":"合成答复"}),
                        json!({"type":"response.completed","response":terminal}),
                    ]
                }
                Api::AnthropicMessages => vec![
                    json!({"type":"message_start","message":{"type":"message","role":"assistant","usage":{"input_tokens":5000,"cache_creation_input_tokens":2,"cache_read_input_tokens":3,"output_tokens":0}}}),
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"合成答复"}}),
                    json!({"type":"content_block_stop","index":0}),
                    json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1000}}),
                ],
            };
            if mode == "malformed" {
                events.push(match api {
                    Api::OpenAiResponses => json!({"type":"error"}),
                    Api::AnthropicMessages => json!({"type":"message_delta","delta":{},"usage":{"output_tokens":"invalid"}}),
                });
            }
            let stop = Stop(Cell::new(false));
            let http = PrefixHttp {
                bytes: events
                    .iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>()
                    .into_bytes(),
                mode,
                stop: &stop,
            };
            let failure = Client {
                transport: &http,
                secrets: &Secrets { missing: false },
                guard: &adapter,
                journal: &adapter,
            }
            .send(&call, &call.inspect().hash(), &stop)
            .unwrap_err();
            assert_eq!(
                failure.code,
                if mode == "cancel" {
                    MemoryErrorCode::Cancelled
                } else {
                    MemoryErrorCode::ProviderUnavailable
                }
            );
            assert!(!failure.retryable);
            let row = adapter.invocations().unwrap().remove(0);
            assert_eq!(
                row.state,
                if mode == "cancel" {
                    InvocationState::Cancelled
                } else {
                    InvocationState::Failed
                }
            );
            assert_eq!(row.error_code, Some(failure.code));
            let known_input = if api == Api::OpenAiResponses {
                5000
            } else {
                5005
            };
            assert_eq!(row.input_tokens, Some(known_input));
            assert_eq!(row.output_tokens, Some(1000));
            assert!(row.reserved_tokens < known_input + 1000);
            assert!(adapter.saved_response(&row.dispatch_id).unwrap().is_none());
            assert!(!adapter.claim(&call).unwrap());
            let pin = env.vault.pin_current().unwrap();
            adapter
                .finish_failure(call.dispatch(), failure, Some(known_input), Some(1000))
                .unwrap();
            assert_eq!(env.vault.pin_current().unwrap(), pin);
            assert_eq!(
                adapter
                    .finish_failure(call.dispatch(), failure, Some(known_input + 1), Some(1000))
                    .unwrap_err()
                    .code,
                MemoryErrorCode::IdempotencyConflict
            );
            assert_eq!(env.vault.pin_current().unwrap(), pin);
            let entry = env
                .vault
                .record_entry(&pin, RecordKind::SessionEvent, input.as_str())
                .unwrap()
                .unwrap();
            let event: enouia_memory_contract::session::SessionEvent =
                enouia_memory_contract::record::parse_record(
                    &env.vault
                        .read_record(
                            &pin,
                            &enouia_memory_contract::record::RecordRef::new(
                                RecordKind::SessionEvent,
                                input.as_str(),
                                entry.revision,
                            ),
                        )
                        .unwrap(),
                )
                .unwrap();
            let events =
                session::events(&env.vault, &pin, &event.session_id, &event.branch_id).unwrap();
            assert!(events.iter().any(|e| e.kind
                == enouia_memory_contract::session::EventKind::AssistantChunk
                && session::text(&env.vault, &pin, e).unwrap() == "合成答复"));
            let mut compile = CompileInput::local(
                "新的明确合成输入",
                owner(),
                RequestId::from_random(env.vault.random_id_bytes()),
            );
            compile.session_id = Some(event.session_id.clone());
            compile.branch_id = Some(event.branch_id.clone());
            let next_input = session::save_input(
                &env.vault,
                &owner(),
                &event.session_id,
                &event.branch_id,
                &compile.query,
                &compile.request_id,
                b"after-failed-stream",
            )
            .unwrap()
            .id;
            let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
            let next_capsule = compiler::compile_for_destination(
                &env.vault,
                &index,
                &compile,
                &Destination {
                    kind: DestinationKind::ExternalProvider,
                    provider_binding: Some(caps.binding.clone()),
                },
            )
            .unwrap();
            let mut next = VaultAdapter {
                input_event: next_input,
                ..adapter
            };
            let next_call = next
                .prepare_saved(
                    api,
                    &next_capsule.capsule.capsule_id,
                    &caps,
                    Limits::default(),
                    CallOptions::text(policy, 128),
                )
                .unwrap();
            // The old reservation alone would allow this ceiling. Known
            // reported usage, including a failed/cancelled prefix, must not.
            next.quota.max_reserved_tokens = row.reserved_tokens
                + next_call.estimated_input_tokens()
                + next_call.dispatch().output.max_output_tokens;
            let never_sent = FakeHttp {
                calls: Cell::new(0),
                bytes: reply(api),
                fail: false,
            };
            let before = env.vault.pin_current().unwrap();
            assert_eq!(
                Client {
                    transport: &never_sent,
                    secrets: &Secrets { missing: false },
                    guard: &next,
                    journal: &next
                }
                .send(&next_call, &next_call.inspect().hash(), &NeverCancel)
                .unwrap_err()
                .code,
                MemoryErrorCode::BudgetExceeded
            );
            assert_eq!(never_sent.calls.get(), 0);
            assert_eq!(env.vault.pin_current().unwrap(), before);
            assert!(env.vault.verify(&before).unwrap().is_clean());
        }
    }
}

#[test]
fn extraction_application_requires_the_original_input_even_for_completed_replay() {
    use enouia_memory_contract::{extraction::ExtractionState, record::RecordRef};
    use enouia_memory_provider::extraction;
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("extraction-input-binding");
        let options = extraction_options(extraction_source(&env), api);
        let policy = grant(&env, api);
        let (sid, run) = extraction::create(
            &env.vault,
            &owner(),
            &env.policy(),
            &options,
            b"original-extraction-input",
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
        let caps = extraction_capabilities(&env, api);
        let call = extraction::prepare(
            &adapter,
            &index,
            &run,
            &caps,
            Limits::default(),
            CallOptions::text(policy.clone(), 1024),
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
        let pin = env.vault.pin_current().unwrap();
        let enouia_memory_contract::record::AnyRecord::SessionEvent(input) = env
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
        let later_input = session::save_input(
            &env.vault,
            &owner(),
            &sid,
            &input.branch_id,
            "同一会话中的另一个明确合成输入",
            &RequestId::from_random(env.vault.random_id_bytes()),
            b"later-extraction-input",
        )
        .unwrap()
        .id;
        let wrong = VaultAdapter {
            input_event: later_input,
            owner: adapter.owner.clone(),
            ..adapter
        };
        drop(index);
        let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
        let before = env.vault.pin_current().unwrap();
        assert_eq!(
            extraction::prepare(
                &wrong,
                &index,
                &run,
                &caps,
                Limits::default(),
                CallOptions::text(policy, 1024),
            )
            .err()
            .unwrap()
            .code,
            MemoryErrorCode::PermissionDenied
        );
        assert_eq!(env.vault.pin_current().unwrap(), before);
        for cursor in 0..=2 {
            let current = extraction::job(&env.vault, &owner(), &sid, &run).unwrap();
            assert_eq!(current.cursor, cursor);
            let before = env.vault.pin_current().unwrap();
            assert_eq!(
                extraction::preview(&wrong, &run).unwrap_err().code,
                MemoryErrorCode::PermissionDenied
            );
            assert_eq!(
                extraction::apply_next(&wrong, &run).unwrap_err().code,
                MemoryErrorCode::PermissionDenied
            );
            assert_eq!(env.vault.pin_current().unwrap(), before);
            let shown = extraction::preview(&adapter, &run).unwrap();
            assert_eq!(shown.job, current);
            if cursor < 2 {
                extraction::set_paused(&env.vault, &owner(), &sid, &run, true).unwrap();
                let before = env.vault.pin_current().unwrap();
                assert_eq!(
                    extraction::apply_next(&wrong, &run).unwrap_err().code,
                    MemoryErrorCode::PermissionDenied
                );
                assert_eq!(
                    extraction::apply_next(&adapter, &run).unwrap_err().code,
                    MemoryErrorCode::Cancelled
                );
                assert_eq!(env.vault.pin_current().unwrap(), before);
                extraction::set_paused(&env.vault, &owner(), &sid, &run, false).unwrap();
                let next = extraction::apply_next(&adapter, &run).unwrap();
                assert_eq!(next.cursor, cursor + 1);
            } else {
                assert_eq!(current.state, ExtractionState::Completed);
                assert_eq!(extraction::apply_next(&adapter, &run).unwrap(), current);
                assert_eq!(env.vault.pin_current().unwrap(), before);
            }
        }
        assert_eq!(http.calls.get(), 1);
        assert!(
            env.vault
                .verify(&env.vault.pin_current().unwrap())
                .unwrap()
                .is_clean()
        );
    }
}
