#!/usr/bin/env python3
"""隔离验证条件优先用模型、选区与取消；使用虚构密钥，不调用外部服务或真机。"""

from contextlib import closing
import base64
import http.server
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import sqlite3
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid

from check_support import executable, python_program, environment, stop

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))


class Model(http.server.BaseHTTPRequestHandler):
    mode = "yes"
    calls = []
    received = threading.Event()

    def log_message(self, *_):
        pass

    def do_POST(self):
        mode = Model.mode
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        Model.calls.append(body)
        Model.received.set()
        content = body["input"][0]["content"] if "input" in body else body["messages"][0]["content"]
        prompt = content[-1]["text"]
        rules = json.loads(prompt.split("条件：", 1)[1].split("。\n", 1)[0])
        if mode == "slow":
            time.sleep(2)
        if mode == "http401":
            self.send_response(401)
            self.end_headers()
            return
        results = [{"index": rule["index"], "choice": "no" if mode in ["no", "low_no"] else "unknown" if mode == "unknown" else "yes",
                    "reason": "局部截图无法确认" if mode == "unknown" else "模拟选区判断", "confidence": 0.3 if mode in ["low", "low_no"] else 1} for rule in rules]
        if mode == "missing":
            results = []
        raw = "not-json" if mode == "malformed" else json.dumps({"results": results})
        data = {"status": "incomplete" if mode == "truncated" else "completed", "output": [{"content": [{"text": raw}]}]} if "input" in body else {"choices": [{"finish_reason": "stop", "message": {"content": raw}}]}
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass


