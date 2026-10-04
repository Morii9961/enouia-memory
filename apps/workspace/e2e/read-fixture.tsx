// Synthetic deferred read responses; no native fault command or real data.
// The production Explorer is mounted unchanged with a test-only read client.
import { createRoot } from "react-dom/client";
import { Memories, Sessions } from "../src/App";
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
