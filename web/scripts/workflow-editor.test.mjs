import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionLimit,
  canonical,
  checkGraph,
  newCondition,
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

test("服务器改变字段顺序不会把已存版本误报为草稿修改", () => {
  assert.equal(
    canonical({ name: "测试", config: { interval: 1, count: 2 } }),
    canonical({ config: { count: 2, interval: 1 }, name: "测试" }),
  );
  assert.notEqual(canonical({ count: 1 }), canonical({ count: 2 }));
});
