#![allow(dead_code)]
//! Synthetic supersede target and captured Runtime user evidence.
#[path = "agent_note_fixture.rs"]
mod note;
use enouia_memory_contract::{common::*, hash::sha256, ids::*, next::*};
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
pub(crate) use note::*;
use std::sync::Arc;

pub(crate) fn publish(e: &Env, v: &SegmentedAutomationVault, input: AdmissionInput) {
    let input = v.prepare(&e.descriptor.system_actor, input).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &input).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: sha256(response.as_bytes()),
    };
    v.apply_batch(
        &e.descriptor.system_actor,
        vec![input],
        vec![response],
        vec![binding],
    )
    .unwrap();
}

/// A lawful System update of the target from another policy source, standing
/// in for a later consumer. Returns the new current target.
pub(crate) fn revise(
    e: &Env,
    v: &SegmentedAutomationVault,
    p: &AutomationPolicy,
    request: &AgentSupersedeRequest,
) -> TargetRef {
    let n = (1..8)
        .find(|n| SourceId::parse(&id("src", *n)).unwrap() != request.source.source_id)
        .unwrap();
    let source = e.source(n, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let mut input = e.input(&source, p, n);
    input.target = Some(request.target.clone());
    input.dimensions.as_mut().unwrap().change = FormationChange::Update;
    publish(e, v, input);
    let current = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories
                .iter()
                .find(|m| m.target.memory.memory_id == request.target.memory.memory_id)
                .unwrap()
                .target
                .clone()
        })
        .unwrap();
    assert_ne!(current, request.target);
    current
}

pub(crate) fn supersede_rig(
    speaker: SpeakerRole,
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentSupersedeRequest,
) {
    supersede_rig_with(speaker, Faults::none())
}
pub(crate) fn supersede_rig_with(
    speaker: SpeakerRole,
    faults: Faults,
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentSupersedeRequest,
) {
    let (e, v, p, note) = rig("前缀「ＡＢＣ　现在变化」后缀", speaker, faults);
    let source = e.source(8, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    publish(&e, &v, e.input(&source, &p, 8));
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories[0].target.clone()
        })
        .unwrap();
    let request = AgentSupersedeRequest {
        request_id: note.request_id,
        target,
        source: note.source,
        quote_hint: "\"ABC 现在变化\"".into(),
        supersede: "Untrusted synthetic supersede interpretation.".into(),
    };
    (e, v, p, request)
}
