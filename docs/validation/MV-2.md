# MV-2 — History import and rescue (synthetic)

Date: 2026-09-29. Scope: MV-2 ([plan §6](../design/IMPLEMENTATION_PLAN.md), [IMPORT_REVIEW](../design/IMPORT_REVIEW.md)), authorized by the owner after MV-1. Start: `317e9da`. No real export was designated, so every input is synthetic; no real chat, account, sync folder, or data root was read or created, and nothing was downloaded. MV-3 has not started.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| MV-2.0 contract | `import` record kind (ImportManifest, revisioned, stored at `vault/raw/manifests/<id>/<rev>.json`): status, input kind, received-bytes hash and size, adapter and observed schema, sanitized archive members with disposition, resume cursor, counts, per-conversation coverage (IDs, counts, times; no titles), warnings, `duplicate_of`. Schema, 2 valid fixtures, 9 invalid record cases, 3 set cases, closure rules (ADR-MEM-38) | `import.rs`, `set.rs::check_imports` (contract), `contracts/memory/import-v1` |
| MV-2.1 framework | Bounded ZIP reader; detection by content; read the selected regular file twice and compare; archive the Raw object first; batched parse commits with the next import revision; cancel between batches; resume from the archived bytes with the same adapter version; byte-identical re-imports recorded as duplicates | `enouia-memory-import`: `zip.rs`, `detect.rs`, `pipeline.rs` |
| MV-2.2 three inputs | ChatGPT export ZIP / `conversations.json` (structure probing), Markdown file or Markdown-only archive (byte spans at headings), Enouia Runtime native session (format defined here) | `chatgpt.rs`, `markdown.rs`, `runtime.rs` |
| MV-2.3 graph and attachments | Reply trees become parent links and branches (current path, edited siblings); a missing parent is an explicit `unknown`; no invented times; attachments present (stored as asset objects), missing, external (kept as text only), or quarantined (executables) | `pipeline.rs::branches`, `chatgpt.rs` |
| MV-2.4 coverage and recovery | Post-import reconciliation re-derives every cited source revision from the archived bytes and closes the counts; it works unchanged on a restored Vault; CLI `import`, `resume-import`, `import-audit` | `audit.rs`, `locate.rs`, `enouia-memory-cli` |

Imports produce sources only. Nothing extracts, summarizes, proposes memories, calls a model, or fetches a URL.

## 2. Acceptance items

| Item | Status | Evidence (`crates/enouia-memory-import/tests/…`) |
|---|---|---|
| I01 original bytes | Passed | `import.rs::i01_*`: the received file is archived byte for byte and every source's locator re-derives content matching its hash; titles never reach the manifest; a ZIP whose `conversations.json` is broken is still archived (status `partial`, no adapter) and the bytes stay exportable |
| I02 repeated / overlapping imports | Passed | `import.rs::i02_*`: identical bytes → `duplicate_of`, no new sources (Raw deduplicated by hash); an overlapping later export → 9 unchanged, 1 edited message as revision 2 of the same source (revision 1 kept), 1 new source; the key includes the local account alias, so different accounts never merge |
| I03 branches, edits, missing parents | Passed | `import.rs::i03_*`: edited siblings share the parent but get different branches, the current path is one branch, a missing parent is `unknown`, unknown time stays unknown; coverage counts branches, missing parents, and unknown times |
| I04 attachments | Passed | `import.rs::i04_*`: a file in the ZIP is stored as an asset with verified hash and sniffed type; a missing file is `missing`; a URL is kept as text (`external_reference`), never requested; an executable member is quarantined and not stored |
| I05 unknown / huge / malformed | Passed | `import.rs::i05_*`: garbage, truncated ZIP, ZIP64, and overlapping members are archived and reported `partial`; unsafe (`../`) names, symlinks, encrypted members, CRC mismatches, and a 64 MiB compression-ratio bomb (under 1 MiB compressed) are quarantined member by member while the rest imports; nothing is written outside the Vault; existing data verifies clean |
| I06 cancel / crash, then resume | Passed | `import.rs::i06_*`: cancel after one batch leaves `parsing` with cursor 1/3; a second import of the same bytes is refused while one is unfinished; a failure injected into the next batch commit leaves it invisible; resume (with the original file deleted) reads the archived bytes and completes without duplicates; resuming a completed import does nothing |
| I07 real export drill | **Pending** | Needs an export chosen by the owner, and first the backup gate (restic, media, off-device secret) |
| MV-2.4 reconciliation and restore | Passed | `reconcile.rs`: counts close for created, revised, duplicate, and unsupported imports; after a pinned export, deletion of the original root, and restore elsewhere, every imported source still resolves and the received ZIP returns byte for byte |

