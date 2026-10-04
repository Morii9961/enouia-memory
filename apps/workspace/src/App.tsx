// Windows Memory Workspace (MV-6). Every value shown comes from the Core;
// text from memories and sources is rendered as plain text, never as HTML.
import { useCallback, useEffect, useRef, useState } from "react";
import { call, describe, newKey, pick, retryable, shell, type J } from "./api";

const PAGES = [
  ["status", "运行状态"],
  ["memories", "记忆浏览"],
  ["import", "导入中心"],
  ["review", "候选审核"],
  ["sessions", "会话"],
  ["context", "上下文检查"],
  ["vault", "Vault 与恢复"],
] as const;
type Page = (typeof PAGES)[number][0];

/** Run an async action with a visible busy state, error and retry. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ text: string; retry?: () => void } | null>(null);
  const run = useCallback(async <T,>(action: (key: string) => Promise<T>): Promise<T | undefined> => {
    const key = newKey();
    const execute = async (): Promise<T | undefined> => {
      setBusy(true);
      setError(null);
      try {
        return await action(key);
      } catch (err) {
        setError({ text: describe(err), retry: retryable(err) ? () => void execute() : undefined });
        return undefined;
      } finally {
        setBusy(false);
      }
    };
    return execute();
  }, []);
  return { busy, error, run, clear: () => setError(null) };
}

/** Only the latest read may publish results, errors or its busy state. */
function useLatestRead() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ text: string; retry?: () => void } | null>(null);
  const generation = useRef(0);
  const pending = useRef(false);
  useEffect(() => () => { generation.current += 1; pending.current = false; }, []);
  const run = useCallback(<T,>(request: () => Promise<T>, publish: (result: T) => void) => {
    const current = ++generation.current;
    const execute = async () => {
      if (generation.current !== current) return;
      pending.current = true;
      setBusy(true);
      setError(null);
      try {
        const result = await request();
        if (generation.current === current) publish(result);
      } catch (err) {
        if (generation.current === current) {
          setError({ text: describe(err), retry: retryable(err) ? () => void execute() : undefined });
        }
      } finally {
        if (generation.current === current) { pending.current = false; setBusy(false); }
      }
    };
    return execute();
  }, []);
  return { busy, error, run, pending };
}

export function ErrorBox({ error }: { error: { text: string; retry?: () => void } | null }) {
  if (!error) return null;
  return (
    <div role="alert" className="error">
      {error.text}
      {error.retry && <button type="button" onClick={error.retry}>重试</button>}
    </div>
  );
}

/** Poll a long operation until it ends; progress is not completion. */
function Operation({ id, onDone }: { id: string; onDone?: (status: J) => void }) {
  const [status, setStatus] = useState<J>(null);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;
  const cancel = useAction();
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    setError(null);
    const poll = async () => {
      try {
        const s = await call("operation_get", { operationId: id });
        if (stopped) return;
        setStatus(s);
        if (!["queued", "running"].includes(s.state)) {
          onDoneRef.current?.(s);
        } else {
          timer = setTimeout(poll, 400);
        }
      } catch (err) {
        if (!stopped) setError(describe(err));
      }
    };
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [id, retry]);
  if (error) return <ErrorBox error={{ text: `无法读取操作状态：${error}`, retry: () => setRetry((n) => n + 1) }} />;
  if (!status) return <p role="status" className="muted">操作已排队…</p>;
  const { done: n, total } = status.progress;
  return (
    <div className="operation" role="status" aria-live="polite" aria-atomic="true">
      <strong>{status.kind}</strong>：<span className={`state ${status.state}`}>{status.state}</span>
      {" "}进度 {n}{total != null ? ` / ${total}` : ""}
      {status.state === "running" && (
        <button type="button" onClick={() => void cancel.run(() => call("operation_cancel", { operationId: id }))} disabled={status.cancelRequested || cancel.busy}>
          {status.cancelRequested ? "正在取消…" : "取消"}
        </button>
      )}
      {status.error && <span className="error-inline">{status.error.code}</span>}
      <ErrorBox error={cancel.error} />
    </div>
  );
}

