# ADR-MEM-150 — Compact journals for new fresh runs

Date: 2026-11-17. Status: Proposed; synthetic implementation, real use waits for merge.

Morii authorized a minimal compact-journal capability after whole-run capacity
preflight refused the inline representation. This continues local initialization,
without a new MV stage or a larger mutable-journal limit.

## Decision

Only new fresh runs use initializer journal version 2 at the existing private
initializer journal path. Each initial immutable Plan is a content-addressed Raw
object, bound to its exact run hash, Vault/descriptor, plan index and plan count.
This retains sources, catalog, policy, authored artifacts, exact quotes, original
seeds/operation IDs and object references without serializing them into every
mutable journal update. A plan object remains limited to 16 MiB.

The mutable journal retains every plan hash, capture state and each work item's
original request, response, generation binding, receipt and state. It remains
limited to 16 MiB, including a conservative upper bound for every completed or
uncertain item. Nothing is evicted after completion. The pure preflight reports
the compact queued/lifecycle sizes and immutable object sizes alongside the old
inline sizes; readiness requires every object and the whole mutable lifecycle
to fit their respective unchanged bounds.

Initial immutable objects are archived before atomic enrollment. Open verifies
their hashes and run/Vault/index bindings before reconstructing the in-memory
Plans. Missing, corrupt, mismatched or malformed objects refuse. Open neither
repairs nor substitutes generated plans. Uncertain application still looks up
only the original full binding; an absent original receipt remains unknown.
There is no replacement key, resend, interval renewal or authority expansion.

Existing inline version-1 journals remain readable and are saved in their original
representation. They are not migrated, compacted or re-enrolled. Non-fresh and
linked/revising initialization keeps that existing path. New compact enrollment
admits only the independent fresh work already required by whole-run preflight;
its immutable seed cannot acquire later target or derivation dependencies.

## Backup and compatibility

Morii export already retains all content-addressed Raw objects and the private
initializer journal. Export/restore must include the plan objects and unchanged
mutable packet state. Synthetic acceptance covers close/reopen, exact object
identity, missing/corrupt-object refusal, uncertainty and isolated backup/restore
with the same run binding. Restored resumption never copies bootstrap enrollment
or creates another logical Morii.

The format belongs to the standalone initializer. Native admission, stored
memory decisions, Workspace commands and Runtime readers do not gain another
field from this slice. An older initializer cannot open a version-2 journal;
use the compatible fresh tool for resumption and preserve the pre-adoption copy.
ADR149's compatible-reader requirement for grouped decisions still applies.
Any actual integration-surface change is regenerated and logged with the patch.

Full original input and analysis bindings remain unchanged. No source/quote
truncation, attachment policy change, Raw deletion, Provider call, read grant,
deployment or real formation is authorized by this design alone.
