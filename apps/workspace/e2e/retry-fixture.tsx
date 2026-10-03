// Built separately for an owned synthetic WebView2 debugging session.
// Never imported by the production entry point or shipped in dist/.
import { createRoot } from "react-dom/client";
import { useState } from "react";
import { ErrorBox, useAction } from "../src/App";
import { call, CallError } from "../src/api";

const keys: string[] = [];
const candidates: string[] = [];

function RetryFixture() {
  const action = useAction();
  const [completed, setCompleted] = useState(0);
  return <section id="retry-fixture">
    <h2>Synthetic lost-response fixture</h2>
    <button disabled={action.busy} onClick={() => void action.run(async (key) => {
      keys.push(key);
      // Real IPC and Core commit first; only the response to this test action
      // is replaced with a deliberate retryable error on its first attempt.
      const text = completed === 0 ? "Synthetic retry boundary evidence." : "Synthetic new submission evidence.";
      const saved = await call("remember", { text, claimKey: "synthetic.retry" }, key);
      candidates.push(saved.candidateId);
      if (keys.length === 1) {
        throw new CallError({code:"storage_failed",retryable:true,rules:["synthetic.lost_response"]});
      }
      setCompleted((count) => count + 1);
    })}>Commit synthetic candidate</button>
    <ErrorBox error={action.error} />
    <p role="status">Completed actions: {completed}</p>
  </section>;
}

export function mount() {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  root.render(<RetryFixture />);
  return () => { root.unmount(); host.remove(); };
}

export function evidence() { return { keys: [...keys], candidates: [...candidates] }; }
