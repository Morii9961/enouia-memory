# ADR-MEM-94: Segmented automatic roots and scalable native reads

Date: 2026-10-26. Status: implemented with synthetic validation. Depends on ADR-MEM-93
`24a03e02bdf18094d60bc4d3268d380bd1b669a1`; Morii confirmed continuation after
the first-slice field report. Main remains `c9bf1be7819dba19915adf6031b1900c032b73d1`.

## Decision

Preserve the frozen record fields. Add opt-in **automation-segmented-host-v1**
(schema 2) for new empty synthetic roots. Never upgrade HostV1/2/3 roots in
place. Their full-state storage, limits and native read capability stay intact.
The new physical layout is necessary: increasing page bounds cannot remove
the old 128-entry/64-commit/32-MiB full-snapshot limits.

Store bounded immutable delta commits addressed by their canonical SHA-256,
linked by exact parent pointers. Each delta contains only its new source batch,
machine request/raw response bindings, catalog revisions, policy revision or
deletion. Reuse the Host lease, short writer lock, owner ACL/reparse checks,
flushed immutable files and intent -> CURRENT -> witness publication. A failed
publication fences that instance; explicit recovery can finish only its exact
validated before/after intent, never manufacture an operation or rewind barriers.

Replay the complete hash/authority/delta chain on open into indexed native
state. Under the lifetime exclusive Host lease, later operations recheck the
descriptor, CURRENT, intent and witness before using that verified state.
Unexpected descriptor/head replacement refuses; reopening verifies immutable data
again. This cache is verified source state, never a cached permission grant:
every query rebuilds eligibility from current policies, deletions, exact source/
lineage refs, entity evidence and the native sensitivity ceiling before counts,
filtering, facets, labels or edges. No persistent search file supplies authority.

Allow 65,536 source revisions, 65,536 machine revisions, 1,024 current policies,
65,536 commits and 1 GiB cumulative canonical commit bytes. Exhaustion refuses
before publication; no silent pruning/truncation. A source delta has at most
64 sources and a machine delta at most eight decisions. Per-record/dependency
bounds from Core-1/5 and ADR-MEM-93 remain unchanged. Multiple independently
scoped policies preserve their original exact subject/project/source grants;
there is no wildcard or broadened System/Agent/owner scope.

The owner ledger is empty and reserved as in ADR-MEM-93; no owner action entry
is activated by this storage work. Existing v1 IPC, MCP tools, Runtime pin,
Provider/embedding adapters, real initialization and deployment are unchanged.

## Read contract

The separately versioned `memory-native-read-2` accepts this physical layout
explicitly. Stable identity pagination is bounded by 100 items and caller bytes
(1 KiB–256 KiB). Counts are exact over the currently authorized latest set,
with explicit scope/coverage. Cursors bind snapshot, query/profile/sensitivity,
authorized-set hash and evaluation time; policy/deletion/head/scope changes
invalidate them. Unknown and inaccessible selections have identical absence.

Literal Chinese/English search and explicit aliases, kind, separate knowledge/
adoption/validity/attention axes, entity, project, time and recorded theme
derivation use the same authorized set. Facets declare their filtering scope
and bounded coverage; theme derivation is not the later G05 family projection.
Statistics use the same selection rules. No native read writes a record, receipt,
search reinforcement or index file. No global graph/timeline or saved experiment
is claimed. Subsequent G03/G05/G04 supply their separately versioned projections.

## Required evidence

Actual new temporary roots, witnessed batch publication/reopen/replay, corrupt
segment and publication-boundary faults, writer exclusion, old-layout refusal,
permission-before-count/alias/facet, byte pressure and stale cursors. Explicit
20,000-record synthetic traversal must show no omissions/duplicates, exact
filtered counts and invalidation after deletion/policy changes. Synthetic
results do not establish real-model quality, UI acceptance or real migration.
