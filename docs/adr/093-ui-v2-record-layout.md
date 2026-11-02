# ADR-MEM-93: UI v2 record fields and projection rules

Date: 2026-10-26. Status: implemented for synthetic validation; Morii confirmed
field continuation after the first-slice report on 2026-10-26. Based on main `c9bf1be7819dba19915adf6031b1900c032b73d1`.

## Decision

Create **automation-host-v3**, with document schema 2 and admission-rules-3.
Never upgrade an existing root, change its descriptor, or offer it to an old
Runtime pin. HostV1/HostV2 forbid the new record fields and catalog. HostV3
requires them, including explicit unknown times and language. Old binaries
reject the new layout at descriptor parsing, before business reads or writes.
The native Core-7 ports remain gated to HostV2 until their separate adoption.

Freeze the closed types and limits in [UI_V2_RECORDS](../contracts/UI_V2_RECORDS.md).
Memory record fields belong to the exact AdmissionInput/result revision and
therefore to its decision/response/commit hashes. Source record fields belong
to the exact captured source revision. Entities and conversations form an
append-only, revisioned catalog in the same witnessed commit as capture;
they are neither extensions nor an alternate authority file. Names do not
merge identities. Entity identity decisions require evidence or an owner
decision; model similarity remains a disposable suggestion.

Keep existing occurrence/state/transition/lineage rules. The rich time's UTC
endpoints must equal Core-5 dimensions; unknown has no endpoints. Never infer
event time from capture/import time. A deliberately supplied source-time basis
must name a supporting source whose occurred_at equals the supplied start.
Calendar interpretation and timezone conversion belong to a versioned later
projection; no machine-local timezone default is adopted here.

An evidence quote is an exact Unicode scalar slice of captured text, with
exclusive end, source revision, existing object locator/hash, copied speaker
and class. No whitespace/width normalization: if normalization is later added,
it needs a separate locator algorithm/version and must still store original
text. A quote copied from an assistant cannot become user evidence. Structural
checks do not prove title/content entailment or detect language/model errors.

Owner plan/confirmation types are separate from machine inputs. Parsing an
owner-labelled document grants no authority. The pure binding validator also
requires native owner identity, current snapshot, exact plan digest, trusted
surface and nonexpired confirmation. The v3 root includes an empty owner action
ledger and withdrawal watermark now; all System deltas preserve it. The reserved owner_action transaction
refuses until G08, avoiding a later schema-field addition. No new owner
execution/publication port exists in this slice; G08 will supply its sole native entry and durable receipt.
Machine formation cannot create owner action records or manual assertions.

Projection metadata binds algorithm/version, exact snapshot, authorization
scope hash, withdrawal watermark, explicit coverage and original dependencies.
Check current policy, source/lineage, deletion and withdrawal eligibility before
computing counts, buckets, labels, summaries or edges. No hidden IDs, empty
positions or cluster names may survive filtering. Cursors bind these same
values and query parameters. Mismatched metadata is unusable, never stale-success.
Deleting/withdrawing any supporting OR context dependency invalidates all
derived summaries, indexes, graph edges and counts; rebuild only from the
currently authorized live set. Caches are disposable and never permission
authority. Physical purge/independent recovery are separate work.

## Compatibility and boundaries

This is a new synthetic-root capability, not a Workspace/MCP command, Runtime
pin update, initialization run, Provider adapter or deployment. Legacy v1
RecordKind/SchemaVersion/owner review bytes remain unchanged. Existing HostV1/2
serialization omits new absent fields and preserves existing fixture hashes.
No old record is silently furnished with a title, inferred date or confirmation.

The Runtime requirements are inputs, not authority. Differences are documented
in the contract: exact quotes rather than normalized matching; conservative
language/timezone syntax validation rather than a new registry dependency;
typed theme annotations do not yet execute Jaccard inheritance; import/manual
source ingestion remains the initialization/G08 responsibility. Existing Core-3
capture still admits only runtime_event, not arbitrary exports or agent input.

## Exit and next work

Types, validation, complete synthetic positive/negative fixtures, new-root
publication/reopen and old-layout refusal tests ship together with the Runtime
surface manifest. Stop after commit/push for Morii's field confirmation.
Then G01 -> G06 -> G23; G03 -> G05 -> G04; G07 -> G13 -> G15 -> G16 -> G19;
finally G08. G12/G22, Core-8, and G20/G21 remain separately owned.