/** Shows the exact records a confirm will write; only its hash confirms. */
function PlanDialog({ plan, returnFocus, onClose }: { plan: J; returnFocus?: HTMLElement | null; onClose: (committed: J | null) => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const key = useRef(newKey());
  const action = useAction();
  useEffect(() => {
    // The initiating button may become disabled during plan preparation;
    // retain its identity before that asynchronous work can blur it.
    const trigger = returnFocus ?? document.activeElement;
    const dialog = ref.current;
    dialog?.showModal();
    dialog?.querySelector<HTMLHeadingElement>("h2")?.focus();
    return () => {
      dialog?.close();
      if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus();
    };
  }, []);
  const cancel = () => {
    void call("review_discard", { planId: plan.planId }).catch(() => undefined);
    onClose(null);
  };
  const confirm = () =>
    void action.run(async () => {
      const done = await call("review_confirm", { planId: plan.planId, diffHash: plan.diffHash }, key.current);
      onClose(done);
    });
  return (
    <dialog ref={ref} aria-labelledby="plan-title" aria-describedby="plan-description" onCancel={(e) => { e.preventDefault(); if (!action.busy) cancel(); }} onKeyDown={(e) => {
      if (e.key !== "Tab") return;
      const dialog = e.currentTarget;
      const controls = Array.from(dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), textarea:not([disabled]), a[href], [tabindex='0']"));
      const first = controls[0];
      const last = controls[controls.length - 1];
      const active = document.activeElement;
      if (!first) { e.preventDefault(); dialog.querySelector<HTMLHeadingElement>("h2")?.focus(); }
      else if (e.shiftKey && (active === first || !controls.includes(active as HTMLElement))) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && active === last) { e.preventDefault(); first.focus(); }
    }}>
      <h2 id="plan-title" tabIndex={-1}>确认写入：{plan.operationKind}{plan.purge ? "（彻底删除）" : ""}</h2>
      <p id="plan-description">
        下面是这次提交会写入的全部记录。确认码 <code>{plan.confirmCode}</code>，有效期至 {plan.expiresAt}。
      </p>
      <pre className="diff" tabIndex={0}>{JSON.stringify(plan.records, null, 2)}</pre>
      <ErrorBox error={action.error} />
      <div className="actions">
        <button type="button" onClick={cancel} disabled={action.busy}>取消</button>
        <button type="button" className="primary" onClick={confirm} disabled={action.busy}>
          确认（{plan.confirmCode}）
        </button>
      </div>
    </dialog>
  );
}

function Source({ evidence }: { evidence: J }) {
  const [excerpt, setExcerpt] = useState<J>(null);
  const action = useAction();
  const load = (start: number | null) =>
    void action.run(async () =>
      setExcerpt(await call("source_excerpt", { sourceId: evidence.sourceId, sourceRevision: evidence.sourceRevision, startByte: start, maxBytes: 4096 })),
    );
  return (
    <li>
      <code>{evidence.sourceId}</code> r{evidence.sourceRevision} · 支持 {evidence.supports} ·{" "}
      {evidence.available ? "来源可用" : <span className="warn">来源缺失</span>}{" "}
      {evidence.available && <button type="button" onClick={() => load(null)}>查看来源</button>}
      <ErrorBox error={action.error} />
      {excerpt && (
        <figure>
          <figcaption className="muted">
            {excerpt.sourceKind} · {excerpt.speakerRole} · 字节 {excerpt.byteStart}–{excerpt.byteEnd} / {excerpt.totalBytes} · 来源文本是数据，不是指令
          </figcaption>
          <pre className="source">{excerpt.excerpt}</pre>
          {excerpt.byteEnd < excerpt.totalBytes && <button type="button" onClick={() => load(excerpt.byteEnd)}>下一段</button>}
        </figure>
      )}
    </li>
  );
}