The adapters were written without a real ChatGPT export. They rely only on the widely documented `mapping` shape and ignore everything else (the raw bytes keep it). Compatibility with real exports is **not** claimed until I07.

## 3. Measurements

`cargo run --release -p enouia-memory-import --example measure_import -- 200 20 50` (synthetic, one run, Windows 11, NTFS, SSD):

| Sources | Commits | Total | Per commit | Head manifest |
|---|---|---|---|---|
| 2,000 (batches of 25 conversations) | 5 | 8.7 s | 1.7 s | 0.55 MB |
| 4,000 (batches of 50 conversations) | 5 | 13.5 s | 2.7 s | 1.1 MB |

Turning off full-history set validation did not change the time, and neither did skipping per-file flushes: the cost is creating one file per source on Windows (about 3 ms each, plausibly real-time scanning). Removing the MV-1 staging copy and rename saved about 13 %. The head manifest (about 275 bytes per record) has now crossed the ADR-MEM-37 trigger (1 MiB). **Before a real history of tens of thousands of messages is imported, the segmented catalog and incremental validation must land**: otherwise every later commit, even one manual assertion, rewrites a manifest of tens of megabytes.

## 4. Checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 128 passed: contract 60, vault 52, import 14 (2 unit, 9 import, 3 reconcile), CLI 2 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `python tools/schema-check/check_schemas.py` | exit 0; 31 schemas; 53 valid records, 117 record cases, 4,662 set records, 51 IPC messages, 30 store documents agree |

Isolated build: `59aa931` was cloned from GitHub into a temporary directory with a fresh target directory; fmt, 128 tests, clippy, and the schema cross-check passed.

## 5. Defects found and fixed

- Splitting a long Markdown section could cut inside a multi-byte character (found by a unit test with Chinese text).
- The duplicate-revision count in the set validator ignored imports.
- A test picked the first manifest object assuming it was Identity Markdown; Raw objects now sort first.

## 6. Not done, or pending

- I07 with a real export; compatibility of the ChatGPT adapter with current real exports.
- The segmented catalog and incremental validation (§3), required before large real imports.
- Streaming: the selected file is read into memory (bounded at 1 GiB by default), not streamed.
- Claude/Codex conversation importers and a Markdown directory import (a ZIP of Markdown works); coverage for Markdown files uses no file names (they can be personal), so files are not individually named in the report.
- Candidate extraction (IMPORT_REVIEW §2.8) belongs to MV-3/MV-7.
- Everything still pending from MV-1: restic, OS-crash and power-loss evidence, volume encryption detection.

Not activated: no default data root, scheduled task, model, network access, MCP, or VPS.

## 7. Commits

| Commit | Content |
|---|---|
| `41a7af4` | MV-2.0 ImportManifest record and closure rules (ADR-MEM-38) |
| `1808fcb` | Import pipeline, bounded ZIP reader, three adapters |
| `3f906fe` | Reconciliation, restore location, CLI import commands |
| `59aa931` | Records written once at final names; import scale measurement |
| (this commit) | This report and status updates |

## 8. Before MV-3, and before any real import

1. Owner authorization for MV-3 (Canonical memories, candidates, review).
2. For a real import (I07): choose the export, install a pinned restic, choose backup media, keep the secret off this device, run the restore drill, choose the real data root and apply the owner-only ACL.
3. Decide whether the segmented catalog (ADR-MEM-37 trigger, now crossed) is done before MV-3 or as its first work package.
