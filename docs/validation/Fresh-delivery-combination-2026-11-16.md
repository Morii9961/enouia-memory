# Fresh delivery combination — review and synthetic verification

2026-11-16. Route A of the [delivery route](../handoff/MEMORY_DELIVERY_ROUTE_2026-11-16.md).
This prepares the three open PRs for Morii's review; none was merged or used with
real input. No product implementation was changed during this combination.

## Exact reviewed source

| Item | Tip | Target at capture |
| --- | --- | --- |
| Merged main | `a543c1eb81e5d7150ebe7820592eea35a9042697` | — |
| [PR110](https://github.com/Morii9961/enouia-memory/pull/110), backup regression | `7b5868cffa4212d92566fbe60f3fb6136a949c38` | main |
| [PR112](https://github.com/Morii9961/enouia-memory/pull/112), restored-run continuation | `492f75f5d47342df85613bf73b2e1602bfbf3133` | PR110 branch |
| [PR113](https://github.com/Morii9961/enouia-memory/pull/113), text-only retention | `214373f3d116ff795bb6788cabf2bb9efd35ac41` | main |

GitHub confirmed all three open, non-draft and individually mergeable against
their current targets. No review submissions or discussion comments existed at
capture. PR110's tip is an ancestor of PR112's tip. Both PR112 and PR113 include
the merged ADR146 baseline. These observations are not owner approval.

Local review branch `codex/fresh-delivery-review-20261116` combines PR112 (including
PR110) with PR113. Tested commit:
`895d74c5531d9e91b256151d894379b6d03d678c`; tree:
`3697468011145f3bd810d7aeb15e643c4046c289`.
The [combined source](https://github.com/Morii9961/enouia-memory/tree/895d74c5531d9e91b256151d894379b6d03d678c)
is review material, not a production baseline or an additional merge request.

Production Rust files merged without conflicts. The only two conflicts were
`docs/adr/README.md` and `docs/integration/RUNTIME.md`; resolution retained both
ADR147/148 entries and both Runtime notes. ADR148 does not change ADR147's
aggregate or remove ADR146's compatible-reader requirement.

## Checks actually run on that combination

An independent Cargo target was used throughout; neither existing worktree's
binary or build directory was overwritten. Pinned toolchain: Rust 1.98.1, offline,
locked dependencies, Windows. All data, roots and backups were synthetic.

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-init --test fresh_driver --test fresh_backup --locked` | `fresh_driver`: 9 passed / 1 ignored; `fresh_backup`: 2 passed / 1 ignored |
| `cargo test -p enouia-memory-init --test fresh_driver trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run --locked -- --ignored --exact --nocapture` | 1 passed / 9 filtered |
| `cargo test -p enouia-memory-init --test fresh_backup native_cli_resumes_activated_restore_without_original_home_or_replacement_keys --locked -- --ignored --exact --nocapture` | 1 passed / 2 filtered |
| `python tools/integration/runtime_surface.py` | passed; aggregate `d44f0e84f6b07e7e55ee66a291cd418a3811734545f1402686ad110deec56199` |
| `git diff --check main...HEAD` | passed |

The first command initially ran under restricted process permissions:
both backup tests refused synthetic root creation with `Denied`; the driver
target did not run. The same unchanged source then passed under normal native
account permissions (backup 67.19 s; driver 53.19 s). Both logs are retained.
An earlier command could not start because Cargo was absent from PATH; adding
the already installed Cargo directory resolved that environment issue. Neither
failed invocation is counted as passed verification.

The two opt-in tests used an explicitly selected durable synthetic parent and
created unique protected homes, retained with their markers. The normal run's
two ignored tests were thus executed separately: **13 tests passed in total**.

Coverage includes explicit metadata-only refusal/default behavior; 65 text-only
memories across two plans; lifecycle journal bounds at each step; close/reopen,
pagination, exact quotes and unchanged run/retention barriers; backup with Raw,
journal and deletion preservation; original receipt-only recovery and expired
policy barriers; native bootstrap/run/recover; and activated restored-root
continuation after the original home becomes unavailable. The restored CLI
checks preserve original keys under unknown outcomes and require explicit
forward recovery for interrupted publication. Backup fixtures keep their
existing selected-originals mode; this run does not claim a new single test that
combines metadata-only payload omission with backup/restore.

Host/bootstrap and Vault backup tests plus scoped Clippy retain the revision
scope recorded in [PR112's report](https://github.com/Morii9961/enouia-memory/blob/492f75f5d47342df85613bf73b2e1602bfbf3133/docs/validation/Fresh-restored-run-resume.md).
PR113's initializer-wide tests and Clippy retain the scope of
[its report](https://github.com/Morii9961/enouia-memory/blob/214373f3d116ff795bb6788cabf2bb9efd35ac41/docs/validation/Fresh-text-only-retention.md).
They were not rerun here. No full-workspace suite, schema cross-check, Linux,
real corpus preflight, real formation or independent-media recovery ran.

## Concrete owner merge sequence

1. Review and merge PR110 at the recorded tip.
2. Retarget PR112 to main after PR110 merges, recheck its diff and merge at the
   recorded tip. If PR110 is squash/rebase merged, first reconcile its ancestry
   so the PR112 diff contains only its restored-run increment.
3. Integrate current main into PR113 while preserving both ADR147/148 entries
   and both Runtime notes as in the tested review tree; review and merge PR113.
   The two documentation conflicts are expected after PR112. Compare the final
   Rust tree with the tested combination before reusing this evidence; rerun
   affected checks only if code changes.
4. Keep the status/route preservation and stale-guidance corrections in their
   independent documentation PR. A three-way merge preview with the tested
   combination completed without conflicts and retained both sets of guidance.
   Resolve any later overlap in `INITIALIZED_VAULT.md`
   by retaining the detailed ADR148 retention paragraph and the corrected
   already-merged ADR145/146 and ADR111/112 descriptions. The saved status
   remains a dated snapshot and is not refreshed by these merges.

The review branch itself is not the production merge vehicle. No original PR
tip, target or main was modified during preparation. Morii's existing merge gate
still applies; this report requests no broader permission.

After all capabilities merge, use the exact merged source and an isolated CLI
build for a new metadata-only manifest and complete read-only Plan/journal
preflight. Only a passing complete preflight leads to the already authorized
real native formation, reopen/source acceptance and independent backup/restore.
Preserve original inputs and unknown packet/key/binding; do not use review code
on real material. Runtime readiness requires actual delivery evidence.
