import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionLimit,
  canonical,
  checkGraph,
  newCondition,
  initialDraft,
} from "../src/lib/workflow-editor.ts";

const node = (id, kind, targets = {}, config = {}) => ({
  id,
  kind,
  targets,
  config,
  label: id,
  position: [0, 0],
});
const draft = (nodes) => ({
  id: "test",
  revision: 0,
  name: "test",
  description: "",
  entry: nodes[0].id,
  nodes,
  viewport: [0, 0, 1],
  updated_at: "",
});

test("新建始终使用独立空白草稿，不接着打开已有内容", () => {
  const existing = { ...draft([node("end", "end")]), revision: 7 };
  const before = structuredClone(existing);
  const first = initialDraft([existing], null, existing);
  const second = initialDraft([existing], "");
  assert.equal(first.saved, false);
  assert.deepEqual(first.draft.nodes, []);
  assert.equal(first.draft.entry, "");
  assert.equal(first.draft.revision, 0);
  assert.notEqual(first.draft.id, existing.id);
  assert.notEqual(second.draft.id, first.draft.id);
  assert.deepEqual(existing, before);
});

test("指定草稿才继续编辑，缺失草稿不会打开其他任务", () => {
  const existing = { ...draft([node("end", "end")]), revision: 7 };
  const selected = initialDraft([existing], existing.id);
  assert.equal(selected.saved, true);
  assert.deepEqual(selected.draft, existing);
  assert.throws(() => initialDraft([existing], "missing"), /已不存在/);
});

test("未落盘的新草稿刷新保留编号，已删除草稿的备份只恢复为新副本", () => {
  const local = draft([node("wait", "wait")]);
  const restored = initialDraft([], local.id, local);
  assert.equal(restored.draft.id, local.id);
  assert.equal(restored.saved, false);
  assert.deepEqual(restored.draft.nodes, local.nodes);
  const old = { ...local, revision: 3, updated_at: "2026-10-28" };
  const copy = initialDraft([], old.id, old);
  assert.notEqual(copy.draft.id, old.id);
  assert.equal(copy.draft.revision, 0);
  assert.equal(copy.draft.updated_at, "");
  assert.equal(old.revision, 3);
  assert.throws(() => initialDraft([], "other", old), /已不存在/);
});

test("页面依据默认留空，条件判断和等待条件都可直接通过检查", () => {
  assert.equal(newCondition().guard, "");
  for (const kind of ["condition", "wait_condition"]) {
    for (const guard of [undefined, "", "任务页"]) {
      const graph = draft([
        node(
          "check",
          kind,
          kind === "condition"
            ? { yes: "end", no: "end" }
            : { yes: "end", timeout: "end" },
          {
            mode: "all",
            interval: 1,
            timeout: 5,
            conditions: [
              {
                source: "text",
                operator: "contains",
                value: "奖励",
                ...(guard === undefined ? {} : { guard }),
              },
            ],
          },
        ),
        node("end", "end", {}, { result: "succeeded" }),
      ]);
      assert.deepEqual(checkGraph(graph), []);
      graph.nodes[0].config.conditions[0].value = "";
      assert.ok(
        checkGraph(graph).some((issue) => issue.message.includes("判断内容")),
      );
    }
  }
});

test("循环次数与动作自身的重复次数共同计入上限", () => {
  assert.equal(
    actionLimit(
      draft([
        node("loop", "loop", { body: "tap", done: "end" }, { count: 3 }),
        node("tap", "tap", { next: "swipe" }, { count: 2 }),
        node("swipe", "swipe", { next: "loop" }, { count: 1 }),
        node("end", "end"),
      ]),
    ),
    9,
  );
});

test("循环中的分支、退出与下一轮采用保守上限", () => {
  for (const kind of ["break", "continue"]) {
    assert.equal(
      actionLimit(
        draft([
          node("loop", "loop", { body: "check", done: "end" }, { count: 4 }),
          node("check", "condition", { yes: "control", no: "home" }),
          node("control", kind, {}, { loop: "loop" }),
          node("home", "home", { next: "loop" }),
          node("end", "end"),
        ]),
      ),
      4,
    );
  }
});

test("不完整连线、普通回线与嵌套循环不显示确定次数", () => {
  assert.equal(
    actionLimit(draft([node("tap", "tap", {}, { count: 2 })])),
    null,
  );
  assert.equal(
    actionLimit(draft([node("back", "back", { next: "back" })])),
    null,
  );
  assert.equal(
    actionLimit(
      draft([
        node("outer", "loop", { body: "inner", done: "end" }, { count: 2 }),
        node("inner", "loop", { body: "home", done: "outer" }, { count: 3 }),
        node("home", "home", { next: "inner" }),
        node("end", "end"),
      ]),
    ),
    null,
  );
});

test("服务器改变字段顺序不会把已存内容误报为草稿修改", () => {
  assert.equal(
    canonical({ name: "测试", config: { interval: 1, count: 2 } }),
    canonical({ config: { count: 2, interval: 1 }, name: "测试" }),
  );
  assert.notEqual(canonical({ count: 1 }), canonical({ count: 2 }));
});
