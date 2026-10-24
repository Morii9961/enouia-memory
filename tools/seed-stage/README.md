# Private seed staging (MV-7)

This bridge moves an explicitly selected `enouia.portable-memory/1` bootstrap
into native Source records and pending agent proposals. It never approves a
memory, changes a Provider setup, or sends a request. Runtime uses its existing
Candidate inbox and trusted review dialog; no new wire command is introduced.

Keep the input, prepared package, receipts and backups private and outside
version control. Only synthetic examples belong in this repository.

## Prepare and inspect

Supply the private bootstrap directory and a **new** private output directory:

```powershell
python tools/seed-stage/prepare.py <bootstrap-directory> <new-package-directory>
cargo run -p enouia-memory-cli --locked -- seed-preview <package-directory>/package.json
```

The preparer verifies original export hashes, exact quotes, normalized source
text and the reviewed supporting originals. It produces derivative ChatGPT
subsets, Markdown provenance notes and a hash-bound staging manifest. These
subsets are explicitly incomplete: omitted parents, selected leaves and missing
attachments remain gaps. The complete original archive is retained in the
bootstrap library; it is not silently declared fully imported into the Vault.

Native IDs use UUID v4; subjects shared within one package use the same ID.
Each separately prepared package receives new internal subject/item IDs.
Business dates retain inclusive local calendar boundaries, unknown times stay
null, and the message observation time is separate. Episode dates map to
`occurred_start`/`occurred_end`, while claim validity stays null: an event's end
does not make the memory expire from later ordinary recall. Non-real contexts are
explicit in the candidate content and tags. Detailed epistemic, date and recall
annotations remain in the saved reason and imported provenance document.

Copied migration summaries use an unknown-class derived document as evidence;
the original user message remains context, without upgrading the summary to
direct testimony. Assistant context and agent provenance notes use
`context_source_refs`. Shared sensitive sources retain the stricter level.

## Stage into an explicitly selected Vault

Use a native-policy-approved Vault outside repositories and synchronization
folders. Preserve an existing Vault and its current backup. New Vault creation
uses the CLI's existing explicit `init` confirmation and owner-only `protect`.

```powershell
cargo run -p enouia-memory-cli --locked -- seed-stage <vault-directory> <package-directory>/package.json --sha256 <preview-hash> --confirm-stage
cargo run -p enouia-memory-cli --locked -- seed-review-check <vault-directory>
```

Staging reuses the ordinary import pipeline and bound proposal publication.
The native source bytes, locator, content hash, role and evidence class must
match the inspected selector. Imports are audited before proposal publication.
The stable agent principal never impersonates the Vault owner. Exact-package
retries recover candidate receipts, including after ordinary owner review;
changed package bytes cannot reuse committed seed ID/revision keys. A rejected
or unknown result is not permission to change keys and duplicate submissions.

Imports and individual proposals commit separately. After interruption, retry
the **same frozen package** with its original digest. Do not regenerate a
package and mix it into an unfinished run. This is an initialization bridge,
not an incremental synchronization protocol or an approval mechanism.

`seed-review-check` builds acceptance plans in memory and returns only counts,
IDs and fault categories. Check that `failed` is empty. It never confirms a plan
and does not prove owner acceptance or source truth.

## Review in Runtime

Launch a compatible Runtime with `--memory-vault <vault-directory>`, or select
the Vault in its native dialog, then open Memory Vault → Candidate inbox.
Inspect content, saved reason, original source revisions, sensitivity and the
exact acceptance plan. The existing plan dialog alone performs owner review.
Prioritize the 32 core records by the `seed.core` tag in the proposed details
when using this initialization dataset; the bridge does not claim a dedicated
UI filter exists. Do not promote sensitive or disputed records by default.

The portable `default_recall` flag is retained as an annotation; it is **not** a
new native compiler policy. Candidates do not enter ordinary Context at all.
Before accepting records with restricted recall, choose the appropriate native
policy or leave them pending. External dispatch remains separately gated.

Export and verify a pinned recovery checkpoint with the existing CLI. A
restored copy is a recovery drill, not another active primary. Two partitions
on one physical disk are not independent backups; these CLI exports are not
encrypted backups. An independent encrypted recovery target remains separate.

## Verification

```powershell
python -m unittest discover -s tools/seed-stage -p test_prepare.py
cargo test -p enouia-memory-cli --locked --test seed
```

The synthetic CLI check covers staging, exact replay, ordinary trusted review,
search, changed-key payload refusal, changed input refusal and pre-import ID
validation. Run the repository's full required checks before completion.
