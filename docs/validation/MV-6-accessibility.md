# MV-6 keyboard and accessibility follow-up

Date: 2026-10-03. Synthetic real-app acceptance: **39/39 checks passed**. The release application's frontend matches the checked source; the subsequent installer retry is tracked separately.

The plan dialog focuses its heading, exposes both its accessible name and description, keeps Tab/Shift+Tab inside its controls, blocks page-switch shortcuts, and prevents cancellation while confirmation is in progress. Before preparing an asynchronous plan, the initiating control is retained explicitly: disabling the button during preparation can otherwise blur it before the dialog opens. Esc returns focus to that control without an unnecessary reload. After a successful review removes the candidate's button, focus moves to the review page heading. The same origin capture is used for forget/purge plans.

The expanded test exposed and then reproduced fixes for keyboard focus leaving the modal and disappearing after confirmation. Disabling the originating button also exposed the asynchronous origin-capture issue. The final run passed Enter activation, initial focus, accessible name/description, focus containment, page-shortcut exclusion, Esc cancellation/restoration, and post-confirmation focus. The other integration checks, including startup and the deliberate retryable lost-response fixture, passed in the same run.

Styles add visible heading focus, readable line spacing, system colors and borders under forced colors, selected-page contrast, and a single-column detail layout at narrower widths. A 720px viewport passed the horizontal-overflow check. Its screenshot and the forced-colors screenshot were visually inspected. These are browser-emulated conditions in WebView2; they do not establish actual Windows contrast-theme or Narrator acceptance.

Required evidence remains applicable: 196 offline Rust workspace tests, format checking, Clippy with warnings denied, 34 independent schema checks, and the frontend plus separate fixture typechecks/builds passed. Only synthetic text and Vaults were used; evidence artifacts remain ignored.

## Manual acceptance still required

Use a synthetic Vault with the app in the foreground. With Narrator, confirm page labels and shortcut descriptions, focused headings, dialog name/description, readable plan records and confirmation code, busy/disabled controls, retryable error alerts, and the startup checkbox's label/state. Run an actual Windows contrast theme and check the current navigation item, focused control, selected record, disabled buttons, error messages, and the plan dialog. These checks remain pending and are not represented by the automated pass.
