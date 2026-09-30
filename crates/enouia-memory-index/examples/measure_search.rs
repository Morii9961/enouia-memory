//! MV-4 timing baseline: `n` synthetic memories (owner statements accepted
//! in reviewed batches of 50), then an index rebuild, an incremental update
//! of one more commit, and the median of repeated queries by path.
//! Synthetic data only; the temporary root is removed.
//!
//! Run: `cargo run --release -p enouia-memory-index --example measure_search -- 2000`

use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ids::{PolicyId, PrincipalId, SubjectId};
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_govern::{
    Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed, confirm, plan, propose,
};
use enouia_memory_index::{Index, SearchRequest, search};
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use serde_json::{Map, json};
use std::sync::Arc;
use std::time::Instant;

const WORDS: &[&str] = &[
    "记忆",
    "函馆",
    "琉璃光院",
    "MoriMeta",
    "乌龙茶",
    "爬山",
    "预算",
    "上下文",
    "本地版本",
    "发布计划",
    "审核",
    "来源",
    "index",
    "vault",
    "agent",
    "周末",
    "旅行",
    "项目",
];

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    let base = std::env::temp_dir().join(format!("enouia-search-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("root")).unwrap();
    let base = std::path::PathBuf::from(
        std::fs::canonicalize(&base)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches(r"\\?\"),
    );
    let root = verify_data_root(&base.join("root"), &RootPolicy::default()).unwrap();
    let clock = Arc::new(FakeClock::new(1_790_000_000_000));
    let ids = Arc::new(SequentialIdSource::new(1));
    let owner = ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    };
    let now = enouia_memory_contract::time::Timestamp::from_unix_ms(1_790_000_000_000).unwrap();
    let genesis = new_genesis(
        ids.as_ref(),
        owner.clone(),
        TrustedSurface::TrustedLocalCli,
        &now,
    )
    .unwrap();
    let vault = Vault::create(&root, genesis, clock.clone(), ids, VaultOptions::default()).unwrap();
    let pin = vault.pin_current().unwrap();
    let policy =
        PolicyId::parse(&vault.record_entries(&pin, RecordKind::Policy).unwrap()[0].record_id)
            .unwrap();
    let tick = || clock.set(clock.now_ms() + 1_000);
    let yes = OwnerConfirmation {
        owner: owner.clone(),
        surface: TrustedSurface::TrustedLocalCli,
    };
    let started = Instant::now();
    let mut decisions = Vec::new();
    for i in 0..n {
        let text = format!(
            "（合成）第 {i} 条：{} 与 {} 的笔记，{}。",
            WORDS[i % WORDS.len()],
            WORDS[(i * 7 + 3) % WORDS.len()],
            WORDS[(i * 13 + 5) % WORDS.len()]
        );
        tick();
        let source = vault
            .record_manual_assertion(
                &ManualAssertionInput {
                    text: text.clone(),
                    operator: owner.clone(),
                    trusted_surface: TrustedSurface::TrustedLocalCli,
                    confirmation: ConfirmationMethod::TypedConfirmation,
                    sensitivity: Sensitivity::Private,
                    access_policy_id: policy.clone(),
                    time_precision: TimePrecision::Millisecond,
                },
                format!("s{i}").as_bytes(),
            )
            .unwrap()
            .id;
        let mut details = Map::new();
        details.insert("claim_key".into(), json!(format!("note.{i}")));
        details.insert(
            "subject_ids".into(),
            json!([SubjectId::parse("sub_00000001-0000-4000-8000-000000000001").unwrap()]),
        );
        tick();
        let Proposed::Stored(written) = propose(
            &vault,
            &Proposal::create(
                ProposedType::Fact,
                &text,
                details,
                vec![EvidenceSpec::content(source, Revision::new(1).unwrap())],
            ),
            &Origin::owner(owner.clone()),
            format!("p{i}").as_bytes(),
        )
        .unwrap() else {
            panic!("stored")
        };
        decisions.push(Decision::Accept {
            candidate_id: written.id,
            revision: Revision::new(1).unwrap(),
        });
        if decisions.len() == 50 || i + 1 == n {
            tick();
            let shown = plan(&vault, &decisions, &owner, TrustedSurface::TrustedLocalCli).unwrap();
            confirm(&vault, &shown, &yes).unwrap();
            decisions.clear();
        }
    }
    println!(
        "{n} memories written in {:.1} s",
        started.elapsed().as_secs_f64()
    );

    let started = Instant::now();
    let (mut index, report) = Index::rebuild(&vault).unwrap();
    println!(
        "rebuild: {} revisions over {} commits in {:.2} s; file {} bytes",
        report.revisions_indexed,
        report.commits_applied,
        started.elapsed().as_secs_f64(),
        std::fs::metadata(index.path()).unwrap().len()
    );
    tick();
    vault
        .record_manual_assertion(
            &ManualAssertionInput {
                text: "（合成）追加一条。".into(),
                operator: owner.clone(),
                trusted_surface: TrustedSurface::TrustedLocalCli,
                confirmation: ConfirmationMethod::TypedConfirmation,
                sensitivity: Sensitivity::Private,
                access_policy_id: policy.clone(),
                time_precision: TimePrecision::Millisecond,
            },
            b"extra",
        )
        .unwrap();
    let started = Instant::now();
    index.update(&vault, &|| false).unwrap();
    println!(
        "incremental update of one commit: {:.3} s",
        started.elapsed().as_secs_f64()
    );

    for query in [
        "记忆",
        "琉璃光院",
        "MoriMeta",
        "茶",
        "index vault",
        "上下文 预算",
    ] {
        let mut times = Vec::new();
        let mut hits = 0;
        for _ in 0..7 {
            let started = Instant::now();
            let page = search(&index, &vault, &owner, &SearchRequest::text(query)).unwrap();
            times.push(started.elapsed().as_secs_f64());
            hits = page.items.len();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "query {query:?}: median {:.1} ms (first {:.1} ms), {hits} hits on page 1",
            times[3] * 1000.0,
            times[0].max(0.0) * 1000.0
        );
    }
    drop(index);
    drop(vault);
    let _ = std::fs::remove_dir_all(&base);
}

trait Now {
    fn now_ms(&self) -> i64;
}

impl Now for FakeClock {
    fn now_ms(&self) -> i64 {
        use enouia_memory_contract::foundation::Clock;
        self.now_unix_ms()
    }
}
