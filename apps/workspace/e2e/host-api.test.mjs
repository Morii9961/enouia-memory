// Exercise the production API module with only its native invoke port replaced.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { transformWithOxc } from "vite";

const source = readFileSync(new URL("../src/api.ts", import.meta.url), "utf8");
const port = 'import { invoke } from "@tauri-apps/api/core";';
assert.equal(source.split(port).length, 2);
const { code } = await transformWithOxc(source.replace(port,
  "const invoke = (...args) => globalThis.__nativeHostApiInvoke(...args);"), "api.ts");
const api = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);

test("lost reply retains the original immutable-argument packet and never retries", async () => {
  let fail;
  let calls = 0;
  let sent;
  globalThis.__nativeHostApiInvoke = (command, { request }) => {
    assert.equal(command, "workspace_call");
    calls++;
    sent = request;
    return new Promise((_, reject) => { fail = reject; });
  };
  const args = { text: "synthetic original", claimKey: "synthetic.native" };
  const pending = api.call("remember", args, "synthetic-original-key");
  args.text = "later edited draft";
  fail("host_outcome_unknown");
  await assert.rejects(pending, (error) => {
    assert.ok(error instanceof api.CallError);
    assert.equal(api.retryable(error), false);
    assert.equal(error.originalRequest.idempotencyKey, "synthetic-original-key");
    assert.equal(error.originalRequest.arguments.text, "synthetic original");
    assert.deepEqual(error.originalRequest, sent);
    assert.match(api.describe(error), /操作可能已保存/);
    return true;
  });
  await Promise.resolve();
  assert.equal(calls, 1);
});

test("refused or disconnected calls carry distinct nonretryable errors", async () => {
  for (const code of ["host_stale_connection", "host_not_ready", "host_busy", "host_disconnected"]) {
    let calls = 0;
    globalThis.__nativeHostApiInvoke = () => { calls++; return Promise.reject(code); };
    await assert.rejects(api.call("workspace_status"), (error) => {
      assert.ok(error instanceof api.CallError);
      assert.equal(error.error.code, code);
      assert.equal(api.retryable(error), false);
      assert.equal(error.originalRequest, undefined);
      return true;
    });
    assert.equal(calls, 1);
  }
});

test("valid Core results and retry flags retain existing behavior", async () => {
  globalThis.__nativeHostApiInvoke = () => Promise.resolve({ result: { synthetic: true } });
  assert.deepEqual(await api.call("workspace_status"), { synthetic: true });
  const core = { code: "busy", retryable: true, rules: [] };
  globalThis.__nativeHostApiInvoke = () => Promise.resolve({ kind: "memory_error", error: core });
  await assert.rejects(api.call("workspace_status"), (error) => {
    assert.equal(error.error, core);
    assert.equal(api.retryable(error), true);
    assert.equal(error.originalRequest, undefined);
    return true;
  });
});

test("unavailable native picker gives an explicit error", async () => {
  globalThis.__nativeHostApiInvoke = () => Promise.reject("native_host_picker_unavailable");
  await assert.rejects(api.pick("import_file"), (error) => {
    assert.ok(error instanceof api.CallError);
    assert.equal(api.retryable(error), false);
    assert.match(api.describe(error), /尚未接入文件选择/);
    return true;
  });
});
