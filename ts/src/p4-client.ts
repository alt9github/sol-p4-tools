import { invoke } from "@tauri-apps/api/core";

export interface P4Workspace { name: string; stream: string; root: string; }
export interface P4PendingFile { depot_path: string; local_path: string; action: string; }
export interface P4Pending { files: P4PendingFile[]; }
export interface P4Diff { file: string; diff: string; }

export const p4ListWorkspaces = (server: string, user: string) =>
  invoke<P4Workspace[]>("list_p4_workspaces", { server, user });

export const p4SetConnection = (server: string, user: string, client: string) =>
  invoke<void>("set_p4_connection", { server, user, client });

export const p4ClearConnection = () =>
  invoke<void>("clear_p4_connection");

export const p4GetStream = (dataDir?: string) =>
  invoke<string>("get_p4_stream", { dataDir });

export const p4GetPending = (pattern: string) =>
  invoke<P4Pending>("get_p4_pending", { pattern });

export const p4GetDiff = (filePath: string, action: string) =>
  invoke<P4Diff>("get_p4_diff", { filePath, action });

export const p4CheckStaleRevisions = (pattern: string) =>
  invoke<string[]>("check_stale_revisions", { pattern });

export const p4CheckConcurrentEdits = (pattern: string) =>
  invoke<string[]>("check_concurrent_edits", { pattern });

export type P4ErrorKind =
  | "connection" | "auth" | "workspace" | "not_in_view"
  | "locked" | "stale" | "not_installed" | "unknown";

export interface P4Error {
  kind: P4ErrorKind;
  message: string;
  detail: string;
}

const PATTERNS: { kind: P4ErrorKind; re: RegExp; msg: string }[] = [
  { kind: "not_installed", re: /no such file|command not found|program not found|cannot find.*p4/i,
    msg: "p4 명령을 찾을 수 없습니다. Perforce CLI가 설치되어 있는지 확인하세요." },
  { kind: "auth", re: /password.*invalid|password.*unset|ticket.*expired|session.*expired|login.*required|p4passwd/i,
    msg: "P4 로그인이 만료되었습니다. 터미널에서 'p4 login' 후 다시 시도하세요." },
  { kind: "connection", re: /connect.*server.*failed|tcp connect|network is unreachable|connection refused|timed? out|name resolution/i,
    msg: "P4 서버에 연결할 수 없습니다. 네트워크 또는 서버 주소를 확인하세요." },
  { kind: "workspace", re: /client '?[^']*'? unknown|no such client|use 'client' command to create/i,
    msg: "워크스페이스(client)를 찾을 수 없습니다. 연결 설정을 다시 확인하세요." },
  { kind: "not_in_view", re: /not in client view|file\(s\) not in client|no such file\(s\)/i,
    msg: "파일이 현재 워크스페이스 영역 밖에 있습니다." },
  { kind: "locked", re: /locked by|exclusive lock/i,
    msg: "다른 사용자가 잠근 파일이 있습니다." },
  { kind: "stale", re: /must (sync|resolve)|can't .* without sync|file\(s\) up-to-date/i,
    msg: "최신 리비전이 아닙니다. p4 sync 후 다시 시도하세요." },
];

export function categorizeP4Error(err: unknown): P4Error {
  const raw = String(err ?? "");
  for (const p of PATTERNS) {
    if (p.re.test(raw)) return { kind: p.kind, message: p.msg, detail: raw };
  }
  return { kind: "unknown", message: `P4 오류: ${raw}`, detail: raw };
}

export function formatP4Error(err: unknown, context?: string): string {
  const e = categorizeP4Error(err);
  return context ? `[${context}] ${e.message}` : e.message;
}
