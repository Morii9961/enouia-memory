# Existing seed preparation/review backend: mainline integration

2026-10-24. Integrate existing source2f1595bf38a32f43dd2c3f7f24f82b9fdc1a3342
with base main de5168a955854726e624162a05775489a5e72e61.
Only README/compatibility history/generated-manifest conflicts required
resolution. Keep current Provider/MCP and58-command Workspace behavior.
Aggregate:77b7b5aae54bb250266f29b76bae52dbea805f42456535c8696bcc3bdf808e15.

Existing offline seed CLI/preparer code is included with its original checked
input hashes, pending-only staging and explicit package confirmation. Candidate
context dependencies stay separate from evidence; saved evidence classes and
past-episode occurrence are not rewritten into stronger evidence/current validity.
Only serde is added to the CLI's already locked dependency edges.

Actual combined-source checks:
- Existing CLI seed journeys:2 passed; standard-library preparer tests:4 passed.
  Inputs/Vaults are generated temporary synthetic fixtures, including replay,
  changed-input refusal and ordinary recall of a past episode.
- Workspace views:2 passed; existing w01 import/review/session/Core journey:
  1 passed,86 filtered. Actual additive source excerpt evidence class is covered.
- Four integration-surface checks, manifest/log, affected format and strict
  CLI/Workspace lib/tests Clippy passed.
- Reference-shell TypeScript passed using the existing ignored dependency
  junction; no install/build or physical UI was run.
- Local ignored logs:target/seed-backend-integration/cli.txt, preparer.txt,
  views.txt, core.txt, clippy.txt, typescript.txt and surface.txt.
No new tests, independent schema, full workspace/frontend, package, smoke or
performance rerun is claimed. Historical migration reports retain their original
dates and evidence; their personal materials were not opened or revalidated here.

Runtime/Claude follow-up:
Runtime stays on08892702 /57 commands. Its old seed-review-context branch has
a47-command pin and direct MemorySurface.tsx/CandidateSource.tsx changes, and
remains protected/unmerged during frontend redesign. Future explicitly requested
adoption must keep a current exact pin and port only the required UI behavior:
render contextSources as non-citations, preserve missing/empty/unknown classes,
read source IDs/revisions only on explicit action and withdraw late observations.
No shared Runtime App.tsx/main.tsx, shell/style, redesign ref/worktree, manual
status snapshot, personal migration/approval, installation or activation changed.
