//! Boundary-only synthetic ports. No Vault, owner session, Provider or network.
use enouia_memory_contract::common::{ActorRef, ActorType};
use enouia_memory_contract::ids::PrincipalId;
use enouia_memory_contract::ipc::{Arguments, IpcRequest};
use enouia_memory_contract::policy::Scope;
use enouia_memory_host::mcp::{
    AgentPort, Grant, MAX_INPUT_BYTES, MAX_REQUESTS, PortFault, Server, input_schema,
};
use serde_json::{Value, json};
use std::io::{self, Cursor, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Mode {
    Valid,
    Unknown,
    WrongId,
    WrongKind,
}
#[derive(Default)]
struct Seen {
    calls: Vec<(ActorRef, IpcRequest)>,
    disconnects: usize,
}
struct Fake {
    seen: Arc<Mutex<Seen>>,
    mode: Mode,
}
impl AgentPort for Fake {
    fn call(
        &mut self,
        principal: &ActorRef,
        request: &IpcRequest,
        _: Arguments,
    ) -> Result<Value, PortFault> {
        self.seen
            .lock()
            .unwrap()
            .calls
            .push((principal.clone(), request.clone()));
        if matches!(self.mode, Mode::Unknown) {
            return Err(PortFault::OutcomeUnknown);
        }
        let fixtures = fixtures();
        let operation = serde_json::to_value(request.operation).unwrap();
        let mut result = fixtures["valid_responses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["operation"] == operation)
            .unwrap()["message"]
            .clone();
        result["requestId"] = serde_json::to_value(&request.request_id).unwrap();
        match self.mode {
            Mode::WrongId => {
                result["requestId"] = json!("req_00000009-0000-4000-8000-000000000001")
            }
            Mode::WrongKind => result["kind"] = json!("review_committed"),
            _ => {}
        }
        Ok(result)
    }
    fn disconnect(&mut self) {
        self.seen.lock().unwrap().disconnects += 1;
    }
}
fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/memory/ipc-manifest.json"
    ))
    .unwrap()
}
fn request(name: &str) -> Value {
    fixtures()["valid_requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["operation"] == name)
        .unwrap()
        .clone()
}
fn principal() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
    }
}
fn scopes() -> Vec<Scope> {
    vec![
        Scope::MemoryRead,
        Scope::SourceRead,
        Scope::ContextRead,
        Scope::MemoryPropose,
        Scope::SessionPropose,
    ]
}
fn server(scopes: Vec<Scope>, mode: Mode) -> (Server<Fake>, Grant, Arc<Mutex<Seen>>) {
    let grant = Grant::from_authenticated_agent(principal(), scopes).unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    (
        Server::new(
            grant.clone(),
            Fake {
                seen: seen.clone(),
                mode,
            },
        ),
        grant,
        seen,
    )
}
fn rpc(id: usize, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn initialize(server: &mut Server<Fake>) {
    let response = server
        .handle(&rpc(
            1,
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},
        "clientInfo":{"name":"synthetic-owner-claim","version":"0"}}),
        ))
        .unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}
fn call(id: usize, name: &str, request: Value) -> Value {
    rpc(id, "tools/call", json!({"name":name,"arguments":request}))
}

#[test]
fn catalog_is_six_closed_tools_with_separate_source_scope() {
    let (mut s, _, seen) = server(scopes(), Mode::Valid);
    assert!(s.handle(&rpc(0, "tools/list", json!({}))).unwrap()["error"].is_object());
    initialize(&mut s);
    let result = s.handle(&rpc(2, "tools/list", json!({}))).unwrap();
    let tools = result["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            tool["inputSchema"]["properties"]["operation"]["const"],
            name
        );
        assert!(!tool.to_string().contains("../memory/"));
    }
    assert!(tools.iter().all(|t| t["name"] != "memory_update"));
    assert_eq!(seen.lock().unwrap().calls.len(), 0);
    let (mut s, _, seen) = server(vec![Scope::MemoryRead], Mode::Valid);
    initialize(&mut s);
    assert_eq!(
        s.handle(&rpc(2, "tools/list", json!({}))).unwrap()["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let denied = s
        .handle(&call(3, "memory_source", request("memory_source")))
        .unwrap();
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "permission_denied"
    );
    assert!(seen.lock().unwrap().calls.is_empty());
}

