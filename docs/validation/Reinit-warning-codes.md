# Re-initialization warning counts (ADR-MEM-139)

2026-11-11. The CLI's read-only `plan` now adds a sorted
`planned.warnings_by_code` map. Existing totals and planning/formation rules are
unchanged. No source text, title, locator or private path is added to the output.

## Synthetic verification

`cargo test -p enouia-memory-init --bin enouia-memory-reinit --locked` passed
(1 test). A synthetic export contains an assistant parent omitted by the legacy
selection and a selected owner turn with no language tag. The full CLI plan
reports both categories, their sum matches the total, source text is absent from
the output, `writes` is zero and no Vault is created. The initial fixture used a
parent absent from the export, which is a different parser state; it was corrected
to test the intended present-but-unselected parent.

Focused binary build, clippy with `-D warnings`, formatting and diff checks passed.
No full workspace suite or Linux-specific check was run for this reporting change.

## Real selected snapshot: counts only, no writes

| Check | Result |
| --- | --- |
| Matched / selected legacy sources | 731 / 731 |
| Candidate statements / batches | 185 / 16 |
| Staged / unparseable / unsupported | 0 / 0 / 0 |
| `language_unknown` | 731 |
| `parent_not_in_selection` | 1 |
| Warning total / sum of categories | 732 / 732 |
| Writes | 0 |

The categories mean an unknown language label and a parent outside the batch's
selection. They do not mean the source matcher lost 732 records. Unknown metadata
stays unknown, and no missing conversation context is invented. No real `run`,
Owner enrollment, Provider or deployment was executed by this slice.
