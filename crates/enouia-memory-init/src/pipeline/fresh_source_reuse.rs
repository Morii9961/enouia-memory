use super::*;
use std::collections::BTreeSet;

/// Keep the first captured identity for each exact import locator. Leave a
/// prefix of already published plans intact when reconciling queued plans.
pub(crate) fn reuse_fresh_sources(plans: &mut [Plan], frozen: usize) -> Result<()> {
    let Some(first) = plans.first() else {
        return Ok(());
    };
    let access = first.policy.access_policy_id.clone();
    let mut captured = BTreeMap::<_, CapturedSource>::new();
    for (index, plan) in plans.iter_mut().enumerate() {
        let mut reused = BTreeMap::new();
        let mut keys = Vec::with_capacity(plan.sources.len());
        let mut unique = BTreeSet::new();
        for source in &plan.sources {
            let record = &source.record;
            let key = digest(&(
                &record.provider,
                &record.account_scope,
                &record.original_conversation_id,
                &record.original_message_id,
                &record.raw_object_hash,
                &record.content_hash,
                &record.locator,
            ));
            if !unique.insert(key.clone()) {
                return Err(AutomationFault::Conflict);
            }
            keys.push(key.clone());
            if let Some(previous) = captured.get(&key) {
                if index < frozen
                    || previous.record.sensitivity != record.sensitivity
                    || previous.record.speaker_role != record.speaker_role
                    || previous.record.evidence_class != record.evidence_class
                    || previous.record.completeness != record.completeness
                    || previous.record.extensions != record.extensions
                    || previous.record.occurred_at != record.occurred_at
                    || previous.attributed_subject != source.attributed_subject
                    || previous.record.access_policy_id != access
                {
                    return Err(AutomationFault::Conflict);
                }
                reused.insert(record.source_id.clone(), previous.clone());
            }
        }
        if index < frozen {
            captured.extend(keys.into_iter().zip(plan.sources.iter().cloned()));
            continue;
        }
        if plan.captured
            || plan.work.iter().any(|w| {
                w.state != "queued"
                    || w.request.is_some()
                    || w.response.is_some()
                    || w.binding.is_some()
                    || w.receipt.is_some()
            })
        {
            return Err(AutomationFault::Conflict);
        }
        let remap = |id: &mut SourceId| {
            if let Some(source) = reused.get(id) {
                *id = source.record.source_id.clone();
            }
        };
        let remap_evidence = |e: &mut EvidenceRef| remap(&mut e.source_id);
        let remap_quote = |q: &mut QuotedEvidence| remap_evidence(&mut q.reference);
        for source in &mut plan.sources {
            if let Some(previous) = reused.get(&source.record.source_id) {
                *source = previous.clone();
            } else {
                source.record.access_policy_id = access.clone();
                if let Knowable::Known(parents) = &mut source.record.parent_source_ids {
                    parents.iter_mut().for_each(remap);
                }
            }
        }
        for (key, source) in keys.into_iter().zip(&plan.sources) {
            captured.entry(key).or_insert_with(|| source.clone());
        }
        for entity in &mut plan.catalog.entities {
            entity.access_policy_id = access.clone();
            entity.evidence.iter_mut().for_each(remap_quote);
            for alias in &mut entity.aliases {
                alias.evidence.iter_mut().for_each(remap_quote);
            }
        }
        plan.catalog.conversations.retain_mut(|conversation| {
            conversation.access_policy_id = access.clone();
            conversation
                .turns
                .retain(|t| !reused.contains_key(&t.source.source_id));
            for turn in &mut conversation.turns {
                // The cross-fragment parent remains on SourceRecord and in Raw.
                // A catalog turn may only reference a parent in its fragment.
                if turn
                    .parent_source
                    .as_ref()
                    .is_some_and(|p| reused.contains_key(&p.source_id))
                {
                    turn.parent_source = None;
                }
            }
            conversation.turn_count = conversation.turns.len() as u32;
            if !reused.is_empty() {
                let branches: BTreeSet<_> =
                    conversation.turns.iter().map(|t| &t.branch_id).collect();
                conversation.branches.retain(|id| branches.contains(id));
            }
            !conversation.turns.is_empty()
        });
        if plan.sources.len() == reused.len() && !plan.catalog.entities.is_empty() {
            // The existing native Capture delta requires at least one new source.
            return Err(AutomationFault::Denied);
        }
        plan.policy.access_policy_id = access.clone();
        plan.policy.source_ids.iter_mut().for_each(remap);
        let policy_ref = plan.policy.reference();
        for work in &mut plan.work {
            let seed = &mut work.seed;
            seed.access_policy_id = access.clone();
            seed.policy = policy_ref.clone();
            seed.evidence.iter_mut().for_each(remap_evidence);
            for reference in seed
                .input_refs
                .iter_mut()
                .chain(&mut seed.context_source_refs)
            {
                remap(&mut reference.source_id);
            }
            for group in &mut seed.support_origin_groups {
                for reference in group.iter_mut() {
                    remap(&mut reference.source_id);
                }
                group.sort();
            }
            seed.support_origin_groups.sort();
            if let Some(fields) = &mut seed.record_fields {
                fields.evidence.iter_mut().for_each(remap_quote);
                for entity in &mut fields.entities {
                    entity.evidence.iter_mut().for_each(remap_quote);
                }
            }
        }
        if !reused.is_empty()
            && let Some(contexts) = plan.analysis_artifact.get_mut("context_evidence")
        {
            for group in contexts.as_array_mut().ok_or(AutomationFault::Denied)? {
                let mut quotes: Vec<QuotedEvidence> =
                    serde_json::from_value(group["quotes"].take())
                        .map_err(|_| AutomationFault::Denied)?;
                quotes.iter_mut().for_each(remap_quote);
                group["quotes"] = json!(quotes);
            }
        }
    }
    Ok(())
}
