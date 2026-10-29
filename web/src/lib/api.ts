import type { InputDefinition } from "./workflow-inputs";
let token = "";
export async function session() {
  const response = await fetch("/api/session");
  if (!response.ok) throw new Error("后台未连接");
  token = (await response.json()).token;
}
export async function apiResponse(
  path: string,
  method = "GET",
  body?: unknown,
  signal?: AbortSignal,
  extraHeaders?: Record<string, string>,
): Promise<Response> {
  const response = await fetch(`/api${path}`, {
    method,
    signal,
    headers: {
      ...extraHeaders,
      Authorization: `Bearer ${token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: "后台未连接" }));
    throw Object.assign(new Error(error.error || "操作失败"), {
      status: response.status,
    });
  }
  return response;
}
export async function api<T>(
  path: string,
  method = "GET",
  body?: unknown,
  signal?: AbortSignal,
  extraHeaders?: Record<string, string>,
): Promise<T> {
  return (await apiResponse(path, method, body, signal, extraHeaders)).json();
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
export interface Device {
  id: string;
  name: string;
  model: string;
  online: boolean;
  connection: string;
  last_seen: string | null;
  keep_awake: boolean;
  display_off: boolean;
  awake_applied: boolean;
  restore_pending: boolean;
  locked: boolean;
  note: string;
  endpoints: string[];
}
export interface NearbyDevice {
  name: string;
  address: string;
  kind: "connect" | "pair";
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
  inputs?: InputDefinition[];
  id: string;
  name: string;
  description: string;
  devices: string[];
  entry: string;
  nodes: Node[];
  tests?: { image?: string }[];
  recognition?: "local" | "vision";
}
export interface Run {
  inputs?: Record<string, number>;
  input_overrides?: Record<string, number>;
  id: string;
  device_id: string;
  workflow: Workflow;
  state: string;
  node: string;
  started_at: string;
  ended_at: string | null;
  reason: string;
  model_calls: number;
  checkpoint: Record<string, unknown>;
  last_shot: string | null;
  pending?: string | null;
  error_detail?: string | null;
  pause_requested?: boolean;
}
export type Rule =
  | { kind: "once"; at: string }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; time: string; days: number[] }
  | { kind: "interval"; minutes: number };
export interface Schedule {
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
  platform: string;
  arch: string;
  local_ocr: boolean;
  model_configured: boolean;
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
  return ["running", "waiting_device", "recovering", "paused"].includes(
    run.state,
  );
}
export function runLabel(run: Run) {
  return state(
    run.pause_requested && run.state !== "paused" ? "pausing" : run.state,
  );
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
        pausing: "正在暂停",
        paused: "已暂停",
        succeeded: "已完成",
        needs_help: "需要处理",
        cancelled: "已停止",
      } as Record<string, string>
    )[value] || value
  );
}

export interface VisionInspection {
  id: string;
  image: string;
  size: [number, number];
  query: { condition: string; target: string | null; read_number: boolean };
  model: Settings["model"];
  calls: number;
  capture_ms: number;
  model_ms: number;
  result: {
    choice: "confirmed" | "unknown";
    reason: string;
    target: [number, number] | null;
    coin: number | null;
  } | null;
  error: string | null;
}
