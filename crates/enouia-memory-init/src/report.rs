//! Rebuildable seven-projection bundle and measured coverage. The native reader
//! authorizes before every projection; a saved bundle is never read authority.
use crate::pipeline::Initializer;
use enouia_memory_contract::{common::Sensitivity, next::*};
use enouia_memory_host::automation::catalog::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl Initializer {
    pub fn projection_bundle(
        &self,
        profile: &AttentionProfile,
        timezone: &str,
        from: &str,
        until: &str,
    ) -> Result<Value, String> {
        let reader = SegmentedMemoryReader::from_authenticated_system(
            self.vault.clone(),
            self.vault.descriptor().system_actor.clone(),
        )
        .map_err(|e| format!("{e:?}"))?;
        let filter = CatalogFilter::default();
        let request = CatalogReadRequest {
            profile,
            filter: &filter,
            options: CatalogReadOptions {
                limit: 100,
                max_bytes: MAX_DOCUMENT_BYTES,
                max_sensitivity: Sensitivity::Private,
                include_facets: true,
            },
        };
        let mut pages = Vec::new();
        let mut cursor = None;
        loop {
            let page = reader
                .memory_page_native(&request, cursor.as_ref())
                .map_err(|e| format!("{e:?}"))?;
            cursor = page.next.clone();
            pages.push(page);
            if cursor.is_none() {
                break;
            }
        }
        let selection = pages[0].selection.clone();
        let items: Vec<_> = pages.iter().flat_map(|p| &p.items).collect();
        let mut time = Vec::new();
        for axis in [
            CatalogTimeAxis::Event,
            CatalogTimeAxis::Learned,
            CatalogTimeAxis::SourceCaptured,
        ] {
            for granularity in [BucketGranularity::Day, BucketGranularity::Month] {
                time.push(
                    reader
                        .memory_time_buckets_native(
                            &request,
                            &TimeBucketQuery {
                                axis,
                                timezone: timezone.into(),
                                granularity,
                                from_date: from.into(),
                                until_date: until.into(),
                            },
                            Some(&selection),
                        )
                        .map_err(|e| format!("{e:?}"))?,
                );
            }
        }
        let stats = reader
            .memory_stats_native(&request, Some(&selection))
            .map_err(|e| format!("{e:?}"))?;
        let mut graph = Vec::new();
        let mut gc = None;
        loop {
            let page = reader
                .memory_graph_native(
                    &request,
                    &GraphQuery {
                        node_limit: 5000,
                        edge_limit_per_kind: 20000,
                    },
                    gc.as_ref(),
                )
                .map_err(|e| format!("{e:?}"))?;
            gc = page.next.clone();
            graph.push(page);
            if gc.is_none() {
                break;
            }
        }
        let mut themes = Vec::new();
        for item in &items {
            if item.base.dimensions.kind == FormationKind::Theme {
                let mut tc = None;
                loop {
                    let page = reader
                        .memory_theme_members_native(
                            &request,
                            &item.base.memory.memory_id,
                            tc.as_ref(),
                        )
                        .map_err(|e| format!("{e:?}"))?;
                    tc = page.next.clone();
                    themes.push(page);
                    if tc.is_none() {
                        break;
                    }
                }
            }
        }
        let mut activity = Vec::new();
        let mut ac = None;
        loop {
            let page = reader
                .memory_activity_feed_native(
                    &request,
                    &ActivityQuery {
                        memory: None,
                        event_kind: None,
                        include_sources: true,
                    },
                    ac.as_ref(),
                )
                .map_err(|e| format!("{e:?}"))?;
            ac = page.next.clone();
            activity.push(page);
            if ac.is_none() {
                break;
            }
        }
        let (
            evidence_total,
            exact_quotes,
            conversation_total,
            conversation_resolved,
            entity_count,
            same_name,
            source_refs,
            memory_refs,
        ) =
            self.vault
                .read(
                    &self.vault.descriptor().system_actor,
                    Sensitivity::Private,
                    |data| {
                        let mut total = 0;
                        let mut exact = 0;
                        let mut ct = 0;
                        let mut cr = 0;
                        let sources: Vec<_> = data.sources.values().map(|s| (*s).clone()).collect();
                        for m in &data.memories {
                            let f = m.memory.decision.input.record_fields.as_ref().unwrap();
                            for q in &f.evidence {
                                total += 1;
                                if q.resolves(&sources) {
                                    exact += 1;
                                }
                                if let Some(source) = data.sources.get(&q.reference.source_id)
                                    && let Some(id) = source
                                        .record_fields
                                        .as_ref()
                                        .and_then(|f| f.conversation_id.as_ref())
                                {
                                    ct += 1;
                                    if data.turns.get(id).is_some_and(|turns| {
                                        turns.contains_key(&q.reference.source_id)
                                    }) {
                                        cr += 1;
                                    }
                                }
                            }
                        }
                        let mut names = BTreeMap::<String, usize>::new();
                        for e in data.entities.values() {
                            *names.entry(e.display_name.clone()).or_default() += 1;
                        }
                        (
                            total,
                            exact,
                            ct,
                            cr,
                            data.entities.len(),
                            names.values().filter(|n| **n > 1).sum::<usize>(),
                            sources.iter().map(|s| s.reference()).collect::<Vec<_>>(),
                            data.memories
                                .iter()
                                .map(|m| m.target.clone())
                                .collect::<Vec<_>>(),
                        )
                    },
                )
                .map_err(|e| format!("{e:?}"))?;
        if self.vault.snapshot().map_err(|e| format!("{e:?}"))? != selection.snapshot {
            return Err("snapshot_changed".into());
        }
        let envelope = |algorithm: &str| ProjectionMetadata {
            schema_version: NextVersion,
            kind: ProjectionKind::ProjectionMetadata,
            algorithm: algorithm.into(),
            version: enouia_memory_contract::json::Revision::new(1).unwrap(),
            snapshot: selection.snapshot.clone(),
            withdrawal_watermark: selection.withdrawal_watermark,
            scope_hash: selection.scope_hash.clone(),
            coverage: ProjectionCoverage {
                scope: "authorized_current_machine_revisions".into(),
                authorized_inputs: items.len() as u32,
                included_inputs: items.len() as u32,
                complete_in_scope: true,
                limits: vec![],
            },
            source_dependencies: source_refs.clone(),
            memory_dependencies: memory_refs.clone(),
        };
        if !envelope("initialization-bundle-1").validate().is_empty() {
            return Err("projection_dependency_capacity".into());
        }
        let search=items.iter().map(|i|json!({"memory":i.base.memory,"title":i.title,"text":i.base.content,"entities":i.entities})).collect::<Vec<_>>();
        let attention=items.iter().map(|i|json!({"memory":i.base.memory,"attention":i.base.attention,"source_captured_at":i.base.source_captured_at})).collect::<Vec<_>>();
        let mut precision = BTreeMap::<String, usize>::new();
        for i in &items {
            *precision
                .entry(
                    serde_json::to_value(i.event_time.precision)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .into(),
                )
                .or_default() += 1;
        }
        let total = items.len();
        let fraction = |n: usize, d: usize| json!({"numerator":n,"denominator":d,"percent":if d==0{None}else{Some(n as f64*100.0/d as f64)}});
        let source_statement_counts =
            items
                .iter()
                .flat_map(|i| &i.base.evidence)
                .fold(BTreeMap::new(), |mut a, q| {
                    *a.entry(q.source_id.clone()).or_insert(0usize) += 1;
                    a
                });
        let multi_sources = source_statement_counts.values().filter(|n| **n > 1).count();
        let ids: BTreeSet<_> = items
            .iter()
            .map(|i| i.base.memory.memory_id.clone())
            .collect();
        Ok(
            json!({"effect":"synthetic_native_read","selection":selection,"pages":pages,
            "coverage":{
                "title":fraction(items.iter().filter(|i|!i.title.is_empty()).count(),total),
                "time_fields":fraction(total,total),"precision_distribution":precision,"unknown_time":fraction(items.iter().filter(|i|i.event_time.precision==RecordPrecision::Unknown).count(),total),
                "entities":fraction(items.iter().filter(|i|!i.entities.is_empty()).count(),total),"entity_count":entity_count,"same_name_unmerged_entities":same_name,
                "evidence":fraction(exact_quotes,evidence_total),"conversation_evidence":fraction(conversation_resolved,conversation_total),
                "language_field":fraction(items.iter().filter(|i|!i.content_lang.is_empty()).count(),total),"language_known":fraction(items.iter().filter(|i|i.content_lang!="und").count(),total),
                "one_statement":{"multi_statement_sources":multi_sources,"semantic_audit":null,"scripted_only":true},
                "projection_kinds_built":7,"runtime_screenshots":null,"runtime_adoption":"not_run", "visible_memory_ids":ids},
            "projections":{
                "search":{"metadata":envelope("literal-alias-index-1"),"items":search},
                "themes":{"metadata":envelope("recorded-theme-family-1"),"pages":themes},
                "edges":{"metadata":envelope("typed-authorized-graph-1"),"pages":graph},
                "time":{"metadata":envelope("iana-overlap-buckets-1"),"pages":time},
                "attention":{"metadata":envelope("attention-rank-1"),"items":attention},
                "statistics":{"metadata":envelope("authorized-facets-1"),"result":stats},
                "activity":{"metadata":envelope("witnessed-native-activity-1"),"pages":activity}}}),
        )
    }
}
