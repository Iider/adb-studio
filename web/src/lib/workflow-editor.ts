import type { Node, Edge } from "@xyflow/svelte";

export type Kind =
  | "back"
  | "home"
  | "recents"
  | "wait"
  | "tap"
  | "swipe"
  | "loop"
  | "condition"
  | "wait_condition"
  | "break"
  | "continue"
  | "end";
export type Condition = {
  regions?: CoordinateProfile[];
  source: "text" | "app" | "round";
  operator: "contains" | "not_contains" | "is" | "is_not" | "eq" | "gt" | "lt";
  value: string;
  guard?: string;
};
export type Config = {
  profiles?: CoordinateProfile[];
  samples?: ConditionSample[];
  seconds?: number;
  count?: number;
  interval?: number;
  x?: number;
  y?: number;
  from_x?: number;
  from_y?: number;
  to_x?: number;
  to_y?: number;
  duration_ms?: number;
  mode?: "all" | "any";
  conditions?: Condition[];
  timeout?: number;
  loop?: string;
  result?: "succeeded" | "needs_help";
};
export type CoordinateProfile = {
  device_id: string;
  width: number;
  height: number;
  display: string;
  x: number;
  y: number;
  to_x: number;
  to_y: number;
};
export type ConditionSample = {
  text: string;
  app: string;
  round: number;
  expected: "yes" | "no" | "unknown";
};
export type DraftNode = {
  id: string;
  kind: Kind;
  label: string;
  config: Config;
  targets: Record<string, string>;
  position: [number, number];
};
export type Draft = {
  id: string;
  revision: number;
  name: string;
  description: string;
  entry: string;
  nodes: DraftNode[];
  viewport: [number, number, number];
  updated_at: string;
};
export type CanvasData = {
  running?: boolean;
  kind: Kind | "entry";
  label: string;
  config: Config;
  targets: Record<string, string>;
  issue?: string;
};
export type CanvasNode = Node<CanvasData, "step">;
export type Issue = { node?: string; message: string };

export function canonical(value: unknown): string {
  function ordered(input: unknown): unknown {
    if (Array.isArray(input)) return input.map(ordered);
    if (input && typeof input === "object")
      return Object.fromEntries(
        Object.entries(input)
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([key, item]) => [key, ordered(item)]),
      );
    return input;
  }
  return JSON.stringify(ordered(value));
}

export function actionLimit(draft: Draft): number | null {
  const nodes = new Map(draft.nodes.map((node) => [node.id, node]));
  if (!nodes.has(draft.entry)) return null;
  const links = new Map<string, string[]>();
  for (const node of draft.nodes) {
    let next = ports(node.kind).map((port) => node.targets[port.id]);
    if (["break", "continue"].includes(node.kind)) {
      const loop = nodes.get(node.config.loop ?? "");
      if (loop?.kind !== "loop") return null;
      next = [node.kind === "break" ? loop.targets.done : loop.id];
    }
    if (next.some((id) => !nodes.has(id))) return null;
    links.set(node.id, next);
  }
  const owners = new Map<string, string>();
  for (const loop of draft.nodes.filter((node) => node.kind === "loop")) {
    if (
      !Number.isInteger(loop.config.count) ||
      Number(loop.config.count) < 1 ||
      Number(loop.config.count) > 10000 ||
      loop.targets.body === loop.targets.done
    )
      return null;
    const todo = [loop.targets.body],
      seen = new Set<string>();
    while (todo.length) {
      const id = todo.pop()!;
      if (id === loop.id || seen.has(id)) continue;
      if (id === loop.targets.done) return null;
      seen.add(id);
      const node = nodes.get(id)!;
      if (node.kind === "end") continue;
      if (node.kind === "loop" || owners.has(id)) return null;
      owners.set(id, loop.id);
      if (["break", "continue"].includes(node.kind)) {
        if (node.config.loop !== loop.id) return null;
      } else todo.push(...links.get(id)!);
    }
  }
  const visiting = new Set<string>(),
    reached = new Set<string>();
  function visit(id: string): boolean {
    if (visiting.has(id)) return false;
    if (reached.has(id)) return true;
    reached.add(id);
    visiting.add(id);
    for (const next of links.get(id)!) {
      if (
        owners.get(next) &&
        owners.get(id) !== owners.get(next) &&
        id !== owners.get(next)
      )
        return false;
      if (owners.get(id) !== next && !visit(next)) return false;
    }
    visiting.delete(id);
    return true;
  }
  if (
    owners.has(draft.entry) ||
    !visit(draft.entry) ||
    reached.size !== nodes.size
  )
    return null;
  let limit = 0;
  for (const node of draft.nodes) {
    const count = ["tap", "swipe"].includes(node.kind)
      ? node.config.count
      : ["back", "home", "recents"].includes(node.kind)
        ? 1
        : 0;
    if (!Number.isInteger(count) || Number(count) < 0 || Number(count) > 10000)
      return null;
    limit +=
      Number(count) *
      (owners.has(node.id)
        ? Number(nodes.get(owners.get(node.id)!)!.config.count)
        : 1);
  }
  return limit;
}

