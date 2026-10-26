#![allow(dead_code)]
use enouia_memory_contract::{
    common::*, foundation::FakeClock, hash::sha256, ids::*, next::*, ports::IdSource,
};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, VerifiedRoot, automation::segmented::SegmentedAutomationVault,
    fault::Faults, verify_data_root,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub const NOW: i64 = 1_792_843_200_000;
pub fn value(path: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures")
                .join(path),
        )
        .unwrap(),
    )
    .unwrap()
}
pub fn id(prefix: &str, n: u32) -> String {
    format!("{prefix}_{n:08x}-0000-4000-8000-000000000001")
}
pub struct Env {
    pub path: PathBuf,
    pub root: VerifiedRoot,
    pub descriptor: AutomationDescriptor,
    pub clock: Arc<FakeClock>,
    base: PathBuf,
}
impl Env {
    pub fn new() -> Self {
        let base = std::env::temp_dir().join("enouia-memory-segmented-synthetic");
        std::fs::create_dir_all(&base).unwrap();
        let canonical = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));
        let path = base.join(format!(
            "root-{}",
            enouia_memory_contract::ids::format_uuid_v4(OsIdSource.random_16())
        ));
        std::fs::create_dir(&path).unwrap();
        let root = verify_data_root(&path, &RootPolicy::default()).unwrap();
        let mut descriptor: AutomationDescriptor =
            serde_json::from_value(value("ui-records/descriptor.json")).unwrap();
        descriptor.layout = AutomationLayout::SegmentedV1;
        Self {
            path,
            root,
            descriptor,
            clock: Arc::new(FakeClock::new(NOW)),
            base,
        }
    }
    pub fn create(&self, faults: Faults) -> SegmentedAutomationVault {
        SegmentedAutomationVault::create(
            &self.root,
            self.descriptor.clone(),
            self.clock.clone(),
            Arc::new(OsIdSource),
            faults,
        )
        .unwrap()
    }
    pub fn open(&self) -> SegmentedAutomationVault {
        SegmentedAutomationVault::open(
            &self.root,
            self.descriptor.host_id.clone(),
            self.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap()
    }
    pub fn policy(&self, n: u32, first: u32, len: u32) -> AutomationPolicy {
        let mut p: AutomationPolicy =
            serde_json::from_value(value("formation/policy.json")).unwrap();
        p.policy_id = PolicyId::parse(&id("pol", 1000 + n)).unwrap();
        p.source_ids = (first..first + len)
            .map(|n| SourceId::parse(&id("src", n)).unwrap())
            .collect();
        p.target_ids.clear();
        p
    }
    pub fn source(&self, n: u32, sensitivity: Sensitivity) -> CapturedSource {
        let mut s: CapturedSource =
            serde_json::from_value(value("ui-records/source.json")).unwrap();
        s.record.source_id = SourceId::parse(&id("src", n)).unwrap();
        s.record.locator = Locator::RuntimeEvent {
            event_id: EventId::parse(&id("evt", n)).unwrap(),
        };
        s.record.original_conversation_id = None;
        s.record.original_message_id = None;
        s.record.branch_id = enouia_memory_contract::json::Knowable::Known(None);
        s.record_fields.as_mut().unwrap().conversation_id = None;
        s.record.sensitivity = sensitivity;
        s.text = if n.is_multiple_of(2) {
            format!("合成条目 {n}：我喜欢夜间散步。")
        } else {
            format!("Synthetic item {n}: I prefer moonlight walks.")
        };
        s.record_fields.as_mut().unwrap().content_lang =
            if n.is_multiple_of(2) { "zh-Hans" } else { "en" }.into();
        s.record.content_hash = sha256(s.text.as_bytes());
        s
    }
    pub fn input(
        &self,
        source: &CapturedSource,
        policy: &AutomationPolicy,
        n: u32,
    ) -> AdmissionInput {
        let mut i: AdmissionInput = serde_json::from_value(value("ui-records/input.json")).unwrap();
        i.policy = policy.reference();
        i.project_id = policy.project_id.clone();
        i.input_refs = vec![source.reference()];
        i.content = source.text.clone();
        i.evidence[0] = EvidenceRef {
            source_id: source.record.source_id.clone(),
            source_revision: source.record.revision,
            locator: source.record.locator.clone(),
            object_hash: source.record.content_hash.clone(),
            evidence_class: source.record.evidence_class,
            supports: "content".into(),
        };
        let f = i.record_fields.as_mut().unwrap();
        f.title = if n.is_multiple_of(2) {
            format!("合成条目 {n}")
        } else {
            format!("Synthetic item {n}")
        };
        f.content_lang = source.record_fields.as_ref().unwrap().content_lang.clone();
        f.entities.clear();
        f.annotations = None;
        f.evidence = vec![QuotedEvidence {
            reference: i.evidence[0].clone(),
            quote: source.text.clone(),
            speaker: source.record.speaker_role,
            start: 0,
            end: source.text.chars().count() as u32,
        }];
        i.sensitivity = source.record.sensitivity;
        if n.is_multiple_of(3) {
            i.interpretation = Interpretation::Inference;
        }
        if n.is_multiple_of(5) {
            i.dimensions.as_mut().unwrap().kind = FormationKind::Fact;
        }
        i
    }
    pub fn populate(&self, v: &SegmentedAutomationVault, count: u32) -> Vec<MemoryReceipt> {
        let mut receipts = Vec::new();
        let mut first = 1;
        while first <= count {
            if count >= 20_000 && (first - 1).is_multiple_of(1024) {
                println!("formed={}/{}", receipts.len(), count);
            }
            let len = (count - first + 1).min(64);
            let p = self.policy(first, first, len);
            v.set_policy(&self.descriptor.created_by, p.clone())
                .unwrap();
            let sources: Vec<_> = (first..first + len)
                .map(|n| {
                    self.source(
                        n,
                        if n.is_multiple_of(7) {
                            Sensitivity::Private
                        } else {
                            Sensitivity::Public
                        },
                    )
                })
                .collect();
            v.capture(
                &self.descriptor.system_actor,
                sources.clone(),
                RecordCatalog::default(),
            )
            .unwrap();
            for chunk in sources.chunks(8) {
                let requests: Vec<_> = chunk
                    .iter()
                    .map(|s| {
                        v.prepare(
                            &self.descriptor.system_actor,
                            self.input(
                                s,
                                &p,
                                u32::from_str_radix(&s.record.source_id.as_str()[4..12], 16)
                                    .unwrap(),
                            ),
                        )
                        .unwrap()
                    })
                    .collect();
                let responses: Vec<_> = requests
                    .iter()
                    .map(|i| {
                        serde_json::to_string(
                            &v.evaluate(&self.descriptor.system_actor, i).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                let bindings = requests
                    .iter()
                    .zip(&responses)
                    .map(|(i, s)| GenerationBinding {
                        operation_id: i.operation_id.clone(),
                        input_hash: digest(i),
                        response_hash: sha256(s.as_bytes()),
                    })
                    .collect();
                receipts.extend(
                    v.apply_batch(&self.descriptor.system_actor, requests, responses, bindings)
                        .unwrap(),
                );
            }
            first += len;
        }
        receipts
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        // Only the unique directory this fixture actually created, beneath its
        // canonical temporary parent. No supplied path or private root is used.
        if self.path.parent() == Some(self.base.as_path()) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
