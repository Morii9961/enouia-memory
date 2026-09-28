//! MV-1.3: minimal source and session objects with every key field on disk,
//! Vault health, and the restricted audit log. B04 (log part): a sentinel
//! string written as record text never appears in any store bookkeeping,
//! journal, audit line, health output, or error text.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::{AuditDecision, AuditEvent};
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::ids::{AuditId, PolicyId, PrincipalId, RequestId};
use enouia_memory_contract::ipc::Operation;
use enouia_memory_contract::json::{Revision, SchemaVersion};
use enouia_memory_contract::parse_record;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::session::{ClientSurface, SessionEvent, SessionRecord};
use enouia_memory_contract::source::{ConfirmationMethod, SourceKind, SourceRecord};
use enouia_memory_vault::audit::AuditLog;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::health::{HealthNote, HealthState};
use enouia_memory_vault::service::{ManualAssertionInput, SessionStart};
use std::path::Path;
use support::Harness;

const SENTINEL: &str = "SENTINEL-合成-7c1f-不应出现在日志";

fn policy() -> PolicyId {
    PolicyId::parse("pol_00000001-0000-4000-8000-000000000001").unwrap()
}

fn assertion(harness: &Harness, text: &str) -> ManualAssertionInput {
    ManualAssertionInput {
        text: text.to_owned(),
        operator: harness.lifecycle.owner(),
        trusted_surface: TrustedSurface::TrustedLocalCli,
        confirmation: ConfirmationMethod::TypedConfirmation,
        sensitivity: Sensitivity::Private,
        access_policy_id: policy(),
        time_precision: TimePrecision::Millisecond,
    }
}

fn read<T: enouia_memory_contract::record::Record>(
    harness: &Harness,
    kind: RecordKind,
    id: &str,
    revision: u64,
) -> T {
    let pin = harness.vault.pin_current().unwrap();
    let bytes = harness
        .vault
        .read_record(
            &pin,
            &RecordRef::new(kind, id, Revision::new(revision).unwrap()),
        )
        .unwrap();
    parse_record(&bytes).unwrap()
}

#[test]
fn a_manual_assertion_is_stored_whole_and_resubmission_replays() {
    let harness = Harness::with_commits("manual", 0);
    harness.clock.set(harness.lifecycle.time(0) + 1_000);
    let input = assertion(&harness, "（合成）我偏好用深色主题。");
    let first = harness
        .vault
        .record_manual_assertion(&input, b"ui-submit-1")
        .unwrap();
    assert!(matches!(first.outcome, CommitOutcome::Committed { .. }));
    let source: SourceRecord = read(&harness, RecordKind::Source, first.id.as_str(), 1);
    assert_eq!(source.source_kind, SourceKind::ManualAssertion);
    let manual = source.manual_assertion.as_ref().unwrap();
    assert_eq!(manual.input_text, input.text);
    assert_eq!(manual.operator, input.operator);
    assert_eq!(manual.trusted_surface, TrustedSurface::TrustedLocalCli);
    assert_eq!(
        source.content_hash,
        enouia_memory_contract::hash::sha256(input.text.as_bytes())
    );

    let again = harness
        .vault
        .record_manual_assertion(&input, b"ui-submit-1")
        .unwrap();
    assert_eq!(again.id, first.id);
    assert!(matches!(again.outcome, CommitOutcome::Replayed { .. }));
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 2);

    let edited = assertion(&harness, "（合成）我偏好用浅色主题。");
    let error = harness
        .vault
        .record_manual_assertion(&edited, b"ui-submit-1")
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::IdempotencyConflict);

    let mut agent = assertion(&harness, "（合成）代理声称的内容。");
    agent.operator = ActorRef {
        actor_id: PrincipalId::parse("prn_00000009-0000-4000-8000-000000000009").unwrap(),
        actor_type: ActorType::Agent,
    };
    assert!(
        harness
            .vault
            .record_manual_assertion(&agent, b"agent")
            .is_err()
    );
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 2);
}

