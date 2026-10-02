// Quick search (hotkey Ctrl+Alt+M): read-only search, Esc hides.
import { useEffect, useRef, useState } from "react";
import { call, describe, shell, type J } from "./api";

export default function Overlay() {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<J[]>([]);
  const [error, setError] = useState("");
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => input.current?.focus();
    window.addEventListener("focus", focus);
    focus();
    return () => window.removeEventListener("focus", focus);
  }, []);

  async function run(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) return;
    try {
      const page = await call("memory_search", { query, includeHistorical: false, cursor: null, limit: 8 });
      setItems(page.items);
      setError("");
    } catch (err) {
      setItems([]);
      setError(describe(err));
    }
  }

  return (
    <main
      className="overlay"
      onKeyDown={(e) => {
        if (e.key === "Escape") void shell.hideWindow();
      }}
    >
      <form onSubmit={run} role="search">
        <label htmlFor="q" className="sr-only">搜索记忆</label>
        <input id="q" ref={input} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索已批准的记忆… (Esc 关闭)" autoComplete="off" />
      </form>
      {error && <p role="alert" className="error">{error}</p>}
      <ul className="results" aria-live="polite">
        {items.map((h) => (
          <li key={h.memoryId}>
            <span className={`tag ${h.currency}`}>{h.currency}</span> {h.snippet}
          </li>
        ))}
      </ul>
      <button type="button" className="link" onClick={() => void shell.showMain()}>打开主窗口</button>
    </main>
  );
}
