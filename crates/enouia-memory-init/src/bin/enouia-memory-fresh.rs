//! Trusted native local CLI, not an MCP or page endpoint (ADR145).
use enouia_memory_contract::{
    common::{ActorRef, ActorType},
    foundation::Clock,
    hash::Sha256Hex,
    ids::*,
    next::*,
    ports::IdSource,
};
use enouia_memory_host::owner_bootstrap::{registered_owner_root, restored_owner_root};
use enouia_memory_init::{
    driver::load_fresh,
    pipeline::{Initializer, fresh_journal_capacity},
};
use enouia_memory_vault::{
    OsIdSource, SystemClock,
    automation::{AutomationFault, segmented::SegmentedAutomationVault},
    fault::Faults,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

fn run(args: &[String]) -> Result<Value, String> {
    let (command, manifest, owner, reconcile) = match args {
        [c, m] if c == "plan" => (c, m, None, None),
        [c, m, h, op, flag] if (c == "run" || c == "recover") && flag == "--bind-current-owner-self" =>
            (c, m, Some((h, op, None)), None),
        [c, m, h, op, head, journal, flag] if c == "reconcile-source-overlap" && flag == "--bind-current-owner-self" =>
            (c, m, Some((h, op, None)), Some((
                CommitId::parse(head).map_err(|_| "fresh.expected_head")?,
                Sha256Hex::parse(journal).ok_or("fresh.expected_journal")?,
            ))),
        [c, m, h, root, export, flag]
            if (c == "resume-restored" || c == "recover-restored")
                && flag == "--bind-current-owner-self" =>
            (c, m, Some((h, root, Some(export))), None),
        _ => return Err("usage: enouia-memory-fresh plan <manifest> | run|recover <manifest> <owner-home> <bootstrap-operation> --bind-current-owner-self | resume-restored|recover-restored <manifest> <owner-home> <restored-root> <activation-export> --bind-current-owner-self | reconcile-source-overlap <manifest> <owner-home> <bootstrap-operation> <expected-head> <expected-journal-sha256> --bind-current-owner-self".into()),
    };
    let input = load_fresh(Path::new(manifest))?;
    let ids = Arc::new(OsIdSource);
    let Some((home, operation_or_root, export)) = owner else {
        // Placeholder identities have the same serialized widths as provisioned
        // IDs. They are never enrolled and confer no authority.
        let descriptor = AutomationDescriptor {
            schema_version: NextVersion,
            kind: DescriptorKind::AutomationVault,
            layout: AutomationLayout::SegmentedOwnerV1,
            vault_id: VaultId::from_random(ids.random_16()),
            host_id: DeviceId::from_random(ids.random_16()),
            created_by: ActorRef {
                actor_id: PrincipalId::from_random(ids.random_16()),
                actor_type: ActorType::Owner,
            },
            system_actor: ActorRef {
                actor_id: PrincipalId::from_random(ids.random_16()),
                actor_type: ActorType::System,
            },
            owner_subject_id: Some(SubjectId::from_random(ids.random_16())),
        };
        let plans = input.plans(&descriptor, &*ids)?;
        let capacity = fresh_journal_capacity(&plans, &descriptor, &input.run_hash)
            .map_err(|e| format!("fresh.preflight: {e:?}"))?;
        return Ok(
            json!({"command":command,"run_hash":input.run_hash,"intake":input.intake,
            "preflight":plans.iter().map(|p| &p.preflight).collect::<Vec<_>>(),
            "journal":capacity,"ready_to_run":capacity.fits,"writes":0}),
        );
    };
    let restored = export.is_some();
    let (root, descriptor) = if let Some(export) = export {
        restored_owner_root(
            Path::new(home),
            Path::new(operation_or_root),
            Path::new(export),
        )
    } else {
        let operation =
            OperationId::parse(operation_or_root).map_err(|_| "fresh.bootstrap_operation")?;
        registered_owner_root(Path::new(home), &operation)
    }
    .map_err(|e| format!("fresh.bootstrap: {e:?}"))?;
    // Full preflight is complete before adoption, recovery or archive writes.
    let plans = input.plans(&descriptor, &*ids)?;
    if let Some((head, journal)) = reconcile {
        let mut init =
            Initializer::open_synthetic(&root, descriptor.host_id, Arc::new(SystemClock), ids)
                .map_err(|e| format!("fresh.initializer: {e:?}"))?;
        let capacity = init
            .reconcile_fresh_source_overlap(&input.run_hash, &plans, &head, &journal)
            .map_err(|e| format!("fresh.reconcile_source_overlap: {e:?}"))?;
        return Ok(
            json!({"command":command,"run_hash":input.run_hash,"journal":capacity,
            "published_memories":0,"retained_receipts":init.plans()[0].completed()}),
        );
    }
    let capacity = fresh_journal_capacity(&plans, &descriptor, &input.run_hash)
        .map_err(|e| format!("fresh.preflight: {e:?}"))?;
    if !capacity.fits {
        return Err("fresh.journal_capacity".into());
    }
    let enrolled = Initializer::check_fresh_run(&root, &input.run_hash)
        .map_err(|e| format!("fresh.run_binding: {e:?}"))?;
    if restored && !enrolled {
        return Err("fresh.restored_run_not_enrolled".into());
    }
    let clock = Arc::new(SystemClock);
    if !enrolled
        && (clock.now_unix_ms() < input.manifest.formed_at.unix_ms()
            || clock.now_unix_ms() >= input.manifest.formation_until.unix_ms())
    {
        return Err("fresh.formation_interval_inactive".into());
    }
    if command == "recover" || command == "recover-restored" {
        // Explicit forward recovery only; opening without it never repairs.
        drop(
            SegmentedAutomationVault::recover(
                &root,
                descriptor.host_id.clone(),
                &descriptor.system_actor,
                clock.clone(),
                ids.clone(),
                Faults::none(),
            )
            .map_err(|e| format!("fresh.recover: {e:?}"))?,
        );
    }
    let mut init = if restored {
        Initializer::open_synthetic(&root, descriptor.host_id.clone(), clock, ids)
    } else {
        match Initializer::adopt_bootstrapped(
            &root,
            descriptor.host_id.clone(),
            clock.clone(),
            ids.clone(),
        ) {
            Err(AutomationFault::Conflict) => {
                Initializer::open_synthetic(&root, descriptor.host_id.clone(), clock, ids)
            }
            result => result,
        }
    }
    .map_err(|e| format!("fresh.initializer: {e:?}"))?;
    let preparations = input
        .batches
        .iter()
        .map(|b| b.preparation.clone())
        .collect::<Vec<_>>();
    init.enqueue_fresh(input.run_hash.clone(), plans, &preparations)
        .map_err(|e| format!("fresh.enroll: {e:?}"))?;
    while init.step().map_err(|e| {
        format!("fresh.step: {e:?}; retain original run and bindings, no automatic resend")
    })? {}
    Ok(
        json!({"command":command,"run_hash":input.run_hash,"intake":input.intake,"journal":capacity,
        "ledger":init.ledger().map_err(|e|format!("fresh.ledger: {e:?}"))?}),
    )
}
fn main() {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
        Err(error) => {
            eprintln!("enouia-memory-fresh: {error}");
            std::process::exit(1);
        }
    }
}