#[test]
fn user_messages_are_saved_with_their_text_before_anything_else() {
    let harness = Harness::with_commits("session", 0);
    harness.clock.set(harness.lifecycle.time(0) + 1_000);
    let owner = harness.lifecycle.owner();
    let start = SessionStart {
        owner: owner.clone(),
        surface: ClientSurface::LocalCli,
        sensitivity: Sensitivity::Private,
        policy_id: policy(),
    };
    let session = harness
        .vault
        .start_session(&start, b"start-1")
        .unwrap()
        .id
        .0;
    let texts = ["（合成）第一句。", "（合成）第二句，稍长一些。"];
    let mut events = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        harness
            .clock
            .set(harness.lifecycle.time(0) + 2_000 + index as i64);
        let request = RequestId::parse(&format!(
            "req_0000000{}-0000-4000-8000-00000000000{}",
            index + 1,
            index + 1
        ))
        .unwrap();
        let written = harness
            .vault
            .append_user_message(
                &session,
                &owner,
                text,
                &request,
                format!("msg-{index}").as_bytes(),
            )
            .unwrap();
        events.push(written.id);
    }
    let record: SessionRecord = read(&harness, RecordKind::Session, session.as_str(), 3);
    assert_eq!(record.last_event_seq, 2);
    let pin = harness.vault.pin_current().unwrap();
    for (index, event_id) in events.iter().enumerate() {
        let event: SessionEvent = read(&harness, RecordKind::SessionEvent, event_id.as_str(), 1);
        assert_eq!(event.sequence, index as u64 + 1);
        let content = event.content_ref.unwrap();
        let bytes = harness
            .vault
            .read_object(&pin, &content.object_hash)
            .unwrap();
        assert_eq!(bytes, texts[index].as_bytes());
        assert_eq!(content.size_bytes, bytes.len() as u64);
    }
    // A lost response: the same message key replays, no third event.
    let replay = harness
        .vault
        .append_user_message(
            &session,
            &owner,
            texts[1],
            &RequestId::parse("req_00000002-0000-4000-8000-000000000002").unwrap(),
            b"msg-1",
        )
        .unwrap();
    assert_eq!(replay.id, events[1]);
    let record: SessionRecord = read(&harness, RecordKind::Session, session.as_str(), 3);
    assert_eq!(record.last_event_seq, 2);
    assert!(
        harness
            .vault
            .verify(&harness.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
}

#[test]
fn health_reports_healthy_degraded_and_recovering_without_writing() {
    let harness = Harness::with_commits("health", 3);
    let health = harness.vault.health();
    assert_eq!(health.state, HealthState::Healthy);
    assert_eq!(health.head_sequence, Some(4));

    let faults = Faults::armed();
    let vault = harness.reopen(faults.clone());
    faults.arm(FaultPoint::RecordsPlaced, 1, FaultAction::Fail(112));
    harness.clock.set(harness.lifecycle.time(4));
    assert!(vault.commit(harness.lifecycle.request(4)).is_err());
    let degraded = vault.health();
    assert_eq!(degraded.state, HealthState::Degraded);
    assert_eq!(degraded.notes, vec![HealthNote::StagingLeftover]);
    assert_eq!(
        degraded.head_sequence,
        Some(4),
        "the failed write is not visible"
    );
    faults.disarm();
    vault.commit(harness.lifecycle.request(4)).unwrap();
    assert_eq!(vault.health().state, HealthState::Healthy);

    std::fs::write(harness.root.path().join("vault/CURRENT"), b"{").unwrap();
    let broken = harness.reopen(Faults::none());
    let health = broken.health();
    assert_eq!(health.state, HealthState::Recovering);
    assert_eq!(health.error_code, Some(MemoryErrorCode::VaultRecovering));
    assert_eq!(
        std::fs::read(harness.root.path().join("vault/CURRENT")).unwrap(),
        b"{"
    );
}

fn audit_event(n: u32, decision: AuditDecision) -> AuditEvent {
    AuditEvent {
        schema_version: SchemaVersion,
        audit_id: AuditId::parse(&format!("aud_{n:08x}-0000-4000-8000-{n:012x}")).unwrap(),
        actor: ActorRef {
            actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
            actor_type: ActorType::Owner,
        },
        operation: Operation::MemoryRead,
        object_refs: vec![RecordRef::new(
            RecordKind::Source,
            "src_00000065-0000-4000-8000-000000000065",
            Revision::new(1).unwrap(),
        )],
        purpose: None,
        destination: None,
        policy_epoch: 1,
        decision,
        error_code: (decision == AuditDecision::Deny).then_some(MemoryErrorCode::PermissionDenied),
        request_id: RequestId::parse("req_0000000a-0000-4000-8000-00000000000a").unwrap(),
        created_at: enouia_memory_contract::time::Timestamp::parse("2026-09-28T08:00:00.000Z")
            .unwrap(),
    }
}

#[test]
fn the_audit_log_rotates_rejects_free_text_shapes_and_survives_a_torn_line() {
    let harness = Harness::with_commits("audit", 0);
    let log = AuditLog::new(harness.vault.managed_root().clone(), 600);
    for n in 1..=6 {
        let decision = if n % 2 == 0 {
            AuditDecision::Deny
        } else {
            AuditDecision::Allow
        };
        log.append(&audit_event(n, decision)).unwrap();
    }
    assert!(log.segments().unwrap().len() >= 2, "rotated by size");
    let mut invalid = audit_event(7, AuditDecision::Deny);
    invalid.error_code = None;
    assert!(log.append(&invalid).is_err(), "a deny needs its code");
    let last = log.segments().unwrap().last().unwrap().clone();
    let path = harness.root.path().join("vault/audit").join(&last);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(b"{\"audit_id\":\"aud_");
    std::fs::write(&path, bytes).unwrap();
    let (events, torn) = log.read_all().unwrap();
    assert_eq!(events.len(), 6);
    assert_eq!(torn, 1);
}

fn scan(dir: &Path, skip: &[&str], root: &Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if skip.iter().any(|s| rel.starts_with(s)) {
            continue;
        }
        if path.is_dir() {
            scan(&path, skip, root, hits);
        } else if String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(SENTINEL) {
            hits.push(rel);
        }
    }
}