def main():
    with tempfile.TemporaryDirectory(prefix="adb-condition-model-") as folder:
        data = Path(folder)
        fake = data / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (fake / "screen.png").write_bytes(helpers["png"]())
        (data / "web").mkdir()
        (data / "web/build").mkdir()
        (data / "bin").mkdir()
        ocr = data / "bin/ocr"
        ocr.write_text('''#!/usr/bin/env python3
import os,sys,json
from pathlib import Path
sys.stdin.buffer.read()
p=Path(os.environ["FAKE_ADB_DIR"])/"ocr-calls"
with p.open("a") as f:f.write("read\\n")
print(json.dumps([{"text":"奖励","x":100,"y":100,"confidence":1}]))
''')
        ocr = python_program(ocr)
        adb = data / "fake-adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        base = f"http://127.0.0.1:{port}/api"
        model_server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
        threading.Thread(target=model_server.serve_forever, daemon=True).start()
        env = {**os.environ, **environment(data), "ADB_STUDIO_ROOT": str(data), "ADB_STUDIO_DATA": str(data),
               "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": str(port), "FAKE_ADB_DIR": str(fake), "CONDITION_TEST_KEY": "test-only"}
        env.pop("MODEL_KEY", None)
        log = (data / "server.log").open("w")
        process = None

        def call(path, method="GET", body=None):
            headers = {"Content-Type": "application/json"}
            if path != "/session":
                headers["Authorization"] = "Bearer " + (data / "access-token").read_text().strip()
            req = urllib.request.Request(base + path, method=method, headers=headers, data=None if body is None else json.dumps(body).encode())
            with urllib.request.urlopen(req, timeout=10) as response:
                return json.load(response)

        def boot():
            p = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=env, stdout=log, stderr=log)
            helpers["wait"](lambda: call("/session"))
            helpers["wait"](lambda: call("/devices"))
            return p

        def settings(api="responses", configured=True):
            call("/settings/model", "PUT", {"endpoint": f"http://127.0.0.1:{model_server.server_port}", "api": api,
                 "model": "mock-condition", "key_env": "CONDITION_TEST_KEY" if configured else "UNCONFIGURED_CONDITION_TEST_KEY"})

        def flow(kind="condition", conditions=None, timeout=0, interval=0.2, mode="all"):
            condition = {"source": "text", "operator": "contains", "value": "奖励", "guard": "任务页", "regions": [{
                "device_id": "mock-phone", "display": "default", "width": 400, "height": 800,
                "x": 0, "y": 0, "to_x": 0.5, "to_y": 0.25}]}
            config = {"conditions": [condition] if conditions is None else conditions, "mode": mode}
            targets = {"yes": "yes", "no" if kind == "condition" else "timeout": "no"}
            if kind == "wait_condition":
                config.update(timeout=timeout, interval=interval)
            draft = {"id": str(uuid.uuid4()), "revision": 0, "name": "模型条件检查", "description": "", "entry": "c", "viewport": [0, 0, 1], "nodes": [
                {"id": "c", "kind": kind, "label": "判断", "config": config, "targets": targets, "position": [0, 0]},
                *[{"id": key, "kind": "end", "label": key, "config": {"result": "succeeded"}, "targets": {}, "position": [300, 0]} for key in ["yes", "no"]]]}
            saved = call("/workflow-drafts/" + draft["id"], "PUT", draft)
            return call("/workflow-drafts/" + draft["id"] + "/save", "POST", {"revision": saved["revision"]})

        def start(w):
            return call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": w["id"]})

        def record(id):
            return next(r for r in call("/runs") if r["id"] == id)

        def finish(id):
            return helpers["wait"](lambda: next((r for r in [record(id)] if r["state"] not in ["running", "recovering", "waiting_device"]), None))

        def run(w):
            return finish(start(w)["id"])

        def ocr_calls():
            return (fake / "ocr-calls").read_text().count("\n") if (fake / "ocr-calls").exists() else 0

        try:
            process = boot()
            settings(configured=False)
            w = flow(conditions=[{"source": "text", "operator": "contains", "value": "奖励"}])
            r = run(w)
            assert r["state"] == "succeeded" and r["node"] == "yes" and r["model_calls"] == 0 and ocr_calls() == 1, r
            # ROOT 指向隔离目录，移走读字工具可在各平台验证模型不依赖本地 OCR。
            ocr.rename(data / "bin/ocr-disabled")
            try:
                start(flow())
                raise AssertionError("两种识别方式都缺失时应拒绝启动")
            except urllib.error.HTTPError as error:
                assert error.code == 400 and "配置模型密钥" in json.load(error)["error"]
            settings()
            before_ocr = ocr_calls()
            r = run(flow())
            assert r["state"] == "succeeded" and r["node"] == "yes" and r["model_calls"] == 1 and r["workflow"]["model_limit"] == 10, r
            content = Model.calls[-1]["input"][0]["content"]
            dimensions = [struct.unpack(">II", base64.b64decode(item["image_url"].split(",")[1])[16:24]) for item in content[:-1]]
            assert dimensions == [(400, 800), (200, 200)], dimensions
            assert ocr_calls() == before_ocr
            (data / "bin/ocr-disabled").rename(ocr)
            w = flow(conditions=[{"source":"text","operator":"not_contains","value":"其他文字","guard":"","regions":[{"device_id":"mock-phone","display":"default","width":400,"height":800,"x":0,"y":0,"to_x":0.5,"to_y":0.25}]}])
            assert run(w)["node"] == "yes"
            content = Model.calls[-1]["input"][0]["content"]
            assert len(content) == 2 and '"operator":"not_contains"' in content[-1]["text"]
            assert "该图就是完整检查范围" in content[-1]["text"]
            draft_id = w["id"].removeprefix("manual-")
            draft = call("/workflow-drafts/" + draft_id)
            draft["nodes"][0]["config"]["conditions"][0]["regions"][0]["width"] = 401
            draft = call("/workflow-drafts/" + draft_id, "PUT", draft)
            w = call("/workflow-drafts/" + draft_id + "/save", "POST", {"revision":draft["revision"]})
            count = len(Model.calls)
            r = run(w)
            assert r["state"] == "needs_help" and "区域标注" in r["reason"] and r["model_calls"] == 0 and len(Model.calls) == count, r
            print("通过：无模型走本地；配置模型后优先看图，页面依据看整页、条件只看裁剪选区，旧任务无需重建", flush=True)
            for api in ["responses", "chat_completions"]:
                settings(api)
                for operator in ["contains", "not_contains"]:
                    for guard in ["", "任务页"]:
                        for mode in ["no", "unknown", "low", "low_no"]:
                            Model.mode = mode
                            r = run(flow(conditions=[{"source":"text", "operator":operator, "value":"奖励", "guard":guard}]))
                            assert r["state"] == "succeeded" and r["node"] == "no" and r["model_calls"] == 1, r
            settings()
            for mode, reason in [("http401", "HTTP 401"), ("malformed", "格式错误"), ("missing", "数量不完整"), ("truncated", "未完成")]:
                Model.mode = mode
                r = run(flow())
                assert r["state"] == "needs_help" and r["node"] == "c" and r["model_calls"] == 1 and reason in r["reason"], r
            assert ocr_calls() == before_ocr
            print("通过：两种接口的否定、未知与低可信均走不符合，包含与不包含均不反转结果；鉴权、格式与截断失败仍请求处理", flush=True)
            Model.mode = "unknown"
            r = run(flow(conditions=[{"source":"text","operator":"contains","value":"奖励"},{"source":"app","operator":"is","value":"com.luna.music"}], mode="any"))
            assert r["state"] == "succeeded" and r["node"] == "yes", r
            r = run(flow(conditions=[{"source":"text","operator":"contains","value":"奖励"},{"source":"app","operator":"is","value":"com.luna.music"}], mode="all"))
            assert r["state"] == "succeeded" and r["node"] == "no", r
            count = len(Model.calls)
            r = run(flow(conditions=[{"source":"app","operator":"is","value":"com.luna.music"}]))
            assert r["state"] == "succeeded" and r["model_calls"] == 0 and len(Model.calls) == count
            Model.mode = "yes"
            assert run(flow("wait_condition", timeout=3))["node"] == "yes"
            Model.mode = "no"
            r = run(flow("wait_condition", timeout=1, interval=1))
            assert r["node"] == "no" and r["model_calls"] == 1, r
            r = run(flow("wait_condition", timeout=1, interval=0.05))
            assert r["state"] == "succeeded" and r["node"] == "no", r
            for mode in ["unknown", "low"]:
                Model.mode = mode
                r = run(flow("wait_condition", timeout=1, interval=1))
                assert r["state"] == "succeeded" and r["node"] == "no" and r["model_calls"] == 1, r
            Model.mode = "slow"
            r = run(flow("wait_condition", timeout=0.5))
            assert r["state"] == "needs_help" and r["node"] == "c" and "时限" in r["reason"], r
            print("通过：组合条件仍由程序合并，非文字条件不用模型；等待未确认走超时，模型请求超过截止时间仍报错", flush=True)
            Model.received.clear()
            id = start(flow())["id"]
            assert Model.received.wait(5)
            call("/runs/" + id + "/pause", "POST")
            r = helpers["wait"](lambda: next((r for r in [record(id)] if r["state"] == "paused"), None))
            assert r["node"] == "c" and r["model_calls"] == 1, r
            settings(configured=False)
            stop(process, data)
            process.wait(timeout=15)
            process = boot()
            Model.mode = "yes"
            call("/runs/" + id + "/resume", "POST")
            r = finish(id)
            assert r["node"] == "yes" and r["model_calls"] == 2 and r["text_recognition"] == "model", r
            assert ocr_calls() == before_ocr
            Model.mode = "slow"
            settings()
            Model.received.clear()
            id = start(flow())["id"]
            assert Model.received.wait(5)
            call("/runs/" + id + "/stop", "POST")
            r = finish(id)
            assert r["state"] == "cancelled" and r["node"] == "c" and r["model_calls"] == 1, r
            time.sleep(2.1)
            assert record(id)["state"] == "cancelled" and record(id)["node"] == "c"
            print("通过：暂停与停止取消在途模型，迟到结果不跳步骤；重启与更改设置后仍按原运行快照继续，次数不丢失", flush=True)
            # 限额与旧记录恢复均使用隔离数据库，不触及正式任务。
            stop(process, data)
            process.wait(timeout=15)
            with closing(sqlite3.connect(data / "data.sqlite")) as db, db:
                r["state"] = "paused"
                r["ended_at"] = None
                r["resume_app"] = "com.luna.music"
                r["workflow"]["model_limit"] = 1
                db.execute("UPDATE records SET body=? WHERE kind='runs' AND id=?", (json.dumps(r), id))
            process = boot()
            call("/runs/" + id + "/resume", "POST")
            r = finish(id)
            assert r["state"] == "needs_help" and "次数已达上限" in r["reason"] and r["model_calls"] == 1, r
            stop(process, data)
            process.wait(timeout=15)
            with closing(sqlite3.connect(data / "data.sqlite")) as db, db:
                r["state"] = "paused"
                r["ended_at"] = None
                r.pop("text_recognition", None)
                for node in r["workflow"]["nodes"]:
                    if node.get("step", {}).get("kind") == "condition":
                        node["step"]["config"]["conditions"][0]["guard"] = ""
                db.execute("UPDATE records SET body=? WHERE kind='runs' AND id=?", (json.dumps(r), id))
            process = boot()
            call("/runs/" + id + "/resume", "POST")
            r = finish(id)
            assert r["node"] == "yes" and r["model_calls"] == 1 and ocr_calls() == before_ocr + 1, r
            print("通过：达到原有预算后不再发送；旧记录恢复不自动改为外部模型；密钥只用于虚构本机服务，未接触真实手机或正式数据", flush=True)
        finally:
            if process and process.poll() is None:
                stop(process, data)
                process.wait(timeout=15)
            log.close()
            model_server.shutdown()


if __name__ == "__main__":
    main()