export const palette: {
  kind: Kind;
  label: string;
  hint: string;
  group: string;
}[] = [
  { kind: "back", label: "返回键", hint: "返回上一个页面", group: "基础动作" },
  { kind: "home", label: "主页键", hint: "回到手机桌面", group: "基础动作" },
  {
    kind: "recents",
    label: "最近任务",
    hint: "打开最近任务列表",
    group: "基础动作",
  },
  { kind: "wait", label: "等待", hint: "等待指定秒数", group: "基础动作" },
  {
    kind: "tap",
    label: "定点点击",
    hint: "设置位置、次数与间隔",
    group: "基础动作",
  },
  {
    kind: "swipe",
    label: "滑动",
    hint: "设置路线、次数与间隔",
    group: "基础动作",
  },
  { kind: "loop", label: "循环", hint: "重复执行一组步骤", group: "流程控制" },
  {
    kind: "condition",
    label: "条件判断",
    hint: "符合与不符合分别处理",
    group: "流程控制",
  },
  {
    kind: "wait_condition",
    label: "等待条件",
    hint: "等条件出现，支持超时",
    group: "流程控制",
  },
  {
    kind: "break",
    label: "退出循环",
    hint: "提前完成所属循环",
    group: "流程控制",
  },
  {
    kind: "continue",
    label: "进入下一轮",
    hint: "跳过本轮剩余步骤",
    group: "流程控制",
  },
  {
    kind: "end",
    label: "结束",
    hint: "完成任务或交给人工处理",
    group: "流程控制",
  },
];

export function ports(
  kind: CanvasData["kind"],
): { id: string; label: string }[] {
  if (kind === "entry") return [{ id: "next", label: "开始执行" }];
  if (kind === "loop")
    return [
      { id: "body", label: "执行本轮" },
      { id: "done", label: "循环完成" },
    ];
  if (kind === "condition")
    return [
      { id: "yes", label: "符合" },
      { id: "no", label: "不符合" },
    ];
  if (kind === "wait_condition")
    return [
      { id: "yes", label: "已满足" },
      { id: "timeout", label: "已超时" },
    ];
  if (["end", "break", "continue"].includes(kind)) return [];
  return [{ id: "next", label: "下一步" }];
}

export function newCondition(): Condition {
  return { source: "text", operator: "contains", value: "", guard: "" };
}

export function defaults(kind: Kind): Config {
  switch (kind) {
    case "wait":
      return { seconds: 2 };
    case "tap":
      return { x: 0.5, y: 0.5, count: 1, interval: 1 };
    case "swipe":
      return {
        from_x: 0.5,
        from_y: 0.8,
        to_x: 0.5,
        to_y: 0.3,
        duration_ms: 300,
        count: 1,
        interval: 1,
      };
    case "loop":
      return { count: 3, interval: 5 };
    case "condition":
      return { mode: "all", conditions: [newCondition()] };
    case "wait_condition":
      return {
        mode: "all",
        conditions: [newCondition()],
        interval: 2,
        timeout: 40,
      };
    case "break":
    case "continue":
      return { loop: "" };
    case "end":
      return { result: "succeeded" };
    default:
      return {};
  }
}

