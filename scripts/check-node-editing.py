#!/usr/bin/env python3
"""隔离验证保留截图、孤儿节点和单步执行，不连接真机。"""
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import tempfile
import urllib.error
import urllib.request
import uuid

from check_support import executable, python_program, environment, stop

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))


def main():
    with tempfile.TemporaryDirectory(prefix="adb-node-editing-") as folder:
        data = Path(folder)
        fake = data / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (fake / "screen.png").write_bytes(helpers["png"]())
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        base = f"http://127.0.0.1:{port}/api"
        env = {**os.environ, **environment(data), "ADB_STUDIO_ROOT": str(data), "ADB_STUDIO_DATA": str(data),
               "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": str(port), "FAKE_ADB_DIR": str(fake)}

        def call(path, method="GET", body=None, expected=200, raw=False, auth=True):
            headers = {"Content-Type": "application/json"}
            if auth and path != "/session":
                headers["Authorization"] = "Bearer " + (data / "access-token").read_text().strip()
            req = urllib.request.Request(base + path, method=method, headers=headers,
                data=None if body is None else json.dumps(body).encode())
            try:
                with urllib.request.urlopen(req, timeout=10) as r:
                    status, result = r.status, r.read() if raw else json.load(r)
            except urllib.error.HTTPError as e:
                status, result = e.code, json.load(e)
            assert status == expected, (path, status, result)
            return result

        def record(rid):
            return next(r for r in call("/runs") if r["id"] == rid)

        def finish(rid):
            return helpers["wait"](lambda: next((r for r in [record(rid)] if r["state"] not in ["running", "recovering", "waiting_device", "paused"]), None))

        def taps():
            return len((fake / "taps").read_text().splitlines()) if (fake / "taps").exists() else 0

        def node(id, kind, config=None, targets=None):
            return {"id": id, "kind": kind, "label": id, "config": config or {}, "targets": targets or {}, "position": [0, 0]}

        def draft(nodes, entry="wait", inputs=None):
            return {"id": str(uuid.uuid4()), "revision": 0, "name": "节点编辑检查", "description": "", "entry": entry,
                    "nodes": nodes, "inputs": inputs or [], "viewport": [0, 0, 1]}

        def save(body):
            d = call("/workflow-drafts/" + body["id"], "PUT", body)
            return d, call("/workflow-drafts/" + body["id"] + "/save", "POST", {"revision": d["revision"]})

        def test(body, nid, inputs=None, expected=200):
            return call("/editor/test-step", "POST", {"draft": body, "node_id": nid, "device_id": "mock-phone", "inputs": inputs or {}}, expected)

        with (data / "server.log").open("w") as log:
            process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=env, stdout=log, stderr=log)
            try:
                helpers["wait"](lambda: call("/session"))
                helpers["wait"](lambda: call("/devices"))
                input_id = str(uuid.uuid4())
                parameter = {"id": input_id, "name": "次数", "type": "integer", "default": None}
                body = draft([node("wait", "wait", {"seconds": 0}, {"next": "end"}), node("end", "end", {"result": "succeeded"}),
                              node("spare", "tap", {"count": {"input": input_id}})], inputs=[parameter])
                saved, workflow = save(body)
                assert len(saved["nodes"]) == 3 and len(workflow["nodes"]) == 2 and not workflow.get("inputs"), workflow
                before = taps()
                r = finish(call("/runs", "POST", {"workflow_id": workflow["id"], "device_id": "mock-phone"})["id"])
                assert r["state"] == "succeeded" and taps() == before, r
                print("通过：未配置孤儿节点及其缺省变量保留在草稿，保存与运行忽略", flush=True)

                observed = call("/editor/observe/mock-phone?retain=true")
                capture = observed["capture_id"]
                image = call("/editor/captures/" + capture, raw=True)
                assert image == (fake / "screen.png").read_bytes()
                call("/editor/captures/" + capture, expected=401, auth=False)
                profile = {"device_id": "mock-phone", "width": 400, "height": 800, "display": "default", "x": .2, "y": .3,
                           "to_x": .8, "to_y": .2, "capture_id": capture, "captured_at": observed["captured_at"]}
                parameter["default"] = 2
                body = draft([node("tap", "tap", {"profiles": [profile], "count": {"input": input_id}, "interval": 0, "after_wait": 0}, {"next": "later"}),
                              node("later", "tap", {"profiles": [profile], "count": 7, "interval": 0}, {"next": "end"}),
                              node("end", "end", {"result": "succeeded"})], "tap", [parameter])
                saved, workflow = save(body)
                before = taps()
                r = finish(test(body, "tap")["id"])
                assert r["state"] == "succeeded" and r["single_step"] == "tap" and r["node"] == "tap" and taps() == before + 2, r
                print("通过：标注截图保存、鉴权读取；单步默认值执行两次点击且不执行后续七次点击", flush=True)

                parameter["default"] = None
                before = taps()
                test(body, "tap", expected=400)
                test(body, "tap", {input_id: 0}, expected=400)
                assert taps() == before
                r = finish(test(body, "tap", {input_id: 3})["id"])
                assert r["state"] == "succeeded" and taps() == before + 3 and r["inputs"][input_id] == 3, r
                print("通过：单步缺值与无效值不操作手机，填写有效值后只执行此节点", flush=True)

                for kind, config in [("back", {}), ("home", {}), ("recents", {}), ("wait", {"seconds": 0}),
                    ("swipe", {"profiles": [profile], "count": 1, "interval": 0, "duration_ms": 100}),
                    ("loop", {"count": 3, "interval": 0}), ("break", {"loop": "old-owner"}), ("continue", {"loop": "old-owner"}),
                    ("condition", {"mode": "all", "conditions": [{"source": "app", "operator": "is", "value": "com.luna.music"}]}),
                    ("wait_condition", {"mode": "all", "interval": .1, "timeout": 2, "conditions": [{"source": "app", "operator": "is", "value": "com.luna.music"}]}),
                    ("end", {"result": "succeeded"})]:
                    isolated = draft([node("one", kind, config)], "")
                    before = taps()
                    r = finish(test(isolated, "one")["id"])
                    assert r["state"] == "succeeded" and taps() == before, (kind, r)
                print("通过：所有普通节点均可独立测试，循环与控制节点不进入其他节点", flush=True)

                def random_value(low, high):
                    return {"random": {"min": low, "max": high}}

                random_body = draft([node("random-tap", "tap", {"profiles": [profile], "count": random_value(2, 4),
                    "interval": random_value(.001, .003), "after_wait": random_value(0, .001)})], "")
                for _ in range(3):
                    before = taps()
                    r = finish(test(random_body, "random-tap")["id"])
                    step = next(n["step"] for n in r["workflow"]["nodes"] if n["id"] == "random-tap")
                    chosen = step["config"]["count"]
                    assert r["state"] == "succeeded" and 2 <= chosen <= 4 and int(chosen) == chosen and taps() == before + chosen, r
                    assert .001 <= step["config"]["interval"] <= .003
                    assert r["random_fields"]["random-tap"]["count"] == {"min": 2, "max": 4}
                random_body["nodes"][0]["config"]["count"] = random_value(4, 2)
                before = taps()
                test(random_body, "random-tap", expected=400)
                assert taps() == before
                loop_body = draft([node("loop", "loop", {"count": random_value(2, 3), "interval": 0}, {"body": "wait", "done": "end"}),
                    node("wait", "wait", {"seconds": random_value(.001, .003)}, {"next": "continue"}),
                    node("continue", "continue", {"loop": "loop"}), node("end", "end", {"result": "succeeded"})], "loop")
                _, random_workflow = save(loop_body)
                r = finish(call("/runs", "POST", {"workflow_id": random_workflow["id"], "device_id": "mock-phone"})["id"])
                events = call("/runs/" + r["id"] + "/events")
                samples = [e for e in events if e["message"].startswith("随机取值：")]
                rounds = r["canvas_progress"]["loops"]["loop"]["ended"]
                assert r["state"] == "succeeded" and 2 <= rounds <= 3
                assert sum(e["node"] == "loop" for e in samples) == 1
                assert sum(e["node"] == "wait" for e in samples) == rounds
                print("通过：随机次数按区间执行，重复进入重新取值，循环轮数保持稳定，反向区间拒绝操作", flush=True)

                paused = test(draft([node("random-wait", "wait", {"seconds": random_value(2, 2.2)})], ""), "random-wait")
                helpers["wait"](lambda: record(paused["id"])["checkpoint"].get("wait_elapsed", 0) > .05)
                call("/runs/" + paused["id"] + "/pause", "POST")
                paused = helpers["wait"](lambda: next((r for r in [record(paused["id"])] if r["state"] == "paused"), None))
                selected = next(n["step"]["config"]["seconds"] for n in paused["workflow"]["nodes"] if n["id"] == "random-wait")
                stop(process, data)
                process.wait(timeout=15)
                process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=env, stdout=log, stderr=log)
                helpers["wait"](lambda: call("/session"))
                call("/runs/" + paused["id"] + "/resume", "POST")
                r = finish(paused["id"])
                assert next(n["step"]["config"]["seconds"] for n in r["workflow"]["nodes"] if n["id"] == "random-wait") == selected
                assert sum(e["message"].startswith("随机取值：") for e in call("/runs/" + r["id"] + "/events")) == 1
                assert r["state"] == "succeeded"
                print("通过：暂停、服务重启与继续保留同一次随机结果，不重复抽取", flush=True)

                exported = call("/workflows/" + workflow["id"] + "/export")
                assert any(asset["id"] == capture + ".png" for asset in exported["assets"])
                imported = call("/workflow-import", "POST", {"operation_id": str(uuid.uuid4()), "file": exported})
                imported_draft = call("/workflow-drafts/" + imported["draft_id"])
                copied = imported_draft["nodes"][0]["config"]["profiles"][0]["capture_id"]
                assert copied != capture and call("/editor/captures/" + copied, raw=True) == image
                print("通过：导出与副本导入保留标注截图，并使用独立图片编号", flush=True)
            finally:
                stop(process, data)
                process.wait(timeout=15)


if __name__ == "__main__":
    main()
