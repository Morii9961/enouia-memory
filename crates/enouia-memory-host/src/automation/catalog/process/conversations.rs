use super::*;
fn source_metadata(source: &CapturedSource) -> NativeSourceMetadata {
    let s = &source.record;
    NativeSourceMetadata {
        reference: source.reference(),
        source_kind: s.source_kind,
        locator: s.locator.clone(),
        speaker: s.speaker_role,
        author_label: s.author_label.clone(),
        evidence_class: s.evidence_class,
        original_conversation_id: s.original_conversation_id.clone(),
        original_message_id: s.original_message_id.clone(),
        original_time: s.original_time.clone(),
        original_timezone: s.original_timezone.clone(),
        occurred_at: s.occurred_at.clone(),
        captured_at: s.captured_at.clone(),
        time_precision: s.time_precision,
        content_hash: s.content_hash.clone(),
        anchor_hash: s.anchor_hash().clone(),
        effective_sensitivity: s.sensitivity,
        extensions: s.extensions.clone(),
        extensions_are_untrusted_data: true,
    }
}
fn conversation_counts(
    data: &AuthorizedSegmentedRead<'_>,
    id: &SessionId,
) -> (u64, u64, Sensitivity) {
    let turns = data.turns.get(id);
    let unique: BTreeSet<_> = turns
        .into_iter()
        .flat_map(|t| t.values())
        .map(|t| &t.turn.turn_id)
        .collect();
    let sensitivity = turns
        .into_iter()
        .flat_map(|t| t.values())
        .fold(data.conversations[id].sensitivity, |s, t| {
            s.max(t.effective_sensitivity)
        });
    (
        unique.len() as u64,
        turns.map_or(0, |t| t.len()) as u64,
        sensitivity,
    )
}
impl SegmentedMemoryReader {
    pub fn memory_source_excerpt_native(
        &self,
        request: &CatalogReadRequest<'_>,
        source: &SourceRevisionRef,
        start: u32,
        limit: u32,
    ) -> ApplicationResult<NativeSourceExcerpt> {
        self.process(request, None, false, |data| {
            if request.filter != &CatalogFilter::default()
                || start > 16_384
                || !(1..=4096).contains(&limit)
            {
                return Err(invalid());
            }
            let h = process_header(
                &data,
                request,
                "selected-source-excerpt-1",
                &(source, start, limit),
                None,
            )?;
            let selected = data
                .current
                .sources
                .get(&source.source_id)
                .filter(|s| s.record.revision == source.source_revision);
            let mut value = NativeSourceExcerpt {
                schema_version: NextVersion,
                kind: "native_source_excerpt".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h,
                source: None,
                source_record: None,
                fields: None,
                excerpt: None,
                start: 0,
                end: 0,
                total_characters: 0,
                complete_in_scope: true,
            };
            if let Some(s) = selected {
                let chars: Vec<_> = s.text.chars().collect();
                if start as usize > chars.len() {
                    return Err(invalid());
                }
                let mut end = chars.len().min(start as usize + limit as usize);
                value.source = Some(s.reference());
                value.source_record = Some(source_metadata(s));
                value.fields = s.record_fields.clone();
                value.start = start;
                value.total_characters = chars.len() as u32;
                loop {
                    value.end = end as u32;
                    value.excerpt = Some(chars[start as usize..end].iter().collect());
                    value.complete_in_scope = end == chars.len();
                    if bytes(&value)? <= request.options.max_bytes {
                        break;
                    }
                    if end == start as usize {
                        return Err(invalid());
                    }
                    end -= ((end - start as usize) / 4).max(1);
                }
                if (start as usize) < chars.len() && end == start as usize {
                    return Err(invalid());
                }
            }
            finish(value, request.options.max_bytes)
        })
    }
    pub fn memory_conversations_native(
        &self,
        request: &CatalogReadRequest<'_>,
        cursor: Option<&ConversationCursor>,
    ) -> ApplicationResult<NativeConversationPage> {
        self.process(request, None, false, |data| {
            if request.filter != &CatalogFilter::default() {
                return Err(invalid());
            }
            let h = process_header(
                &data,
                request,
                "authorized-conversation-list-1",
                &(),
                cursor.map(|c| &c.header),
            )?;
            let remaining: Vec<_> = data
                .current
                .conversations
                .iter()
                .filter(|(id, _)| cursor.is_none_or(|c| *id > &c.after))
                .collect();
            let mut page = NativeConversationPage {
                schema_version: NextVersion,
                kind: "native_conversation_page".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                matched_in_scope: data.current.conversations.len() as u64,
                items: Vec::new(),
                next: None,
            };
            let mut used = bytes(&page)? + 2048;
            for (id, record) in remaining.iter().take(request.options.limit) {
                let (turns, sources, sensitivity) = conversation_counts(&data.current, id);
                let item = ConversationSummary {
                    conversation: (**record).clone(),
                    visible_captured_turns: turns,
                    visible_captured_sources: sources,
                    effective_sensitivity: sensitivity,
                    platform_complete_not_claimed: true,
                };
                let size = cost(&item)?;
                if used + size > request.options.max_bytes {
                    break;
                }
                used += size;
                page.items.push(item);
            }
            if page.items.len() < remaining.len() {
                page.next = page.items.last().map(|c| ConversationCursor {
                    header: h,
                    after: c.conversation.conversation_id.clone(),
                });
                if page.next.is_none() {
                    return Err(invalid());
                }
            }
            finish(page, request.options.max_bytes)
        })
    }
    pub fn memory_conversation_detail_native(
        &self,
        request: &CatalogReadRequest<'_>,
        conversation: &SessionId,
        cursor: Option<&ConversationTurnCursor>,
    ) -> ApplicationResult<NativeConversationDetail> {
        self.process(request, None, false, |data| {
            if request.filter != &CatalogFilter::default() {
                return Err(invalid());
            }
            if cursor.is_some_and(|c| &c.conversation != conversation) {
                return Err(AutomationFault::Stale.into());
            }
            let h = process_header(
                &data,
                request,
                "authorized-conversation-turns-1",
                conversation,
                cursor.map(|c| &c.header),
            )?;
            let record = data.current.conversations.get(conversation).copied();
            let (turns, sources, sensitivity) = if record.is_some() {
                conversation_counts(&data.current, conversation)
            } else {
                (0, 0, Sensitivity::Public)
            };
            let mut page = NativeConversationDetail {
                schema_version: NextVersion,
                kind: "native_conversation_detail".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                conversation: record.cloned(),
                effective_sensitivity: record.map(|_| sensitivity),
                visible_captured_turns: turns,
                visible_captured_sources: sources,
                turns: Vec::new(),
                next: None,
                platform_complete_not_claimed: true,
            };
            let remaining: Vec<_> = data
                .current
                .turns
                .get(conversation)
                .into_iter()
                .flat_map(|m| m.iter())
                .filter(|(id, _)| cursor.is_none_or(|c| *id > &c.after_source))
                .collect();
            let mut used = bytes(&page)? + 2048;
            for (id, turn) in remaining.iter().take(request.options.limit) {
                let source = data.current.sources[*id];
                let mut metadata = source_metadata(source);
                // This row includes the fragment's branch/turn metadata too.
                metadata.effective_sensitivity = turn.effective_sensitivity;
                let item = NativeConversationTurn {
                    turn: turn.turn.clone(),
                    source_record: metadata,
                    fields: source.record_fields.clone().ok_or_else(invalid)?,
                };
                let size = cost(&item)?;
                if used + size > request.options.max_bytes {
                    break;
                }
                used += size;
                page.turns.push(item);
            }
            if page.turns.len() < remaining.len() {
                page.next = page.turns.last().map(|t| ConversationTurnCursor {
                    header: h,
                    conversation: conversation.clone(),
                    after_source: t.turn.source.source_id.clone(),
                });
                if page.next.is_none() {
                    return Err(invalid());
                }
            }
            finish(page, request.options.max_bytes)
        })
    }
}
