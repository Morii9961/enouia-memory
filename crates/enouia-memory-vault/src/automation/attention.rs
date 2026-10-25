use super::*;
pub const ATTENTION_PROJECTION: &str = "indexes/core6-attention.json";
pub struct AuthorizedRecallSet {
    pub snapshot: SnapshotBinding,
    pub memories: Vec<(MachineMemory, Timestamp)>,
    pub now: Timestamp,
}
impl AutomationVault {
    /// Native System only. Apply current authorization before returning bodies,
    /// IDs or counts. Historical ranking never revives an old grant/source.
    pub fn authorized_recall_set(
        &self,
        actor: &ActorRef,
        max: Sensitivity,
    ) -> Result<AuthorizedRecallSet> {
        self.system(actor)?;
        self.available()?;
        if self.descriptor.layout != AutomationLayout::HostV2 {
            return Err(AutomationFault::Denied);
        }
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (commit, _) = self.current_locked()?;
        let now = self.now()?;
        if !commit.data.policy.active
            || now < commit.data.policy.valid_from
            || now >= commit.data.policy.valid_until
        {
            return Err(AutomationFault::Denied);
        }
        let mut latest = BTreeMap::new();
        for memory in &commit.data.memories {
            latest.insert(&memory.memory.memory_id, memory);
        }
        let memories = latest
            .values()
            .filter(|m| self.eligible(&commit, m, max, true).is_ok())
            .filter_map(|memory| {
                let source_time = memory
                    .decision
                    .input
                    .input_refs
                    .iter()
                    .filter_map(|r| {
                        commit
                            .data
                            .sources
                            .iter()
                            .find(|s| s.reference() == *r)
                            .map(|s| s.record.captured_at.clone())
                    })
                    .max()?;
                Some(((*memory).clone(), source_time))
            })
            .collect();
        Ok(AuthorizedRecallSet {
            snapshot: commit.snapshot(),
            memories,
            now,
        })
    }
    /// Disposable metadata only. No CURRENT/record write, receipt or touch.
    pub fn maintain_attention(
        &self,
        actor: &ActorRef,
        profile: &AttentionProfile,
    ) -> Result<AttentionProjection> {
        if !profile.validate().is_empty() {
            return Err(AutomationFault::Denied);
        }
        let data = self.authorized_recall_set(actor, Sensitivity::HighlySensitive)?;
        let interval = profile.maintenance_interval_ms as i64;
        let at = Timestamp::from_unix_ms(data.now.unix_ms() / interval * interval)
            .ok_or(AutomationFault::Unavailable)?;
        let rows = data
            .memories
            .iter()
            .map(|(memory, source_time)| AttentionRow {
                memory: memory.memory.clone(),
                state: attention_state(profile, memory, source_time, &at),
                source_time: source_time.clone(),
            })
            .collect();
        let projection = AttentionProjection {
            schema_version: NextVersion,
            kind: AttentionProjectionKind::AttentionProjection,
            algorithm: profile.algorithm,
            snapshot: data.snapshot,
            profile_hash: digest(profile),
            at,
            rows,
        };
        let bytes = canonical_bytes(&projection)?;
        // A corrupt/stale projection is replaceable; it supplies no authority.
        if self
            .root
            .read_bounded(ATTENTION_PROJECTION, MAX_DOCUMENT_BYTES)?
            .as_deref()
            != Some(bytes.as_slice())
        {
            self.root.write_atomic(ATTENTION_PROJECTION, &bytes)?;
        }
        Ok(projection)
    }
}
