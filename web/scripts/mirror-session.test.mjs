import { test } from "node:test";
import assert from "node:assert/strict";
import {
  rememberMirror,
  forgetMirror,
  restoreMirror,
} from "../src/lib/mirror-session.ts";

function session() {
  const values = new Map();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
  };
}

test("刷新仅恢复当前页签明确开启过的手机", () => {
  const storage = session();
  assert.equal(restoreMirror("phone-a", "reload", storage), false);
  rememberMirror("phone-a", storage);
  assert.equal(restoreMirror("phone-a", "reload", storage), true);
  assert.equal(restoreMirror("phone-b", "reload", storage), false);
  assert.equal(restoreMirror("phone-a", "reload", session()), false);
});

test("新页签即使复制了会话记录，也不会自动抢投屏", () => {
  const storage = session();
  rememberMirror("phone-a", storage);
  for (const navigation of ["navigate", "back_forward", "prerender"])
    assert.equal(restoreMirror("phone-a", navigation, storage), false);
});

test("同一次文档访问只恢复一次，切回设备页不会重复恢复", () => {
  const storage = session();
  const page = {};
  rememberMirror("phone-a", storage);
  assert.equal(restoreMirror("phone-a", "reload", storage, page), true);
  assert.equal(restoreMirror("phone-a", "reload", storage, page), false);
  assert.equal(restoreMirror("phone-a", "reload", storage, {}), true);
});

test("停止、离开或被接管后刷新不再恢复", () => {
  const storage = session();
  rememberMirror("phone-a", storage);
  forgetMirror("phone-a", storage);
  assert.equal(restoreMirror("phone-a", "reload", storage), false);
});

test("旧手机组件的清理不会删除新手机的投屏记录", () => {
  const storage = session();
  rememberMirror("phone-a", storage);
  rememberMirror("phone-b", storage);
  forgetMirror("phone-a", storage);
  assert.equal(restoreMirror("phone-b", "reload", storage), true);
});

test("会话存储不可用时，保留手动投屏并跳过自动恢复", () => {
  const storage = new Proxy(
    {},
    {
      get() {
        throw new Error("存储已禁用");
      },
    },
  );
  assert.doesNotThrow(() => rememberMirror("phone-a", storage));
  assert.doesNotThrow(() => forgetMirror("phone-a", storage));
  assert.equal(restoreMirror("phone-a", "reload", storage), false);
  assert.doesNotThrow(() => rememberMirror("phone-a"));
  assert.doesNotThrow(() => forgetMirror("phone-a"));
});
