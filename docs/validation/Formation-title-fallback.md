# Formation title fallback (ADR-MEM-137)

2026-11-11. Synthetic data only. This slice follows the ADR136 backup PR; the
combined Runtime aggregate is recorded in `docs/integration/RUNTIME.md`.

## Focused Windows evidence

- `cargo test -p enouia-memory-vault --test formation_title --locked`: passed.
  The ten table cases cover LF, CRLF, leading empty/control spans, Tab, NUL,
  single-line fallback, case-insensitive retention of a supplied title, a Chinese
  quote longer than 60 characters, and continued rejection of invalid supplied
  titles (control characters, empty, too long). Exact content and resolvable quoted
  evidence are preserved.
- `cargo test -p enouia-memory-init --test captured_formation
  multiline_quote_forms_with_a_control_free_fallback_title --locked -- --exact`:
  passed (1 test, 19 filtered). A scripted analyzer returns a title absent from a
  multiline captured turn. The real worker forms an adopted memory with the first
  span as title and retains the complete original content/evidence. No model call.
- `cargo clippy -p enouia-memory-vault --lib --test formation_title --locked --
  -D warnings`: passed. Targeted formatting and Runtime surface check passed.

The two focused Windows tests above were also rerun successfully after integrating
the already verified backup prerequisite. On WSL Ubuntu-24.04, a separate detached worktree at
combined implementation commit `aaa8370` passed the same `formation_title` test
and the Runtime surface checker (aggregate `d9b1f19a…`). The Windows-only worker
test was not run on Linux. No full suite, real data, Provider, Runtime repository
change, production deployment or manual acceptance is claimed.