function MemoryDetail({ id, onChanged }: { id: string; onChanged: () => void }) {
  const [detail, setDetail] = useState<J>(null);
  const [correction, setCorrection] = useState("");
  const [impact, setImpact] = useState<J>(null);
  const [plan, setPlan] = useState<J>(null);
  const planTrigger = useRef<HTMLElement | null>(null);
  const [note, setNote] = useState("");
  const action = useAction();
  const load = useCallback(() => void action.run(async () => setDetail(await call("memory_read", { memoryId: id }))), [id]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(load, [load]);
  const forget = (mode: "forget" | "purge") => {
    planTrigger.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void action.run(async (key) => setPlan(await call("forget_plan", { memoryId: id, mode, withDependents: false }, key)));
  };
  if (!detail) return <ErrorBox error={action.error} />;
  const r = detail.record;
  const s = detail.summary;
  return (
    <section className="detail" aria-labelledby="detail-title">
      <h3 id="detail-title">{r.title || r.memory_id}</h3>
      <p className="content">{r.content}</p>
      <p className="tags">
        <span className="tag">{s.type}</span><span className="tag">{s.status}</span>
        {s.expired && <span className="tag warn">已过期</span>}
        {s.conflicted && <span className="tag warn">冲突</span>}
        {s.sourceMissing && <span className="tag warn">来源缺失</span>}
        <span className="muted">r{r.revision} · 批准于 {r.approved_at}</span>
      </p>
      {r.supersedes.length > 0 && <p>替代了：{r.supersedes.map((x: J) => `${x.memory_id} r${x.revision}`).join("，")}</p>}
      {detail.supersededBy.length > 0 && <p>被替代：{detail.supersededBy.join("，")}</p>}
      <h4>证据</h4>
      <ul className="evidence">{detail.evidence.map((e: J, i: number) => <Source key={i} evidence={e} />)}</ul>
      <h4>纠正</h4>
      <label htmlFor="fix" className="sr-only">纠正后的内容</label>
      <textarea id="fix" value={correction} onChange={(e) => setCorrection(e.target.value)} placeholder="写下正确的内容；提交后进入候选审核，不会直接改写记忆。" />
      <div className="actions">
        <button type="button" disabled={!correction.trim() || action.busy} onClick={() => void action.run(async (key) => {
          await call("correction_propose", { memoryId: id, revision: r.revision, text: correction }, key);
          setCorrection("");
          setNote("已提交纠正候选，请到“候选审核”确认。");
        })}>提交纠正候选</button>
        <button type="button" onClick={() => void action.run(async () => setImpact(await call("delete_preview", { memoryId: id, withDependents: false })))}>删除影响预览</button>
        <button type="button" disabled={action.busy} onClick={() => forget("forget")}>忘记…</button>
        <button type="button" className="danger" disabled={action.busy} onClick={() => forget("purge")}>彻底删除…</button>
      </div>
      {note && <p role="status">{note}</p>}
      <ErrorBox error={action.error} />
      {impact && (
        <div className="impact">
          <p>彻底删除会移除 {impact.targets.length} 条记录、{impact.objectCount} 个对象；{impact.losingProvenance.length} 条记忆会失去证据。</p>
        </div>
      )}
      {plan && <PlanDialog plan={plan} returnFocus={planTrigger.current} onClose={(done) => { setPlan(null); if (done) onChanged(); }} />}
    </section>
  );
}

export function Memories({ readPage = call }: { readPage?: typeof call } = {}) {
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [inactive, setInactive] = useState(false);
  const [rows, setRows] = useState<J[]>([]);
  const [next, setNext] = useState<string | null>(null);
  const [mode, setMode] = useState<"list" | "search">("list");
  const [selected, setSelected] = useState<string | null>(null);
  const action = useLatestRead();
  const load = (cursor: string | null, kind = mode, searchQuery = submittedQuery) =>
    void action.run(() => kind === "search"
      ? readPage("memory_search", { query: searchQuery, includeHistorical: inactive, cursor, limit: 25 })
      : readPage("memory_list", { includeInactive: inactive, cursor, limit: 25 }), (page) => {
      setRows((current) => cursor ? [...current, ...page.items] : page.items);
      setNext(page.nextCursor);
    });
  useEffect(() => load(null), [inactive]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="split">
      <section aria-label="记忆列表" aria-busy={action.busy}>
        <form role="search" onSubmit={(e) => { e.preventDefault(); const kind = query.trim() ? "search" : "list"; setMode(kind); setSubmittedQuery(query); load(null, kind, query); }}>
          <label htmlFor="mq" className="sr-only">搜索</label>
          <input id="mq" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="按字面搜索（空白则浏览全部）" />
          <button type="submit">搜索</button>
          <label><input type="checkbox" checked={inactive} onChange={(e) => setInactive(e.target.checked)} /> 含历史</label>
        </form>
        <ErrorBox error={action.error} />
        {action.busy && <p role="status" className="muted">正在读取记忆…</p>}
        <ul className="list">
          {rows.map((m) => (
            <li key={m.memoryId}>
              <button type="button" aria-current={selected === m.memoryId} onClick={() => setSelected(m.memoryId)}>
                <span className="tag">{m.type}</span>
                {m.status && m.status !== "active" && <span className="tag">{m.status}</span>}
                {m.currency && m.currency !== "current" && <span className="tag">{m.currency}</span>}
                {m.expired && <span className="tag warn">过期</span>}
                {m.conflicted && <span className="tag warn">冲突</span>}
                {m.sourceMissing && <span className="tag warn">来源缺失</span>}
                <span className="snippet">{m.snippet}</span>
              </button>
            </li>
          ))}
        </ul>
        {rows.length === 0 && !action.busy && <p className="muted">没有可见的已批准记忆。</p>}
        {next && <button type="button" onClick={() => load(next)} disabled={action.busy}>加载更多</button>}
      </section>
      {selected ? <MemoryDetail key={selected} id={selected} onChanged={() => { setSelected(null); load(null); }} /> : <p className="muted">选择一条记忆查看来源和替代链。</p>}
    </div>
  );
}

function Import() {
  const [picked, setPicked] = useState<J>(null);
  const [preview, setPreview] = useState<J>(null);
  const [alias, setAlias] = useState("acct-main");
  const [op, setOp] = useState<string | null>(null);
  const [imports, setImports] = useState<J[]>([]);
  const action = useAction();
  const refresh = useCallback(() => void action.run(async () => setImports((await call("import_list")).items)), []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(refresh, [refresh]);
  return (
    <div>
      <p>选择导出文件后由后台归档原件并解析；页面只拿到文件名和令牌。</p>
      <div className="actions">
        <button type="button" onClick={() => void action.run(async () => {
          const p = await pick("import_file");
          if (!p) return;
          setPicked(p);
          setPreview(await call("import_preview", { importToken: p.token }));
        })}>选择文件…</button>
      </div>
      <ErrorBox error={action.error} />
      {preview && (
        <div className="card">
          <p><strong>{preview.displayName}</strong> · {preview.bytes} 字节 · {preview.inputKind} · {preview.recognized ? `可解析，${preview.units} 个单元` : "不支持的格式（仍会原样归档）"}</p>
          {preview.duplicateOf && <p className="warn">与已有导入 {preview.duplicateOf} 相同，将记为重复。</p>}
          {preview.warnings.length > 0 && <p className="muted">警告：{preview.warnings.join(", ")}</p>}
          <label>账户别名 <input value={alias} onChange={(e) => setAlias(e.target.value)} pattern="[a-z0-9][a-z0-9_-]*" /></label>
          <button type="button" className="primary" disabled={!picked || action.busy} onClick={() => void action.run(async (key) => {
            const started = await call("import_start", { importToken: picked.token, accountAlias: alias }, key);
            setOp(started.operationId);
            setPicked(null);
            setPreview(null);
          })}>开始导入</button>
        </div>
      )}
      {op && <Operation id={op} onDone={refresh} />}
      <h3>导入记录</h3>
      <table>
        <thead><tr><th>状态</th><th>格式</th><th>来源数</th><th>附件缺失</th><th>警告</th><th></th></tr></thead>
        <tbody>
          {imports.map((m) => (
            <tr key={m.importId}>
              <td>{m.status}{m.duplicateOf ? "（重复）" : ""}</td>
              <td>{m.inputKind}</td>
              <td>{m.counts.sources_created}</td>
              <td>{m.counts.attachments_missing}</td>
              <td>{m.warnings.join(", ")}</td>
              <td>{m.status === "parsing" && <button type="button" disabled={action.busy} onClick={() => void action.run(async (key) => setOp((await call("import_resume", { importId: m.importId, accountAlias: m.accountScope ?? alias }, key)).operationId))}>继续</button>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Review() {
  const [items, setItems] = useState<J[]>([]);
  const [total, setTotal] = useState(0);
  const [plan, setPlan] = useState<J>(null);
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [text, setText] = useState("");
  const [claim, setClaim] = useState("");
  const focusAfterCommit = useRef(false);
  const planTrigger = useRef<HTMLElement | null>(null);
  const action = useAction();
  const load = useCallback(() => void action.run(async () => {
    const page = await call("candidate_list", { cursor: null, limit: 50 });
    setItems(page.items);
    setTotal(page.total);
  }), []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(load, [load]);
  useEffect(() => {
    if (focusAfterCommit.current) {
      focusAfterCommit.current = false;
      document.querySelector<HTMLHeadingElement>("main h2")?.focus();
    }
  }, [items]);
  const decide = (c: J, act: string) => {
    planTrigger.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void action.run(async () =>
      setPlan(await call("review_plan", { decisions: [{
        candidateId: c.candidateId, revision: c.revision, action: act,
        editedContent: act === "edit_accept" ? edits[c.candidateId] ?? c.content : null, mergeTarget: null,
      }] })),
    );
  };
  return (
    <div>
      <form className="card" onSubmit={(e) => { e.preventDefault(); void action.run(async (key) => { await call("remember", { text, claimKey: claim }, key); setText(""); setClaim(""); load(); }); }}>
        <h3>记住一件事</h3>
        <label htmlFor="rt">原话（保存为来源，再生成待审核候选）</label>
        <textarea id="rt" value={text} onChange={(e) => setText(e.target.value)} />
        <label>主题键 <input value={claim} onChange={(e) => setClaim(e.target.value)} placeholder="例如 preference.reading" /></label>
        <button type="submit" disabled={!text.trim() || !claim.trim() || action.busy}>保存为候选</button>
      </form>
      <ErrorBox error={action.error} />
      <h3>待审核（{total}）</h3>
      {items.map((c) => (
        <article key={c.candidateId} className="card" aria-label={`候选 ${c.candidateId}`}>
          <p className="tags"><span className="tag">{c.proposalKind}</span><span className="tag">{c.proposedType}</span><span className="tag">{c.sensitivity}</span><span className="muted">{c.originKind} · {c.createdAt}</span></p>
          {c.target && <p className="old">旧事实（r{c.target.revision}）：{c.target.content}</p>}
          <label htmlFor={`e-${c.candidateId}`} className="sr-only">候选内容</label>
          <textarea id={`e-${c.candidateId}`} value={edits[c.candidateId] ?? c.content} onChange={(e) => setEdits({ ...edits, [c.candidateId]: e.target.value })} />
          <p className="muted">证据：{c.evidence.map((e: J) => `${e.sourceId} r${e.sourceRevision}`).join("，")}{c.conflicts.length > 0 && <span className="warn"> · {c.conflicts.length} 处冲突</span>}</p>
          <div className="actions">
            <button type="button" className="primary" disabled={action.busy} onClick={() => decide(c, (edits[c.candidateId] ?? c.content) !== c.content ? "edit_accept" : "accept")}>接受…</button>
            <button type="button" disabled={action.busy} onClick={() => decide(c, "reject")}>拒绝…</button>
          </div>
        </article>
      ))}
      {plan && <PlanDialog plan={plan} returnFocus={planTrigger.current} onClose={(committed) => {
        setPlan(null);
        if (committed !== null) { focusAfterCommit.current = true; load(); }
      }} />}
    </div>
  );
}

export function Sessions({ inspect, request = call }: { inspect: (capsuleId: string) => void; request?: typeof call }) {
  const [sessions, setSessions] = useState<J[]>([]);
  const [current, setCurrent] = useState<{ sessionId: string; branchId: string } | null>(null);
  const [detail, setDetail] = useState<J>(null);
  const [text, setText] = useState("");
  const [answer, setAnswer] = useState<J>(null);
  const [summary, setSummary] = useState("");
  const action = useAction();
  const listRead = useLatestRead();
  const detailRead = useLatestRead();
  const writing = useRef(false);
  const selectedBranch = useRef<string | null>(null);
  const drafts = useRef(new Map<string, { text: string; summary: string }>());
  const branchKey = (s: { sessionId: string; branchId: string }) => `${s.sessionId}:${s.branchId}`;
  const list = useCallback(() => listRead.run(() => request("session_list"), (page) => setSessions(page.items)), [listRead.run, request]);
  useEffect(() => { void list(); }, [list]);
  const open = (s: { sessionId: string; branchId: string }) =>
    detailRead.run(() => request("session_detail", s), (saved) => {
      const key = branchKey(s);
      if (selectedBranch.current !== key) { setAnswer(null); action.clear(); }
      selectedBranch.current = key;
      const draft = drafts.current.get(key);
      setText(draft?.text ?? "");
      setSummary(draft?.summary ?? "");
      setCurrent(s);
      setDetail(saved);
    });
  const editDraft = (field: "text" | "summary", value: string) => {
    if (!current) return;
    const key = branchKey(current);
    drafts.current.set(key, { ...(drafts.current.get(key) ?? { text: "", summary: "" }), [field]: value });
    if (field === "text") setText(value); else setSummary(value);
  };
  const clearSubmittedDraft = (field: "text" | "summary", submitted: string) => {
    if (current && drafts.current.get(branchKey(current))?.[field] === submitted) editDraft(field, "");
  };
  const write = (commit: (key: string) => Promise<void>) => {
    if (writing.current || detailRead.pending.current) return;
    void action.run(async (key) => {
      writing.current = true;
      try { await commit(key); } finally { writing.current = false; }
    });
  };
  const busy = action.busy || detailRead.busy;
  return (
    <div className="split">
      <section aria-label="会话列表">
        <button type="button" disabled={busy || listRead.busy} onClick={() => write(async (key) => { const s = await request("session_new", {}, key); await list(); await open({ sessionId: s.sessionId, branchId: s.branchId }); })}>新会话</button>
        <ErrorBox error={listRead.error} />
        <ul className="list">
          {sessions.map((s) => s.branches.map((b: J) => (
            <li key={b.branchId}>
              <button type="button" disabled={action.busy} aria-current={current?.branchId === b.branchId} onClick={() => { if (!writing.current) void open({ sessionId: s.sessionId, branchId: b.branchId }); }}>
                {s.updatedAt} · 分支 {b.branchId.slice(3, 11)} · {b.lastEventSeq} 个事件
              </button>
            </li>
          )))}
        </ul>
      </section>
      <section aria-label="会话内容" aria-busy={busy}>
        <ErrorBox error={action.error} />
        <ErrorBox error={detailRead.error} />
        {detailRead.busy && <p role="status" className="muted">正在读取会话…</p>}
        {detail && current && (
          <>
            <p className="muted">最后已保存事件：{detail.lastSavedEventId ?? "无"}</p>
            <ol className="transcript">
              {detail.transcript.map((e: J) => (
                <li key={e.eventId} className={e.kind}>
                  <span className="muted">{e.kind} · {e.deliveryState}</span>
                  {e.text != null && <p>{e.text}</p>}
                </li>
              ))}
            </ol>
            {detail.turns.filter((t: J) => t.state !== "completed").map((t: J) => (
              <p key={t.turnId} className="warn">轮次 {t.turnId.slice(5, 13)}：{t.state}（最后持久事件 {t.last_persisted_event_id ?? t.lastPersistedEventId}）</p>
            ))}
            <h4>检查点（暂定，未经审核）</h4>
            <ul>{detail.checkpoints.map((c: J) => <li key={c.checkpoint_id}>{c.status} · {typeof c.summary === "string" ? c.summary : c.checkpoint_id}</li>)}</ul>
            <form onSubmit={(e) => { e.preventDefault(); write(async (key) => {
              setAnswer(await request("session_ask", { ...current, text }, key));
              clearSubmittedDraft("text", text);
              await open(current);
            }); }}>
              <label htmlFor="ask">提问（本地 Mock，只引用已批准记忆）</label>
              <textarea id="ask" value={text} disabled={busy} onChange={(e) => editDraft("text", e.target.value)} />
              <button type="submit" disabled={!text.trim() || busy}>发送</button>
            </form>
            {answer && (
              <div className="card" aria-live="polite">
                <p><strong>{answer.status}</strong></p>
                {answer.statements.map((s: string, i: number) => <p key={i}>{s}</p>)}
                <p className="muted">来源：{answer.sources.map((s: J) => s.source_id).join("，") || "无"}</p>
                <button type="button" onClick={() => inspect(answer.capsuleId)}>查看依据（上下文检查）</button>
              </div>
            )}
            <form onSubmit={(e) => { e.preventDefault(); write(async (key) => { await request("session_checkpoint", { ...current, summary }, key); clearSubmittedDraft("summary", summary); await open(current); }); }}>
              <label htmlFor="cp">检查点摘要</label>
              <input id="cp" value={summary} disabled={busy} onChange={(e) => editDraft("summary", e.target.value)} />
              <button type="submit" disabled={!summary.trim() || busy}>保存检查点</button>
            </form>
          </>
        )}
      </section>
    </div>
  );
}

function Context({ capsuleId }: { capsuleId: string | null }) {
  const [query, setQuery] = useState("");
  const [id, setId] = useState(capsuleId);
  const [view, setView] = useState<J>(null);
  const [request, setRequest] = useState<J>(null);
  const action = useAction();
  useEffect(() => {
    if (!id) return;
    void action.run(async () => { setRequest(null); setView(await call("context_inspect", { capsuleId: id })); });
  }, [id]); // eslint-disable-line react-hooks/exhaustive-deps
  const delivery: Record<string, string> = {
    preview_not_sent: "仅预览：没有发送给任何目的地",
    dispatched: "已发送（见下方实际请求）",
  };
  return (
    <div>
      <form onSubmit={(e) => { e.preventDefault(); void action.run(async (key) => setId((await call("context_preview", { query, sessionId: null, branchId: null }, key)).capsuleId)); }}>
        <label htmlFor="cq">编译预览</label>
        <input id="cq" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="输入问题，查看会带上哪些记忆" />
        <button type="submit" disabled={!query.trim() || action.busy}>预览</button>
      </form>
      <ErrorBox error={action.error} />
      {view && (
        <>
          <p className="state-line"><strong>{delivery[view.delivery]}</strong> · 目的地 {view.capsule.destination.kind} · 预算 {JSON.stringify(view.capsule.budget)}</p>
          <table>
            <thead><tr><th>记录</th><th>决定</th><th>原因</th><th>排序</th><th>来源可达</th></tr></thead>
            <tbody>
              {view.inspection.decisions.map((d: J, i: number) => (
                <tr key={i} className={d.decision}>
                  <td><code>{d.record_kind}:{d.record_id.slice(0, 13)}</code> r{d.revision}</td>
                  <td>{d.decision === "included" ? "包含" : "排除"}</td>
                  <td>{d.reason}</td>
                  <td>{d.rank ?? ""}</td>
                  <td>{d.source_reachable ? "是" : "否"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {view.dispatches.map((d: J) => (
            <p key={d.dispatchId}>
              发送 <code>{d.dispatchId.slice(0, 12)}</code>：{d.state}（准备 {d.preparedAt}{d.sentAt ? ` · 发送 ${d.sentAt}` : " · 未发送"}{d.completedAt ? ` · 完成 ${d.completedAt}` : ""}）{" "}
              <button type="button" onClick={() => void action.run(async () => setRequest(await call("dispatch_inspect", { dispatchId: d.dispatchId })))}>查看实际请求</button>
            </p>
          ))}
          {request && (
            <div className="card">
              <p>{request.verified ? "已按保存记录重新渲染并核对哈希" : "哈希不符"} · 工具 {request.tools} 个 · {request.destination.kind}</p>
              {request.messages.map((m: J, i: number) => <div key={i}><strong>{m.role}</strong><pre className="source">{m.text}</pre></div>)}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function Vault({ status, refresh }: { status: J; refresh: () => void }) {
  const [op, setOp] = useState<string | null>(null);
  const [restore, setRestore] = useState<J>(null);
  const [phrase, setPhrase] = useState("");
  const action = useAction();
  const v = status?.vault ?? {};
  const after = (f: () => Promise<unknown>) => void action.run(async () => { await f(); refresh(); });
  return (
    <div>
      <ErrorBox error={action.error} />
      {v.state !== "open" && (
        <div className="card">
          <h3>{v.state === "locked" ? `Vault 已锁定（${v.rootName}）` : "没有打开的 Vault"}</h3>
          {v.state === "locked" && <button type="button" className="primary" onClick={() => after(() => call("vault_unlock"))}>解锁</button>}
          <button type="button" onClick={() => after(async () => { const p = await pick("vault_root"); if (p) await call("vault_open", { rootToken: p.token }); })}>打开已有 Vault…</button>
          <p>新建：在空目录中创建 Vault。请输入 <code>create new vault</code> 确认。</p>
          <input value={phrase} onChange={(e) => setPhrase(e.target.value)} aria-label="确认短语" />
          <button type="button" disabled={phrase !== "create new vault"} onClick={() => after(async () => { const p = await pick("vault_root"); if (p) await call("vault_create", { rootToken: p.token, confirmPhrase: phrase }); })}>新建 Vault…</button>
        </div>
      )}
      {v.state === "open" && (
        <>
          <dl className="facts">
            <dt>目录</dt><dd>{v.rootName}</dd>
            <dt>Vault 健康</dt><dd>{v.health}</dd>
            <dt>最新提交</dt><dd>#{v.headSequence} · {v.lastCommitAt}</dd>
            <dt>磁盘剩余</dt><dd>{v.freeBytes != null ? `${(v.freeBytes / 2 ** 30).toFixed(1)} GiB` : "未知"}</dd>
            <dt>仅所有者可访问</dt><dd>{v.ownerOnlyAcl == null ? "未知" : v.ownerOnlyAcl ? "是" : "否（可用 CLI protect）"}</dd>
            <dt>最后备份（本次运行）</dt><dd>{status.lastBackup ? `${status.lastBackup.state} ${status.lastBackup.result?.commitId ?? ""}` : "本次运行未备份；Vault 不记录备份"}</dd>
            <dt>最后验证恢复</dt><dd>未记录（恢复预览只校验备份）</dd>
          </dl>
          <div className="actions">
            <button type="button" onClick={() => void action.run(async () => setOp((await call("index_rebuild")).operationId))}>重建索引</button>
            <button type="button" onClick={() => void action.run(async () => setOp((await call("vault_verify")).operationId))}>校验 Vault</button>
            <button type="button" onClick={() => void action.run(async () => { const p = await pick("backup_destination"); if (p) setOp((await call("backup_export", { destinationToken: p.token })).operationId); })}>备份到空目录…</button>
            <button type="button" onClick={() => void action.run(async () => { const p = await pick("export_folder"); if (p) setRestore(await call("restore_preview", { exportToken: p.token })); })}>恢复预览…</button>
            <button type="button" onClick={() => after(() => call("vault_lock"))}>锁定 Vault</button>
          </div>
          {op && <Operation id={op} onDone={refresh} />}
          {restore && (
            <p className="card">备份有效：提交 #{restore.sequence}（{restore.commitId}），{restore.files} 个文件，{restore.sameVaultAsOpen ? "与当前 Vault 相同" : "来自另一个 Vault"}。实际恢复需在 CLI 中恢复到空目录（<code>restore</code>）。</p>
          )}
        </>
      )}
    </div>
  );
}

function StartupSettings() {
  const [startup, setStartup] = useState<J>(null);
  const [note, setNote] = useState("");
  const action = useAction();
  useEffect(() => { void action.run(async () => setStartup(await shell.startupStatus())); }, [action.run]);
  return <section className="card" aria-labelledby="startup-title">
    <h3 id="startup-title">登录后启动</h3>
    <p id="startup-description">开启后，登录 Windows 时只启动托盘；不会自动打开 Vault。默认关闭。</p>
    <label>
      <input type="checkbox" checked={startup?.enabled ?? false} disabled={!startup?.supported || action.busy || startup?.state === "different_installation"}
        aria-describedby="startup-description" onChange={(e) => {
          const enabled = e.target.checked;
          void action.run(async () => { setStartup(await shell.startupSet(enabled)); setNote(enabled ? "已开启登录后启动。" : "已关闭登录后启动。"); });
        }} /> 登录 Windows 后启动 Enouia Memory
    </label>
    {startup?.state === "different_installation" && <p className="warn">另一个安装位置已有启动项。请在该版本中关闭，再在这里开启。</p>}
    {startup?.state === "unsupported" && <p>此系统不支持自启动设置。</p>}
    <p role="status" aria-atomic="true">{note}</p>
    <ErrorBox error={action.error} />
  </section>;
}

function Status({ status }: { status: J }) {
  if (!status) return <p className="muted">读取状态…</p>;
  const hotkey = status.companion?.hotkey;
  return (
    <div>
      <table>
        <thead><tr><th>组件</th><th>状态</th><th>说明</th></tr></thead>
        <tbody>
          {status.components.map((c: J) => (
            <tr key={c.component}><td>{c.component}</td><td className={`state ${c.state}`}>{c.state}</td><td>{c.mode}{c.watermarkSequence != null ? ` · 水位 #${c.watermarkSequence}` : ""}</td></tr>
          ))}
        </tbody>
      </table>
      <p>待审核候选 {status.pendingCandidates ?? "—"} · 运行中操作 {status.operationsRunning} · 热键 {hotkey?.combo ?? ""} {hotkey?.state === "conflict" ? <span className="warn">被占用（未注册）</span> : hotkey?.state}</p>
      <h3>四种停止是不同的操作</h3>
      <ul>
        <li><strong>关闭窗口</strong>：只隐藏窗口；后台操作继续，托盘图标保留。</li>
        <li><strong>锁定 Vault</strong>：取消并等待所有操作，释放 Vault 和索引；解锁前一切读写都被拒绝。</li>
        <li><strong>退出</strong>：同锁定，然后结束进程（托盘菜单或下方按钮）。</li>
        <li><strong>暂停同步</strong>：MV-9 之前没有同步，此项不可用。</li>
      </ul>
      <p className="muted">Activity 属于 Runtime，本应用不会启动、停止或读取它。</p>
      <StartupSettings />
      <button type="button" className="danger" onClick={() => void shell.exit()}>退出 Enouia Memory</button>
    </div>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("status");
  const [status, setStatus] = useState<J>(null);
  const [capsule, setCapsule] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const refresh = useCallback(() => void call("workspace_status").then(setStatus).catch(() => undefined), []);
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [refresh]);
  const go = (next: Page) => {
    setPage(next);
    requestAnimationFrame(() => heading.current?.focus());
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const n = Number(e.key);
      if (e.altKey && !e.ctrlKey && !e.metaKey && !document.querySelector("dialog[open]") && n >= 1 && n <= PAGES.length) {
        e.preventDefault();
        go(PAGES[n - 1][0]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const open = status?.vault?.state === "open";
  const needsVault = !open && page !== "status" && page !== "vault";
  return (
    <div className="app">
      <nav aria-label="页面">
        <p className="brand">Enouia Memory</p>
        {PAGES.map(([id, label], i) => (
          <button key={id} type="button" aria-current={page === id ? "page" : undefined} aria-keyshortcuts={`Alt+${i + 1}`} onClick={() => go(id)} title={`Alt+${i + 1}`}>
            {label}{id === "review" && status?.pendingCandidates ? ` (${status.pendingCandidates})` : ""}
          </button>
        ))}
        <p className="muted vault-state">Vault：{status?.vault?.state ?? "…"}</p>
      </nav>
      <main id="main-content">
        <h2 ref={heading} tabIndex={-1}>{PAGES.find(([id]) => id === page)?.[1]}</h2>
        {needsVault ? (
          <p>请先在“Vault 与恢复”中打开或解锁 Vault。<button type="button" onClick={() => go("vault")}>前往</button></p>
        ) : (
          <>
            {page === "status" && <Status status={status} />}
            {page === "memories" && <Memories />}
            {page === "import" && <Import />}
            {page === "review" && <Review />}
            {page === "sessions" && <Sessions inspect={(id) => { setCapsule(id); go("context"); }} />}
            {page === "context" && <Context key={capsule ?? "none"} capsuleId={capsule} />}
            {page === "vault" && <Vault status={status} refresh={refresh} />}
          </>
        )}
      </main>
    </div>
  );
}
