# ADR-MEM-87 — Automatic memory transactions in an explicit V2 synthetic root

Date: 2026-10-24. Status: Adopted, Core-3 only. Morii authorized the next slice
after Core-2. No Core-4, real Provider, private migration or Runtime adoption.

## Decision

Add a separate automation storage namespace. Its descriptor and CURRENT use
schema major 2; existing SchemaVersion, RecordKind/AnyRecord, OWNER_ONLY, old
Host recovery and Workspace/MCP semantics remain unchanged. Never place machine
records in a schema-1 root. Creating this format requires an explicitly chosen
empty verified synthetic root and native owner setup; existing roots refuse
adoption. Root setup is not per-memory review or a fabricated approval.

One native automation Host owns the existing lifetime Host OS lease and takes
the existing Writer OS lock for each short transaction. Reuse protected-root
checks, managed atomic files and AuthorityState initial/check_writer/advance.
Immutable bounded full-state commits contain data and authority; one CURRENT
selects both. This is the sole authority for the chosen new-format synthetic
root, not a production sidecar, second writer or dual write.

Persist selected runtime source events, original text/hash, trusted speaker
attribution and exact revisions. Machine decisions keep knowledge/adoption
separate, including system-staged uncertainty with automatic reevaluation.
Bind original generation requests and raw response hashes to decisions, result
revisions, operation IDs and batch receipts. Construct actual evaluation context
from current stored sources/policy/targets inside writer exclusion. Model JSON
cannot supply owner authority, omit selected dependencies or invent a receipt.

Commit the whole selected batch atomically. Pure set/delta checks enforce
append-only history, policy/deletion watermarks, exact target lineage, sensitivity
inheritance and result/commit binding. Only validated System decisions create
machine memories. Policy administration and optional deletion remain distinct
native owner controls; no ReviewRecord or confirmation is generated. Return a
memory receipt only after CURRENT and its durable witness agree. Exact replays
return original receipts under current source/policy/deletion checks; changed
arguments conflict and stale generation never silently rebases.

Add optional `allow_revise_created` only under `admission-rules-2`: an exact
policy may revise machine memories created under that same policy identity,
subject to all current revision, subject/project, source and access checks.
Native stored provenance supplies this scope; model JSON cannot assert it and
legacy owner records remain forbidden. Default false is omitted so existing
rule-1 policy bytes/hashes remain unchanged. Old strict next readers reject the
new populated field/rule version before evaluation. This narrow delegation
avoids requiring the owner to grant every freshly generated target ID.

Write an exact before/after publication intent before CURRENT, then a witness.
Normal open/read never repairs or guesses a commit. An explicitly authenticated
native primary may finish only that already-authorized intent after validating
both immutable states, the legal delta and observed before/after pointer/witness.
This separate V2 recovery creates no new decision, operation or approval and
never rewinds a watermark. This does not change ADR-MEM-73's old-root owner
confirmation rules. Missing/corrupt/conflicting evidence stays unavailable.

The minimum context reader uses current authorized eligible latest revisions,
retains attribution (subject said X, not independent truth), excludes staged,
deleted, stale-source or out-of-scope results and never falls back to an older
adopted assertion when a newer staged revision needs reevaluation. Older
revisions remain inspectable history, not rewritten approval history. This is
a bounded lexical baseline, not vectors, a full timeline or general semantics.

## Evidence and remaining boundaries

The native Host bridge consumes exact persisted Core-2 batch bindings, treats
generation responses as untrusted, and connects only to this explicit synthetic
store. No default HostService/Workspace/MCP route or Runtime pin changes.
Its opt-in completion callback runs after durable generation and outside job
control exclusion, then applies through a short native transaction. The default
Core-2 worker remains generation-only.
Synthetic sessions must form, correct, stage, reevaluate, reopen and recall
without a human review step. See [contract](../contracts/CORE3_AUTOMATIC_LOOP.md)
and [validation](../validation/CORE3-automatic-loop.md). Model quality, actual
client adoption, continuous egress, whole-filesystem rollback protection,
independent restore, power-loss guarantees and real migration are not proved.
