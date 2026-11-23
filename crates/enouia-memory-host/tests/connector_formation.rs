//! ADR-MEM-157/158 connector writer over the platform-neutral Vault API.
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*};
use enouia_memory_host::automation::connector_formation::*;
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
use fixture::*;
use std::sync::Arc;

fn owner_env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
fn producer() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Client,
        actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
    }
}
fn config() -> ConnectorConfig {
    ConnectorConfig {
        account_scope: "chatgpt-connector-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        max_sensitivity: Sensitivity::Private,
        allowed_forms: vec![FormationKind::Fact, FormationKind::Preference],
        window_days: 30,
    }
}
fn request(n: u32, text: &str) -> RuntimeCaptureRequest {
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut r: RuntimeCaptureRequest = serde_json::from_value(v["base"].clone()).unwrap();
    r.request_id = RequestId::parse(&id("req", n)).unwrap();
    r.conversation_id = "chatgpt-synthetic".into();
    r.turns[0].message_id = format!("m{n}");
    r.turns[0].parent_message_id = (n > 1).then(|| "m1".into());
    r.turns[0].sequence = n;
    r.turns[0].attachments.clear();
    r.turns[0].text = text.into();
    r
}
fn unknown() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
fn statement(message: &str, quote: &str) -> ConnectorStatement {
    ConnectorStatement {
        message_id: message.into(),
        quote: quote.into(),
        title: quote.into(),
        interpretation: Interpretation::ClearSelfReport,
        kind: FormationKind::Preference,
        event_time: unknown(),
        valid_time: unknown(),
        note: Some("distilled in ChatGPT".into()),
        entities: vec![],
    }
}

#[test]
fn chatgpt_submission_opens_a_window_forms_once_and_replays() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let owner = e.descriptor.created_by.clone();
    assert!(ConnectorWriter::new(v.clone(), owner.clone(), owner.clone(), config(),).is_err());
    let w = ConnectorWriter::new(v.clone(), owner.clone(), producer(), config()).unwrap();
    let submission = ConnectorSubmission {
        request: request(1, "I prefer quiet rooms."),
        analyst_model: "synthetic-chatgpt-model".into(),
        statements: vec![statement("m1", "quiet rooms")],
    };
    for bad in [
        statement("m1", "loud rooms"),
        statement("m9", "quiet rooms"),
    ] {
        let mut s = submission.clone();
        s.statements = vec![bad];
        assert!(w.submit(&s, false, NOW).is_err());
    }
    // Preflight before any window exists writes nothing.
    let before = v.snapshot().unwrap();
    assert!(w.submit(&submission, true, NOW).unwrap().window_needed);
    assert_eq!(v.snapshot().unwrap(), before);
    let first = w.submit(&submission, false, NOW).unwrap();
    assert!(first.capture.as_ref().unwrap().captured);
    assert_eq!(first.jobs.len(), 1);
    assert_eq!(first.jobs[0].state, FormationJobState::Formed);
    assert_eq!(first.jobs[0].receipts, 1);
    let windows = v.capture_windows(&owner).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].unused_source_ids.len(), MAX_SOURCES - 1);
    // A second submission reuses the open window; a turn without statements forms nothing.
    let empty = ConnectorSubmission {
        request: request(2, "Thanks."),
        analyst_model: "synthetic-chatgpt-model".into(),
        statements: vec![],
    };
    let second = w.submit(&empty, false, NOW).unwrap();
    assert_eq!(second.jobs[0].state, FormationJobState::NoMemory);
    assert_eq!(v.capture_windows(&owner).unwrap().len(), 1);
    // Preflight with an open window checks the capture without writing.
    let third = ConnectorSubmission {
        request: request(3, "I like tea."),
        analyst_model: "synthetic-chatgpt-model".into(),
        statements: vec![statement("m3", "tea")],
    };
    let before = v.snapshot().unwrap();
    let dry = w.submit(&third, true, NOW).unwrap();
    assert!(!dry.window_needed && dry.capture.unwrap().preflight);
    assert_eq!(v.snapshot().unwrap(), before);
    // Resubmitting the first request replays; nothing forms twice.
    let again = w.submit(&submission, false, NOW).unwrap();
    assert!(again.capture.as_ref().unwrap().replayed);
    assert!(again.jobs.iter().all(|j| j.receipts <= 1));
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        let formed: Vec<_> = d
            .memories
            .iter()
            .filter(|m| {
                m.memory.request.model_version.as_deref() == Some("synthetic-chatgpt-model")
            })
            .collect();
        assert_eq!(formed.len(), 1);
        let fields = formed[0].memory.request.record_fields.as_ref().unwrap();
        assert_eq!(fields.title, "quiet rooms");
        let annotations = fields.annotations.as_ref().unwrap();
        assert_eq!(annotations.prompt_version, PROMPT_VERSION);
        assert_eq!(
            annotations.formation_note.as_deref(),
            Some("distilled in ChatGPT")
        );
    })
    .unwrap();
}

