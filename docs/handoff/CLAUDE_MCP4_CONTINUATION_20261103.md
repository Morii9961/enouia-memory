# Claude MCP-4 continuation log

2026-11-03, Asia/Shanghai. Running log for the Claude session that took over
from [the Codex handoff](CLAUDE_MCP4_HANDOFF_20261103.md). Updated per slice so
a later session can resume without this conversation.

Branch `codex/captured-source-lifecycle`, draft PR #83 (not merged). Morii asked
to keep going slice by slice in the handoff's order until quota runs out; no
merge, deployment, real model, private root, Runtime change or MCP-5/new MV stage.

## Delivered

| Commit | Slice | Evidence |
| --- | --- | --- |
| `5f6c147` | ADR121 durable correction reports, `report_saved` original-key receipts, replay after target revision | [report](../validation/MCP-4-agent-correction-intake.md): final-tree full suite 909 passed / 0 failed / 4 ignored, strict Clippy, 67 schemas |
| `f42f3b1` | ADR122 System consumption: CorrectionAnalyzer port, `step_corrections`, opt-in `start_worker_with_corrections`; bound-target Stale no longer livelocks | [report](../validation/MCP-4-correction-consumption.md): full suite 913 passed / 0 failed / 4 ignored |

## Next, in order

1. ~~System consumption~~ (ADR122).
2. Explicit MCP service / versioned transport for correction reports.
3. `memory_supersede_report`, then `memory_forget_request` (record only;
   forget-hide still awaits Morii's decision, do not implement hiding).

## Notes for whoever resumes

- `automatic_loop::continuation_cases::revocation_cancels_inflight_...` failed
  once under full-suite load (OutcomeUnknown vs Revoked) on a pre-final tree and
  passed in isolation and in the final run. Old continuation path; not touched.
- `cargo test --workspace` stops at the first failing target; use
  `--no-fail-fast` when counting.
- After any Rust edit under the surface globs, rerun
  `python tools/integration/runtime_surface.py --write` and update RUNTIME.md.

## Stopped

2026-11-04: Morii asked to stop after ADR122, merge PR #83 and hand over to Codex.
Current handoff: [CODEX_MCP4_HANDOFF_20261104](CODEX_MCP4_HANDOFF_20261104.md).
