# MV-6 Core — complete confirmation result replay

Date: 2026-10-06. Base: `9c14cb9`. End: the feature commit containing this report. Scope: the existing Core owner `review_confirm` route, including its file-purge completion. Wire/API/schema/dependencies/persistent hashes and store writer protocol are unchanged. MV-7 remains unopened.

## Reproduction and correction

Two page calls confirmed the same synthetic purge plan with distinct transport IDs. The first paused after removing files, before receipt completion; the second had an opportunity to enter confirmation before the first complete result was cached. Both answered the same commit, review, delete and purge receipt IDs, but their `filesRemoved` values were 3 and 0. The result-cache race allowed the second call to repeat post-commit effects and overwrite the original complete result with different counters. The regression failed on the prior Core.

Core now holds its existing confirmation-result mutex from cache lookup through domain confirmation, optional file purge, plan retirement and result publication. Confirmation calls are serialized within one Core, including different plans. A concurrent retry sees the first successful complete cached result. Retryable failures still retain the plan, and unsuccessful calls release the mutex normally; subsequent attempts remain eligible. Wrong diff hashes, open-Vault requirements and lifecycle cleanup keep their existing checks. No durable result cache or new error rule is introduced.

## Evidence

`concurrent_purge_confirm_replays_one_complete_result` uses a synthetic canonical memory and an actual page purge plan. The injected ID source pauses receipt allocation after the first file purge. Both envelopes validate, return the same complete result including the positive removal count, and publish only one purge receipt. A subsequent retry returns that result without head movement. While the first confirmation is paused, the page status command remains available and reports the open Vault. The controller always releases both injected waits before assertions, including the unguarded negative path.

The fixed focused regression passed, and `a_busy_confirm_keeps_its_plan_for_the_retry` passed after the lock change, retaining the existing retryable storage-contention behavior.

The owner's immediate shutdown request interrupted the first final suite; `bdeb6c8` saved the work as WIP. Verification resumed on 2026-10-07 without a production source change. All required root checks passed with the pinned offline toolchain: formatting, 269 workspace Rust tests (58 Core tests, including the status assertion), and all-targets Clippy with warnings denied. Python 3.12 / python-jsonschema independently passed all 34 schemas and their fixture corpora. The regenerated, logged Runtime aggregate is `04ca8e62840b65dc7b5c239245ee77641d26be9ee2b878668681cdb840ae20b8`; the surface checker and `git diff --check` passed. This completes the previously interrupted slice.

The branch was then integrated with main through `3b511b7`, retaining main's early-cancel resume, reference frontend and installer fixes. The combined revision passed all required root checks again: 270 workspace Rust tests (59 Core tests), formatting, all-targets Clippy, the independent 34-schema cross-check and the logged Runtime surface check. The combined aggregate is `22c85e95c3acf1151751c03a383113d2415d4fe882e2b976da0fd8c3976e59a2`. The reference frontend build and both fixture/read harness builds passed. Installer template checks passed 11/11; isolated NSIS drills passed locked-file handling 2/2, startup cleanup 4/4 and version guards 7/7, with their non-startup test registry keys removed. These build and synthetic macro checks do not establish interactive UI/installation or Runtime adoption.

## Closing during the post-removal gap

An acceptance-only follow-up at base `946849a` composes the confirmation and lifecycle guards. `closing_during_purge_waits_for_the_complete_response_and_expires_replay` pauses a real synthetic purge after file removal, before receipt allocation, then starts page `vault_lock` or native Core `shutdown()`. Neither close completes during the pause, and a separate page status call still observes the open Vault. The controller releases the injected wait before assertions, including timeout paths.

In both schedules, confirmation returns a complete positive removal result with its response commit binding before close finishes. Closed-Vault replay is refused; reopening preserves exactly the returned purge receipt and a clean verified Vault, but expires the old confirmation plan/cache. A stale retry returns `workspace.plan_unknown` without head movement. The focused test and final root checks passed: 271 workspace Rust tests (60 Core tests), formatting, all-targets Clippy, the independent 34-schema cross-check and the logged surface checker. Production source, wire/API/storage format and dependencies are unchanged. The recorded aggregate for this acceptance revision is `ef2282cd0abf8aeb5598fda80ec96df5c372bd9b269e3f729a73540494582e77`.

## Runtime adoption and limits

Bump the pin. Keep confirmation work off Runtime's native event thread; pending confirmations may now wait behind another plan's completion. Continue to sequence owner actions, use complete results as the operation outcome and refresh target/review state when another plan makes it stale. Status and operation observation remain separate paths. This is synthetic local same-plan result evidence, not a complete proof of every different-plan race, general mutex-poison recovery, durable process-restart caching, actual Runtime responsiveness, real-data purging, backup destruction or OS/power-loss durability.
