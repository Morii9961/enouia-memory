# Documentation index

All project documentation lives under `docs/`.

| Folder | Content | Status |
|---|---|---|
| [design/](design/README.md) | Memory design package v1.2 (Chinese): vision, architecture, data model, import/review, context, interfaces, privacy/recovery, plan, acceptance, decisions, handoff | Target specification. v1.1 corrects repository ownership, the data root and the MV-0R entry; v1.2 records that Runtime hosts the local frontend (ADR-MEM-45); v1.0 is in Git history (`da0d1e3`) |
| [reviews/](reviews/MV0_REVIEW_AND_REPO_CORRECTION.md) | Independent MV-0 review: findings F1–F6 and the migration plan R0–R5 | Authoritative for MV-0R |
| [handoff/](handoff/) | Startup prompts: the original MV-0 prompt (superseded) and the MV-0R correction prompt | Historical or current task inputs, not standing authorization |
| [adr/](adr/README.md) | Decision register: design ADR-MEM-01…18 and implementation ADR-MEM-19…46 | Current |
| [integration/](integration/RUNTIME.md) | Runtime hosting handoff (ADR-MEM-45): ownership, host duties, change routing, the [surface manifest](integration/runtime-surface.json) and the compatibility log | Current |
| [contracts/](contracts/CONTRACT_NOTES.md) | Contract notes and the [constraint enforcement map](contracts/CONSTRAINT_MAP.md) | Current (MV-0R) |
| [validation/](validation/MV-6.md) | Stage evidence: the [MV-0R](validation/MV-0R.md), [MV-1](validation/MV-1.md), [MV-2](validation/MV-2.md), [MV-3](validation/MV-3.md), [MV-4](validation/MV-4.md), [MV-5](validation/MV-5.md), [MV-6](validation/MV-6.md), and [MV-6 follow-up](validation/MV-6-followup.md) reports | Current |
| [history/](history/) | The MV-0 draft report and ADR draft written in the Runtime working tree, kept as history | Historical |
| `history/private/` | Original v0.1 working draft. It contains personal examples, so it is kept locally and ignored by Git | Private, not published |

The 2026-10-05 MV-6 [lifecycle](validation/MV-6-vault-lifecycle.md), [concurrency](validation/MV-6-core-concurrency.md), [operation outcome](validation/MV-6-operation-outcomes.md), [reference scheduling](validation/MV-6-shell-lifecycle.md), [picker admission](validation/MV-6-picker-admission.md), [excerpt pagination](validation/MV-6-excerpt-pagination.md), and [remember replay](validation/MV-6-remember-replay.md) reports record follow-ups to ADR-MEM-46 and the corresponding Runtime pin adoption boundaries.

The [concurrent source replay finding and correction](validation/MV-6-pending-source-replay.md) records the eighth slice: the initially failing synthetic diagnostic and its subsequent manual-assertion service fix. Other service and full-page concurrency cases remain follow-ups.

The 2026-10-06 [concurrent Context session replay report](validation/MV-6-session-replay.md) records the ninth slice, including full `session_new` page calls and input/fork/checkpoint service regressions.

The [Mock dispatch/reply replay report](validation/MV-6-mock-replay.md) records the tenth slice, including full `session_ask` page calls around saved input/capsule state and a deletion during paused replay.

The [correction and forgetting request-binding report](validation/MV-6-proposal-binding.md) records the eleventh slice: published proposals bind original targets and arguments, including one forced concurrent correction replay.

The [full session ask replay report](validation/MV-6-full-session-replay.md) extends acceptance to two same-key page schedules starting before input creation, through saved compilation, Mock dispatch and reply.

The [late proposal admission report](validation/MV-6-proposal-admission.md) records recovery of identical owner proposals when a receipt appears after initial lookup, with a forced changed-claim refusal.

The [different-key proposal deduplication report](validation/MV-6-proposal-dedupe.md) binds new candidate publication to the dedupe snapshot, with one Core re-evaluation on head movement and a direct governance conflict regression.

The [full remember replay and target freshness report](validation/MV-6-full-remember-replay.md) composes the source and proposal fixes from initial page admission, including changed-claim refusal and a correction target changed during head retry.

## Import record (2026-09-28)

The design package, the two prompts, and the review were written in this directory. They were moved unchanged from the repository root into the folders above, apart from the substitutions listed here. The unredacted originals are kept in the ignored `.local/originals/` with their SHA-256 values.

- Machine paths (drive-letter project paths and a user-profile temp path) were replaced by neutral names such as "Memory 仓库（本地 checkout）", "Runtime 仓库（本地 checkout）", `docs/design/…`, and `<本地临时目录>`. This follows the design rule of removing private machine paths before publication.
- Links broken by the move were repointed to `../reviews/` and `../handoff/`. Links to the private v0.1 draft became plain text.
- The review's source references to the Runtime draft (`set.rs:1314` and similar) became plain text. Those line numbers point into the pre-migration draft.
- Files changed by these substitutions: `design/README.md`, `design/DECISIONS_AND_SOURCES.md`, `design/CLAUDE_HANDOFF.md`, `handoff/CLAUDE_START_PROMPT.md`, `handoff/CLAUDE_MV0_CORRECTION_PROMPT.md`, `reviews/MV0_REVIEW_AND_REPO_CORRECTION.md`. The other design documents are byte-identical to their originals.

The original v0.1 draft is unchanged (SHA-256 `bfc7d55d219a1eb667c666739f943cab034db1a2508ed4f56e9ea81f5abd1ea4`).
