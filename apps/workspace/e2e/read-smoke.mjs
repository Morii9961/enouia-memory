// Production Explorer, Sessions and Overlay with synthetic deferred clients.
// No Vault, model, native fault command, or third-party page is involved.
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [exe, fixture, out] = process.argv.slice(2);
if (!out) throw new Error("usage: read-smoke.mjs <exe> <fixture.js> <out-dir>");
mkdirSync(out, { recursive: true });
const report = { provenance: "production Explorer, Sessions and Overlay with synthetic deferred clients", checks: [] };
const check = (name, ok) => { report.checks.push({ name, ok: Boolean(ok) }); console.log(`${ok ? "PASS" : "FAIL"} ${name}`); };
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const port = 9397;
let ws;
const app = spawn(exe, [], {
  windowsHide: true, stdio: "ignore",
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
const appError = new Promise((_, reject) => app.once("error", reject));
let evaluate;
try {
  let target;
  for (let i = 0; i < 100 && !target; i++) {
    target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json())
      .then((pages) => pages.find((p) => p.type === "page" && p.url.startsWith("http://tauri.localhost/") && !p.url.includes("overlay")))
      .catch(() => null);
    if (!target) await Promise.race([sleep(150), appError]);
  }
  if (!target) throw new Error("No owned app page");
  ws = new WebSocket(target.webSocketDebuggerUrl);
  const ready = new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let id = 0;
  const pending = new Map();
  ws.onmessage = ({ data }) => { const message = JSON.parse(data); pending.get(message.id)?.(message); pending.delete(message.id); };
  await ready;
  evaluate = async (expression) => {
    const requestId = ++id;
    const response = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => { pending.delete(requestId); reject(new Error("Debugger request timed out")); }, 15000);
      pending.set(requestId, (message) => { clearTimeout(timeout); resolve(message); });
    });
    ws.send(JSON.stringify({ id: requestId, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
    const message = await response;
    if (message.error || message.result?.exceptionDetails) throw new Error(JSON.stringify(message.error ?? message.result.exceptionDetails));
    return message.result?.result?.value;
  };
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) { if (await evaluate(expression)) return; await sleep(30); }
    throw new Error(`Fixture wait failed: ${expression}`);
  };
  await wait("document.readyState === 'complete' && !!document.querySelector('#root > *')");
  await evaluate(readFileSync(fixture, "utf8"));
  await evaluate(`window.__unmountReads = MV6ReadTest.mount();
    window.__readQuery = (text) => {
      const input = document.querySelector('#read-fixture #mq');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, text);
      input.dispatchEvent(new Event('input', {bubbles:true}));
    };
    window.__readText = () => document.querySelector('#read-fixture').innerText;`);
  await wait("MV6ReadTest.requests().length === 1");
  await evaluate("MV6ReadTest.respond(0, '')");
  const submit = async (query) => {
    const count = await evaluate("MV6ReadTest.requests().length");
    await evaluate(`__readQuery(${JSON.stringify(query)})`);
    await sleep(30);
    await evaluate("document.querySelector('#read-fixture form').requestSubmit()");
    await wait(`MV6ReadTest.requests().length === ${count + 1}`);
    return count;
  };
  const textIncludes = (text) => `__readText().includes(${JSON.stringify(text)})`;

  const old = await submit("older search");
  const latest = await submit("latest search");
  await evaluate(`MV6ReadTest.respond(${latest}, 'latest result', 'latest-page')`);
  await wait(textIncludes("latest result"));
  await evaluate(`MV6ReadTest.respond(${old}, 'obsolete result', 'obsolete-page')`);
  await sleep(100);
  check("latest_result_survives_old_success", await evaluate(`${textIncludes("latest result")} && !${textIncludes("obsolete result")}`));

  const failing = await submit("older failure");
  const successful = await submit("latest success");
  await evaluate(`MV6ReadTest.respond(${successful}, 'current successful result', 'stable-page')`);
  await wait(textIncludes("current successful result"));
  await evaluate(`MV6ReadTest.fail(${failing})`);
  await sleep(100);
  check("obsolete_failure_not_shown", await evaluate("!document.querySelector('#read-fixture [role=alert]')"));

  const olderBusy = await submit("older pending");
  const currentBusy = await submit("current pending");
  await evaluate(`MV6ReadTest.respond(${olderBusy}, 'obsolete busy result', 'obsolete-page')`);
  await sleep(100);
  check("latest_read_remains_busy", await evaluate("[...document.querySelectorAll('#read-fixture button')].find((b) => b.textContent === '加载更多')?.disabled === true"));
  await evaluate(`MV6ReadTest.respond(${currentBusy}, 'current paged result', 'current-page')`);
  await wait(textIncludes("current paged result"));

  const pageIndex = await evaluate("MV6ReadTest.requests().length");
  await evaluate("__readQuery('unsubmitted draft')");
  await sleep(30);
  await evaluate("[...document.querySelectorAll('#read-fixture button')].find((b) => b.textContent === '加载更多').click()");
  await wait(`MV6ReadTest.requests().length === ${pageIndex + 1}`);
  const pagination = await evaluate(`MV6ReadTest.requests()[${pageIndex}]`);
  check("pagination_uses_submitted_query", pagination.command === "memory_search" && pagination.args.query === "current pending" && pagination.args.cursor === "current-page");
  await evaluate(`MV6ReadTest.respond(${pageIndex}, 'next page')`);
  await wait(textIncludes("next page"));
  check("pagination_appends_current_page", await evaluate(`${textIncludes("current paged result")} && ${textIncludes("next page")}`));

  const filterIndex = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#read-fixture input[type=checkbox]').click()");
  await wait(`MV6ReadTest.requests().length === ${filterIndex + 1}`);
  const filter = await evaluate(`MV6ReadTest.requests()[${filterIndex}]`);
  check("history_filter_retains_submitted_query", filter.command === "memory_search" && filter.args.query === "current pending" && filter.args.includeHistorical === true && filter.args.cursor === null);
  await evaluate(`MV6ReadTest.respond(${filterIndex}, 'filtered result')`);
  await wait(textIncludes("filtered result"));
  const empty = await submit("missing result");
  await evaluate(`MV6ReadTest.respond(${empty}, '')`);
  await wait("__readText().includes('没有可见的已批准记忆。')");
  check("empty_result_clears_previous_rows", await evaluate(`!${textIncludes("filtered result")}`));

  const retryIndex = await submit("retry submitted query");
  await evaluate(`MV6ReadTest.fail(${retryIndex})`);
  await wait("!!document.querySelector('#read-fixture [role=alert] button')");
  await evaluate("__readQuery('retry unsubmitted draft')");
  await sleep(30);
  await evaluate("document.querySelector('#read-fixture [role=alert] button').click()");
  await wait(`MV6ReadTest.requests().length === ${retryIndex + 2}`);
  const retried = await evaluate(`MV6ReadTest.requests()[${retryIndex + 1}]`);
  check("latest_retry_uses_original_query", retried.command === "memory_search" && retried.args.query === "retry submitted query" && retried.args.cursor === null);
  await evaluate(`MV6ReadTest.respond(${retryIndex + 1}, 'retry recovered result')`);
  await wait(textIncludes("retry recovered result"));
  check("latest_retry_recovers", await evaluate("!document.querySelector('#read-fixture [role=alert]') && document.querySelector('#read-fixture [aria-busy]').getAttribute('aria-busy') === 'false'"));
  await evaluate("window.__unmountReads()");

  // Sessions use deferred synthetic responses too. No real session write
  // occurs here; the separate real-app run verifies actual IPC/Core writes.
  await evaluate(`window.__unmountSessions = MV6ReadTest.mountSessions();
    window.__sessionText = () => document.querySelector('#session-fixture').innerText;
    window.__sessionSet = (selector, value) => {
      const input = document.querySelector('#session-fixture ' + selector);
      const prototype = input.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, value);
      input.dispatchEvent(new Event('input', {bubbles:true}));
    };`);
  await wait("MV6ReadTest.requests().length === 1");
  await evaluate(`MV6ReadTest.respondValue(0, {items:[
    {sessionId:'synthetic-alpha',updatedAt:'synthetic',branches:[{branchId:'brnAlpha001',lastEventSeq:1}]},
    {sessionId:'synthetic-beta',updatedAt:'synthetic',branches:[{branchId:'brnBeta0001',lastEventSeq:1}]}
  ]})`);
  await wait("document.querySelectorAll('#session-fixture .list button').length === 2");
  const selectSession = async (name) => {
    const count = await evaluate("MV6ReadTest.requests().length");
    await evaluate(`[...document.querySelectorAll('#session-fixture .list button')].find((b) => b.textContent.includes(${JSON.stringify(name)})).click()`);
    await wait(`MV6ReadTest.requests().length === ${count + 1}`);
    return count;
  };
  const selectedIs = (name) => `[...document.querySelectorAll('#session-fixture .list button')].find((b) => b.getAttribute('aria-current') === 'true')?.textContent.includes(${JSON.stringify(name)})`;
  const sessionContains = (text) => `__sessionText().includes(${JSON.stringify(text)})`;
  const delayedAlpha = await selectSession("Alpha001");
  const selectedBeta = await selectSession("Beta0001");
  await evaluate(`MV6ReadTest.respondSession(${selectedBeta}, 'beta transcript')`);
  await wait(sessionContains("beta transcript"));
  await evaluate(`MV6ReadTest.respondSession(${delayedAlpha}, 'obsolete alpha transcript')`);
  await sleep(100);
  check("session_latest_detail_matches_selected_branch", await evaluate(`${selectedIs("Beta0001")} && ${sessionContains("beta transcript")} && !${sessionContains("obsolete alpha transcript")}`));

  const loadingAlpha = await selectSession("Alpha001");
  check("session_selection_changes_atomically_with_detail", await evaluate(selectedIs("Beta0001")));
  await evaluate(`MV6ReadTest.respondSession(${loadingAlpha}, 'alpha transcript')`);
  await wait(sessionContains("alpha transcript"));
  await evaluate("__sessionSet('#ask', 'Draft Alpha'); __sessionSet('#cp', 'Summary Alpha')");
  await sleep(30);
  const draftBeta = await selectSession("Beta0001");
  await evaluate(`MV6ReadTest.respondSession(${draftBeta}, 'beta transcript')`);
  await wait(sessionContains("beta transcript"));
  check("session_drafts_do_not_cross_branches", await evaluate("document.querySelector('#session-fixture #ask').value === '' && document.querySelector('#session-fixture #cp').value === ''"));
  await evaluate("__sessionSet('#ask', 'Draft Beta'); __sessionSet('#cp', 'Summary Beta')");
  await sleep(30);
  const restoreAlpha = await selectSession("Alpha001");
  await evaluate(`MV6ReadTest.respondSession(${restoreAlpha}, 'alpha transcript')`);
  await wait(sessionContains("alpha transcript"));
  check("session_question_draft_restored", await evaluate("document.querySelector('#session-fixture #ask').value === 'Draft Alpha'"));
  check("session_checkpoint_draft_restored", await evaluate("document.querySelector('#session-fixture #cp').value === 'Summary Alpha'"));

  const answerBeta = await selectSession("Beta0001");
  await evaluate(`MV6ReadTest.respondSession(${answerBeta}, 'beta transcript')`);
  await wait(sessionContains("beta transcript"));
  await evaluate("__sessionSet('#ask', 'Send Beta')");
  await sleep(30);
  const askIndex = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#session-fixture #ask').closest('form').requestSubmit()");
  await wait(`MV6ReadTest.requests().length === ${askIndex + 1}`);
  const askRequest = await evaluate(`MV6ReadTest.requests()[${askIndex}]`);
  check("session_write_keeps_selected_branch_and_key", askRequest.command === "session_ask" && askRequest.args.branchId === "brnBeta0001" && askRequest.args.text === "Send Beta" && typeof askRequest.key === "string");
  await evaluate(`MV6ReadTest.respondValue(${askIndex}, {status:'synthetic_completed',statements:['answer for beta'],sources:[],capsuleId:'synthetic-capsule'})`);
  await wait(`MV6ReadTest.requests().length === ${askIndex + 2}`);
  await evaluate(`MV6ReadTest.respondSession(${askIndex + 1}, 'beta after answer')`);
  await wait(sessionContains("beta after answer"));
  const afterAnswerAlpha = await selectSession("Alpha001");
  await evaluate(`MV6ReadTest.respondSession(${afterAnswerAlpha}, 'alpha transcript')`);
  await wait(sessionContains("alpha transcript"));
  check("session_answer_does_not_cross_branches", await evaluate(`!${sessionContains("answer for beta")}`));

  const oldFailure = await selectSession("Alpha001");
  const newSuccess = await selectSession("Beta0001");
  await evaluate(`MV6ReadTest.respondSession(${newSuccess}, 'beta newest transcript')`);
  await wait(sessionContains("beta newest transcript"));
  await evaluate(`MV6ReadTest.fail(${oldFailure})`);
  await sleep(100);
  check("session_obsolete_failure_not_shown", await evaluate("!document.querySelector('#session-fixture [role=alert]')"));

  await evaluate("__sessionSet('#ask', 'Persist Beta')");
  await sleep(30);
  const writingIndex = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#session-fixture #ask').closest('form').requestSubmit()");
  await wait(`MV6ReadTest.requests().length === ${writingIndex + 1}`);
  await evaluate("document.querySelector('#session-fixture .list button').click()");
  await sleep(100);
  const duringWriteCount = await evaluate("MV6ReadTest.requests().length");
  check("session_cannot_switch_during_write", duringWriteCount === writingIndex + 1 && await evaluate(selectedIs("Beta0001")));
  if (duringWriteCount > writingIndex + 1) await evaluate(`MV6ReadTest.respondSession(${writingIndex + 1}, 'alpha during write')`);
  await evaluate(`MV6ReadTest.respondValue(${writingIndex}, {status:'synthetic_completed',statements:['persisted beta answer'],sources:[],capsuleId:'synthetic-capsule'})`);
  await wait(`MV6ReadTest.requests().length === ${duringWriteCount + 1}`);
  await evaluate(`MV6ReadTest.respondSession(${duringWriteCount}, 'beta persisted transcript')`);
  await wait(sessionContains("beta persisted transcript"));
  check("session_acknowledged_write_refreshes_same_branch", await evaluate(selectedIs("Beta0001")));

  await evaluate("__sessionSet('#ask', 'Retry original Beta')");
  await sleep(30);
  const failedAsk = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#session-fixture #ask').closest('form').requestSubmit()");
  await wait(`MV6ReadTest.requests().length === ${failedAsk + 1}`);
  await evaluate(`MV6ReadTest.fail(${failedAsk})`);
  await wait("!!document.querySelector('#session-fixture [role=alert] button')");
  await evaluate("__sessionSet('#ask', 'New unsent Beta')");
  await sleep(30);
  await evaluate("document.querySelector('#session-fixture [role=alert] button').click()");
  await wait(`MV6ReadTest.requests().length === ${failedAsk + 2}`);
  const askAttempts = await evaluate(`MV6ReadTest.requests().slice(${failedAsk}, ${failedAsk + 2})`);
  check("session_retry_keeps_original_payload_and_key", askAttempts[0].key === askAttempts[1].key && askAttempts[1].args.text === "Retry original Beta" && askAttempts[1].args.branchId === "brnBeta0001");
  check("session_retry_blocks_branch_switch", await evaluate("[...document.querySelectorAll('#session-fixture .list button')].every((b) => b.disabled)"));
  await evaluate(`MV6ReadTest.respondValue(${failedAsk + 1}, {status:'synthetic_completed',statements:['retried original beta'],sources:[],capsuleId:'synthetic-capsule'})`);
  await wait(`MV6ReadTest.requests().length === ${failedAsk + 3}`);
  await evaluate(`MV6ReadTest.respondSession(${failedAsk + 2}, 'beta after retry')`);
  await wait(sessionContains("beta after retry"));
  check("session_retry_preserves_new_unsent_question", await evaluate("document.querySelector('#session-fixture #ask').value === 'New unsent Beta'"));

  await evaluate("__sessionSet('#cp', 'Original checkpoint Beta')");
  await sleep(30);
  const failedCheckpoint = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#session-fixture #cp').closest('form').requestSubmit()");
  await wait(`MV6ReadTest.requests().length === ${failedCheckpoint + 1}`);
  await evaluate(`MV6ReadTest.fail(${failedCheckpoint})`);
  await wait("!!document.querySelector('#session-fixture [role=alert] button')");
  await evaluate("__sessionSet('#cp', 'New unsent checkpoint Beta')");
  await sleep(30);
  await evaluate("document.querySelector('#session-fixture [role=alert] button').click()");
  await wait(`MV6ReadTest.requests().length === ${failedCheckpoint + 2}`);
  const checkpointAttempts = await evaluate(`MV6ReadTest.requests().slice(${failedCheckpoint}, ${failedCheckpoint + 2})`);
  check("session_checkpoint_retry_keeps_payload_and_key", checkpointAttempts[0].key === checkpointAttempts[1].key && checkpointAttempts[1].args.summary === "Original checkpoint Beta" && checkpointAttempts[1].args.branchId === "brnBeta0001");
  await evaluate(`MV6ReadTest.respondValue(${failedCheckpoint + 1}, {})`);
  await wait(`MV6ReadTest.requests().length === ${failedCheckpoint + 3}`);
  await evaluate(`MV6ReadTest.respondSession(${failedCheckpoint + 2}, 'beta after checkpoint')`);
  await wait(sessionContains("beta after checkpoint"));
  check("session_retry_preserves_new_unsent_checkpoint", await evaluate("document.querySelector('#session-fixture #cp').value === 'New unsent checkpoint Beta'"));
  await evaluate("window.__unmountSessions()");

  await evaluate(`window.__unmountOverlay = MV6ReadTest.mountOverlay();
    window.__overlayText = () => document.querySelector('#overlay-fixture').innerText;
    window.__overlayQuery = (value) => {
      const input = document.querySelector('#overlay-fixture #q');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
      input.dispatchEvent(new Event('input', {bubbles:true}));
    };`);
  await wait("!!document.querySelector('#overlay-fixture #q')");
  const submitOverlay = async (query) => {
    const count = await evaluate("MV6ReadTest.requests().length");
    await evaluate(`__overlayQuery(${JSON.stringify(query)})`);
    await sleep(30);
    await evaluate("document.querySelector('#overlay-fixture form').requestSubmit()");
    await wait(`MV6ReadTest.requests().length === ${count + 1}`);
    return count;
  };
  const overlayContains = (text) => `__overlayText().includes(${JSON.stringify(text)})`;
  const oldOverlay = await submitOverlay("old overlay query");
  const latestOverlay = await submitOverlay("latest overlay query");
  await evaluate(`MV6ReadTest.respond(${latestOverlay}, 'current overlay result')`);
  await wait(overlayContains("current overlay result"));
  await evaluate(`MV6ReadTest.respond(${oldOverlay}, 'old overlay result')`);
  await sleep(100);
  check("overlay_latest_success_wins", await evaluate(`${overlayContains("current overlay result")} && !${overlayContains("old overlay result")}`));
  const oldOverlayFailure = await submitOverlay("old overlay failure");
  const nextOverlay = await submitOverlay("new overlay success");
  await evaluate(`MV6ReadTest.respond(${nextOverlay}, 'new overlay result')`);
  await wait(overlayContains("new overlay result"));
  await evaluate(`MV6ReadTest.fail(${oldOverlayFailure})`);
  await sleep(100);
  check("overlay_obsolete_error_not_shown", await evaluate("!document.querySelector('#overlay-fixture [role=alert]')"));
  check("overlay_obsolete_error_does_not_clear_results", await evaluate(overlayContains("new overlay result")));
  const erasedOverlay = await submitOverlay("pending erased query");
  await evaluate("__overlayQuery('')");
  await sleep(30);
  await evaluate(`MV6ReadTest.respond(${erasedOverlay}, 'erased overlay response')`);
  await sleep(100);
  check("overlay_erasing_query_invalidates_pending_result", await evaluate("document.querySelector('#overlay-fixture .results').children.length === 0"));
  const beforeEmptyOverlay = await evaluate("MV6ReadTest.requests().length");
  await evaluate("document.querySelector('#overlay-fixture form').requestSubmit()");
  await sleep(50);
  check("overlay_empty_query_no_request_or_results", await evaluate(`MV6ReadTest.requests().length === ${beforeEmptyOverlay} && document.querySelector('#overlay-fixture .results').children.length === 0`));
  const oldOverlayBusy = await submitOverlay("old overlay busy");
  const latestOverlayBusy = await submitOverlay("latest overlay busy");
  await evaluate(`MV6ReadTest.respond(${oldOverlayBusy}, 'old overlay busy result')`);
  await sleep(50);
  check("overlay_latest_read_remains_busy", await evaluate("document.querySelector('#overlay-fixture main').getAttribute('aria-busy') === 'true'"));
  await evaluate(`MV6ReadTest.respond(${latestOverlayBusy}, 'ready overlay result')`);
  await wait(overlayContains("ready overlay result"));
  check("overlay_latest_completion_clears_busy", await evaluate("document.querySelector('#overlay-fixture main').getAttribute('aria-busy') === 'false'"));
  await evaluate("window.dispatchEvent(new Event('focus'))");
  await sleep(30);
  check("overlay_reopen_clears_previous_query_and_results", await evaluate("document.querySelector('#overlay-fixture #q').value === '' && document.querySelector('#overlay-fixture .results').children.length === 0"));
  const blurredOverlay = await submitOverlay("blurred overlay query");
  await evaluate("window.dispatchEvent(new Event('blur'))");
  await sleep(30);
  await evaluate(`MV6ReadTest.respond(${blurredOverlay}, 'blurred overlay response')`);
  await sleep(50);
  check("overlay_blur_invalidates_pending_read", await evaluate("document.querySelector('#overlay-fixture #q').value === '' && document.querySelector('#overlay-fixture .results').children.length === 0"));
  const retryOverlay = await submitOverlay("retry overlay query");
  await evaluate(`MV6ReadTest.fail(${retryOverlay})`);
  await wait("!!document.querySelector('#overlay-fixture [role=alert]')");
  const hasOverlayRetry = await evaluate("!!document.querySelector('#overlay-fixture [role=alert] button')");
  check("overlay_current_error_has_retry", hasOverlayRetry);
  if (hasOverlayRetry) {
    await evaluate("__overlayQuery('unsubmitted overlay draft')");
    await sleep(30);
    await evaluate("document.querySelector('#overlay-fixture [role=alert] button').click()");
    await wait(`MV6ReadTest.requests().length === ${retryOverlay + 2}`);
    const retriedOverlay = await evaluate(`MV6ReadTest.requests()[${retryOverlay + 1}]`);
    check("overlay_retry_keeps_submitted_query", retriedOverlay.args.query === "retry overlay query");
    await evaluate(`MV6ReadTest.respond(${retryOverlay + 1}, 'overlay retry recovered')`);
    await wait(overlayContains("overlay retry recovered"));
    check("overlay_retry_recovers", await evaluate("!document.querySelector('#overlay-fixture [role=alert]') && document.querySelector('#overlay-fixture main').getAttribute('aria-busy') === 'false'"));
  } else {
    check("overlay_retry_keeps_submitted_query", false);
    check("overlay_retry_recovers", false);
  }
  await evaluate("window.__unmountOverlay()");
} finally {
  ws?.close();
  app.kill();
  writeFileSync(join(out, "report.json"), JSON.stringify(report, null, 2));
}
if (report.checks.length !== 36 || report.checks.some((item) => !item.ok)) process.exitCode = 1;
