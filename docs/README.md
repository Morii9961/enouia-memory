# Documentation index

All project documentation lives under `docs/`.

| Folder | Content | Status |
|---|---|---|
| [design/](design/README.md) | Memory design package v1.1 (Chinese): vision, architecture, data model, import/review, context, interfaces, privacy/recovery, plan, acceptance, decisions, handoff | Target specification. v1.1 corrects repository ownership, the data root and the MV-0R entry; v1.0 is in Git history (`da0d1e3`) |
| [reviews/](reviews/MV0_REVIEW_AND_REPO_CORRECTION.md) | Independent MV-0 review: findings F1–F6 and the migration plan R0–R5 | Authoritative for MV-0R |
| [handoff/](handoff/) | Startup prompts: the original MV-0 prompt (superseded) and the MV-0R correction prompt | Historical or current task inputs, not standing authorization |
| [adr/](adr/README.md) | Decision register: design ADR-MEM-01…18 and implementation ADR-MEM-19…44 | Current |
| [contracts/](contracts/CONTRACT_NOTES.md) | Contract notes and the [constraint enforcement map](contracts/CONSTRAINT_MAP.md) | Current (MV-0R) |
| [validation/](validation/MV-6.md) | Stage evidence: the [MV-0R](validation/MV-0R.md), [MV-1](validation/MV-1.md), [MV-2](validation/MV-2.md), [MV-3](validation/MV-3.md), [MV-4](validation/MV-4.md), [MV-5](validation/MV-5.md), [MV-6](validation/MV-6.md), and [MV-6 follow-up](validation/MV-6-followup.md) reports | Current |
| [history/](history/) | The MV-0 draft report and ADR draft written in the Runtime working tree, kept as history | Historical |
| `history/private/` | Original v0.1 working draft. It contains personal examples, so it is kept locally and ignored by Git | Private, not published |

## Import record (2026-09-28)

The design package, the two prompts, and the review were written in this directory. They were moved unchanged from the repository root into the folders above, apart from the substitutions listed here. The unredacted originals are kept in the ignored `.local/originals/` with their SHA-256 values.

- Machine paths (drive-letter project paths and a user-profile temp path) were replaced by neutral names such as "Memory 仓库（本地 checkout）", "Runtime 仓库（本地 checkout）", `docs/design/…`, and `<本地临时目录>`. This follows the design rule of removing private machine paths before publication.
- Links broken by the move were repointed to `../reviews/` and `../handoff/`. Links to the private v0.1 draft became plain text.
- The review's source references to the Runtime draft (`set.rs:1314` and similar) became plain text. Those line numbers point into the pre-migration draft.
- Files changed by these substitutions: `design/README.md`, `design/DECISIONS_AND_SOURCES.md`, `design/CLAUDE_HANDOFF.md`, `handoff/CLAUDE_START_PROMPT.md`, `handoff/CLAUDE_MV0_CORRECTION_PROMPT.md`, `reviews/MV0_REVIEW_AND_REPO_CORRECTION.md`. The other design documents are byte-identical to their originals.

The original v0.1 draft is unchanged (SHA-256 `bfc7d55d219a1eb667c666739f943cab034db1a2508ed4f56e9ea81f5abd1ea4`).
