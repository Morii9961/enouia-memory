// Quick search (hotkey Ctrl+Alt+M): read-only search, Esc hides.
import { useEffect, useRef, useState } from "react";
import { call, shell, type J } from "./api";
import { ErrorBox, useLatestRead } from "./App";

export default function Overlay({ readPage = call }: { readPage?: typeof call } = {}) {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<J[]>([]);
  const reads = useLatestRead();
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const clear = () => { reads.clear(); setItems([]); setQuery(""); };
    const focus = () => { clear(); input.current?.focus(); };
    // On the window, not the page root: a click on text or padding moves
    // focus to <body>, outside React's container.
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") { clear(); void shell.hideWindow(); }
    };
    window.addEventListener("focus", focus);
    window.addEventListener("blur", clear);
    window.addEventListener("keydown", escape);
    focus();
    return () => {
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", clear);
      window.removeEventListener("keydown", escape);
    };
  }, [reads.clear]);

  function clear() { reads.clear(); setItems([]); setQuery(""); }
  function run(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) { clear(); return; }
    setItems([]);
    void reads.run(() => readPage("memory_search", { query, includeHistorical: false, cursor: null, limit: 8 }), (page) => setItems(page.items));
  }

  return (
    <main className="overlay" aria-busy={reads.busy}>
      <form onSubmit={run} role="search">
        <label htmlFor="q" className="sr-only">搜索记忆</label>
        <input id="q" ref={input} value={query} onChange={(e) => { if (!e.target.value.trim()) clear(); else setQuery(e.target.value); }} placeholder="搜索已批准的记忆… (Esc 关闭)" autoComplete="off" />
      </form>
      <ErrorBox error={reads.error} />
      {reads.busy && <p role="status" className="muted">正在搜索…</p>}
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
