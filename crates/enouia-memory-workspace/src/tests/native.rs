//! Native Provider hosting through the page channel (ADR-MEM-48). Fake
//! transports and secrets only: no credential store, no HTTP.

use super::*;
use enouia_memory_contract::MemoryError;
use enouia_memory_contract::foundation::Cancellation;
use enouia_memory_contract::json::SchemaVersion;
use enouia_memory_contract::policy::{PolicyOrigin, PolicyRecord};
use enouia_memory_contract::ports::{SecretBytes, SecretStore};
use enouia_memory_contract::provider::{Capability, ProviderCapabilities, TokenCounting};
use enouia_memory_provider::client::Limits;
use enouia_memory_provider::codec::{Api, WireRequest};
use enouia_memory_provider::transport::Transport;
use enouia_memory_provider::vault::Quota;
use std::sync::atomic::AtomicBool;

struct Secrets;
impl SecretStore for Secrets {
    fn read(&self, _: &str) -> Result<SecretBytes, MemoryError> {
        Ok(SecretBytes::new(b"synthetic-secret".to_vec()))
    }
}

/// Replies with fixed bytes, or holds the call open until cancelled.
#[derive(Default)]
struct FakeHttp {
    calls: AtomicU64,
    hold: bool,
    entered: AtomicBool,
    bodies: Mutex<Vec<Vec<u8>>>,
}

impl Transport for FakeHttp {
    fn exchange(
        &self,
        request: &WireRequest,
        _: &SecretBytes,
        cancellation: &dyn Cancellation,
        _: Duration,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), MemoryError>,
    ) -> Result<(), MemoryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.bodies.lock().unwrap().push(request.body().to_vec());
        self.entered.store(true, Ordering::SeqCst);
        if self.hold {
            while !cancellation.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            return Err(MemoryError::new(
                MemoryErrorCode::Cancelled,
                ComponentId::Provider,
            ));
        }
        let reply = match request.binding().provider.as_str() {
            "openai" => json!({"status": "completed", "output": [{"type": "message",
                "role": "assistant", "content": [{"type": "output_text", "text": "合成外部答复"}]}],
                "usage": {"input_tokens": 90, "output_tokens": 6}}),
            _ => json!({"type": "message", "role": "assistant",
                "content": [{"type": "text", "text": "合成外部答复"}], "stop_reason": "end_turn",
                "usage": {"input_tokens": 90, "output_tokens": 6}}),
        };
        for chunk in serde_json::to_vec(&reply).unwrap().chunks(7) {
            sink(chunk)?;
        }
        Ok(())
    }
}

fn destination(api: Api, now: &enouia_memory_contract::time::Timestamp) -> NativeDestination {
    NativeDestination {
        api,
        capabilities: ProviderCapabilities {
            schema_version: SchemaVersion,
            binding: api.binding("synthetic-model").unwrap(),
            text_input: Capability::Supported,
            image_input: Capability::Unsupported,
            streaming: Capability::Unsupported,
            tool_calling: Capability::Unsupported,
            cancellation: Capability::Supported,
            token_counting: TokenCounting::Estimated,
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(4096),
            verified_at: Some(now.clone()),
        },
    }
}

fn configure(env: &Env, http: Arc<FakeHttp>) {
    let now = env.ws.open().unwrap().vault.now().unwrap();
    env.ws
        .set_native_provider(Some(NativeProvider {
            transport: http,
            secrets: Arc::new(Secrets),
            destinations: vec![
                destination(Api::OpenAiResponses, &now),
                destination(Api::AnthropicMessages, &now),
            ],
            quota: Quota::default(),
            limits: Limits::default(),
        }))
        .unwrap();
}

