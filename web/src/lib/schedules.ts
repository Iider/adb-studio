import type { Rule, Schedule } from "./api";

export type ScheduleInput = Pick<
  Schedule,
  "name" | "device_id" | "workflow_id" | "enabled" | "rule" | "tap" | "swipe"
>;

export const weekdays = [
  "周一",
  "周二",
  "周三",
  "周四",
  "周五",
  "周六",
  "周日",
];

export function shanghaiDateInput(at: string) {
  return new Date(new Date(at).getTime() + 8 * 3600000)
    .toISOString()
    .slice(0, 16);
}

export function scheduleTime(at: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(at));
}

export function ruleLabel(rule: Rule) {
  if (rule.kind === "once") return `单次 · ${scheduleTime(rule.at)}`;
  if (rule.kind === "interval") return `每 ${rule.minutes} 分钟`;
  if (rule.kind === "weekly")
    return `${[...rule.days]
      .sort((a, b) => a - b)
      .map((day) => weekdays[day - 1])
      .join("、")} ${rule.time}`;
  return `每天 ${rule.time}`;
}

export function onceExpired(schedule: Schedule, now = Date.now()) {
  return (
    schedule.rule.kind === "once" && new Date(schedule.rule.at).getTime() <= now
  );
}

export function scheduleInput(schedule: Schedule): ScheduleInput {
  return {
    name: schedule.name,
    device_id: schedule.device_id,
    workflow_id: schedule.workflow_id,
    enabled: schedule.enabled,
    rule: schedule.rule,
    tap: schedule.tap ?? null,
    swipe: schedule.swipe ?? null,
  };
}
