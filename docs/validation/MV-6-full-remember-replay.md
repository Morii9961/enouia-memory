# MV-6 Core — full remember replay and target freshness

Date: 2026-10-06. Base: `b967ba8`. End: the acceptance commit containing this report. Scope: MV-6 / ADR-MEM-46, synthetic page acceptance of the existing source, proposal binding and optimistic head guards. Production behavior, wire/API, dependencies, payload hashes and storage format are unchanged.

## Composed page checks

Three deterministic `remember` cases start before any manual source has been prepared. Both page envelopes use the same logical key, distinct transport request IDs and the same text. The existing injected ID source pauses both calls after their initial receipt lookup.

- `full_remember_replays_a_source_published_after_its_initial_lookup`: one complete call publishes its source and candidate before the other resumes its initial source allocation. Both return the published source and candidate IDs.
- `full_remember_replays_a_concurrent_source_commit`: both calls reach source commit allocation before either is released. The test uses bounded retries only for the existing retryable index/writer contention faults. Both eventually return the same published IDs.
- `full_remember_refuses_a_changed_claim_after_source_replay`: the ordered schedule instead supplies different claims. Exactly one call succeeds; the other returns `idempotency_conflict` / `workspace.key_reuse` with no result.

Each case verifies one stored source, one pending candidate, agreement between the returned source ID and both candidate evidence references, and the actual page excerpt's original synthetic text with `untrusted: true`. Identical subsequent retries use a fresh transport ID and leave the Vault head unchanged. These compose earlier service/late-proposal checks through the complete page path; the bounded test retry helper is not an adapter retry policy.

`correction_head_retry_rechecks_the_target_revision` prepares two different-key, distinct correction sources against revision 1 of a synthetic memory. Both calls pass proposal target checks and pause at candidate allocation. The first finishes, and its candidate is accepted through page `review_plan` and `review_confirm` before the second resumes. Core's head-movement retry rereads the target: the stale call returns `revision_conflict` / `fault.revision_mismatch` with no result, the memory is revision 2, and no pending stale candidate is left.

## Negative controls and validation

Removing only remember's post-admission request-binding guard makes the changed-claim case fail with two successful calls. Removing only the new-candidate head precondition makes the correction freshness case fail with two successful proposals. In each negative control, production source bytes are restored in `finally`, with SHA-256 equality verified. The fixed focused cases passed.

Required root checks passed with the pinned offline toolchain: formatting, all 260 workspace Rust tests (53 Core tests), and all-targets Clippy with warnings denied. Python 3.12 / python-jsonschema independently passed all 34 schemas and their fixture corpora. The regenerated, logged Runtime surface aggregate is `baabf91151a6ccbc21ad7646513896730a2fe9f69d0ee011a78a63cca2172507`; the surface checker and `git diff --check` passed.

## Limits and Runtime adoption

All inputs, actors, sources and Vaults are synthetic. The forced schedules do not establish every interleaving, indefinite retry behavior, every policy/deletion race, atomic source/proposal publication, duplicate-without-receipt binding, actual Runtime adoption or real-data/OS/power-loss durability. Runtime must continue to sequence submissions, reuse keys only for identical arguments and refresh current revisions after stale-target errors. Adopt the evidence with the earlier source/proposal fixes at the pin bump and rerun it through Runtime's own adapter. MV-7 remains unopened.
