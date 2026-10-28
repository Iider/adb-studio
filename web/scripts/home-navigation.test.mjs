import { test } from "node:test";
import assert from "node:assert/strict";
import {
  homeTabs,
  readHomeNavigation,
  homeNavigationUrl,
} from "../src/lib/home-navigation.ts";

const base = new URL("http://localhost/");

test("所有主页栏目都能从刷新后的网址恢复", () => {
  for (const tab of homeTabs) {
    const state = { ...readHomeNavigation(base), tab };
    assert.deepEqual(readHomeNavigation(homeNavigationUrl(base, state)), state);
  }
});

test("设备任务面板、任务详情、执行记录保留当前选择", () => {
  for (const query of [
    "?device=phone%20%26%20one&section=task",
    "?tab=workflows&workflow=task%2Fone&version=3",
    "?tab=runs&run=run-123",
  ]) {
    const state = readHomeNavigation(new URL(query, base));
    assert.deepEqual(readHomeNavigation(homeNavigationUrl(base, state)), state);
  }
});

test("切换栏目清理旧页面参数，保留其他参数与锚点", () => {
  const url = new URL(
    "?device=old&section=task&workflow=old&version=3&run=old&extra=1#anchor",
    base,
  );
  const state = { ...readHomeNavigation(base), tab: "settings" };
  const next = homeNavigationUrl(url, state);
  assert.equal(next.search, "?extra=1&tab=settings");
  assert.equal(next.hash, "#anchor");
  assert.equal(homeNavigationUrl(next, state).href, next.href);
});

test("旧链接的版本参数被忽略，未知栏目安全回退", () => {
  assert.deepEqual(
    readHomeNavigation(new URL("?tab=unknown&run=old&workflow=old", base)),
    readHomeNavigation(base),
  );
  for (const version of ["3", "-1", "oops"]) {
    const url = new URL(
      `?tab=workflows&workflow=task&version=${version}`,
      base,
    );
    const state = readHomeNavigation(url);
    assert.equal(state.selectedWorkflow, "task");
    assert.equal(
      homeNavigationUrl(url, state).searchParams.has("version"),
      false,
    );
  }
});
