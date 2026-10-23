# ADR-MEM-81 — Workspace saved-history comparison

Date: 2026-10-23. Scope: expose the existing ADR-MEM-51 local owner review helper.

Add read-only `history_compare({batches:[{taskId,batchIndex}]})`, returning
`history_comparison`, with null idempotencyKey. Accept 1–32 distinct selections;
the native task admission resolves each child Session/run. Caller-supplied run,
identity, source text, model, path or Provider setup is not accepted.

Use `history_review::preview` unchanged. Compare original Fact/Preference
suggestions by its exact topic keys; retain complete source coverage, original
suggestions, qualifiers, citations, unknown time, branch relationships and current
candidate revisions/content/details/status. Comparison hints never assert current
truth, contradiction, supersession, approval or semantic model quality.

The result contains `base` (camelCase commitId/sequence/policyEpoch/deletionEpoch),
`batches` (camelCase task/child bindings), `runs` (the helper's serialized
RunCoverage), and `topics`. Topic keys, comparisons and flags preserve the
helper's existing serialization, including PascalCase enum values and snake_case
nested record fields. Entries use camelCase, adding separate original `details`
and current `candidate.details` to the existing extraction inspection fields.
This is a closed native-built view, not a way to submit review text.

Require one unchanged pin from task resolution through helper reads and result
serialization. Recheck native enabled state. Existing owner, import/source and
policy checks remain. Existing helper bounds (32 runs, 256 suggestions, 512
sources, 4096 comparisons) apply; serialized result is capped at 512 KiB.
Overflow rejects the entire result, never truncates. No durable state, cursor,
Provider request, automatic follow-up or mutation is introduced.

Runtime adopts an exact commit and regenerated surface through its own embedded
adapter. This does not activate the separate Host transport, MCP, cloud services
or a personal Vault. Product entry integration remains with Runtime/Claude.
See [focused evidence](../validation/Workspace-history-comparison-v1.md).