/// Plan and confirm a private grant for both APIs; returns its policy ID.
fn grant(env: &Env) -> Value {
    let plan = env.ok(
        "egress_grant_plan",
        json!({"apis": ["openai", "anthropic"], "maxSensitivity": "private"}),
    );
    assert_eq!(plan["kind"], "grant");
    let inspected: Value = serde_json::from_str(plan["inspection"].as_str().unwrap()).unwrap();
    assert!(
        inspected.to_string().contains("synthetic-model"),
        "{inspected}"
    );
    assert_eq!(
        plan["diffHash"],
        json!(sha256(plan["inspection"].as_str().unwrap().as_bytes()))
    );
    // A wrong hash is refused and leaves the plan for the exact one.
    let wrong = env.err(
        "provider_confirm",
        json!({"planId": plan["planId"], "diffHash": sha256(b"other")}),
    );
    assert_eq!(wrong["code"], "permission_denied", "{wrong}");
    let confirmed = env.ok(
        "provider_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(confirmed["kind"], "grant");
    // Confirming again answers the saved result without a commit.
    let head = env
        .ws
        .open()
        .unwrap()
        .vault
        .pin_current()
        .unwrap()
        .commit_id;
    let again = env.ok(
        "provider_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(again, confirmed);
    assert_eq!(
        env.ws
            .open()
            .unwrap()
            .vault
            .pin_current()
            .unwrap()
            .commit_id,
        head
    );
    confirmed["policyId"].clone()
}

fn new_session(env: &Env) -> (Value, Value) {
    let s = env.ok("session_new", json!({}));
    (s["sessionId"].clone(), s["branchId"].clone())
}

fn prepare(env: &Env, session: &(Value, Value), api: &str, policy: &Value) -> Value {
    env.ok(
        "external_prepare",
        json!({"sessionId": session.0, "branchId": session.1, "text": "合成外部问题",
            "api": api, "policyId": policy, "streaming": false, "outputTokens": 256}),
    )
}

#[test]
fn external_commands_refuse_until_the_host_configures_native_sending() {
    let env = Env::new("native-unconfigured");
    let status = env.ok("provider_status", json!({}));
    assert_eq!(status["configured"], false);
    assert_eq!(status["destinations"], json!([]));
    assert_eq!(status["grants"], json!([]));
    let workspace = env.ok("workspace_status", json!({}));
    let provider = workspace["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["component"] == "provider")
        .unwrap()
        .clone();
    assert_eq!(provider["mode"], "local_mock_only");
    let refused = env.err(
        "egress_grant_plan",
        json!({"apis": ["openai"], "maxSensitivity": "private"}),
    );
    assert_eq!(refused["code"], "provider_unavailable");
    assert_eq!(refused["rules"][0], "provider.not_configured");
    assert_eq!(refused["retryable"], false);
    let session = new_session(&env);
    let refused = env.err(
        "external_prepare",
        json!({"sessionId": session.0, "branchId": session.1, "text": "合成外部问题",
            "api": "openai", "policyId": "pol_00000000-0000-4000-8000-000000000003",
            "streaming": false, "outputTokens": 256}),
    );
    assert_eq!(refused["rules"][0], "provider.not_configured");
    // Nothing was saved for the refused external turn.
    let detail = env.ok(
        "session_detail",
        json!({"sessionId": session.0, "branchId": session.1}),
    );
    assert_eq!(detail["transcript"], json!([]));
    assert_eq!(detail["invocations"], json!([]));
    // The local Mock still answers.
    let turn = env.ok(
        "session_ask",
        json!({"sessionId": session.0, "branchId": session.1, "text": "合成本地问题"}),
    );
    assert_eq!(turn["destination"], "local_mock");
}

#[test]
fn owner_grants_inspects_and_sends_each_api_exactly_once() {
    let env = Env::new("native-send");
    let http = Arc::new(FakeHttp::default());
    configure(&env, http.clone());
    let status = env.ok("provider_status", json!({}));
    assert_eq!(status["configured"], true);
    assert_eq!(status["destinations"].as_array().unwrap().len(), 2);
    let policy = grant(&env);
    let status = env.ok("provider_status", json!({}));
    assert_eq!(status["grants"][0]["policyId"], policy);
    assert_eq!(status["grants"][0]["active"], true);
    // The grant is a Policy record too; Sessions keep the genesis policy.
    let session = new_session(&env);
    {
        let open = env.ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        let header: SessionRecord = parse(
            &open
                .vault
                .read_record(
                    &pin,
                    &RecordRef::new(RecordKind::Session, session.0.as_str().unwrap(), one()),
                )
                .unwrap(),
        )
        .unwrap();
        let genesis: Vec<PolicyRecord> =
            latest_records(&open.vault, &pin, RecordKind::Policy).unwrap();
        let genesis = genesis
            .iter()
            .find(|p| p.origin == PolicyOrigin::GenesisDefault)
            .unwrap();
        assert_eq!(header.policy_id, genesis.policy_id);
    }
    for (n, api) in ["openai", "anthropic"].iter().enumerate() {
        let call = prepare(&env, &session, api, &policy);
        let body = call["body"].as_str().unwrap();
        assert!(body.contains("合成外部问题"), "{body}");
        assert!(!body.contains("synthetic-secret"));
        assert_eq!(call["wireHash"], json!(sha256(body.as_bytes())));
        assert_eq!(call["api"], *api);
        // Preparing sends nothing.
        assert_eq!(http.calls.load(Ordering::SeqCst), n as u64);
        // A different hash is refused; the call stays for the exact one.
        let refused = env.err(
            "external_send",
            json!({"callId": call["callId"], "wireHash": sha256(b"other")}),
        );
        assert_eq!(refused["code"], "permission_denied");
        let key = format!("synthetic-external-send-{n:04}");
        let args = json!({"callId": call["callId"], "wireHash": call["wireHash"]});
        let started = env.send_keyed("external_send", args.clone(), &key);
        assert_eq!(started["error"], Value::Null, "{started}");
        let done = env.wait(&started["result"]);
        assert_eq!(done["state"], "succeeded", "{done}");
        assert_eq!(done["result"]["finish"], "completed");
        assert_eq!(done["result"]["dispatchId"], call["dispatchId"]);
        assert_eq!(done["result"]["outputTokens"], 6);
        // A page retry with the same key observes the same operation.
        let replay = env.send_keyed("external_send", args.clone(), &key);
        assert_eq!(replay["operationId"], started["operationId"]);
        // A new key cannot reuse the consumed call.
        let again = env.send_keyed("external_send", args, &format!("{key}-again"));
        assert_eq!(again["error"]["rules"][0], "provider.call_unknown");
        assert_eq!(http.calls.load(Ordering::SeqCst), n as u64 + 1);
        assert_eq!(
            http.bodies.lock().unwrap().last().unwrap().as_slice(),
            body.as_bytes()
        );
    }
    let detail = env.ok(
        "session_detail",
        json!({"sessionId": session.0, "branchId": session.1}),
    );
    let invocations = detail["invocations"].as_array().unwrap();
    assert_eq!(invocations.len(), 2);
    assert!(invocations.iter().all(|i| i["state"] == "completed"));
    let replies: Vec<_> = detail["transcript"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["text"] == "合成外部答复")
        .collect();
    assert_eq!(replies.len(), 2, "{detail}");
    let verify = env.ok("vault_verify", json!({}));
    assert_eq!(env.wait(&verify)["result"]["clean"], true);
}

#[test]
fn lock_cancels_and_joins_a_running_send_and_the_owner_closes_it_locally() {
    let env = Env::new("native-lock");
    let http = Arc::new(FakeHttp {
        hold: true,
        ..FakeHttp::default()
    });
    configure(&env, http.clone());
    let policy = grant(&env);
    let session = new_session(&env);
    let call = prepare(&env, &session, "anthropic", &policy);
    let started = env.ok(
        "external_send",
        json!({"callId": call["callId"], "wireHash": call["wireHash"]}),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !http.entered.load(Ordering::SeqCst) {
        assert!(
            std::time::Instant::now() < deadline,
            "transport not entered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // A running send blocks reconfiguration and local interruption.
    let busy = env.ws.set_native_provider(None).unwrap_err();
    assert_eq!(busy.rules, vec!["provider.call_running".to_owned()]);
    let busy = env.err(
        "external_interrupt_plan",
        json!({"inputEventId": call["inputEventId"], "dispatchId": call["dispatchId"]}),
    );
    assert_eq!(busy["rules"][0], "provider.call_running");
    // Lock disables native access, then cancels and joins the worker.
    env.ok("vault_lock", json!({}));
    // Close joined the worker before forgetting the operation table.
    let id = OperationId::parse(started["operationId"].as_str().unwrap()).unwrap();
    assert!(env.ws.operations().status(&id).is_none());
    assert!(env.ws.native.sending.lock().unwrap().is_empty());
    env.ok("vault_unlock", json!({}));
    // The disabled late result was not published: the admission is unknown.
    let detail = env.ok(
        "session_detail",
        json!({"sessionId": session.0, "branchId": session.1}),
    );
    assert_eq!(detail["invocations"][0]["state"], "outcome_unknown");
    // The prepared call did not survive the lock.
    let gone = env.err(
        "external_send",
        json!({"callId": call["callId"], "wireHash": call["wireHash"]}),
    );
    assert_eq!(gone["rules"][0], "provider.call_unknown");
    // The owner closes the unknown turn locally after inspecting the plan.
    let plan = env.ok(
        "external_interrupt_plan",
        json!({"inputEventId": call["inputEventId"], "dispatchId": call["dispatchId"]}),
    );
    assert_eq!(plan["kind"], "interrupt");
    let closed = env.ok(
        "provider_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(closed["kind"], "interrupt");
    let detail = env.ok(
        "session_detail",
        json!({"sessionId": session.0, "branchId": session.1}),
    );
    assert_eq!(detail["invocations"][0]["state"], "cancelled");
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    // With no send running, the host can remove its native setup.
    env.ws.set_native_provider(None).unwrap();
    assert_eq!(env.ok("provider_status", json!({}))["configured"], false);
    let verify = env.ok("vault_verify", json!({}));
    assert_eq!(env.wait(&verify)["result"]["clean"], true);
}