#[test]
fn all_six_calls_and_legacy_alias_keep_native_principal_and_keys() {
    let (mut s, _, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    for (n, name) in [
        "context_get",
        "memory_search",
        "memory_read",
        "memory_source",
        "memory_propose",
        "session_checkpoint",
        "memory_update",
    ]
    .iter()
    .enumerate()
    {
        let original = request(name);
        let result = s.handle(&call(n + 2, name, original.clone())).unwrap();
        assert_eq!(result["result"]["isError"], false, "{name}: {result}");
        let calls = &seen.lock().unwrap().calls;
        assert_eq!(calls[n].0, principal());
        assert_eq!(serde_json::to_value(&calls[n].1).unwrap(), original);
        if *name == "memory_propose" || *name == "memory_update" {
            assert!(matches!(
                result["result"]["structuredContent"]["result"]["state"].as_str(),
                Some("pending" | "duplicate")
            ));
        }
        if *name == "session_checkpoint" {
            assert_eq!(
                result["result"]["structuredContent"]["result"]["status"],
                "provisional"
            );
        }
    }
}

#[test]
fn owner_scopes_identity_spoofing_and_non_tool_operations_never_dispatch() {
    let mut owner = principal();
    owner.actor_type = ActorType::Owner;
    assert!(Grant::from_authenticated_agent(owner, scopes()).is_none());
    for scope in [
        Scope::OwnerReview,
        Scope::OwnerIdentity,
        Scope::ProviderSend,
        Scope::OperationRead,
    ] {
        assert!(Grant::from_authenticated_agent(principal(), vec![scope]).is_none());
    }
    let (mut s, _, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    for (n, name) in [
        "candidate_review",
        "identity_review",
        "vault_unlock",
        "external_send",
        "delete_commit",
    ]
    .iter()
    .enumerate()
    {
        assert!(s.handle(&call(n + 2, name, json!({}))).unwrap()["error"].is_object());
    }
    let mut forged = request("memory_read");
    forged["principal"] = json!("owner");
    assert_eq!(
        s.handle(&call(9, "memory_read", forged)).unwrap()["result"]["isError"],
        true
    );
    assert_eq!(
        s.handle(&call(10, "memory_read", request("memory_search")))
            .unwrap()["result"]["structuredContent"]["error"]["code"],
        "tool_operation_mismatch"
    );
    let mut legacy = request("memory_update");
    legacy["arguments"]["proposalKind"] = json!("create");
    assert_eq!(
        s.handle(&call(11, "memory_update", legacy)).unwrap()["result"]["isError"],
        true
    );
    assert!(seen.lock().unwrap().calls.is_empty());
}

#[test]
fn uncertain_or_invalid_reply_retains_original_and_closes_without_resend() {
    for mode in [Mode::Unknown, Mode::WrongId, Mode::WrongKind] {
        let (mut s, _, seen) = server(scopes(), mode);
        initialize(&mut s);
        let original = request("memory_propose");
        let result = s
            .handle(&call(2, "memory_propose", original.clone()))
            .unwrap();
        assert_eq!(
            result["result"]["structuredContent"]["originalRequest"],
            original
        );
        assert_eq!(
            result["result"]["structuredContent"]["error"],
            json!({"code":"outcome_unknown","retryable":false})
        );
        assert_eq!(
            s.handle(&call(3, "memory_propose", original)).unwrap()["error"]["message"],
            "session_closed"
        );
        assert_eq!(seen.lock().unwrap().calls.len(), 1);
        assert_eq!(seen.lock().unwrap().disconnects, 1);
    }
}

#[test]
fn revocation_is_shared_and_eof_invalidates_reuse() {
    let (mut s, grant, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    grant.revoke();
    assert_eq!(
        s.handle(&call(2, "memory_read", request("memory_read")))
            .unwrap()["error"]["message"],
        "admission_revoked"
    );
    assert!(seen.lock().unwrap().calls.is_empty());
    let (mut s, grant, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    s.serve(&mut Cursor::new(Vec::<u8>::new()), &mut Vec::new())
        .unwrap();
    let next_seen = Arc::new(Mutex::new(Seen::default()));
    let mut next = Server::new(
        grant,
        Fake {
            seen: next_seen.clone(),
            mode: Mode::Valid,
        },
    );
    initialize(&mut next);
    assert_eq!(
        next.handle(&rpc(2, "tools/list", json!({}))).unwrap()["error"]["message"],
        "admission_revoked"
    );
    assert_eq!(seen.lock().unwrap().disconnects, 1);
    assert!(next_seen.lock().unwrap().calls.is_empty());
}

#[test]
fn stdio_rejects_duplicates_utf8_batches_and_oversize_without_backend_calls() {
    for raw in [
        br#"{"jsonrpc":"2.0","method":"ping","m\u0065thod":"tools/call","id":2}"#.as_slice(),
        br#"{"jsonrpc":"2.0","method":"tools/call","id":2,"params":{"name":"memory_read","name":"memory_source"}}"#.as_slice(),
        &[255],
        br#"[]"#.as_slice(),
    ] {
        let (mut s,_,seen)=server(scopes(),Mode::Valid); initialize(&mut s);
        let mut bytes=raw.to_vec(); bytes.push(b'\n'); let mut output=Vec::new();
        s.serve(&mut Cursor::new(bytes),&mut output).unwrap();
        let result:Value=serde_json::from_slice(&output).unwrap();
        assert!(result["error"].is_object()); assert!(seen.lock().unwrap().calls.is_empty());
    }
    for raw in [
        vec![b'x'; MAX_INPUT_BYTES + 2],
        b"{\"jsonrpc\":\"2.0\"}".to_vec(),
    ] {
        let (mut s, _, seen) = server(scopes(), Mode::Valid);
        assert!(s.serve(&mut Cursor::new(raw), &mut Vec::new()).is_err());
        assert!(seen.lock().unwrap().calls.is_empty());
        assert_eq!(seen.lock().unwrap().disconnects, 1);
    }
}

struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn stdout_failure_ends_session_after_one_dispatch_and_never_reads_the_next_call() {
    let (mut s, _, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    let packet = call(2, "memory_propose", request("memory_propose")).to_string();
    let input = format!(
        "{packet}\n{}\n",
        call(3, "memory_propose", request("memory_propose"))
    );
    assert!(s.serve(&mut Cursor::new(input), &mut BrokenOutput).is_err());
    assert_eq!(seen.lock().unwrap().calls.len(), 1);
    assert_eq!(seen.lock().unwrap().disconnects, 1);
}

#[test]
fn repeated_rpc_ids_and_session_budget_cannot_dispatch_again() {
    let (mut s, _, seen) = server(scopes(), Mode::Valid);
    initialize(&mut s);
    let packet = call(2, "memory_read", request("memory_read"));
    assert_eq!(s.handle(&packet).unwrap()["result"]["isError"], false);
    assert_eq!(
        s.handle(&packet).unwrap()["error"]["message"],
        "duplicate_request_id"
    );
    for id in 3..=MAX_REQUESTS {
        assert!(s.handle(&rpc(id, "ping", json!({}))).unwrap()["result"].is_object());
    }
    assert_eq!(
        s.handle(&rpc(MAX_REQUESTS + 1, "ping", json!({}))).unwrap()["error"]["message"],
        "session_request_limit"
    );
    assert_eq!(seen.lock().unwrap().calls.len(), 1);
    assert_eq!(seen.lock().unwrap().disconnects, 1);
    assert!(input_schema("candidate_review").is_none());
}
