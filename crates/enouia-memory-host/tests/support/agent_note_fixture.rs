//! Synthetic Runtime capture + explicit Agent note fixture, shared by protocol tests.
#[path = "../../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod segmented;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
pub(crate) use segmented::*;
use std::sync::Arc;
pub(crate) fn actor(role: ActorType, n: u32) -> ActorRef {
    ActorRef {
        actor_type: role,
        actor_id: PrincipalId::parse(&id("prn", n)).unwrap(),
    }
}
pub(crate) fn scope(p: &AutomationPolicy) -> SegmentedAgentScope {
    SegmentedAgentScope {
        subject_id: p.subject_id.clone(),
        project_id: p.project_id.clone(),
        policy_ids: vec![p.policy_id.clone()],
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        source_read: true,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
pub(crate) fn rig(
    text: &str,
    speaker: SpeakerRole,
    faults: Faults,
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentNoteRequest,
) {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    let v = Arc::new(e.create(faults));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let permit = v
        .authorize_owner_runtime_capture(
            &e.descriptor.created_by,
            actor(ActorType::Client, 77),
            RuntimeCaptureScope {
                policy_id: p.policy_id.clone(),
                source_ids: p.source_ids.clone(),
                max_sensitivity: Sensitivity::Private,
                platform: "enouia-runtime".into(),
                account_scope: "account-synthetic".into(),
                default_timezone: "Asia/Shanghai".into(),
                valid_until: scope(&p).valid_until,
            },
        )
        .unwrap();
    let mut capture: RuntimeCaptureRequest =
        serde_json::from_value(value("runtime-owner-capture/cases.json")["base"].clone()).unwrap();
    capture.turns[0].text = text.into();
    capture.turns[0].speaker = speaker;
    capture.turns[0].assistant_model =
        (speaker == SpeakerRole::Assistant).then(|| "synthetic-model".into());
    capture.turns[0].attachments.clear();
    let captured = v.capture_runtime(&permit, &capture, false).unwrap();
    let request = AgentNoteRequest {
        request_id: RequestId::parse(&id("req", 12)).unwrap(),
        source: captured.sources[0].clone(),
        quote_hint: text.into(),
        note: "Optional synthetic interpretation; not Owner confirmation.".into(),
    };
    (e, v, p, request)
}
