# ADR-MEM-156 — Morii-opened capture windows for daily Runtime capture

Date: 2026-11-21. Status: Accepted (Morii authorized R4.3 item B); synthetic validation.

## Context

Runtime capture on an Owner root (ADR-MEM-102/111/112/114) needs a
`RuntimeCaptureScope` naming one formation policy and the source IDs that policy
already lists (at most `MAX_SOURCES`, 128). Captured turns then form under that
policy while it is active and inside its window. The real Vault only has the
fresh-initialization policies. Their source IDs are all used and they end on
2026-11-24, so daily capture and formation had no policy to use, and Runtime
would have had to build policies by hand.

## Decision

Add Morii-only `SegmentedAutomationVault::open_capture_window(owner, days,
max_sensitivity, allowed_forms)` and `capture_windows(owner)`, exposed through
`NativeMemoryOwner`.

`open_capture_window` commits one ordinary formation policy:

- Morii's own subject and no project;
- `allow_create` true, no revision targets, `allow_revise_created` false;
- `MAX_SOURCES` freshly generated source IDs, and fresh access and egress IDs;
- valid from now for `days` (1 to 90);
- the given forms and sensitivity ceiling.

It uses the existing policy commit and validation, so no new delta or format is
added. It refuses non-Owner layouts, a missing Morii subject, an out-of-range day
count and an empty form list.

`capture_windows` lists active, in-window Morii-subject create-only policies
that still have reserved source IDs not yet captured or deleted, with those
unused IDs in policy order. Runtime picks a window with enough unused IDs and
time left, and passes the unused IDs as the capture scope. When none is left, it
opens a new window. Exhausted or expired windows simply stop being listed.

## Defaults agreed for R4.3

- Windows of 30 days.
- Ceiling `Private` while formation uses an external model, following Morii's
  default that external models do not get HighlySensitive material.
- Forms chosen by Runtime from what Morii enables.

The local Mock analyzer may use a higher ceiling only on a synthetic root.

## Consequences and limits

- Each window adds one policy. The Vault allows 1024 policies, which leaves room
  for several years of daily windows at 128 turns each. Consolidation is a later
  slice if needed.
- Real-root formation needs a `CaptureAnalyzer`. Under ADR-MEM-157 it is the
  `SubmittedAnalyzer`, which forms exactly what ChatGPT submits; R4.3 item C
  (Mock or Provider analyzer) is cancelled.
- No persisted format, IPC command or wire change. `NativeMemoryOwner` gains two
  methods, so the Runtime surface digest changes; Workspace stays at 61 commands.

## Verification

Synthetic Owner root:

- Refused: 0 or 91 days, no forms, a System actor and a locked owner.
- A 30-day window has 128 IDs, Morii's subject and create-only scope.
- The existing owner Runtime capture accepts the window's policy and IDs. The
  unused list then shrinks by exactly the captured sources.
- After 31 days no window is listed.
