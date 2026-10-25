# Core-5 dimensional formation and consolidation

Date: 2026-10-24. [ADR-MEM-89](../adr/089-dimensional-formation.md).
Base: Core-4 `d6e1c55a1c0c5fcf17b858efd9198688e24f782d`.

## Version and entry

Create a new empty synthetic root with schema 2, layout
`automation-host-v2`, `admission-rules-3` and an explicit `allowed_forms` policy.
The native Core-3 writer/recovery and Core-4 optional fake transport are reused.
No old root is upgraded; HostV1 roots refuse rule 3 or dimensional results.
This build continues to read/write its existing HostV1 rule-1/2 roots unchanged.
Old Core-3/4 strict descriptor parsers reject the new layout before records or
queries. V1 readers reject schema 2 earlier still. Optional fields are omitted
when absent/empty, preserving old input/policy bytes and hashes.

Global SchemaVersion=1, RecordKind/AnyRecord, OWNER_ONLY, default Host/MCP/Workspace
routes and Runtime pins stay unchanged. Unknown major/raw diagnostics still do
not confer business read support. Capability selection must include layout and
rule version, not just the integer schema major.

`AdmissionInput.dimensions` is a closed optional value object. A native request
may select a form and exact derivation/reversion scope; generation cannot change
that selected kind/parent/reversion binding. A normal seed may leave dimensions
absent for analysis, but HostV2 publication requires a populated validated result.
Missing is distinct from a supplied null, which refuses.

## Dimensions and claims

| Field | Meaning |
| --- | --- |
| kind | fact, preference, episode, project_state, plan, open_loop, relationship_experience, subjective_understanding or theme |
| occurrence | unknown, intended, or reported_occurred; this is an assertion label, not objective verification |
| state/change | Applicable plan/project/open-loop status and explicit update, correction, pause/resume, cancellation, completion, synthesis or reversion |
| conditions | Up to eight bounded qualifications; no unbounded extension object or inferred universal applicability |
| event_time | Explicit reported event range, or unknown; never filled from capture/import/commit time, and future reported events refuse |
| valid_time | Independent optional half-open applicability range; expired/future ranges exclude current recall without completing or deleting a record |
| counter_sources | Exact context dependencies for disputed/uncertain analysis, separate from supporting citations |
| derived_from | Up to eight exact current machine parent revisions/hashes, with full original-source closure |
| reverts_to | Exact historical revision/hash of the same identity for a new-revision reversion |

Subject/project already belong to the exact AdmissionInput. The source's
speaker/evidence/attributed subject remain authoritative for self-report. The
containing native commit's `created_at` is learned/committed time; there is no
model-writable learned-time field. Event time may remain unknown even when
occurrence is reported. Precision beyond the supplied range is not invented.

Episode requires reported occurrence and no plan state. Plan refuses reported
occurrence/event time; even plan completion does not manufacture an episode.
Project states preserve planned -> decided -> implemented -> tested -> released.
Open loops retain open/paused/cancelled/completed without age-based resolution.
Pause requires a nonterminal state; resume returns to the recorded pre-pause
state. Cancellation and completion are distinct transitions; ordinary updates
cannot reopen terminal states. An explicit new-source correction may revise
an earlier report. Identity/kind stays stable; an actual episode is a separate
record rather than silently changing a planned item into an experience.

Non-reversion revisions need a new exact supporting source revision relative
to their target's dependencies. Reversion reproduces the historical content,
interpretation and semantic dimensions exactly, preserving original dependencies,
then records a new revision/change reason. It cannot rewind CURRENT, lower
sensitivity or use stale/deleted source support. Corrections/reversions retain
all immutable source and memory histories.

Subjective understanding and themes cannot be clear self-report. Derived
results cannot relabel themselves as the subject's direct statement. Inference
remains hypothesis/staged, conflict disputed/staged; neither is independently
verified or promoted because the model repeated itself. The system can retain,
reevaluate or abstain without an owner review queue. Relationship experience
still needs actual evidence/subject attribution; this code does not infer a
relationship from nicknames or diagnose an emotion.

## Consolidation, dependencies and recall

`AutomationVault::prepare_synthesis` and native
`AutomationHost::submit_synthesis` select exact current eligible parents,
flatten their original source refs, dedupe them, and prepare a separate theme
analysis. Generation sees the original selected text, not a new source claiming
that a prior summary independently supports itself. The policy must allow theme;
all normal source, scope, sensitivity, snapshot and Core-4 egress checks apply.
Parents are never merged away. Repeated reflection retains the same original
source identities and no new independent evidence class or corroboration count.

Set/delta validation checks parent identity/hash, current policy, subject/project,
access/egress and source closure. Current source resolution checks revisions and
deletions. A parent's later revision makes old derived references stale; native
eligibility recursively checks exact current parents before result/receipt use.
The immutable predecessor graph is bounded by the existing chain/state caps.

Only eligible adopted statements enter ordinary context. Dimensional output
retains form/state/occurrence, conditions and event/valid time alongside the
self-report qualification. Staged themes/understanding are inspectable native
history, not unqualified facts inserted into the answer. No periodic cognition,
vector lookup, notifications, front-end timer or Activity/Usage input is added.

## Validation and remaining adoption

The implementation uses synthetic source statements and scripted labels. Pure
checks validate labels/dependencies; they cannot prove entailment, extract every
important event, infer identity, discover all contradiction or assign dates.
The independent nine-unit comparison measures typed representation and detects
deliberate missing/misclassified output. It is not a live-model score or an
estimate for private history. See [actual evidence](../validation/CORE5-formation.md).

The fixed research package's OmbreBrain-folio `bf6887575fa8e6fa519279be9dc7e5886a29d2ea`
and Ombre-Brain `115d831d979bf67efc7150f976e2cb34c333c935` inform formation/reflection
separation only. No external code/scripts were executed or copied, no external
runtime/database/embedding was introduced. Real quality evaluation, personal
migration, physical purge/independent restore and Runtime UI/native adoption
remain separate. Rollback uses supported roots or exact forward reversion,
never changing layout headers or restoring an old deletion/policy watermark.