#[test]
fn named_entities_become_records_and_mentions_and_replay_without_duplicates() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let owner = e.descriptor.created_by.clone();
    let w = ConnectorWriter::new(v.clone(), owner, producer(), config()).unwrap();
    let entity = |name: &str, entity_type: EntityType, role: EntityRole| ConnectorEntity {
        name: name.into(),
        entity_type,
        reality: EntityReality::Real,
        role,
    };
    let mut named = statement("m1", "hiking with Lin at Huangshan");
    named.entities = vec![
        entity("Lin", EntityType::Person, EntityRole::Participant),
        entity("Huangshan", EntityType::Place, EntityRole::Place),
    ];
    let submission = ConnectorSubmission {
        request: request(1, "I went hiking with Lin at Huangshan."),
        analyst_model: "synthetic-chatgpt-model".into(),
        statements: vec![named.clone()],
    };
    // Too many entities, or a blank name, are refused before the Vault.
    let mut crowded = submission.clone();
    crowded.statements[0].entities = (0..9)
        .map(|n| entity(&format!("E{n}"), EntityType::Thing, EntityRole::Mentioned))
        .collect();
    assert!(w.submit(&crowded, false, NOW).is_err());
    let mut blank = submission.clone();
    blank.statements[0].entities[0].name = " ".into();
    assert!(w.submit(&blank, false, NOW).is_err());
    // Preflight (with a window open) records nothing.
    let mut opener = request(2, "Thanks.");
    opener.turns[0].parent_message_id = None;
    let plain = ConnectorSubmission {
        request: opener,
        analyst_model: "synthetic-chatgpt-model".into(),
        statements: vec![],
    };
    w.submit(&plain, false, NOW).unwrap();
    let before = v.snapshot().unwrap();
    assert!(w.submit(&submission, true, NOW).is_ok());
    assert_eq!(v.snapshot().unwrap(), before);
    let first = w.submit(&submission, false, NOW).unwrap();
    assert_eq!(first.jobs[0].state, FormationJobState::Formed);
    let read = || {
        v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            let memory = d
                .memories
                .iter()
                .find(|m| {
                    m.memory.request.model_version.as_deref() == Some("synthetic-chatgpt-model")
                })
                .unwrap();
            let fields = memory.memory.request.record_fields.as_ref().unwrap();
            let named: Vec<_> = fields
                .entities
                .iter()
                .map(|m| {
                    let record = &d.entities[&m.entity_id];
                    assert_eq!(m.evidence, fields.evidence);
                    assert_eq!(record.evidence, fields.evidence);
                    (record.display_name.clone(), record.entity_type, m.role)
                })
                .collect();
            (named, d.entities.len())
        })
        .unwrap()
    };
    let (named, count) = read();
    assert_eq!(
        named,
        vec![
            (
                "Lin".to_string(),
                EntityType::Person,
                EntityRole::Participant
            ),
            (
                "Huangshan".to_string(),
                EntityType::Place,
                EntityRole::Place
            ),
        ]
    );
    assert_eq!(count, 2);
    // A resubmission replays: the same records, no duplicates, no second memory.
    let again = w.submit(&submission, false, NOW).unwrap();
    assert!(again.capture.unwrap().replayed);
    assert_eq!(read(), (named, 2));
}
