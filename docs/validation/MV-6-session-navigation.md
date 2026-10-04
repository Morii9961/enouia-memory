# MV-6 session navigation, drafts and in-flight writes

Date: 2026-10-04. Base: `b39dbd2`. Scope: Windows session UI, with the existing Core/Mock contract. MV-7 remains unstarted.

## Reproduced failures

The production Sessions page was mounted in the independent synthetic deferred-client fixture. The baseline passed only **1/10** navigation checks: the actual write targeted its captured branch/key, but an older detail could replace the selected branch's transcript, selection changed before the matching detail arrived, question/checkpoint drafts crossed branches, answers crossed branches, stale errors reappeared, and a branch could change while its write was pending. After addressing navigation, two further tests reproduced retry clearing a newly edited, unsent question or checkpoint.

## Changes

Session list/detail reads use the same latest-generation rule as Explorer. Selection and matching detail publish together. While a detail is loading, the previous branch remains selected and its inputs are disabled; a new selection may supersede that read. The current question and checkpoint drafts are kept separately for each session/branch in component-local memory and restored on return. They are not claimed to be persisted across page exit, app exit, or lock. Changing branches clears the previous answer and write retry UI.

Write attempts retain the existing original action, payload and idempotency key. Branch switching and new writes are blocked until the attempt and its same-branch detail refresh finish, including on retry. Only a draft still matching the acknowledged submitted text is cleared; a newer unsent draft typed after failure is preserved. New session creation waits for its list and detail refresh instead of launching nested busy states independently. No Core authorization, schema, provider or native permission changed.

## Verified

- **15/15** synthetic session checks passed: latest matching detail; atomic selection; branch-specific drafts and restoration; branch/key binding; no cross-branch answer/error; no switch during write; same-branch refresh; original retry payload/key; retry switch guard; and preservation of newer question/checkpoint drafts. The shared fixture also reran the existing **9/9** Explorer checks, totaling **24/24**.
- Frontend and separate read/retry fixture typechecks/builds passed. The actual rebuilt 0.1.0 application passed **43/43** checks on a new synthetic Vault, including actual Core session creation, Mock answering, saved transcript/context, original write-key reuse, and acknowledged-record recovery after an actual renderer crash.
- The final rebuilt installer passed **8/8** fresh installation/uninstallation checks; its ownership template passed **14/14** checks. Current-day pinned offline format, **196 Rust tests**, Clippy and independent **34-schema** checks remain applicable to unchanged Core/Rust sources.

The deferred session client simulates responses, including writes; it does not establish real persistence. Persistence evidence comes from the separate actual IPC/Core run. Fixture bundles and markers are absent from the production frontend; no fixture API or additional native permission is exposed. All test inputs are synthetic and local evidence remains ignored.

## Limits

This is local Mock/UI acceptance. It does not establish real-provider calls, multi-client Host authentication, manual Narrator/login/interactive upgrade acceptance, code signing, OS-crash or power-loss recovery. Other stage gaps remain pending.
