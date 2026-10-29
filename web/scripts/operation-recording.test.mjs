import { test } from "node:test";
import assert from "node:assert/strict";
import {
  recordingAction,
  recordingSteps,
  recordingNodes,
  sameScreen,
  RecordingClock,
} from "../src/lib/operation-recording.ts";
import { checkGraph, blankDraft } from "../src/lib/workflow-editor.ts";
import { screenPoint } from "../src/lib/touch.ts";
const screen = {
  width: 1080,
  height: 2400,
  display: "default",
  input_display: 0,
};
const point = (x, y) => ({ x, y, width: 576, height: 1280 });
const gesture = (points, started = 1000, ended = 1300) => ({
  type: "gesture",
  points,
  started,
  ended,
});
const action = (input) => recordingAction(input, "phone-a", screen);
const key = (key, started) =>
  action({ type: "key", key, started, ended: started + 1 });

test("投屏缩放与黑边不影响手机坐标，轻微抖动仍为单击", () => {
  const p = screenPoint(
    { left: 0, top: 0, width: 600, height: 600 },
    576,
    1280,
    300,
    300,
  );
  assert.equal(
    screenPoint(
      { left: 0, top: 0, width: 600, height: 600 },
      576,
      1280,
      5,
      300,
    ),
    undefined,
  );
  const result = action(gesture([p, { ...p, x: p.x + 1 }], 100, 180));
  assert.equal(result.kind, "tap");
  assert.deepEqual(result.config.profiles[0], {
    device_id: "phone-a",
    width: 1080,
    height: 2400,
    display: "default",
    x: 0.5,
    y: 0.5,
    to_x: (p.x + 1) / 576,
    to_y: 0.5,
  });
});
test("直线滑动保留原始时长和坐标，不改写动作参数", () => {
  const result = action(
    gesture([point(200, 1000), point(201, 700), point(200, 400)], 100, 475),
  );
  assert.equal(result.kind, "swipe");
  assert.equal(result.config.duration_ms, 375);
  assert.equal(result.config.count, 1);
  assert.equal(result.config.profiles[0].to_y, 400 / 1280);
});
test("拒绝长按、曲线、折返、过短与过长滑动、屏幕变化", () => {
  for (const input of [
    gesture([point(200, 400), point(200, 400)], 0, 500),
    gesture([point(200, 1000), point(400, 700), point(200, 400)]),
    gesture([point(200, 1000), point(200, 400), point(200, 1000)]),
    gesture([point(200, 1000), point(200, 400)], 0, 99),
    gesture([point(200, 1000), point(200, 400)], 0, 2001),
    gesture([
      point(200, 1000),
      { ...point(200, 400), width: 1280, height: 576 },
    ]),
  ])
    assert.throws(() => action(input));
  assert.throws(() =>
    recordingAction(gesture([point(100, 100), point(100, 100)]), "phone-a", {
      ...screen,
      input_display: 2,
    }),
  );
  assert.equal(sameScreen(screen, { ...screen, display: "other" }), false);
});
test("等待附在点击后，导航键之间补等待，首尾空闲不产生节点", () => {
  const tap = action(gesture([point(200, 400), point(200, 400)], 10000, 10050));
  const actions = [tap, key(4, 11550), key(3, 12551), key(187, 13552)];
  const steps = recordingSteps(actions);
  assert.deepEqual(
    steps.map((s) => s.kind),
    ["tap", "back", "wait", "home", "wait", "recents"],
  );
  assert.equal(steps[0].config.after_wait, 1.5);
  assert.equal(steps[2].config.seconds, 1);
  assert.equal(tap.config.after_wait, 0);
});
test("暂停时间不进入动作间隔，保留暂停前后有效等待", () => {
  const clock = new RecordingClock();
  clock.resume(1000);
  const a = key(4, clock.at(1500));
  clock.pause(2000);
  assert.equal(clock.at(50000), 1000);
  clock.resume(60000);
  const b = key(3, clock.at(60500));
  assert.equal(recordingSteps([a, b])[1].config.seconds, 1);
});
test("整组加入现有节点右侧，保留原图，多次录制不重叠", () => {
  const existing = [
    {
      id: "__entry",
      position: { x: 0, y: 0 },
      data: { kind: "entry", targets: { next: "old" } },
    },
    {
      id: "old",
      position: { x: 700, y: 200 },
      measured: { width: 300, height: 160 },
      data: { kind: "end", targets: {} },
    },
  ];
  const original = structuredClone(existing);
  const added = recordingNodes([key(4, 100), key(3, 1001)], existing);
  assert.equal(added[0].position.x, 1120);
  assert.equal(added[0].data.targets.next, added[1].id);
  assert.deepEqual(added.at(-1).data.targets, {});
  assert.deepEqual(existing, original);
  assert.ok(
    recordingNodes([key(4, 100)], [...existing, ...added])[0].position.x >
      added[0].position.x + 260,
  );
  const draft = blankDraft("recording-test");
  draft.nodes = [
    ...added.map((n) => ({
      id: n.id,
      kind: n.data.kind,
      label: n.data.label,
      config: n.data.config,
      targets: n.data.targets,
      position: [n.position.x, n.position.y],
    })),
    {
      id: "end",
      kind: "end",
      label: "结束",
      config: { result: "succeeded" },
      targets: {},
      position: [0, 0],
    },
  ];
  draft.entry = added[0].id;
  draft.nodes.at(-2).targets.next = "end";
  assert.deepEqual(checkGraph(draft), []);
});
test("容量限制包含导航键后的等待，不会静默截断", () => {
  const existing = Array.from({ length: 198 }, (_, i) => ({
    id: String(i),
    position: { x: 0, y: 0 },
    data: { kind: "wait" },
  }));
  assert.throws(
    () => recordingNodes([key(4, 100), key(3, 1001)], existing),
    /200/,
  );
  assert.equal(recordingNodes([key(4, 100)], existing).length, 1);
  assert.equal(recordingNodes([], existing).length, 0);
});

test("响应式代理参数可转换，修改新节点不污染录制数据", () => {
  const config = new Proxy({ count: 1, profiles: [{ x: 0.5 }] }, {});
  const converted = recordingSteps([
    { kind: "tap", config, started: 0, ended: 50 },
  ]);
  converted[0].config.profiles[0].x = 0.8;
  assert.equal(config.profiles[0].x, 0.5);
});
