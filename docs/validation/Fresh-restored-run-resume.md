# Fresh-run continuation on activated Morii restores — synthetic validation

2026-11-16. Implements [ADR147](../adr/147-fresh-restored-run-resume.md), based on
merged ADR145 main `442b22f` and the synthetic backup regression in PR110
(`cc1d418`). No real Morii input, Vault or backup is used. The final combined
source also includes merged ADR146 main `a543c1e`.

## Native entry

The existing `enouia-memory-fresh` binary adds:

```text
enouia-memory-fresh resume-restored <MANIFEST> <MORII_DATA_HOME> <RESTORED_ROOT> <ACTIVATION_EXPORT> --bind-current-owner-self
enouia-memory-fresh recover-restored <MANIFEST> <MORII_DATA_HOME> <RESTORED_ROOT> <ACTIVATION_EXPORT> --bind-current-owner-self
```

The exact export selected at activation remains the binding for this restored
root, including after continuation produces newer commits. This command makes
no new backup-currency assertion and cannot activate a root. Reuse the trusted
native current-OS-account/authorized-command boundary, protected durable-home
checks and current-user-only restored root. Flags or JSON do not authenticate a human.
Keep the original descriptor/Morii/Host IDs, exact fresh run and persisted plans.
Missing/legacy/non-fresh journals, changed manifests, another export, unactivated
restores and roots outside the chosen home refuse. No original home registration
is copied, recreated or changed; Runtime root selection is untouched.

Only explicit `recover-restored` requests ordinary forward recovery, after the
run guard. Receipt-only handling, original policy expiry and deletion barriers
remain. There is no new persisted format, initial bootstrap/adoption, policy,
read grant, second run, replacement packet or automatic resend.

## Checks actually run

All checks below were repeated on the final ADR146 combination with this
worktree's own isolated Cargo target. An earlier worktree shared the main target;
ADR146's saved report demonstrated CLI replacement between concurrent builds.
This slice stopped using that shared target before the final verification. Its
new native continuation test includes the publication-interruption check below.

- `cargo test -p enouia-memory-host --lib owner_bootstrap --locked`:
  **16 passed, 54 filtered out**. New test checks home containment, quarantine,
  exact restored descriptor, production refusal of synthetic enrollment and
  byte-identical preservation of the original bootstrap registration.
- `cargo test -p enouia-memory-vault --test segmented_backup --locked`:
  **5 passed**. Covers the new read-only activated-restore proof as well as the
  existing raw/journal/deletion round trip, occupied/invalid root refusals,
  corrupt/missing/unsafe exports and activation currency checks. No proof call
  activates an unactivated root or rewrites its receipt.
- Explicit opt-in native Windows-account test:
  `cargo test -p enouia-memory-init --test fresh_backup native_cli_resumes_activated_restore_without_original_home_or_replacement_keys --locked -- --ignored --exact --nocapture`:
  **1 passed, 2 filtered out**. Uses a newly allocated durable synthetic parent,
  actual native bootstrap and the production CLI. After source-home removal from
  its original location, an activated restore resolves one original committed
  receipt and completes two queued statements. All three records' native quoted evidence
  resolves; repeated resume/recover makes no further commit. A synthetic interruption
  between CURRENT and witness publication keeps `resume-restored` at
  `RecoveryRequired`; wrong-export/changed-analysis recovery leaves the witness
  unchanged. Only explicit recovery of the original run completes publication,
  preserving the same snapshot and journal. Another restored copy
  holds a genuine prepared but unapplied request: it remains `OutcomeUnknown`,
  retains the entire original work item, and leaves the following statement queued.
  Unactivated, wrong-export, changed-analysis and unbound empty-restore cases refuse.
  The original bootstrap bytes stay unchanged and no registration is created in
  the restored home. The protected synthetic homes are retained for diagnosis.
- Existing native CLI compatibility:
  `cargo test -p enouia-memory-init --test fresh_driver trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run --locked -- --ignored --exact --nocapture`:
  **1 passed, 8 filtered out** on the final ADR146 combination. Original
  prepare/commit/run/recover forms two synthetic records, including the explicit
  mixed-message text view, and reopens/resolves sources without multiplication.
- Scoped Clippy for the modified initializer binary/test, Host library, and Vault
  library/backup test, all with `--no-deps --locked -- -D warnings`: **passed**.
- Targeted rustfmt, `git diff --check`, public-content review and Runtime surface
  regeneration/check: **passed**. Combined aggregate is
  `d44f0e84f6b07e7e55ee66a291cd418a3811734545f1402686ad110deec56199`;
  Workspace remains 61 commands.

## Limits

All durable native homes and fixtures are synthetic. No actual Morii corpus,
independent encrypted media, real restore activation, authority transfer, Provider,
read grant, deployment, data deletion, Runtime checkout or project-status snapshot
was used or changed. This is Windows evidence, not a Linux or full-workspace run.
No schema or validator changed; no schema cross-check was needed or run.

PR110's two earlier full/expired-journal regression results retain their original
revision scope; they were not rerun here. The selected checks above cover the
new boundary and the old trusted CLI path. ADR146 admission is retained from
merged main without being rewritten by this slice. Actual corpus capacity and
acceptance remain separate. The new entry waits for Morii's merge before real
restored-root use. Existing independent backup validation and stop-old-Host-before-
activation gates remain in force.
