#![cfg(windows)]
//! ADR-MEM-166: combined Agent protocol, idle reads with a stop signal, and the
//! bridge library function.
#[path = "support/agent_note_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, next::*, policy::Scope, ports::IdSource};
use enouia_memory_host::{
    automation::qualified_agent::QualifiedAgentHost,
    mcp::{
        agent::{self, AgentWriters},
        native::Enrollment,
        notes,
    },
    windows::Pipe,
};
use enouia_memory_vault::{OsIdSource, fault::Faults};
use fixture::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn initialization() -> [Value; 2] {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-agent","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}
fn tool_names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn combined_tools_list_shows_exactly_the_granted_write_tools() {
    let (e, v, p, _) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let owner = &e.descriptor.created_by;
    let principal = actor(ActorType::Agent, 88);
    let reader = host.reader(owner, principal.clone(), scope(&p)).unwrap();
    let note = host
        .note_writer(owner, principal.clone(), scope(&p))
        .unwrap();
    let correction = host
        .correction_reporter(owner, principal.clone(), scope(&p))
        .unwrap();
    let forget = host
        .forget_requester(owner, principal.clone(), scope(&p))
        .unwrap();
    let other = host
        .note_writer(owner, actor(ActorType::Agent, 89), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    let names = |writers: AgentWriters<'_, '_>| {
        let mut server = agent::Server::new(&reader, writers, &profile).unwrap();
        for message in initialization() {
            server.handle(&message);
        }
        tool_names(&server.handle(&list).unwrap())
    };
    let reads = names(AgentWriters::default());
    assert!(!reads.is_empty());
    assert!(!reads.iter().any(|n| n.starts_with("memory_note")
        || n == "memory_correction_report"
        || n == "memory_forget_request"));
    let all = names(AgentWriters {
        note: Some(&note),
        correction: Some(&correction),
        forget: Some(&forget),
    });
    let mut expected = reads.clone();
    expected.extend([
        "memory_note".to_string(),
        "memory_correction_report".to_string(),
        "memory_forget_request".to_string(),
    ]);
    assert_eq!(all, expected);
    let only_note = names(AgentWriters {
        note: Some(&note),
        ..Default::default()
    });
    assert_eq!(only_note.last().unwrap(), "memory_note");
    assert_eq!(only_note.len(), reads.len() + 1);
    // An ungranted write tool is not served; the read side refuses the name.
    let mut server = agent::Server::new(
        &reader,
        AgentWriters {
            note: Some(&note),
            ..Default::default()
        },
        &profile,
    )
    .unwrap();
    for message in initialization() {
        server.handle(&message);
    }
    let refused = server
        .handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_forget_request","arguments":{}}}))
        .unwrap();
    assert!(refused.get("error").is_some() || refused["result"]["isError"] == true);
    // A writer of another principal is refused at construction.
    assert!(
        agent::Server::new(
            &reader,
            AgentWriters {
                note: Some(&other),
                ..Default::default()
            },
            &profile,
        )
        .is_none()
    );
}

#[test]
fn combined_bridge_saves_a_note_survives_idle_and_ends_on_stop() {
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let note = host
        .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    // No `--host-pid` (ADR-MEM-165); the bin is a wrapper over `mcp::bridge::run`.
    let mut child = Command::new(env!("CARGO_BIN_EXE_enouia-memory-mcp"))
        .args([
            "--protocol",
            "automation-agent-v1",
            "--pipe",
            token.as_str(),
            "--vault-id",
            e.descriptor.vault_id.as_str(),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let enrollment = Enrollment::enroll(
        child.id(),
        principal,
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let stop = AtomicBool::new(false);
    // Shorter than the idle period below: idling is not a timeout.
    let timeout = Duration::from_millis(500);
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(Duration::from_secs(15)).unwrap();
            let result = agent::native::serve_connection(
                &reader,
                AgentWriters {
                    note: Some(&note),
                    ..Default::default()
                },
                &profile,
                &mut pipe,
                enrollment,
                timeout,
                &stop,
            );
            (result, Instant::now())
        });
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut next = || serde_json::from_str::<Value>(&output.next().unwrap().unwrap()).unwrap();
        for message in initialization() {
            writeln!(input, "{message}").unwrap();
        }
        assert_eq!(next()["id"], 1);
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
        )
        .unwrap();
        let tools = tool_names(&next());
        assert!(tools.contains(&"memory_note".to_string()));
        assert!(!tools.contains(&"memory_forget_request".to_string()));
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":"note","method":"tools/call","params":{"name":"memory_note","arguments":{"schema_version":2,"kind":"agent_note_request","capability":notes::CAPABILITY,"request":request}}})
        )
        .unwrap();
        assert_eq!(
            next()["result"]["structuredContent"]["receipt"]["state"],
            "hint_saved"
        );
        std::thread::sleep(Duration::from_millis(1500));
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":"after-idle","method":"ping"})
        )
        .unwrap();
        assert_eq!(next()["id"], "after-idle");
        let stopped = Instant::now();
        stop.store(true, Ordering::SeqCst);
        let (result, ended) = server.join().unwrap();
        assert!(result.is_err());
        assert!(ended.duration_since(stopped) < Duration::from_secs(2));
        drop(input);
        let _ = child.wait();
    });
}
