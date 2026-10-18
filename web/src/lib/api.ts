let token = "";
export async function session() {
  const response = await fetch("/api/session");
  if (!response.ok) throw new Error("后台未连接");
  token = (await response.json()).token;
}
export async function api<T>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: "后台未连接" }));
    throw new Error(error.error || "操作失败");
  }
  return response.json();
}
export async function photo(id: string) {
  const r = await fetch(`/api/devices/${encodeURIComponent(id)}/screenshot`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!r.ok) {
    const e = await r.json();
    throw new Error(e.error);
  }
  return URL.createObjectURL(await r.blob());
}
export interface TapPoint {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Device {
  tap_point?: TapPoint | null;
  id: string;
  name: string;
  model: string;
  online: boolean;
  connection: string;
  last_seen: string | null;
  keep_awake: boolean;
  awake_applied: boolean;
  restore_pending: boolean;
  locked: boolean;
  note: string;
  endpoints: string[];
}
export interface Node {
  id: string;
  label: string;
  type: string;
  next?: string;
  cases?: { contains: string; next: string }[];
  otherwise?: string;
  [key: string]: unknown;
}
export interface Workflow {
  id: string;
  version: number;
  name: string;
  description: string;
  devices: string[];
  entry: string;
  nodes: Node[];
  active: boolean;
  sample_validated: boolean;
  model_limit: number;
}
export interface Run {
  id: string;
  device_id: string;
  workflow: Workflow;
  state: string;
  node: string;
  started_at: string;
  ended_at: string | null;
  reason: string;
  model_calls: number;
  trial: boolean;
  checkpoint: Record<string, unknown>;
  last_shot: string | null;
}
export type Rule =
  | { kind: "once"; at: string }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; time: string; days: number[] }
  | { kind: "interval"; minutes: number };
export interface Schedule {
  tap?: { count: number; interval: number } | null;
  swipe?: { minutes: number; interval: number } | null;
  id: string;
  name: string;
  device_id: string;
  workflow_id: string;
  enabled: boolean;
  rule: Rule;
  next_at: string;
  last_result: string | null;
}
export interface Settings {
  model: { endpoint: string; model: string; api: string; key_env: string };
  autostart: boolean;
  timezone: string;
  retention_days: number;
  host: string;
}
export interface Event {
  id: string;
  at: string;
  node: string;
  message: string;
}
export function busy(run: Run) {
  return ["running", "waiting_device", "recovering"].includes(run.state);
}
export function time(value: string | null, short = false) {
  if (!value) return "尚未连接";
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    ...(short ? {} : { month: "2-digit", day: "2-digit" }),
    hour: "2-digit",
    minute: "2-digit",
    second: short ? "2-digit" : undefined,
    hour12: false,
  }).format(new Date(value));
}
export function state(value: string) {
  return (
    (
      {
        running: "执行中",
        waiting_device: "等待手机",
        recovering: "恢复中",
        succeeded: "已完成",
        needs_help: "需要处理",
        cancelled: "已停止",
      } as Record<string, string>
    )[value] || value
  );
}