export function summary(data: CanvasData): string {
  const c = data.config;
  switch (data.kind) {
    case "entry":
      return "从这里开始";
    case "wait":
      return `等待 ${c.seconds ?? 0} 秒`;
    case "tap":
      if (c.profiles?.length)
        return "已标注 " + c.profiles.length + " 个屏幕 · " + c.count + " 次";
      return `位置 ${Math.round((c.x ?? 0) * 100)}%、${Math.round((c.y ?? 0) * 100)}% · ${c.count} 次`;
    case "swipe":
      return `${c.count} 次 · 每次 ${c.duration_ms} 毫秒`;
    case "loop":
      return `${c.count} 轮 · 轮间等待 ${c.interval} 秒`;
    case "condition":
    case "wait_condition":
      return `${c.mode === "any" ? "任一" : "全部"}满足 · ${c.conditions?.length ?? 0} 个条件`;
    case "break":
      return "从所属循环的完成出口继续";
    case "continue":
      return "跳过本轮余下步骤";
    case "end":
      return c.result === "needs_help" ? "需要人工处理" : "任务完成";
    default:
      return palette.find((item) => item.kind === data.kind)?.hint ?? "";
  }
}

export function blankDraft(): Draft {
  return {
    id: crypto.randomUUID(),
    revision: 0,
    name: "未命名工作流",
    description: "",
    entry: "",
    nodes: [],
    viewport: [30, 25, 0.9],
    updated_at: "",
  };
}

export function initialDraft(
  drafts: Draft[],
  requestedId: string | null,
  backup: Draft | null = null,
): { draft: Draft; saved: boolean } {
  if (!requestedId) return { draft: blankDraft(), saved: false };
  const chosen = drafts.find((draft) => draft.id === requestedId);
  if (chosen) return { draft: chosen, saved: true };
  if (backup?.id === requestedId) {
    return {
      draft: {
        ...backup,
        id: backup.revision === 0 ? requestedId : crypto.randomUUID(),
        revision: 0,
        updated_at: "",
      },
      saved: false,
    };
  }
  throw new Error("这份草稿已不存在，请返回任务列表查看");
}

export function toCanvas(draft: Draft): CanvasNode[] {
  return [
    {
      id: "__entry",
      type: "step",
      data: { kind: "entry", label: "开始", config: {}, targets: {} },
      position: { x: 280, y: 0 },
      deletable: false,
      draggable: false,
    },
    ...draft.nodes.map((node) => ({
      id: node.id,
      type: "step" as const,
      data: {
        kind: node.kind,
        label: node.label,
        config: node.config,
        targets: node.targets,
      },
      position: { x: node.position[0], y: node.position[1] },
    })),
  ];
}

export function edgesFor(
  nodes: CanvasNode[],
  entry: string,
  selected = "",
): Edge[] {
  const ids = new Set(nodes.map((node) => node.id));
  return nodes.flatMap((node) => {
    const targets = node.id === "__entry" ? { next: entry } : node.data.targets;
    const links = Object.entries(targets).map(([port, target]) => ({
      port,
      target,
      association: false,
    }));
    if (
      ["break", "continue"].includes(node.data.kind) &&
      node.data.config.loop
    ) {
      const loop = nodes.find(
        (item) =>
          item.id === node.data.config.loop && item.data.kind === "loop",
      );
      const target =
        node.data.kind === "continue" ? loop?.id : loop?.data.targets.done;
      if (target) links.push({ port: "control", target, association: true });
    }
    return links
      .filter((link) => link.target && ids.has(link.target))
      .map((link) => ({
        id: `${node.id}:${link.port}`,
        source: node.id,
        target: link.target,
        sourceHandle: link.port,
        selected: selected === `${node.id}:${link.port}`,
        label: link.association
          ? node.data.kind === "continue"
            ? "下一轮"
            : "退出循环"
          : ports(node.data.kind).find((port) => port.id === link.port)?.label,
        type: "smoothstep",
        deletable: !link.association,
        style: link.association ? "stroke-dasharray: 5 4" : undefined,
        markerEnd: { type: "arrowclosed" as const },
      }));
  });
}

