# Segmented Owner backup (ADR-MEM-136)

2026-11-11. Synthetic data only; no real Owner root, Provider, network service,
VPS deployment or legacy deletion.

## Windows evidence

- `cargo test -p enouia-memory-vault --test segmented_backup --test segmented
  --locked -- --test-threads=2`: 8 passed.
- `cargo test -p enouia-memory-init --test legacy --locked -- --test-threads=2`:
  2 passed. The legacy test includes a stored attachment and a simulated lost
  application acknowledgement. After backup/restore the initializer looks up the
  original receipt, reproduces its ledger, makes no new commit and calls no model.
- `cargo check -p enouia-memory-cli --bin enouia-memory-owner-backup --locked`:
  passed.
- Focused clippy on the Vault library/backup test and new CLI, `-D warnings`:
  passed. Targeted formatting and Runtime surface checker passed.

Backup tests cover independent verification with the source moved away, exact
snapshot and receipts, logical deletion barriers, archived bytes and journal
preservation; refusal of busy Host/journal leases, nested/nonempty destinations,
unknown journals and old layouts; corruption, extra files, missing reachable
segments, unsafe paths and a rehashed descriptor that contradicts genesis.
Invalid exports leave restore targets empty. Restored roots refuse open/recovery
until activation; a newer/different backup or changed restored bytes is refused.

The first restricted Windows test run failed at the existing native ACL/lock
boundary. Running the same synthetic tests with native filesystem permissions
passed; no implementation bypass was introduced. An initial compile found three
local type errors, fixed before the checks above. Cargo also reported fallback to
copying its incremental cache; it did not affect the checks.

## Linux and acceptance limits

On WSL Ubuntu-24.04, an isolated detached worktree at implementation commit
`6ed5e62` passed `segmented_backup` (4 tests), init `legacy` (2 tests), both with
`--test-threads=2`, and the new CLI's focused `cargo check --locked`. The existing
Linux checkout's WIP and the synthetic bootstrap root were preserved.

No encrypted restic media or separate failure domain was exercised. Native round trips are not a real
independent-backup acceptance, a Windows-to-Linux data migration or ChatGPT
interoperability evidence. Those remain separate owner-authorized drills.
