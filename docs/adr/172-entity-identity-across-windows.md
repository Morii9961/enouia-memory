# ADR-MEM-172 — Entity identity across windows: same-name links and Morii's decisions

Date: 2026-11-23. Status: Accepted (Morii's entity decision, second part);
synthetic validation.

## Context

Morii's decision (2026-11-23):

- An identical name links automatically, marked as automatic and revocable.
- Everything else is a suggestion that he confirms in the Runtime UI.

Initialization records one entity per mention, and since ADR-MEM-171 daily
memories do too. The same person is therefore spread over many records, often
in different capture windows. The existing `EntityMerge` and `EntitySplit` only
apply when the target record lies in the memory's own policy, so they cannot
link records across windows. That is deliberate: an entity record stays bounded
by its window's privacy scope.

## Decision

**Morii's decisions.** A new owner action, `EntityIdentity { entities (2–16
records of one type), same, reason }`, goes through the usual plan, confirmation
and commit.

- **`same: true`** groups the listed records, together with any groups they are
  already in, into one identity.
- **`same: false`** separates the first listed record from the others. It
  leaves its group and is excluded from automatic same-name links from then on.

Decisions apply in commit order when a read is authorized. A read applies a
decision only when every record it lists is visible to that read, so no reader
learns a link to a record it cannot see. A mention of a grouped record resolves
to the group's smallest record ID, with the deciding proof (`entity_identity`).
This is the existing redirect mechanism, so the catalog, entity filter, graph
`same_entity` edges and Agent `related` all treat confirmed groups as one
entity. Unlike `EntityMerge`, it applies across windows; the canonical ID is
only a label.

**Automatic same-name links.** Records of the same type whose display names
match after collapsing whitespace and lowercasing are linked automatically,
unless Morii separated one. Two memories naming such records that do not already
resolve to the same identity get a new `same_name` edge:

- It is always an `algorithm_suggestion` with `score_milli` 900 and witness
  `same_name:<name>`.
- It appears in the System graph (header note
  `same_name_edges_are_unconfirmed_identity_suggestions`) and in Agent
  `related`.
- Nothing is stored. A separating decision removes it.

Aliases do not count yet.

**Confirmation view.** `NativeMemoryOwner::entity_identities(max, limit)` reads
Morii's full history and lists `EntityIdentityGroup`s:

- **Group fields:** `basis`, `entity_type`, `name` and `records`.
- **Record fields:** `entity_id`, `resolved_entity_id`, `display_name`,
  `memories` and `opted_out`.
- **Basis:** `confirmed` when every record resolves to one identity,
  `same_name` when a name link is involved, `single` for a lone record.
- **Order:** groups first, then single records, by name, with `total_groups`
  and `complete`.

Runtime builds the confirmation UI from it:

- **Confirm** plans `same: true` over a group's records.
- **Not the same** plans `same: false` with the record to separate listed first.
- **Link to…** plans `same: true` for a record and a chosen group.

**Contract.**

- `GraphEdgeKind::SameName` is added. Graph coverage allows seven kinds and
  related coverage six.
- Owner proofs accept `entity_identity` and up to 16 entities.
- The edge-kind schemas, owner-action schemas and proof schemas are updated.
- The graph coverage schema limit of 5 was stale since ADR-MEM-162; it is now 7.

## Consequences

- Daily memories about the same person or place connect without Morii acting.
  Wrong guesses cost one "not the same", and nothing he has not confirmed
  becomes a recorded relation.
- Code matching edge kinds or owner actions must handle `same_name` and
  `entity_identity` when the pin moves. Older binaries refuse a vault that
  contains the new action.
- No change to other persisted formats; the new action is stored as an
  ordinary owner action.

## Verification

A synthetic Owner root receives three connector memories in two capture
windows: "Lin" twice and "Lin Xiao" once.

1. The graph shows one `same_name` suggestion between the two "Lin" memories.
   The view lists that pair as `same_name` and "Lin Xiao" as `single`. Planning
   with an unknown record is refused.
2. After Morii confirms "Lin Xiao" with one "Lin", the graph shows a
   `same_entity` recorded edge carrying one owner proof, alongside the remaining
   `same_name` suggestion. The view shows one mixed group of three records.
3. After he separates the other "Lin", only the confirmed edge remains. The view
   shows a `confirmed` group of two and the separated record as `single` with
   `opted_out`.

Before any decision, an Agent reader scoped to both windows also gets exactly
one `same_name` suggestion in `related`, from the first "Lin" memory to the
other, with witness `same_name:Lin` and six coverage entries.
