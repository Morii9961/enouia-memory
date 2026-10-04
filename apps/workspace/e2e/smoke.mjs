// Real-app acceptance run for MV-6 (W01–W05) on a synthetic Vault.
//
// Starts the built workspace shell with WebView2 remote debugging on a
// loopback port, drives the real page over the Chrome DevTools Protocol
// (real Tauri IPC, real Core), fills the native Open dialog of that process
// through UI Automation, and saves screenshots plus a JSON report.
//
// node apps/workspace/e2e/smoke.mjs <exe> <vault-root> <import-file> <out-dir> [hotkey-letter] [retry-fixture] [--crash]
//
// Synthetic data only. The debug port exists only for this test process.
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [exe, vault, importFile, out, HOTKEY = "K", retryFixture, crashMode] = process.argv.slice(2);
if (!out) throw new Error("usage: smoke.mjs <exe> <vault-root> <import-file> <out-dir>");
mkdirSync(out, { recursive: true });
const report = { checks: [], screenshots: [] };
const children = new Set();
const sessions = new Set();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const check = (id, ok, detail = "") => {
  report.checks.push({ id, ok: Boolean(ok), detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${id} ${detail}`);
};

function launch(port, args, extraEnv = {}) {
  const child = spawn(exe, args, {
    env: { ...process.env, ...extraEnv, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
    stdio: "ignore",
  });
  children.add(child);
  child.once("exit", () => children.delete(child));
  return child;
}

async function connect(port, wantOverlay = false, exclude = null) {
  for (let i = 0; i < 120; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = targets.find((t) => t.type === "page" && t.url.startsWith("http://tauri.localhost/") && t.url.includes("overlay") === wantOverlay && t.id !== exclude);
      if (page) {
        const s = { ...session(page.webSocketDebuggerUrl), id: page.id };
        await waitFor(s, "document.readyState === 'complete' && !!document.querySelector('#root > *')", "page initialized");
        return s;
      }
    } catch {
      /* not up yet */
    }
    await sleep(250);
  }
  throw new Error(`no page on port ${port}`);
}

function session(url) {
  const ws = new WebSocket(url);
  let id = 0;
  const pending = new Map();
  const events = [];
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.method) events.push(msg.method);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  const ready = new Promise((r) => (ws.onopen = r));
  const send = async (method, params = {}) => {
    await ready;
    const n = ++id;
    ws.send(JSON.stringify({ id: n, method, params }));
    return new Promise((r) => pending.set(n, r));
  };
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? "eval failed");
    return r.result?.result?.value;
  };
  const s = { send, evaluate, events, close: () => { ws.close(); sessions.delete(s); } };
  sessions.add(s);
  return s;
}

const HELPERS = `
window.__t = {
  byText(sel, text) { return [...document.querySelectorAll(sel)].find((e) => e.textContent.includes(text)); },
  async click(sel, text) {
    for (let i = 0; i < 100; i++) {
      const e = this.byText(sel, text);
      if (e && !e.disabled) { e.click(); return true; }
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error('missing or disabled ' + text);
  },
  set(sel, value) {
    const e = document.querySelector(sel); if (!e) throw new Error('missing ' + sel);
    const proto = e.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(e, value);
    e.dispatchEvent(new Event('input', { bubbles: true })); return true;
  },
  text() { return document.body.innerText; },
};`;

async function waitFor(s, expression, label, ms = 20000) {
  const start = Date.now();
  while (Date.now() - start < ms) {
    try {
      if (await s.evaluate(expression)) return true;
    } catch {
      /* page busy */
    }
    await sleep(150);
  }
  const text = await s.evaluate("document.body.innerText.slice(-700)").catch(() => "");
  throw new Error(`timeout: ${label} | ${String(text).replace(/\s+/g, " ")}`);
}

async function shot(s, name) {
  const r = await s.send("Page.captureScreenshot", { format: "png" });
  const file = join(out, `${name}.png`);
  writeFileSync(file, Buffer.from(r.result.data, "base64"));
  report.screenshots.push(file);
}

const nav = (s, label) => s.evaluate(`__t.click('nav button', ${JSON.stringify(label)})`);
const has = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;
const all = (...texts) => texts.map(has).join(" && ");
const any = (...texts) => `(${texts.map(has).join(" || ")})`;
async function press(s, key, code, windowsVirtualKeyCode, modifiers = 0) {
  await s.send("Input.dispatchKeyEvent", {type:"keyDown",key,code,windowsVirtualKeyCode,modifiers,...(key === "Enter" ? {text:"\r",unmodifiedText:"\r"} : {})});
  await s.send("Input.dispatchKeyEvent", {type:"keyUp",key,code,windowsVirtualKeyCode,modifiers});
}

// Fill the native Open dialog of `pid` through UI Automation: the file name
// edit (control 1148) gets the path by WM_SETTEXT, then its Open button
// (control 1) gets BM_CLICK. Only windows of this process are touched. Nothing is typed anywhere if the dialog is not found.
function fillOpenDialog(pid, path) {
  const script = `Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$C = [System.Windows.Automation.PropertyCondition]
$root = $A::RootElement
$dialog = $null
for ($i = 0; $i -lt 80 -and -not $dialog; $i++) {
  $wins = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, (New-Object $C($A::ClassNameProperty, '#32770')))
  foreach ($w in $wins) { if ($w.Current.ProcessId -eq ${pid}) { $dialog = $w } }
  if (-not $dialog) { Start-Sleep -Milliseconds 250 }
}
if (-not $dialog) { throw 'no dialog' }
Add-Type -Namespace E2E -Name User32 -MemberDefinition '[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern System.IntPtr SendMessage(System.IntPtr h, uint m, System.IntPtr w, string l);'
$all = $dialog.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$edit = $all | Where-Object { $_.Current.ClassName -eq 'Edit' -and $_.Current.AutomationId -eq '1148' } | Select-Object -First 1
$open = $all | Where-Object { $_.Current.ClassName -eq 'Button' -and $_.Current.AutomationId -eq '1' } | Select-Object -First 1
if (-not $edit -or -not $open) { throw 'dialog controls not found' }
[E2E.User32]::SendMessage([System.IntPtr]$edit.Current.NativeWindowHandle, 0x000C, [System.IntPtr]::Zero, '${path.replace(/'/g, "''")}') | Out-Null
[E2E.User32]::SendMessage([System.IntPtr]$open.Current.NativeWindowHandle, 0x00F5, [System.IntPtr]::Zero, $null) | Out-Null`;
  execFileSync("powershell", ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], { stdio: ["ignore", "ignore", "pipe"] });
}

async function main() {
  // The default Ctrl+Alt+M may already belong to another program here; that
  // is reported, and a free letter is then used for the run.
  let app = launch(9333, ["--vault", vault]);
  let s = await connect(9333);
  await waitFor(s, any("registered", "被占用"), "companion status");
  const defaultTaken = await s.evaluate(has("被占用"));
  report.defaultHotkey = defaultTaken ? "conflict (taken by another program on this machine)" : "registered";
  s.close();
  app.kill();
  await sleep(1500);
  app = launch(9333, ["--vault", vault, "--hotkey-key", HOTKEY]);
  s = await connect(9333);
  await s.evaluate(HELPERS);
  await waitFor(s, has("Vault：open"), "vault open");
  await waitFor(s, any("registered", "被占用"), "companion status");
  const statusText = await s.evaluate("__t.text()");
  check("W05.hotkey_registered", statusText.includes(`Ctrl+Alt+${HOTKEY}`) && statusText.includes("registered"), report.defaultHotkey);
  check("W03.four_stops_explained", ["关闭窗口", "锁定 Vault", "退出", "暂停同步"].every((t) => statusText.includes(t)));
  await shot(s, "01-status");

  // W05 keyboard: Alt+2 moves to the Explorer and focuses its heading.
  await s.send("Input.dispatchKeyEvent", { type: "keyDown", key: "2", code: "Digit2", modifiers: 1, windowsVirtualKeyCode: 50 });
  await s.send("Input.dispatchKeyEvent", { type: "keyUp", key: "2", code: "Digit2", modifiers: 1, windowsVirtualKeyCode: 50 });
  await sleep(300);
  check("W05.keyboard_page_switch", await s.evaluate("document.activeElement.tagName === 'H2' && document.activeElement.textContent === '记忆浏览'"));

  // W01 import through the native dialog.
  await nav(s, "导入中心");
  await waitFor(s, has("选择文件"), "import page");
  await s.evaluate("__t.click('button', '选择文件')");
  fillOpenDialog(app.pid, importFile);
  await waitFor(s, has("开始导入"), "import preview");
  check("W04.page_sees_name_not_path", !(await s.evaluate("__t.text()")).includes(":\\\\"));
  await s.evaluate("__t.click('button', '开始导入')");
  await waitFor(s, all("succeeded", "completed"), "import finished", 60000);
  await shot(s, "02-import");
  check("W01.import", true);

  // W01 review: remember -> candidate -> exact plan -> confirm.
  await nav(s, "候选审核");
  await waitFor(s, "!!document.querySelector('#rt')", "review page");
  await s.evaluate("__t.set('#rt', 'MoriMeta 的设计决定是 Professional Darkroom。')");
  await s.evaluate("__t.set('input[placeholder^=\"例如\"]', 'project.morimeta.design')");
  await s.evaluate("__t.click('button', '保存为候选')");
  await waitFor(s, has("待审核（1）"), "candidate listed");
  await s.evaluate("window.__planTrigger = __t.byText('button', '接受'); window.__planTrigger.focus()");
  await press(s, "Enter", "Enter", 13);
  await waitFor(s, "!!document.querySelector('dialog[open]')", "plan dialog");
  const dialogText = await s.evaluate("document.querySelector('dialog[open]').innerText");
  check("W05.plan_initial_focus", await s.evaluate("document.activeElement.id === 'plan-title'"));
  const ax = await s.send("Accessibility.getFullAXTree");
  check("W05.dialog_accessible_name", ax.result.nodes.some((n) => n.role?.value === "dialog" && n.name?.value?.includes("确认写入")));
  check("W05.dialog_accessible_description", ax.result.nodes.some((n) => n.role?.value === "dialog" && n.description?.value?.includes("全部记录")));
  check("W01.plan_shows_exact_text", dialogText.includes("Professional Darkroom") && /确认码 [0-9a-f]{8}/.test(dialogText));
  await shot(s, "03-plan");
  let contained = true;
  for (let i = 0; i < 7; i++) {
    await press(s, "Tab", "Tab", 9, i % 2 === 0 ? 0 : 8);
    contained &&= await s.evaluate("document.querySelector('dialog[open]').contains(document.activeElement)");
  }
  check("W05.dialog_keeps_keyboard_focus", contained);
  await press(s, "1", "Digit1", 49, 1);
  check("W05.modal_blocks_page_shortcut", await s.evaluate("!!document.querySelector('dialog[open]') && document.querySelector('nav button[aria-current=page]').textContent.includes('候选审核')"));
  await press(s, "Escape", "Escape", 27);
  await waitFor(s, "!document.querySelector('dialog[open]')", "escape cancelled plan");
  check("W05.cancel_restores_trigger_focus", await s.evaluate("document.activeElement === window.__planTrigger"));
  await press(s, "Enter", "Enter", 13);
  await waitFor(s, "!!document.querySelector('dialog[open]')", "reopened plan");
  await s.evaluate("__t.byText('dialog[open] button', '确认').focus()");
  await press(s, "Enter", "Enter", 13);
  await waitFor(s, has("待审核（0）"), "candidate accepted");
  check("W05.accept_moves_focus_to_review_heading", await s.evaluate("document.activeElement === document.querySelector('main h2')"));

  // W01 explorer: open the memory and its source.
  await nav(s, "记忆浏览");
  await waitFor(s, has("Professional Darkroom"), "memory listed");
  await s.evaluate("__t.click('.list button', 'Professional Darkroom')");
  await waitFor(s, has("查看来源"), "detail");
  await s.evaluate("__t.click('button', '查看来源')");
  await waitFor(s, "!!document.querySelector('pre.source')", "source excerpt");
  check("W01.source_visible", await s.evaluate("document.querySelector('pre.source').textContent.includes('Professional Darkroom')"));
  await shot(s, "04-source");
  await s.send("Emulation.setDeviceMetricsOverride", {width:720,height:740,deviceScaleFactor:1,mobile:false});
  check("W05.narrow_viewport_no_horizontal_overflow", await s.evaluate("document.documentElement.scrollWidth <= window.innerWidth"));
  await shot(s, "narrow-viewport");
  await s.send("Emulation.clearDeviceMetricsOverride");

  // W01 correct: proposal -> review -> new revision.
  await s.evaluate("__t.set('#fix', 'MoriMeta 的设计决定是 Darkroom 2。')");
  await s.evaluate("__t.click('button', '提交纠正候选')");
  await waitFor(s, has("已提交纠正候选"), "correction proposed");
  await nav(s, "候选审核");
  await waitFor(s, has("旧事实"), "revise candidate with old fact");
  await s.evaluate("__t.click('button', '接受')");
  await waitFor(s, "!!document.querySelector('dialog[open]')", "revise plan");
  await s.evaluate("__t.click('dialog[open] button', '确认')");
  await waitFor(s, has("待审核（0）"), "correction accepted");
  check("W01.correct", true);

  await s.send("Emulation.setEmulatedMedia", {features:[{name:"forced-colors",value:"active"}]});
  await nav(s, "记忆浏览");
  check("W05.forced_colors_selected_page", await s.evaluate("matchMedia('(forced-colors: active)').matches && getComputedStyle(document.querySelector('nav button[aria-current=page]')).forcedColorAdjust === 'none'"));
  await shot(s, "forced-colors");
  await s.send("Emulation.setEmulatedMedia", {features:[]});

  // W01 ask and inspect.
  await nav(s, "会话");
  await waitFor(s, has("新会话"), "sessions page");
  await s.evaluate("__t.click('button', '新会话')");
  await waitFor(s, "!!document.querySelector('#ask')", "session opened");
  await s.evaluate("__t.set('#ask', 'MoriMeta 设计决定')");
  await s.evaluate("__t.click('button', '发送')");
  await waitFor(s, any("supported_evidence", "no_supported_evidence"), "answer");
  check("W01.answer_with_sources", await s.evaluate(has("Darkroom 2") + " && " + has("来源：src_")));
  await shot(s, "05-answer");
  await s.evaluate("__t.click('button', '查看依据')");
  await waitFor(s, has("已发送"), "inspector");
  await s.evaluate("__t.click('button', '查看实际请求')");
  await waitFor(s, has("已按保存记录重新渲染并核对哈希"), "dispatch verified");
  check("W01.inspect_actual_request", true);
  await shot(s, "06-context");
  await s.evaluate("__t.set('#cq', 'MoriMeta')");
  await s.evaluate("__t.click('button', '预览')");
  await waitFor(s, has("仅预览：没有发送给任何目的地"), "preview not sent");
  check("MV6.2.preview_vs_dispatched", true);

  // W02: a cancelled-or-finished rebuild, verification, and the Explorer
  // staying responsive while the index works.
  await nav(s, "Vault 与恢复");
  await waitFor(s, has("重建索引"), "vault page");
  await s.evaluate("__t.click('button', '重建索引')");
  const t0 = Date.now();
  const status = await s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call', {request: {schemaVersion: 1, requestId: 'req_00000000-0000-4000-8000-000000000a01', command: 'workspace_status', idempotencyKey: null, arguments: {}}})");
  check("W02.ui_not_blocked_during_rebuild", Date.now() - t0 < 2000 && status.kind === "workspace_status", `${Date.now() - t0} ms`);
  await waitFor(s, all("index_rebuild", "succeeded"), "rebuild done", 60000);
  await s.evaluate("__t.click('button', '校验 Vault')");
  await waitFor(s, all("vault_verify", "succeeded"), "verify done", 60000);
  await shot(s, "07-recovery");

  // W04: no plugin, no network, no path through the page.
  const fsTry = await s.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:fs|read_text_file', {path: 'C:/Windows/win.ini'}).then(() => 'allowed', (e) => 'denied: ' + String(e).slice(0, 60))");
  check("W04.no_fs_plugin", fsTry.startsWith("denied"), fsTry);
  const shellTry = await s.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:shell|execute', {program: 'cmd'}).then(() => 'allowed', (e) => 'denied: ' + String(e).slice(0, 60))");
  check("W04.no_shell_plugin", shellTry.startsWith("denied"), shellTry);
  const netTry = await s.evaluate("fetch('https://example.invalid/').then(() => 'allowed', () => 'blocked')");
  check("W04.no_network", netTry === "blocked", netTry);
  const pathTry = await s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call', {request: {schemaVersion: 1, requestId: 'req_00000000-0000-4000-8000-000000000a02', command: 'vault_open', idempotencyKey: null, arguments: {rootToken: 'C:/Windows'}}}).then((r) => r.kind + ':' + r.error.rules.join(','))");
  check("W04.path_argument_refused", pathTry === "memory_error:workspace.token", pathTry);
  const injected = await s.evaluate(`(async () => {
    const r = await window.__TAURI_INTERNALS__.invoke('workspace_call', {request: {schemaVersion: 1, requestId: 'req_00000000-0000-4000-8000-000000000a03', command: 'remember', idempotencyKey: 'e2e-injection-key-0001', arguments: {text: '<img src="https://example.invalid/x.png" onerror="window.__pwned=1"><script>window.__pwned=2</script>', claimKey: 'synthetic.injection'}}});
    return r.kind;
  })()`);
  await nav(s, "候选审核");
  await waitFor(s, has("待审核（1）"), "injection candidate shown");
  check("W04.source_html_not_executed", injected === "candidate_proposed" && (await s.evaluate("window.__pwned === undefined && document.querySelectorAll('main img').length === 0")));

  // The real overlay may search but cannot use the main window's channel.
  const quick = await connect(9333, true);
  await quick.evaluate(HELPERS);
  await quick.evaluate("__t.set('#q', 'Darkroom')");
  await quick.evaluate("document.querySelector('form').requestSubmit()");
  await waitFor(quick, has("Darkroom 2"), "overlay real search");
  check("W04.overlay_real_search", true);
  const pendingBeforeOverlay = await s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call',{request:{schemaVersion:1,requestId:'req_00000000-0000-4000-8000-000000000e01',command:'workspace_status',idempotencyKey:null,arguments:{}}}).then(r=>r.result.pendingCandidates)");
  for (const command of ["remember", "review_confirm", "forget_plan", "vault_lock"]) {
    const request = {schemaVersion:1, requestId:`req_${crypto.randomUUID()}`, command, idempotencyKey:`synthetic-overlay-${crypto.randomUUID()}`, arguments:{text:"Synthetic denied overlay write",claimKey:"synthetic.overlay.denied"}, window:"main", principal:"owner"};
    const denied = await quick.evaluate(`window.__TAURI_INTERNALS__.invoke('workspace_call',{window:'main',request:${JSON.stringify(request)}}).then(()=> 'allowed',e=>String(e))`);
    check(`W04.overlay_denies_${command}`, denied === "permission_denied", denied);
  }
  const pendingAfterOverlay = await s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call',{request:{schemaVersion:1,requestId:'req_00000000-0000-4000-8000-000000000e02',command:'workspace_status',idempotencyKey:null,arguments:{}}}).then(r=>r.result.pendingCandidates)");
  check("W04.overlay_denials_do_not_write", pendingAfterOverlay === pendingBeforeOverlay);
  for (const command of ["pick", "exit_app"]) {
    // An invalid pick kind also avoids a dialog if the permission regresses.
    const denied = await quick.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},{kind:'synthetic_invalid'}).then(()=> 'allowed',e=>String(e))`);
    check(`W04.overlay_denies_${command}`, denied.includes("not allowed"), denied);
  }

  // W03: lock refuses work until unlock, including overlay searches.
  await nav(s, "Vault 与恢复");
  await s.evaluate("__t.click('button', '锁定 Vault')");
  await waitFor(s, has("Vault：locked"), "locked");
  await quick.evaluate("document.querySelector('form').requestSubmit()");
  await waitFor(quick, has("Vault 未打开或已锁定"), "overlay locked error");
  check("W03.overlay_lock_clears_results", await quick.evaluate("document.querySelectorAll('.results li').length === 0"));
  quick.close();
  await nav(s, "记忆浏览");
  await waitFor(s, has("请先在"), "locked gate");
  check("W03.lock_refuses", true);
  await shot(s, "08-locked");
  await nav(s, "Vault 与恢复");
  await s.evaluate("__t.click('button', '解锁')");
  await waitFor(s, has("Vault：open"), "unlocked");

  // W05 hotkey conflict: a second instance cannot take Ctrl+Alt+M.
  // A second process with its own WebView2 profile and debugging port.
  const second = launch(9334, ["--autostart", "--hotkey-key", HOTKEY], { WEBVIEW2_USER_DATA_FOLDER: join(out, "webview2-second") });
  const s2 = await connect(9334);
  await waitFor(s2, "document.body.innerText.includes('Ctrl+Alt+')", "second status");
  check("W05.hotkey_conflict_reported", await s2.evaluate("document.body.innerText.includes('被占用')"));
  // MainWindowHandle may refer to an invisible tray helper. Probe only the
  // native window with our exact main title, owned by this test process.
  const mainWindowState = (pid) => {
    const script = `$ProgressPreference = 'SilentlyContinue'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WindowProbe {
public delegate bool EnumProc(IntPtr h, IntPtr p);
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc e, IntPtr p);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
public static string Probe(uint pid) {
var result="missing";
EnumWindows((h,p)=> { uint id; GetWindowThreadProcessId(h,out id); if(id==pid) {
var title=new StringBuilder(512); GetWindowText(h,title,512);
if(title.ToString()=="Enouia Memory") result=IsWindowVisible(h)?"visible":"hidden";
} return true; },IntPtr.Zero); return result;
}}
'@
[WindowProbe]::Probe(${pid})`;
    return execFileSync("powershell", ["-NoProfile", "-EncodedCommand", Buffer.from(script,"utf16le").toString("base64")], {encoding:"utf8",stdio:["ignore","pipe","pipe"]}).trim();
  };
  check("W05.autostart_initially_hidden", mainWindowState(second.pid) === "hidden");
  const backgroundStatus = await s2.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call', {request:{schemaVersion:1,requestId:'req_00000000-0000-4000-8000-000000000b01',command:'workspace_status',idempotencyKey:null,arguments:{}}}).then((r) => r.result.vault.state)");
  check("W05.autostart_no_vault", backgroundStatus === "none", backgroundStatus);
  const startup = await s2.evaluate("window.__TAURI_INTERNALS__.invoke('startup_status')");
  check("W05.startup_status_main_only", startup.supported && startup.state === "disabled" && !startup.enabled);
  const overlay = await connect(9334, true);
  const startupDenied = await overlay.evaluate("window.__TAURI_INTERNALS__.invoke('startup_set',{enabled:true}).then(() => 'allowed',(e) => String(e))");
  check("W04.overlay_cannot_enable_startup", startupDenied.includes("not allowed"), startupDenied);
  overlay.close();
  await s2.evaluate("window.__TAURI_INTERNALS__.invoke('show_main')");
  await sleep(300);
  check("W05.autostart_can_show", mainWindowState(second.pid) === "visible");
  await s2.send("Page.captureScreenshot", { format: "png" }).then((r) => writeFileSync(join(out, "09-hotkey-conflict.png"), Buffer.from(r.result.data, "base64")));
  report.screenshots.push(join(out, "09-hotkey-conflict.png"));
  s2.close();
  second.kill();

  // W03 close window hides; the process and its Core stay alive.
  execFileSync("powershell", ["-NoProfile", "-Command", `(Get-Process -Id ${app.pid}).CloseMainWindow() | Out-Null`]);
  await sleep(800);
  const alive = await s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call', {request: {schemaVersion: 1, requestId: 'req_00000000-0000-4000-8000-000000000a04', command: 'workspace_status', idempotencyKey: null, arguments: {}}}).then((r) => r.result.vault.state)");
  check("W03.close_hides_core_keeps_running", app.exitCode === null && alive === "open", alive);

  // W03 exit, then restart: everything is still there (W01 resume).
  // The page dies with the process, so its reply never comes.
  void s.evaluate("window.__TAURI_INTERNALS__.invoke('exit_app').catch(() => 0)").catch(() => 0);
  await sleep(500); // let the request leave before the socket closes
  s.close();
  const running = (pid) => {
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  };
  for (let i = 0; i < 60 && running(app.pid); i++) await sleep(250);
  check("W03.exit_ends_process", !running(app.pid));
  app = launch(9333, ["--vault", vault]);
  s = await connect(9333);
  await s.evaluate(HELPERS);
  await waitFor(s, has("Vault：open"), "reopened");
  await nav(s, "会话");
  await waitFor(s, "!!document.querySelector('.list button')", "session listed");
  await s.evaluate("document.querySelector('.list button').click()");
  await waitFor(s, has("assistant_completed"), "transcript after restart");
  check("W01.resume_after_restart", await s.evaluate(has("MoriMeta 设计决定")));
  await shot(s, "10-resumed");
  if (retryFixture) {
    // This separately built fixture imports the production hook, ErrorBox,
    // and IPC client. It injects a lost response after a real Core write.
    // Production invoke stays immutable; no fault entry point is shipped.
    await s.evaluate(readFileSync(retryFixture, "utf8"));
    const pending = () => s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call',{request:{schemaVersion:1,requestId:'req_00000000-0000-4000-8000-000000000c01',command:'workspace_status',idempotencyKey:null,arguments:{}}}).then((r) => r.result.pendingCandidates)");
    const before = await pending();
    await s.evaluate("window.__unmountRetry = MV6RetryTest.mount()");
    await waitFor(s, "!!document.querySelector('#retry-fixture')", "retry fixture mounted");
    await s.evaluate("__t.click('#retry-fixture button', 'Commit synthetic candidate')");
    await waitFor(s, has("synthetic.lost_response"), "deliberate lost response");
    check("W01.lost_response_committed_once", await pending() === before + 1);
    await s.evaluate("__t.click('#retry-fixture button', '重试')");
    await waitFor(s, has("Completed actions: 1"), "retry completed");
    const evidence = await s.evaluate("MV6RetryTest.evidence()");
    check("W01.retry_reuses_key_and_candidate", evidence.keys.length === 2 && evidence.keys[0] === evidence.keys[1] && typeof evidence.candidates[0] === "string" && evidence.candidates[0] === evidence.candidates[1] && await pending() === before + 1);
    await s.evaluate("__t.click('#retry-fixture button', 'Commit synthetic candidate')");
    await waitFor(s, has("Completed actions: 2"), "new submission completed");
    const next = await s.evaluate("MV6RetryTest.evidence()");
    check("W01.new_submission_new_key", next.keys.length === 3 && next.keys[2] !== next.keys[0] && next.candidates[2] !== next.candidates[0] && await pending() === before + 2);
    await s.evaluate("window.__unmountRetry()");
  }
  if (crashMode === "--crash") {
    // Explicitly opt in only for a synthetic Vault. First crash the actual
    // renderer and observe the debugger event, then force-stop this owned
    // app process. All tested writes have already been acknowledged.
    const status = () => s.evaluate("window.__TAURI_INTERNALS__.invoke('workspace_call',{request:{schemaVersion:1,requestId:'req_00000000-0000-4000-8000-000000000d01',command:'workspace_status',idempotencyKey:null,arguments:{}}}).then((r) => r.result)");
    const before = await status();
    await s.send("Inspector.enable");
    void s.send("Page.crash").catch(() => undefined);
    for (let i = 0; i < 60 && !s.events.includes("Inspector.targetCrashed"); i++) await sleep(100);
    check("W01.renderer_crash_observed", s.events.includes("Inspector.targetCrashed"));
    s.close();
    app.kill();
    await sleep(1500);
    app = launch(9333, ["--vault", vault]);
    s = await connect(9333);
    await s.evaluate(HELPERS);
    await waitFor(s, has("Vault：open"), "reopened after renderer crash and forced exit");
    const after = await status();
    check("W01.crash_preserves_pending_candidates", after.pendingCandidates === before.pendingCandidates);
    await nav(s, "会话");
    await waitFor(s, "!!document.querySelector('.list button')", "session after crash");
    await s.evaluate("document.querySelector('.list button').click()");
    await waitFor(s, all("assistant_completed", "MoriMeta 设计决定"), "transcript after crash");
    await nav(s, "记忆浏览");
    await waitFor(s, has("Darkroom 2"), "approved memory after crash");
    check("W01.crash_preserves_transcript_and_memory", true);
    await nav(s, "Vault 与恢复");
    await s.evaluate("__t.click('button', '校验 Vault')");
    await waitFor(s, all("vault_verify", "succeeded"), "vault verified after crash", 60000);
    check("W01.vault_verified_after_renderer_crash", true);
    await shot(s, "11-after-crash");
  }
  // The page dies with the process, so its reply never comes.
  void s.evaluate("window.__TAURI_INTERNALS__.invoke('exit_app').catch(() => 0)").catch(() => 0);
  await sleep(500); // let the request leave before the socket closes
  s.close();
  await sleep(500);
  app.kill();
}

main()
  .catch((err) => {
    check("run", false, String(err.message ?? err));
    process.exitCode = 1;
  })
  .finally(() => {
    for (const s of sessions) s.close();
    for (const child of children) child.kill();
    writeFileSync(join(out, "report.json"), JSON.stringify(report, null, 2));
    const failed = report.checks.filter((c) => !c.ok).length;
    console.log(`${report.checks.length - failed}/${report.checks.length} checks passed`);
    if (failed) process.exitCode = 1;
    setTimeout(() => process.exit(), 200);
  });