#[test]
fn sentinel_text_never_reaches_bookkeeping_audit_health_or_errors() {
    let harness = Harness::with_commits("sentinel", 0);
    harness.clock.set(harness.lifecycle.time(0) + 1_000);
    let owner = harness.lifecycle.owner();
    let text = format!("（合成）{SENTINEL}");
    let written = harness
        .vault
        .record_manual_assertion(&assertion(&harness, &text), b"sentinel")
        .unwrap();
    let session = harness
        .vault
        .start_session(
            &SessionStart {
                owner: owner.clone(),
                surface: ClientSurface::LocalCli,
                sensitivity: Sensitivity::Private,
                policy_id: policy(),
            },
            b"s",
        )
        .unwrap()
        .id
        .0;
    harness
        .vault
        .append_user_message(
            &session,
            &owner,
            &text,
            &RequestId::parse("req_00000003-0000-4000-8000-000000000003").unwrap(),
            b"m",
        )
        .unwrap();
    let log = AuditLog::new(harness.vault.managed_root().clone(), 1 << 20);
    log.append(&audit_event(1, AuditDecision::Allow)).unwrap();
    let conflict = harness
        .vault
        .record_manual_assertion(&assertion(&harness, "（合成）另一句"), b"sentinel")
        .unwrap_err();
    let outputs = format!("{conflict} {conflict:?} {:?}", harness.vault.health());
    assert!(!outputs.contains(SENTINEL));

    // The text is expected exactly where it was written: the source record
    // and the session content object. Nowhere else.
    let mut hits = Vec::new();
    scan(
        harness.root.path(),
        &["vault/records/source/", "vault/session-content/objects/"],
        harness.root.path(),
        &mut hits,
    );
    assert!(hits.is_empty(), "sentinel leaked into {hits:?}");
    let mut expected = Vec::new();
    scan(harness.root.path(), &[], harness.root.path(), &mut expected);
    assert_eq!(expected.len(), 2, "{expected:?}");
    assert!(expected.iter().any(|p| p.contains(written.id.as_str())));
}
