import { get, writable } from "svelte/store";

export const TOAST_ENTER_MS = 320;
export const TOAST_HOLD_MS = 3000;

type Notification = { id: number; kind: "success" | "error"; message: string };
export const notifications = writable<Notification[]>([]);
let nextId = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function dismiss(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
  notifications.update((items) => items.filter((item) => item.id !== id));
}

function show(kind: Notification["kind"], message: string) {
  message = message.trim();
  if (!message) return;
  if (["Failed to fetch", "Load failed"].includes(message))
    message = "连接失败，请检查工作台是否在线。";
  const current = get(notifications);
  if (current.some((item) => item.kind === kind && item.message === message))
    return;
  if (current.length >= 3) dismiss(current[0].id);
  const id = ++nextId;
  notifications.update((items) => [...items, { id, kind, message }]);
  timers.set(
    id,
    setTimeout(() => dismiss(id), TOAST_ENTER_MS + TOAST_HOLD_MS),
  );
}

export const notify = {
  success: (message: string) => show("success", message),
  error: (message: string) => show("error", message),
  dismiss,
  clear: () => {
    for (const timer of timers.values()) clearTimeout(timer);
    timers.clear();
    notifications.set([]);
  },
};
