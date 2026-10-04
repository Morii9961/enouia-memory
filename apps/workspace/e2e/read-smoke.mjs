// Production Explorer + synthetic deferred reads in an owned WebView2.
// No Vault, model, native fault command, or third-party page is involved.
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [exe, fixture, out] = process.argv.slice(2);
if (!out) throw new Error("usage: read-smoke.mjs <exe> <fixture.js> <out-dir>");
mkdirSync(out, { recursive: true });
const report = { provenance: "production Explorer with synthetic deferred read client", checks: [] };
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
} finally {
  ws?.close();
  app.kill();
  writeFileSync(join(out, "report.json"), JSON.stringify(report, null, 2));
}
if (report.checks.length !== 9 || report.checks.some((item) => !item.ok)) process.exitCode = 1;
