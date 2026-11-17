# Fresh preparation — equivalent JSON and lineage lookup work

2026-11-17. Based on merged main `f57d84b758fa3b1f868242e4f9fab0f407601332`.
All verification for this change uses synthetic inputs.

## Repeated work removed

The initializer already parses each original JSON material for supplementary
metadata. Resolving every message through the byte-based locator parsed that
same material again for every message. Preparation now reuses its immutable
parsed original JSON for direct JSON pointers. The byte-based resolver delegates
to the same pure JSON extraction function, retaining exact string bytes,
string-parts joining, pointer escaping and existing errors.

The cached path applies only when the original material itself parsed as JSON.
Archive members and byte ranges keep the existing byte-based resolution path;
archive member selection and hash verification are unchanged. Content hashes
are still checked against the adapter result. No locator, Raw object, provenance,
parser output or source permission is replaced by a cache assertion.

Branch-lineage lookup previously scanned the complete message set twice at
each ancestor. Each lookup now builds scoped parent/child indexes and walks
those indexes. It retains platform/account/conversation scope, first-match
parent behavior, fork boundaries, missing parents and the existing depth cutoff.
The reference scan is retained only as a small synthetic compatibility oracle.

This changes work performed, not native acceptance, budgets, schema, policy,
wire, command or persistent formats. It is independent of the conversation-title
projection correction in PR115; it does not claim to resolve a rejected plan.

## Checks actually run

- `cargo test -p enouia-memory-import -p enouia-memory-init --locked -- --nocapture`:
  **97 passed, 0 failed, 2 ignored**, including empty unit/doc targets (16 targets).
  This includes importer archive/revision/cancellation checks, initialization,
  fresh input, Raw/quote retention, packing, backup/reopen and uncertainty barriers.
  The two opt-in trusted CLI tests remained ignored and were not separately run.
- New direct JSON locator checks cover Unicode, mixed string/non-string parts,
  missing parts, escaped pointers, unsupported targets and invalid raw JSON.
- A 512-message synthetic conversation retains exact Raw, text, parents,
  roles, language, timezone and unknown metadata. Preparation took **131.69 ms**
  in this debug run; this is an observation, not a CI timing threshold or a
  real-corpus speed claim.
- Small branch fixtures match the previous algorithm across forks, missing and
  duplicate IDs, account/conversation/platform scopes, cycles and depth cutoffs.
  The long-chain fixture resolves 64 lookups over 4,000 synthetic messages.
- Scoped all-targets Clippy for both changed packages, with `--no-deps --locked
  -- -D warnings`: passed. Its first run flagged the test fixture tuple's type
  complexity; a shared test-only row alias resolved it. The two lineage tests
  were then rerun: **2 passed**, with the long-chain lookups taking **753.94 ms**.
  Production logic was unchanged by that fixture-only correction.

The first package-test invocation could not compile the new synthetic fixture
until its integer range was explicitly typed; the complete rerun above is the
passing result. Initial failed logs are retained locally.

No dependency or schema changed. No changed path belongs to the declared Runtime
surface, so no surface regeneration or checker run was needed. No full-workspace,
Linux, schema cross-check or real-input preflight is claimed for this branch.

## Combined native CLI comparison

The title correction (PR115) and this branch merge without conflicts in review
commit `43ea10683fc90c05d4bd8e40af53afb2ec8ea5c2`. Its focused native title/Raw/quote/
reopen regression passed: **1 passed, 10 filtered**. This review branch is not a
production baseline.

Optimized CLIs then planned the same 512-message synthetic manifest (8 plans)
sequentially: merged main `f57d84b` took **2.604 s**; the review combination took
**0.873 s**. Both exited successfully, reported zero writes and agreed exactly on
run hash, intake, every plan preflight, the complete queued/lifecycle journal
budget and readiness. The fixture uses an ordinary title, so it adds no display
normalization difference. This is a local synthetic observation, not a timing
threshold, real-input test or real-corpus throughput claim.

No real input, Vault, Provider, read grant, Runtime checkout, deployment, deletion
or manual status snapshot is used or changed by this verification. The branch
must merge before it is used in the real delivery run.
