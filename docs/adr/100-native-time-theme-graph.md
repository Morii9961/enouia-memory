# ADR-MEM-100: Native time buckets, theme families and typed graph

Date: 2026-10-26. Status: implemented; synthetic validation. Builds on ADR93/94.
ADR95/96/97 belong to the concurrent MCP branch and are not imported here.

## Decision

Keep schema 2 and the accepted record fields. Use the existing new-empty-root
`automation-segmented-host-v1`; no root conversion or stored record change.
Add pure read-2 ports to the separately authenticated native reader. Every call
rechecks current authorization, exact snapshot/lineage/deletion, sensitivity and
lifecycle before records, labels, counts, membership or edges enter a projection.

Selections now emit **literal-family-catalog-2**, version 2. Old version-1 DTOs
remain parseable as their original contract, but their cursors cannot continue
under the new algorithm. Query/parameter/profile/scope hashes and evaluation
instant are explicit. Current permission checks always use the fresh clock.

Time buckets use the caller's explicit IANA timezone and civil date window,
with day/month granularity. Add a Host dependency edge to the already locked
Jiff 0.2.37 and an exact jiff-tzdb 0.1.8 edge. Read its compiled TZif data
through `jiff_tzdb::get` / `TimeZone::tzif`; report the actual IANA release
(currently 2026c). Runtime cannot silently resolve another data release. Do not read system timezone,
environment timezone or OS zoneinfo. Civil boundaries follow Jiff's compatible
midnight disambiguation and start-of-day behavior; skipped dates have an empty
instant interval. No assumption that a civil day has 24 hours. Unknown memory
time stays separate; there is no capture/learning fallback. Known intervals
contribute to each possibly overlapping half-open bucket. Counts are not
additive across buckets and do not turn `within` uncertainty into a duration.
Captured-material counts explicitly use current authorized sources, unfiltered
by memory filters, on their own capture axis. Staged/adopted counters use the
requested memory axis and filters. Return at most 366 buckets, or refuse.

Theme records retain their evidence-bound names and stable Memory IDs. Their
recorded derivations are exact members (score 1000); currently authorized
records sharing a non-subject entity are **algorithm suggestions** (750).
These scores are a deterministic rubric, not a probability or verified topic.
A person's presence solely as subject does not create a universal family.
Membership is disposable and is computed lazily from the fresh authorized set;
it is never appended to `derived_from` or a machine/owner record. One family
can have more than 128 members. Whole-theme aggregations have a declared
16,777,216 input-by-theme work bound; ordinary pages and a selected family's
membership do not materialize that cross product. Owner name/pin/dissolution
application and its authenticated inheritance checks belong to G08; this slice
never sets `owner_named` or fabricates an owner record.

The graph selects up to 5,000 stable-ID current nodes, pages up to 100 nodes/
edges under caller bytes, and emits typed recorded derivation, current reversion
when both endpoints are selected, same-conversation and same-entity relations.
Conversation relations require a nonempty currently authorized conversation
and its live turn/participant dependencies. Same-entity means recorded mention
of the same explicit ID, not independent verification or automatic merging.
Theme cooccurrence is separately labeled an algorithm suggestion. For each
kind, report exact candidate and retained counts **within selected nodes**.
At most 20,000 edges per kind and 262,144 grouped memberships are materialized;
excess membership work refuses rather than inventing a partial exact count.
Clipped edges/selected nodes make coverage incomplete. No layout coordinates,
semantic-neighbor model, hidden endpoint or cluster name is produced.
Revision/owner correction relations requiring history or actual owner actions
remain attached to the later G15/G08 work, not manufactured from text similarity.

## Adoption and evidence

Runtime must adopt the exact source/manifest, read-2 ports, catalog algorithm
version, scoped coverage, independent material axis and qualified membership/
edge classes. No v1 command, default Host/MCP route, Runtime file or pin changes.
Tests use only new native synthetic roots. Timezone behavior is checked against
[Jiff 0.2.37 TimeZone](https://docs.rs/jiff/0.2.37/jiff/tz/struct.TimeZone.html),
[Date](https://docs.rs/jiff/0.2.37/jiff/civil/struct.Date.html) and
[bundled database](https://docs.rs/jiff/0.2.37/jiff/tz/struct.TimeZoneDatabase.html),
and the locally locked source. See [contract](../contracts/UI_V2_PROJECTIONS.md)
and [validation](../validation/UI-v2-projections.md).
