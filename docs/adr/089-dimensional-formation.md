# ADR-MEM-89 — Dimensional formation and reversible consolidation (Core-5)

Date: 2026-10-24. Status: Adopted for explicit synthetic roots. Morii authorized
continued Core work; real models, personal migration and Runtime adoption remain
separate. This implements representation/rules and scripted synthetic flow, not
proof of natural-language extraction quality.

Add opt-in `automation-host-v2` layout and `admission-rules-3`; old descriptors
and rule-1/2 policy/input bytes remain unchanged. Old Core-3/4 readers refuse
this layout before loading records. V1 global schema and all default routes
remain unchanged. New roots require a policy with an explicit allowed-form set.

Keep formal dimensions on the exact admitted input: kind, occurrence assertion,
plan/project/open-loop state, conditions, event/valid time, change reason,
counter-source dependencies, exact derivation parents and optional historical
reversion target. Unknown event time stays unknown; learned time is the actual
containing native commit time, not a model field. A reported event is still
attributed self-report, not independently established truth.

Pure rules separate plan/pause/cancellation/completion and preserve stable
identity/type and new-source revision requirements. System interpretation and
theme synthesis remain qualified hypotheses/staged; they never become Morii
self-report by repeating summaries. Derivation expands to original sources,
inherits scope/sensitivity and stores exact parent revisions/hashes. Stale or
deleted parents invalidate descendants before recall. Originals are never
merged away. Reversion creates a new revision reproducing an exact historical
claim and dependencies; it does not rewind CURRENT or restore deleted sources.

Reuse the existing Core-3 transaction, receipts and recovery, plus Core-4's fake
policy path. No external source is executed/copied. Design borrows the fixed
research package's formation/reflection separation, without autonomous fact
inflation or search reinforcement. See [contract](../contracts/CORE5_FORMATION.md)
and [evidence](../validation/CORE5-formation.md).
