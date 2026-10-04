// Synthetic deferred read responses; no native fault command or real data.
// The production Explorer is mounted unchanged with a test-only read client.
import { createRoot } from "react-dom/client";
import { Context, ErrorBox, Memories, MemoryDetail, Review, Sessions, Source, useWorkspaceStatus } from "../src/App";
import { CallError } from "../src/api";
import Overlay from "../src/Overlay";

type Pending = { command: string; args: Record<string, unknown>; key?: string; resolve: (value: unknown) => void; reject: (error: unknown) => void };
const pending: Pending[] = [];

export function mount() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "read-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<Memories readPage={(command, args = {}) => new Promise((resolve, reject) => {
    pending.push({ command, args, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}

export function requests() { return pending.map(({ command, args, key }) => ({ command, args, key })); }
export function respond(index: number, snippet: string, cursor: string | null = null) {
  pending[index].resolve({ items: snippet ? [{ memoryId: `synthetic-${index}`, type: "fact", status: "active", snippet }] : [], nextCursor: cursor });
}
export function fail(index: number) {
  pending[index].reject(new CallError({ code: "storage_failed", retryable: true, rules: ["synthetic.old_read_error"] }));
}

export function mountSessions() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "session-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<Sessions inspect={() => undefined} request={(command, args = {}, key) => new Promise((resolve, reject) => {
    pending.push({ command, args, key, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}

export function respondValue(index: number, value: unknown) { pending[index].resolve(value); }
export function respondSession(index: number, text: string) {
  pending[index].resolve({ lastSavedEventId: `synthetic-event-${index}`, transcript: [{ eventId: `synthetic-event-${index}`, kind: "user_input", deliveryState: "local", text }], turns: [], checkpoints: [] });
}

export function mountOverlay() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "overlay-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<Overlay readPage={(command, args = {}) => new Promise((resolve, reject) => {
    pending.push({ command, args, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}

const statusClient = (command: string, args: Record<string, unknown> = {}) => new Promise<any>((resolve, reject) => {
  pending.push({ command, args, resolve, reject });
});
function StatusFixture() {
  const { status, error, refresh } = useWorkspaceStatus(statusClient);
  return <section><p data-status>Vault: {status?.vault?.state ?? "unknown"}</p>
    {status?.vault?.state === "open" && <p data-open>Vault content gate open</p>}
    <ErrorBox error={error} /><button type="button" data-refresh onClick={refresh}>Refresh status</button></section>;
}
export function mountStatus() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "status-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<StatusFixture />);
  return () => { root.unmount(); host.remove(); };
}

export function mountContext() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "context-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<Context capsuleId="synthetic-original" readPage={(command, args = {}, key) => new Promise((resolve, reject) => {
    pending.push({ command, args, key, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}
export function respondContext(index: number, preview = false) {
  pending[index].resolve({ delivery: preview ? "preview_not_sent" : "dispatched", capsule:{destination:{kind:"mock"},budget:{}}, inspection:{decisions:[]}, dispatches:preview ? [] : ["synthetic-dispatch-a", "synthetic-dispatch-b"].map(dispatchId=>({dispatchId,state:"completed",preparedAt:"synthetic-time"})) });
}
export function respondDispatch(index: number, text: string) {
  pending[index].resolve({ verified:true,tools:0,destination:{kind:"mock"},messages:[{role:"user",text}] });
}

let renderSource: ((id: string, revision: number, available: boolean) => void) | undefined;
export function mountSource() {
  pending.length = 0;
  const host = document.createElement("ul");
  host.id = "source-fixture";
  document.body.append(host);
  const root = createRoot(host);
  const client = (command: string, args: Record<string, unknown> = {}) => new Promise<any>((resolve, reject) => {
    pending.push({ command, args, resolve, reject });
  });
  renderSource = (sourceId, sourceRevision, available) => root.render(<Source evidence={{sourceId, sourceRevision, available, supports:1}} readPage={client} />);
  renderSource("synthetic-source-a", 1, true);
  return () => { renderSource = undefined; root.unmount(); host.remove(); };
}
export function changeSource(id: string, revision: number, available = true) { renderSource?.(id, revision, available); }
export function respondExcerpt(index: number, text: string, start = 0, end = 8) {
  pending[index].resolve({sourceKind:"manual_assertion",speakerRole:"user",byteStart:start,byteEnd:end,totalBytes:16,excerpt:text});
}

export function mountReview() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "review-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<Review readPage={(command, args = {}, key) => new Promise((resolve, reject) => {
    pending.push({ command, args, key, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}

export function mountCorrection() {
  pending.length = 0;
  const host = document.createElement("section");
  host.id = "correction-fixture";
  document.body.append(host);
  const root = createRoot(host);
  root.render(<MemoryDetail id="synthetic-approved-memory" onChanged={() => undefined} readPage={(command, args = {}, key) => new Promise((resolve, reject) => {
    pending.push({ command, args, key, resolve, reject });
  })} />);
  return () => { root.unmount(); host.remove(); };
}
export function respondMemory(index: number) {
  pending[index].resolve({record:{memory_id:"synthetic-approved-memory",title:"Synthetic approved record",content:"Original approved content",revision:7,approved_at:"synthetic-time",supersedes:[]}, summary:{type:"fact",status:"active"}, supersededBy:[], evidence:[]});
}
