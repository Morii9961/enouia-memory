// Synthetic deferred read responses; no native fault command or real data.
// The production Explorer is mounted unchanged with a test-only read client.
import { createRoot } from "react-dom/client";
import { Memories } from "../src/App";
import { CallError } from "../src/api";

type Pending = { command: string; args: Record<string, unknown>; resolve: (value: unknown) => void; reject: (error: unknown) => void };
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

export function requests() { return pending.map(({ command, args }) => ({ command, args })); }
export function respond(index: number, snippet: string, cursor: string | null = null) {
  pending[index].resolve({ items: snippet ? [{ memoryId: `synthetic-${index}`, type: "fact", status: "active", snippet }] : [], nextCursor: cursor });
}
export function fail(index: number) {
  pending[index].reject(new CallError({ code: "storage_failed", retryable: true, rules: ["synthetic.old_read_error"] }));
}
