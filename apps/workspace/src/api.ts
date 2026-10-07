// The page's only way out: the shell's `workspace_call` (workspace IPC v1)
// and `pick` (native dialogs that return a token, never a path).
import { invoke } from "@tauri-apps/api/core";

export type WsError = { code: string; retryable: boolean; rules: string[] };
// Results are shaped by the Core (camelCase; stored records snake_case).
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type J = any;

export class CallError extends Error {
  constructor(public readonly error: WsError) {
    super(error.code);
  }
}

const WRITES = new Set([
  "review_confirm", "forget_plan", "remember", "correction_propose", "import_start",
  "import_resume", "session_new", "session_ask", "session_checkpoint", "context_preview",
]);

const uuid = () => crypto.randomUUID();

/** A fresh idempotency key. Keep it to retry the same write safely. */
export const newKey = () => `ui-${uuid()}`;

export async function call(command: string, args: Record<string, unknown> = {}, key?: string): Promise<J> {
  const request = {
    schemaVersion: 1,
    requestId: `req_${uuid()}`,
    command,
    idempotencyKey: WRITES.has(command) ? key ?? newKey() : null,
    arguments: args,
  };
  const response: J = await invoke("workspace_call", { request });
  if (response.kind === "memory_error") throw new CallError(response.error);
  return response.result;
}

export type Picked = { token: string; displayName: string; bytes: number | null };
export type PickKind = "import_file" | "vault_root" | "backup_destination" | "export_folder";

export async function pick(kind: PickKind): Promise<Picked | null> {
  const result: J = await invoke("pick", { kind });
  if (result.cancelled) return null;
  if (result.error) throw new CallError({ retryable: false, rules: [], ...result.error });
  return result as Picked;
}

export const shell = {
  showMain: () => invoke("show_main"),
  hideWindow: () => invoke("hide_window"),
  exit: () => invoke("exit_app"),
  startupStatus: () => invoke<J>("startup_status"),
  startupSet: (enabled: boolean) => invoke<J>("startup_set", { enabled }),
};

const CODES: Record<string, string> = {
  vault_locked: "Vault 未打开或已锁定",
  index_not_ready: "索引正在重建或未就绪，请稍后重试",
  revision_conflict: "内容已变化（计划过期、哈希不符或游标失效），请刷新后重来",
  idempotency_conflict: "同一请求键被用于不同内容",
  not_found: "找不到对象（可能已删除或令牌过期）",
  invalid_request: "请求不符合契约",
  permission_denied: "没有权限",
  busy: "Vault 正忙，请稍后重试",
  storage_full: "磁盘空间不足",
  storage_failed: "存储失败",
  broken_provenance: "来源缺失或已损坏",
  cancelled: "已取消",
  budget_exceeded: "超出上下文预算",
};

// Rules that say more than their code (ADR-MEM-46 and the import pipeline).
const RULES: Record<string, string> = {
  "import.resume_existing": "这个文件之前的导入被中断了，请在导入记录中继续它，而不是重新开始",
  "import.adapter_changed": "这次导入已无法继续：当前版本解析该文件的方式与开始导入时不同",
  "workspace.vault_in_use": "这个 Vault 已在另一个应用中打开，请先在那里锁定或退出",
  "root.inside_repository": "该文件夹位于 Git 工作区内，不能作为 Vault",
  "root.cloud_sync_folder": "该文件夹位于同步盘（如 OneDrive）中，不能作为 Vault",
  "root.not_local_fixed_disk": "该文件夹不在本机固定磁盘上",
  "root.unsupported_filesystem": "该磁盘不是 NTFS 或 ReFS",
  "root.system_location": "不能使用 Program Files 或 Windows 目录",
  "root.reparse_point": "路径中有符号链接或连接点",
  "root.not_canonical": "请使用文件夹的完整原始路径（不是短名称或映射盘）",
  "root.network_path": "不能使用网络位置",
  "root.insufficient_space": "磁盘剩余空间不足",
  "root.missing": "文件夹不存在",
  "root.not_a_directory": "所选项目不是文件夹",
  "root.unreadable": "无法读取该文件夹的属性",
};

export function describe(error: unknown): string {
  if (error instanceof CallError) {
    const specific = error.error.rules.map((r) => RULES[r]).find(Boolean);
    if (specific) return specific;
    const text = CODES[error.error.code] ?? error.error.code;
    const rules = error.error.rules.length ? `（${error.error.rules.join(", ")}）` : "";
    return `${text}${rules}`;
  }
  return "后台调用失败";
}

export const retryable = (error: unknown) => error instanceof CallError && error.error.retryable;
