use super::*;
use enouia_memory_contract::{common::SourceRevisionRef, json::Knowable};
use std::collections::{BTreeMap, BTreeSet};
impl State {
    pub(super) fn check_catalog(
        &self,
        incoming: &[CapturedSource],
        catalog: &RecordCatalog,
        now: &Timestamp,
    ) -> Result<()> {
        let source = |r: &SourceRevisionRef| match incoming
            .iter()
            .find(|s| s.record.source_id == r.source_id)
        {
            Some(s) => (s.reference() == *r).then_some(s),
            None => self.source(r),
        };
        let quote = |q: &QuotedEvidence, sensitivity: Sensitivity, access: &PolicyId| {
            source(&SourceRevisionRef {
                source_id: q.reference.source_id.clone(),
                source_revision: q.reference.source_revision,
            })
            .is_some_and(|s| {
                !self.deleted_sources.contains(&s.record.source_id)
                    && s.record.sensitivity <= sensitivity
                    && &s.record.access_policy_id == access
                    && q.resolves(std::slice::from_ref(s))
                    && self.policies.values().any(|p| {
                        p.active
                            && now >= &p.valid_from
                            && now < &p.valid_until
                            && p.source_ids.contains(&s.record.source_id)
                            && p.access_policy_id == s.record.access_policy_id
                            && s.record.sensitivity <= p.max_sensitivity
                    })
            })
        };
        let mut ids = BTreeSet::new();
        for e in &catalog.entities {
            if !ids.insert(&e.entity_id)
                || enouia_memory_contract::ids::check_prefixed_uuid(&e.entity_id, "ent").is_err()
                || e.revision.get()
                    != self
                        .entities
                        .get(&e.entity_id)
                        .and_then(|v| v.last())
                        .map_or(1, |v| v.revision.get() + 1)
                || e.display_name.trim().is_empty()
                || e.display_name.chars().count() > 128
                || e.display_name.chars().any(char::is_control)
                || e.aliases.len() > 32
                || e.evidence.is_empty()
                || e.evidence.len() > 8
                || (e.entity_type == EntityType::Project) != e.project_id.is_some()
                || e.first_seen
                    .as_ref()
                    .zip(e.last_seen.as_ref())
                    .is_some_and(|(a, b)| a > b)
                || !e
                    .evidence
                    .iter()
                    .all(|q| quote(q, e.sensitivity, &e.access_policy_id))
                || !e.aliases.iter().all(|a| {
                    !a.name.trim().is_empty()
                        && a.name.chars().count() <= 128
                        && !a.name.chars().any(char::is_control)
                        && !a.evidence.is_empty()
                        && a.evidence.len() <= 8
                        && a.evidence
                            .iter()
                            .all(|q| quote(q, e.sensitivity, &e.access_policy_id))
                })
            {
                return Err(AutomationFault::Denied);
            }
        }
        let entity = |id: &str| {
            catalog
                .entities
                .iter()
                .find(|e| e.entity_id == id)
                .or_else(|| self.entities.get(id).and_then(|v| v.last()))
        };
        let mut conversations = BTreeSet::new();
        for c in &catalog.conversations {
            let text = |s: &str, n: usize| {
                !s.trim().is_empty() && s.chars().count() <= n && !s.chars().any(char::is_control)
            };
            if !conversations.insert(&c.conversation_id)
                || c.revision.get()
                    != self
                        .conversations
                        .get(&c.conversation_id)
                        .and_then(|v| v.last())
                        .map_or(1, |v| v.revision.get() + 1)
                || !text(&c.platform, 64)
                || !enouia_memory_contract::source::is_account_alias(&c.account_scope)
                || c.title.as_ref().is_some_and(|s| !text(s, 256))
                || c.original_conversation_id
                    .as_ref()
                    .is_some_and(|s| !text(s, 256))
                || !c.started_at.valid()
                || !c.ended_at.valid()
                || c.started_at
                    .from
                    .as_ref()
                    .zip(c.ended_at.from.as_ref())
                    .is_some_and(|(a, b)| a > b)
                || c.participants.is_empty()
                || c.participants.len() > 32
                || c.participants.iter().any(|p| {
                    !text(&p.label, 128)
                        || p.entity_id.as_ref().is_some_and(|id| {
                            entity(id).is_none_or(|e| {
                                e.sensitivity > c.sensitivity
                                    || e.access_policy_id != c.access_policy_id
                            })
                        })
                })
                || c.branches.is_empty()
                || c.branches.len() > 32
                || c.branches.iter().collect::<BTreeSet<_>>().len() != c.branches.len()
                || c.turns.len() > 128
                || c.turns
                    .iter()
                    .map(|t| &t.source)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != c.turns.len()
                || c.turns
                    .iter()
                    .map(|t| (&t.branch_id, t.sequence))
                    .collect::<BTreeSet<_>>()
                    .len()
                    != c.turns.len()
                || c.turns
                    .iter()
                    .map(|t| &t.turn_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != c.turn_count as usize
                || c.turns.iter().any(|t| {
                    t.sequence == 0
                    || !c.branches.contains(&t.branch_id)
                    || t.parent_source.as_ref().is_some_and(|p| {
                        !c.turns
                            .iter()
                            .any(|a| &a.source == p && a.sequence < t.sequence)
                    }) || source(&t.source).is_none_or(|s| {
                    s.record.sensitivity > c.sensitivity
                        || s.record.access_policy_id != c.access_policy_id
                        || s.record_fields
                            .as_ref()
                            .and_then(|f| f.conversation_id.as_ref())
                            != Some(&c.conversation_id)
                        || s.record
                            .original_conversation_id
                            .as_ref()
                            .is_some_and(|id| Some(id) != c.original_conversation_id.as_ref())
                        || matches!(&s.record.branch_id,Knowable::Known(Some(b)) if b!=&t.branch_id)
                })
                })
            {
                return Err(AutomationFault::Denied);
            }
        }
        for s in incoming {
            if let Some(id) = s
                .record_fields
                .as_ref()
                .and_then(|f| f.conversation_id.as_ref())
            {
                let c = catalog
                    .conversations
                    .iter()
                    .find(|c| &c.conversation_id == id)
                    .or_else(|| self.conversations.get(id).and_then(|v| v.last()))
                    .ok_or(AutomationFault::Denied)?;
                if !c.turns.iter().any(|t| t.source == s.reference()) {
                    return Err(AutomationFault::Denied);
                }
            }
        }
        Ok(())
    }
    pub(super) fn authorized<'a>(
        &'a self,
        descriptor: &'a AutomationDescriptor,
        max: Sensitivity,
        now: Timestamp,
    ) -> Result<AuthorizedSegmentedRead<'a>> {
        if self.head.as_ref().is_none_or(|h| now < h.created_at) {
            return Err(AutomationFault::Denied);
        }
        let policies: BTreeMap<_, _> = self
            .policies
            .values()
            .filter(|p| p.active && now >= p.valid_from && now < p.valid_until)
            .map(|p| (p.policy_id.clone(), (p, p.reference())))
            .collect();
        let mut allowed: BTreeMap<SourceId, Vec<&AutomationPolicy>> = BTreeMap::new();
        for (p, _) in policies.values() {
            for id in &p.source_ids {
                allowed.entry(id.clone()).or_default().push(p);
            }
        }
        let sources: BTreeMap<_, _> = self
            .sources
            .iter()
            .filter_map(|(id, v)| {
                let s = v.last()?;
                (!self.deleted_sources.contains(id)
                    && allowed.get(id).is_some_and(|ps| {
                        ps.iter().any(|p| {
                            s.record.sensitivity <= max.min(p.max_sensitivity)
                                && s.record.access_policy_id == p.access_policy_id
                                && s.attributed_subject
                                    .as_ref()
                                    .is_none_or(|subject| subject == &p.subject_id)
                        })
                    }))
                .then_some((id.clone(), s))
            })
            .collect();
        let quoted = |q: &QuotedEvidence| {
            sources
                .get(&q.reference.source_id)
                .is_some_and(|s| s.record.revision == q.reference.source_revision)
        };
        let entities: BTreeMap<_, _> = self
            .entities
            .iter()
            .filter_map(|(id, v)| {
                let e = v.last()?;
                (e.sensitivity <= max
                    && e.evidence.iter().all(&quoted)
                    && e.aliases.iter().all(|a| a.evidence.iter().all(&quoted)))
                .then_some((id.clone(), e))
            })
            .collect();
        let mut memo = BTreeMap::new();
        struct Scope<'s> {
            state: &'s State,
            descriptor: &'s AutomationDescriptor,
            max: Sensitivity,
            policies: &'s BTreeMap<PolicyId, (&'s AutomationPolicy, PolicyRef)>,
            sources: &'s BTreeMap<SourceId, &'s CapturedSource>,
            entities: &'s BTreeMap<String, &'s EntityRecord>,
        }
        fn eligible(
            id: &MemoryId,
            scope: &Scope<'_>,
            memo: &mut BTreeMap<MemoryId, bool>,
            depth: usize,
        ) -> bool {
            let state = scope.state;
            let descriptor = scope.descriptor;
            let max = scope.max;
            let policies = scope.policies;
            let sources = scope.sources;
            let entities = scope.entities;
            if let Some(v) = memo.get(id) {
                return *v;
            }
            // An in-progress node fails closed on a malformed dependency cycle.
            memo.insert(id.clone(), false);
            let Some(entry) = state.memories.get(id).and_then(|v| v.last()) else {
                return false;
            };
            let m = &entry.memory;
            let i = &m.decision.input;
            let Some((p, reference)) = policies.get(&i.policy.policy_id) else {
                return false;
            };
            let valid = depth < 128
                && !state.deleted_memories.contains(id)
                && &i.policy == reference
                && i.actor == descriptor.system_actor
                && i.subject_id == p.subject_id
                && i.project_id == p.project_id
                && i.access_policy_id == p.access_policy_id
                && i.egress_policy_id == p.egress_policy_id
                && i.sensitivity <= max.min(p.max_sensitivity)
                && i.input_refs.iter().all(|r| {
                    p.source_ids.contains(&r.source_id)
                        && sources.get(&r.source_id).is_some_and(|s| {
                            s.record.revision == r.source_revision
                                && s.record.sensitivity <= i.sensitivity
                                && s.record.access_policy_id == p.access_policy_id
                        })
                })
                && i.record_fields.as_ref().is_some_and(|f| {
                    f.entities.iter().all(|e| {
                        entities.contains_key(&e.entity_id)
                            && state.entity_in_policy(&e.entity_id, p, i.sensitivity.min(max))
                    })
                })
                && i.dimensions.as_ref().is_some_and(|d| {
                    d.derived_from.iter().all(|r| {
                        state
                            .memories
                            .get(&r.memory.memory_id)
                            .and_then(|v| v.last())
                            .is_some_and(|p| p.target == *r)
                            && eligible(&r.memory.memory_id, scope, memo, depth + 1)
                    })
                });
            memo.insert(id.clone(), valid);
            valid
        }
        let scope = Scope {
            state: self,
            descriptor,
            max,
            policies: &policies,
            sources: &sources,
            entities: &entities,
        };
        let memories = self
            .memories
            .iter()
            .filter(|(id, _)| eligible(id, &scope, &mut memo, 0))
            .map(|(_, v)| v.last().unwrap())
            .collect();
        Ok(AuthorizedSegmentedRead {
            withdrawal_watermark: Revision::new(1).unwrap(),
            snapshot: self.snapshot()?,
            now,
            memories,
            conversations: self
                .conversations
                .iter()
                .filter_map(|(id, versions)| {
                    let c = versions.last()?;
                    (!c.turns.is_empty()
                        && c.sensitivity <= max
                        && c.turns.iter().all(|t| {
                            sources
                                .get(&t.source.source_id)
                                .is_some_and(|s| s.record.revision == t.source.source_revision)
                        })
                        && c.participants.iter().all(|p| {
                            p.entity_id
                                .as_ref()
                                .is_none_or(|id| entities.contains_key(id))
                        }))
                    .then_some((id.clone(), c))
                })
                .collect(),
            entities,
            sources,
        })
    }
}