export function checkGraph(draft: Draft): Issue[] {
  const issues: Issue[] = [];
  const ids = new Set(draft.nodes.map((node) => node.id));
  if (!draft.nodes.length) return [{ message: "先添加一个步骤" }];
  if (!ids.has(draft.entry)) issues.push({ message: "请选择开始执行的步骤" });
  for (const node of draft.nodes) {
    for (const port of ports(node.kind)) {
      if (!ids.has(node.targets[port.id]))
        issues.push({
          node: node.id,
          message: `“${node.label}”的“${port.label}”尚未连接`,
        });
    }
    const c = node.config;
    if (
      ["tap", "swipe", "loop"].includes(node.kind) &&
      (!Number.isInteger(c.count) ||
        Number(c.count) < 1 ||
        Number(c.count) > 10000)
    )
      issues.push({
        node: node.id,
        message: `“${node.label}”的次数应为 1 到 10000 的整数`,
      });
    if (
      node.kind === "wait" &&
      (!Number.isFinite(c.seconds) ||
        Number(c.seconds) < 0 ||
        Number(c.seconds) > 86400)
    )
      issues.push({ node: node.id, message: "等待时间应为 0 到 86400 秒" });
    if (
      ["tap", "swipe", "loop", "wait_condition"].includes(node.kind) &&
      (!Number.isFinite(c.interval) ||
        Number(c.interval) < 0 ||
        Number(c.interval) > 86400 ||
        (node.kind === "wait_condition" && Number(c.interval) === 0))
    )
      issues.push({ node: node.id, message: `“${node.label}”的间隔无效` });
    const coordinates =
      node.kind === "tap"
        ? [c.x, c.y]
        : node.kind === "swipe"
          ? [c.from_x, c.from_y, c.to_x, c.to_y]
          : [];
    if (
      coordinates.some(
        (value) =>
          !Number.isFinite(value) || Number(value) < 0 || Number(value) > 1,
      )
    )
      issues.push({
        node: node.id,
        message: `“${node.label}”的位置应在 0% 到 100% 之间`,
      });
    if (
      node.kind === "swipe" &&
      (!Number.isFinite(c.duration_ms) ||
        Number(c.duration_ms) < 100 ||
        Number(c.duration_ms) > 2000)
    )
      issues.push({
        node: node.id,
        message: "单次滑动时长应为 100 到 2000 毫秒",
      });
    if (
      node.kind === "wait_condition" &&
      (!Number.isFinite(c.timeout) ||
        Number(c.timeout) <= 0 ||
        Number(c.timeout) > 86400)
    )
      issues.push({
        node: node.id,
        message: "最长等待应大于 0，最多 86400 秒",
      });
    if (
      ["break", "continue"].includes(node.kind) &&
      !draft.nodes.some((item) => item.id === c.loop && item.kind === "loop")
    )
      issues.push({
        node: node.id,
        message: `请为“${node.label}”选择所属循环`,
      });
    if (
      ["condition", "wait_condition"].includes(node.kind) &&
      (!c.conditions?.length ||
        c.conditions.some((condition) => !condition.value.trim()))
    )
      issues.push({
        node: node.id,
        message: `请补齐“${node.label}”的判断内容`,
      });
  }
  const reached = new Set<string>();
  const todo = [draft.entry];
  while (todo.length) {
    const id = todo.pop()!;
    if (reached.has(id)) continue;
    reached.add(id);
    const node = draft.nodes.find((item) => item.id === id);
    if (node) {
      todo.push(...Object.values(node.targets));
      if (["break", "continue"].includes(node.kind) && node.config.loop)
        todo.push(node.config.loop);
    }
  }
  for (const node of draft.nodes)
    if (!reached.has(node.id))
      issues.push({ node: node.id, message: `从开始无法到达“${node.label}”` });
  if (!draft.nodes.some((node) => node.kind === "end" && reached.has(node.id)))
    issues.push({ message: "流程尚未连接到结束步骤" });
  return issues;
}
