# UI v2 Owner actions — synthetic validation

2026-10-28, ADR-MEM-103. Implementation base:
`1154e3bad8321cf5d5659994226f1c4af3c2c98e` (the decision checkpoint atop
process/detail `6ca8fccd5b0cadcb464c49aec35300744def25b9`). Morii explicitly
selected A; the new empty layout is `automation-segmented-owner-v1`, schema 2,
with mandatory native `owner_subject_id`. Existing roots are not upgraded.

## Requirement result

| Requirement | Delivered backend behavior |
|---|---|
| 01 G08 / 02 §5 | Authenticated native Owner factory, exact snapshot/impact plan, actual gesture confirmation, private single-use consent, witnessed atomic commit and original-operation recovery lookup |
| Correct / Corrected by you | Actual bound-subject literal manual source plus target-bound System admission; mark only from the witnessed Owner record for the current correction result |
| Withdraw / Forget | Logical target deletion/barriers, qualified dependent disappearance, native impact hash, no source purge; literal same-claim/source suppression survives reopen |
| Protect / Pin / Tag | Actual Owner overlays separate from machine annotations/profile; native expiry; qualified tag count/completeness |
| G05 / 02 §7 | Full native family membership beyond 128, exact impact count, Jaccard >= 0.5 rename/pin/dissolve inheritance, recomputation and reopen |
| G09 / G14 | Compatible existing entity merge, exact quoted-evidence split, own-policy aliases/facets/typed edges and authorized conversation participant projection |
| G18 / Core-7 | Current authorization before data/counts/proofs; v4 snapshots invalidate old cursors; receipts omit global sequence; metadata omits hidden turn arrays/ordinals and parent links |
| 02 §3–§6 | ADR93 stored fields retained; explicit root subject binding and replayable execution effects are described in the Owner contract field table |

## Focused evidence

Eleven native Owner scenarios pass: exact literal/attributed correction with one
witnessed transaction and reopen; wrong actor/subject/hash/snapshot, expiry and
lock; two unknown-publication fault points plus explicit forward recovery;
current Owner marks/activity and forget disappearance; literal same-source
regeneration refusal after withdrawal/reopen; old-root rejection and expiry at
commit; entity filter/alias/typed graph merge and selected split; foreign-policy
alias refusal and authorization expiry between confirmation and commit; 132-member
family rename/pin/withdraw/recompute/reopen/dissolve; conversation participant
merge/mixed-turn split without rewriting stored turns; old-root header adoption and
initialized Owner-subject rebinding refusal against the witnessed genesis hash. A compile-fail doctest
proves that JSON cannot deserialize the native consent capability.

The 150-turn process regression still exposes 150 under Private and 86 under
Public. Current metadata contains no raw turns; Public branch order is dense
1..86 and every returned parent belongs to the authorized captured-turn set.
A public source body does not declassify private fragment relationships.

The native-emitted Owner corpus includes twelve positive documents and 32 negative
mutations (44 documents); preserved process v3 plus native v4 examples and
negatives total 102 documents. Rejections cover forged machine Owner/confirmation
flags, missing/null/old-layout subject binding, changed plan hashes, invalid
metadata/floors, physical-purge claims, fabricated correction marks, global
receipt sequence and hidden raw turn metadata. Rust native validation, the Rust
subset schema checker and independent python-jsonschema are separate checks.
Parsing a stored/prospective shape still grants no native execution authority.

## Final checks

- Pinned Rust 1.98.1, GNU Windows target, offline/locked resolution.
- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --locked`: **704 passed, 0 failed, 4 intentionally ignored**
  (two scale regressions and two explicit synthetic example emitters).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Python 3.12 / python-jsonschema 4.26.0 independent cross-check: **55 schemas**, all
  manifests passed, including Owner 44, process 102, catalog 41 and projections 31.
- `python tools/integration/runtime_surface.py`: passed, aggregate
  `862eb802156dfb168e4eeba16979352699094eab8ab3c978a4ac1d5bf862eec4`,
  generated and logged in RUNTIME.md with this slice.
- Explicit release scale regressions passed: 20,000 real synthetic native records,
  **282 initial pages**, 19,936 after deletion/revocation/reopen; write time
  341.0730988 s, total 508.6473234 s (test 509.63 s). 5,000 graph nodes, **50 pages**,
  exact coverage and old-cursor invalidation after deletion (75.88 s). These used
  the same final v4 reader implementation on the old segmented regression root;
  the later Owner-genesis binding changes only the new Owner root admission gate.

All Cargo calls disabled incremental/dev/test debug information only for these
owned processes and used two build jobs. Native ACL assertions remained enabled.
An interrupted full run had no complete result and is not passing evidence; the
final rerun above completed with exit 0. Clippy style findings were corrected
before the final passing run. Own cached test executables were removed only after
checking the exact owned path, Cargo cache tag and absence of owned processes to
make room for the explicit scale cases; no source/private root/other tree changed.

## Differences, adoption and stopping point

Core-5 Theme correction revisions still refuse; real Owner theme actions operate
on projections. Correction is literal self-report under the explicit Owner subject
binding, not inferred foreign-subject attribution or independent truth. Every
entity redirect/alias retains the affected memory's own policy. Dissolution keeps
the original recorded synthesis. Withdraw/forget retain original sources and
immutable history and do not claim physical purge. Suppression covers literal
same-scope/content/evidence SourceId, not semantic paraphrases or equivalent new
sources. Capacity failures are explicit; no hidden input is counted as a hole.

The native backend is callable by Runtime's eventual authenticated adapter; no
Runtime file, Memory pin, UI gesture wiring, MCP writer, initialization pipeline,
private vault, real model/embedding, credential or deployment was touched. The
concurrent MCP and initialization modules require explicit integration of their
independent code; this slice does not claim their adoption of the new Owner root.
Initialization must provide the actual trusted Owner-to-Subject binding when
creating its new root. G08 completes this session's authorized backend route;
G12/G22, G11 and G20/G21 remain with their assigned sessions. No main merge.
