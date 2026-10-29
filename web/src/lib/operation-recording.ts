import {
  defaults,
  type CanvasNode,
  type Config,
  type CoordinateProfile,
  type Kind,
} from "./workflow-editor.ts";
import type { ScreenPoint } from "./touch.ts";

export type RecordedInput =
  | { type: "gesture"; points: ScreenPoint[]; started: number; ended: number }
  | { type: "key"; key: 3 | 4 | 187; started: number; ended: number };
export type RecordingScreen = {
  width: number;
  height: number;
  display: string;
  input_display: number;
};
export type RecordedAction = {
  kind: Kind;
  config: Config;
  started: number;
  ended: number;
};

export function sameScreen(a: RecordingScreen, b: RecordingScreen): boolean {
  return (
    a.width === b.width &&
    a.height === b.height &&
    a.display === b.display &&
    a.input_display === b.input_display
  );
}

export function recordingAction(
  input: RecordedInput,
  device: string,
  screen: RecordingScreen,
): RecordedAction {
  const { started, ended } = input;
  if (!Number.isFinite(started) || !Number.isFinite(ended) || ended < started)
    throw new Error("动作时间无效，请重新录制");
  if (input.type === "key")
    return {
      kind: ({ 3: "home", 4: "back", 187: "recents" } as const)[input.key],
      config: {},
      started,
      ended,
    };
  const first = input.points[0],
    last = input.points.at(-1);
  if (
    !first ||
    !last ||
    input.points.some(
      (p) => p.width !== first.width || p.height !== first.height,
    )
  )
    throw new Error("屏幕尺寸已变化，请重新核对后继续");
  if (screen.input_display !== 0)
    throw new Error("当前投屏无法确认这块屏幕的位置，请使用主屏录制");
  const ratio = first.width / first.height;
  if (Math.abs(ratio - screen.width / screen.height) > 0.015)
    throw new Error("投屏与手机屏幕比例不符，请重新开启投屏");
  const points = input.points.map((p) => ({
    x: (p.x / p.width) * screen.width,
    y: (p.y / p.height) * screen.height,
  }));
  const a = points[0],
    b = points.at(-1)!;
  const tolerance = Math.max(8, Math.min(screen.width, screen.height) * 0.012);
  const travel = Math.max(
    ...points.map((p) => Math.hypot(p.x - a.x, p.y - a.y)),
  );
  const duration = Math.round(ended - started);
  const kind = travel <= tolerance ? "tap" : "swipe";
  if (kind === "tap" && duration >= 500)
    throw new Error("暂不支持长按；已暂停，之前的动作已保留");
  if (kind === "swipe") {
    if (duration < 100 || duration > 2000)
      throw new Error("滑动需在 0.1～2 秒内完成；本次滑动未记录");
    const dx = b.x - a.x,
      dy = b.y - a.y,
      length = Math.hypot(dx, dy);
    if (length <= tolerance)
      throw new Error("暂不支持折返或曲线手势；本次手势未记录");
    let furthest = 0;
    for (const p of points) {
      const along = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length;
      const across = Math.abs((p.x - a.x) * dy - (p.y - a.y) * dx) / length;
      if (
        across > Math.max(tolerance, length * 0.08) ||
        along < furthest - tolerance ||
        along > length + tolerance
      )
        throw new Error("暂不支持曲线或折返手势；本次手势未记录");
      furthest = Math.max(furthest, along);
    }
  }
  const profile: CoordinateProfile = {
    device_id: device,
    width: screen.width,
    height: screen.height,
    display: screen.display,
    x: first.x / first.width,
    y: first.y / first.height,
    to_x: last.x / last.width,
    to_y: last.y / last.height,
  };
  return {
    kind,
    started,
    ended,
    config: {
      ...defaults(kind),
      count: 1,
      interval: 0,
      after_wait: 0,
      profiles: [profile],
      ...(kind === "swipe" ? { duration_ms: duration } : {}),
    },
  };
}

const labels: Partial<Record<Kind, string>> = {
  tap: "定点点击",
  swipe: "滑动",
  back: "返回键",
  home: "主页键",
  recents: "最近任务",
  wait: "等待",
};
export function recordingSteps(
  actions: RecordedAction[],
): { kind: Kind; config: Config }[] {
  const steps: { kind: Kind; config: Config }[] = [];
  actions.forEach((action, index) => {
    const previous = actions[index - 1];
    if (previous) {
      const gap = Math.max(
        0,
        Math.round((action.started - previous.ended) / 10) / 100,
      );
      if (gap > 86400) throw new Error("两次动作间隔超过 24 小时，请分段录制");
      if (gap > 0) {
        const last = steps.at(-1)!;
        if (last.kind === "tap" || last.kind === "swipe")
          last.config.after_wait = gap;
        else steps.push({ kind: "wait", config: { seconds: gap } });
      }
    }
    steps.push({
      kind: action.kind,
      config: JSON.parse(JSON.stringify(action.config)),
    });
  });
  return steps;
}

export function recordingNodes(
  actions: RecordedAction[],
  existing: CanvasNode[],
): CanvasNode[] {
  const steps = recordingSteps(actions);
  if (
    existing.filter((n) => n.data.kind !== "entry").length + steps.length >
    200
  )
    throw new Error("加入后超过 200 个步骤，请先减少画布中的节点");
  const right = Math.max(
    0,
    ...existing.map(
      (n) => n.position.x + (n.measured?.width ?? n.width ?? 260),
    ),
  );
  const top = Math.min(0, ...existing.map((n) => n.position.y));
  const nodes: CanvasNode[] = steps.map((step, i) => ({
    id: crypto.randomUUID(),
    type: "step",
    position: { x: right + 120, y: top + i * 240 },
    data: { ...step, label: labels[step.kind]!, targets: {} },
  }));
  nodes.forEach((node, i) => {
    if (nodes[i + 1]) node.data.targets.next = nodes[i + 1].id;
  });
  return nodes;
}

/** 以有效录制时长表示事件时间，暂停段不进入动作间隔。 */
export class RecordingClock {
  private accumulated = 0;
  private started: number | null = null;
  resume(now: number) {
    if (this.started === null) this.started = now;
  }
  at(now: number) {
    return this.accumulated + (this.started === null ? 0 : now - this.started);
  }
  pause(now: number) {
    this.accumulated = this.at(now);
    this.started = null;
  }
}
