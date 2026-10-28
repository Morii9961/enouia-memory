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
            if self
                .conversations
                .get(&c.conversation_id)
                .is_some_and(|versions| {
                    c.turns.iter().any(|t| {
                        versions.iter().flat_map(|old| &old.turns).any(|old| {
                            old.source.source_id == t.source.source_id && old.turn_id != t.turn_id
                        })
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
        let memories: Vec<&SegmentedMemory> = self
            .memories
            .iter()
            .filter(|(id, _)| eligible(id, &scope, &mut memo, 0))
            .map(|(_, v)| v.last().unwrap())
            .collect();
        let conversations: BTreeMap<_, _> = self
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
            .collect();
        let mut turns: BTreeMap<SessionId, BTreeMap<SourceId, AuthorizedConversationTurn>> =
            BTreeMap::new();
        for (id, versions) in &self.conversations {
            if !conversations.contains_key(id) {
                continue;
            }
            for c in versions {
                // A public source does not declassify turn/branch metadata recorded
                // in a more sensitive conversation fragment.
                if c.sensitivity > max || c.access_policy_id != conversations[id].access_policy_id {
                    continue;
                }
                for t in &c.turns {
                    if sources.get(&t.source.source_id).is_some_and(|s| {
                        s.record.revision == t.source.source_revision
                            && s.record_fields
                                .as_ref()
                                .and_then(|f| f.conversation_id.as_ref())
                                == Some(id)
                    }) {
                        let mut projected = t.clone();
                        projected.parent_source = projected.parent_source.filter(|p| {
                            sources
                                .get(&p.source_id)
                                .is_some_and(|s| s.record.revision == p.source_revision)
                        });
                        turns.entry(id.clone()).or_default().insert(
                            t.source.source_id.clone(),
                            AuthorizedConversationTurn {
                                turn: projected,
                                effective_sensitivity: c
                                    .sensitivity
                                    .max(sources[&t.source.source_id].record.sensitivity),
                            },
                        );
                    }
                }
            }
        }
        // Densely rank only authorized captured turns; original ordinals must not
        // reveal withheld fragment sizes or leave visible holes.
        for group in turns.values_mut() {
            let visible: BTreeSet<_> = group.values().map(|v| v.turn.source.clone()).collect();
            for value in group.values_mut() {
                value.turn.parent_source = value
                    .turn
                    .parent_source
                    .take()
                    .filter(|p| visible.contains(p));
            }
            let mut ordered: Vec<_> = group
                .values()
                .map(|v| {
                    (
                        v.turn.branch_id.clone(),
                        v.turn.sequence,
                        v.turn.turn_id.clone(),
                    )
                })
                .collect();
            ordered.sort();
            let mut ranks = BTreeMap::new();
            let mut totals: BTreeMap<BranchId, u32> = BTreeMap::new();
            for (branch, _, turn) in ordered {
                if !ranks.contains_key(&(branch.clone(), turn.clone())) {
                    let next = totals.entry(branch.clone()).or_default();
                    *next += 1;
                    ranks.insert((branch, turn), *next);
                }
            }
            for value in group.values_mut() {
                value.turn.sequence =
                    ranks[&(value.turn.branch_id.clone(), value.turn.turn_id.clone())];
            }
        }
        let targets: BTreeMap<_, _> = memories
            .iter()
            .map(|m| (&m.memory.memory.memory_id, *m))
            .collect();
        let mut owner_actions: Vec<_> = self
            .owner_actions
            .values()
            .filter(|(r, e)| {
                e.sensitivity <= max
                    && match &r.plan.action {
                        OwnerAction::Correct { .. } => e.correction.as_ref().is_some_and(|c| {
                            targets.contains_key(&c.decision.input.result.memory_id)
                                && self.policy(&c.decision.input).is_some_and(|p| {
                                    p.active
                                        && now >= p.valid_from
                                        && now < p.valid_until
                                        && self.source_in_policy(
                                            &c.source.reference(),
                                            p,
                                            c.decision.input.sensitivity,
                                        )
                                })
                        }),
                        OwnerAction::Attention { target, .. } => {
                            targets.contains_key(&target.memory.memory_id)
                        }
                        OwnerAction::ThemeRename { theme, .. }
                        | OwnerAction::ThemePin { theme, .. }
                        | OwnerAction::ThemeDissolve { theme, .. } => {
                            targets.contains_key(&theme.memory.memory_id)
                        }
                        OwnerAction::Tag {
                            targets: tagged, ..
                        } => tagged
                            .iter()
                            .any(|t| targets.contains_key(&t.memory.memory_id)),
                        OwnerAction::EntityMerge {
                            from_entity,
                            into_entity,
                            ..
                        }
                        | OwnerAction::EntitySplit {
                            from_entity,
                            into_entity,
                            ..
                        } => {
                            entities.contains_key(from_entity) && entities.contains_key(into_entity)
                        }
                        OwnerAction::Withdraw { .. } | OwnerAction::Forget { .. } => false,
                    }
            })
            .map(|(record, effects)| AuthorizedOwnerAction { record, effects })
            .collect();
        owner_actions.sort_by_key(|a| a.record.sequence);
        let mut entity_redirects: BTreeMap<(MemoryId, String), String> = BTreeMap::new();
        let mut entity_proofs = BTreeMap::new();
        let mut entity_aliases: BTreeMap<(MemoryId, String), BTreeSet<String>> = BTreeMap::new();
        let identity_actions = owner_actions
            .iter()
            .filter(|a| {
                matches!(
                    a.record.plan.action,
                    OwnerAction::EntityMerge { .. } | OwnerAction::EntitySplit { .. }
                )
            })
            .count();
        if identity_actions.saturating_mul(memories.len()) > 16_777_216 {
            return Err(AutomationFault::Capacity);
        }

        for action in &owner_actions {
            let (from, into, selected) = match &action.record.plan.action {
                OwnerAction::EntityMerge {
                    from_entity,
                    into_entity,
                    ..
                } => (from_entity, into_entity, None),
                OwnerAction::EntitySplit {
                    from_entity,
                    into_entity,
                    evidence,
                    ..
                } => (from_entity, into_entity, Some(evidence)),
                _ => continue,
            };
            for m in &memories {
                let i = &m.memory.decision.input;
                if i.sensitivity > action.effects.max_sensitivity {
                    continue;
                }
                let Some(policy) = self.policy(i) else {
                    continue;
                };
                if !self.entity_in_policy(into, policy, i.sensitivity) {
                    continue;
                }
                for mention in &i.record_fields.as_ref().unwrap().entities {
                    let key = (m.memory.memory.memory_id.clone(), mention.entity_id.clone());
                    let resolved = entity_redirects.get(&key).unwrap_or(&mention.entity_id);
                    if resolved != from {
                        continue;
                    }
                    if selected.is_some_and(|evidence| {
                        !evidence.iter().any(|q| {
                            mention.evidence.iter().any(|m| {
                                q.reference.source_id == m.reference.source_id
                                    && q.reference.source_revision == m.reference.source_revision
                                    && q.start < m.end
                                    && m.start < q.end
                            })
                        })
                    }) {
                        continue;
                    }
                    entity_redirects.insert(key.clone(), into.clone());
                    entity_proofs.insert(key, OwnerActionProof::from_record(action.record));
                }
                if selected.is_none() && self.entity_in_policy(from, policy, i.sensitivity) {
                    let reaches_into = i.record_fields.as_ref().unwrap().entities.iter().any(|e| {
                        entity_redirects
                            .get(&(m.memory.memory.memory_id.clone(), e.entity_id.clone()))
                            .unwrap_or(&e.entity_id)
                            == into
                    });
                    if reaches_into {
                        let old = (m.memory.memory.memory_id.clone(), from.clone());
                        let mut names = entity_aliases.get(&old).cloned().unwrap_or_default();
                        if let Some(e) = entities.get(from) {
                            names.insert(e.display_name.clone());
                            names.extend(e.aliases.iter().map(|a| a.name.clone()));
                        }
                        entity_aliases
                            .entry((m.memory.memory.memory_id.clone(), into.clone()))
                            .or_default()
                            .extend(names);
                    }
                }
            }
        }
        Ok(AuthorizedSegmentedRead {
            owner_actions,
            entity_redirects,
            entity_proofs,
            entity_aliases,
            withdrawal_watermark: Revision::new(1 + self.withdrawal_watermark).unwrap(),
            snapshot: self.snapshot()?,
            now,
            memories,
            conversations,
            turns,
            entities,
            sources,
        })
    }
}
