#!/usr/bin/env python3
"""隔离验证纯视觉流程、模型边界、HTTPS 来源与取消；不操作真机。"""

import base64
import http.server
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))
png, wait = helpers["png"], helpers["wait"]
BASE = "http://127.0.0.1:17844"


class Model(http.server.BaseHTTPRequestHandler):
    mode = "valid"
    calls = []
    negative = b""
    route_images = {}
    started = threading.Event()
    release = threading.Event()

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        Model.calls.append(body)
        content = body["input"][0]["content"] if "input" in body else body["messages"][0]["content"]
        image = content[0].get("image_url")
        if isinstance(image, dict):
            image = image["url"]
        image = base64.b64decode(image.split(",")[1])
        answer = {"choice": "unknown" if image == Model.negative or Model.mode == "unknown" else "confirmed",
                  "reason": "模拟页面依据", "confidence": 1, "target": [250, 250], "coin": 123}
        prompt = content[-1].get("text", "")
        if "只根据当前截图选择一个明确符合的页面状态" in prompt:
            answer = {"choice": Model.route_images.get(image, "unknown"), "reason": "模拟页面分流",
                      "confidence": 1, "target": None, "coin": None}
        if Model.mode == "outside":
            answer["target"] = [9999, 200]
        if Model.mode == "shift" and len(Model.calls) > 1:
            answer["target"] = [375, 250]
        if Model.mode == "missing":
            answer.pop("target")
        if Model.mode == "foreign":
            answer["choice"] = "run-shell"
        if Model.mode == "low":
            answer["confidence"] = 0.2
        if Model.mode == "budget" and len(Model.calls) == 1:
            answer["choice"] = "invalid"
        if Model.mode == "blocked":
            Model.started.set()
            Model.release.wait(10)
        raw = "invalid-json" if Model.mode == "malformed" else json.dumps(answer)
        data = {"output": [{"content": [{"text": raw}]}]} if "input" in body else {"choices": [{"message": {"content": raw}}]}
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    with tempfile.TemporaryDirectory(prefix="adb-vision-check-") as directory:
        data = Path(directory)
        fake = data / "fake"
        fake.mkdir()
        for key, value in {"online": "1", "stay_on_while_plugged_in": "3", "screen_off_timeout": "600000"}.items():
            (fake / key).write_text(value)
        positive = png()
        Model.negative = png(401, 800)
        (fake / "screen.png").write_bytes(positive)
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb.chmod(0o755)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env = {**os.environ, "ADB_STUDIO_ROOT": str(ROOT), "ADB_STUDIO_DATA": str(data),
               "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": "17844", "FAKE_ADB_DIR": str(fake),
               "TEST_VISION_KEY": "test-only", "ADB_STUDIO_ORIGINS": "https://studio.test:8443"}
        logfile = (data / "server.log").open("w")
        process = subprocess.Popen([str(ROOT / "target/debug/adb-studio")], env=env, stdout=logfile, stderr=logfile)

        def call(path, method="GET", body=None, expected=200, extra=None):
            headers = {"Content-Type": "application/json", "Authorization": "Bearer " + (data / "access-token").read_text().strip(), **(extra or {})}
            req = urllib.request.Request(BASE + "/api" + path, method=method, headers=headers,
                                         data=None if body is None else json.dumps(body).encode())
            try:
                with urllib.request.urlopen(req, timeout=150) as response:
                    status = response.status
                    result = json.load(response)
            except urllib.error.HTTPError as error:
                status = error.code
                result = json.load(error)
            assert status == expected, (path, status, result)
            return result

        def record(id):
            return next(r for r in call("/runs") if r["id"] == id)

        def done(id):
            return wait(lambda: (r if (r := record(id))["state"] not in {"running", "recovering", "waiting_device"} else None))

        def inspect(image=positive, **query):
            return call("/recognition/check", "POST", {"image_base64": base64.b64encode(image).decode(), "query": {"condition": "before", **query}})

        def taps():
            return (fake / "taps").read_text().count("\n") if (fake / "taps").exists() else 0

        def start(id):
            return call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": id, "version": 1, "trial": True})

        try:
            wait(lambda: urllib.request.urlopen(BASE + "/api/session", timeout=2))
            wait(lambda: any(d["id"] == "mock-phone" for d in call("/devices")))
            model = {"endpoint": f"http://127.0.0.1:{server.server_port}", "model": "mock-vision", "api": "responses", "key_env": "TEST_VISION_KEY"}
            call("/settings/model", "PUT", model)
            reply = call("/settings", extra={"Host": "studio.test:8443", "Origin": "https://studio.test:8443"})
            assert reply["platform"] in {"macos", "linux"}
            call("/settings", expected=401, extra={"Host": "studio.test:8443", "Origin": "http://studio.test:8443"})
            request = urllib.request.Request(BASE + "/api/session", headers={"Host": "studio.test:8443"})
            with urllib.request.urlopen(request) as response:
                assert "Secure" in response.headers["Set-Cookie"]
            stream = socket.create_connection(("127.0.0.1", 17844), timeout=3)
            stream.sendall(("GET /api/ws HTTP/1.1\r\nHost: studio.test:8443\r\nOrigin: https://studio.test:8443\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: " + base64.b64encode(os.urandom(16)).decode() + "\r\nAuthorization: Bearer " + (data / "access-token").read_text().strip() + "\r\n\r\n").encode())
            assert b"101" in stream.recv(4096).split(b"\r\n")[0]
            stream.close()
            print("通过：HTTPS 来源、Secure Cookie、消息连接与错误来源拒绝", flush=True)

            answer = inspect(target="button", read_number=True)
            assert answer["result"]["target"] == [100, 200] and answer["result"]["coin"] == 123 and answer["calls"] == 1
            before_sample = answer["image"]
            negative_sample = inspect(Model.negative)["image"]
            answer = inspect(Model.negative)
            assert answer["result"]["choice"] == "unknown" and answer["result"]["target"] is None
            for mode in ["outside", "missing", "foreign", "low", "malformed"]:
                Model.mode = mode
                result = inspect(target="button")
                assert result["error"] and result["calls"] == 2 and result["result"] is None, (mode, result)
            Model.mode = "valid"
            call("/recognition/check", "POST", {"device_id": "mock-phone", "query": {"condition": "before"}})
            call("/recognition/check", "POST", {"image_base64": "invalid", "query": {"condition": "before"}}, expected=400)
            call("/recognition/check", "POST", {"image_base64": base64.b64encode(positive).decode(), "query": {"condition": "before", "region": [1, 0, 0, 1]}}, expected=400)
            assert taps() == 0
            model["api"] = "chat_completions"
            call("/settings/model", "PUT", model)
            assert inspect()["result"]["choice"] == "confirmed"
            model["api"] = "responses"
            call("/settings/model", "PUT", model)
            print("通过：两类模型接口、纯看图、数字、未知画面、格式与坐标边界", flush=True)

            workflow = {"schema_version": 1, "id": "vision-test", "version": 1, "name": "模拟视觉点击", "description": "只在隔离服务运行", "devices": ["mock-phone"], "recognition": "vision", "entry": "tap", "model_limit": 5,
                        "nodes": [{"id": "tap", "label": "视觉点击", "type": "vision_tap", "package": "com.luna.music", "guard": "before", "target": "button", "region": [0.1, 0.1, 0.9, 0.9], "verify": "after", "next": "end"}, {"id": "end", "label": "结束", "type": "end", "result": "succeeded"}],
                        "tests": [{"node": "tap", "image": before_sample, "expected": "confirmed", "target_region": [0.2, 0.2, 0.3, 0.3]}, {"node": "tap", "image": negative_sample, "expected": "unknown"}, {"node": "tap", "image": before_sample, "expected": "confirmed", "after": True}, {"node": "tap", "image": negative_sample, "expected": "unknown", "after": True}]}
            call("/workflows", "POST", workflow)
            baseline = taps()
            Model.calls = []
            run = start("vision-test")
            result = done(run["id"])
            assert result["state"] == "succeeded", result
            assert result["model_calls"] == 3 and taps() == baseline + 1 and result["model_settings"] == model
            assert not (data / "bin/ocr").exists()
            assert not any('"swift"' in line for line in (fake / "commands").read_text().splitlines())
            print("通过：截图样本验证、三次看图单次点击、操作后新图与配置快照，无本地识别依赖", flush=True)

            route_no = png(450, 800)
            route_no_sample = inspect(route_no)["image"]
            Model.route_images = {positive: "route_yes", route_no: "route_no"}
            route = {"schema_version": 1, "id": "vision-route-test", "version": 1,
                     "name": "模拟视觉分流", "description": "按画面选择唯一去向", "devices": ["mock-phone"],
                     "recognition": "vision", "entry": "route", "model_limit": 2,
                     "nodes": [{"id": "route", "label": "看图分流", "type": "vision_route", "package": "com.luna.music",
                                "cases": [{"condition": "可操作状态", "next": "route_yes"},
                                          {"condition": "已完成状态", "next": "route_no"}]},
                               {"id": "route_yes", "label": "可操作", "type": "end", "result": "succeeded"},
                               {"id": "route_no", "label": "已完成", "type": "end", "result": "succeeded"}],
                     "tests": [{"node": "route", "image": before_sample, "expected": "route_yes"},
                               {"node": "route", "image": route_no_sample, "expected": "route_no"},
                               {"node": "route", "image": negative_sample, "expected": "unknown"}]}
            call("/workflows", "POST", route)
            call("/workflows/vision-route-test@1/validate", "POST")
            baseline = taps()
            result = done(start("vision-route-test")["id"])
            assert result["state"] == "succeeded" and result["model_calls"] == 1 and taps() == baseline, result
            (fake / "screen.png").write_bytes(route_no)
            result = done(start("vision-route-test")["id"])
            assert result["state"] == "succeeded" and result["node"] == "route_no" and taps() == baseline, result
            (fake / "screen.png").write_bytes(Model.negative)
            result = done(start("vision-route-test")["id"])
            assert result["state"] == "needs_help" and taps() == baseline, result
            (fake / "screen.png").write_bytes(positive)
            print("通过：视觉分流两个去向均可到达，未知页面停止", flush=True)
            for restart in [None, False, True]:
                launch = {"id": "launch", "label": "打开应用", "type": "launch",
                          "package": "com.luna.music", "next": "route"}
                if restart is not None:
                    launch["restart"] = restart
                item = {**route, "id": "launch-test-" + str(restart).lower(), "entry": "launch",
                        "nodes": [launch, *route["nodes"]]}
                call("/workflows", "POST", item)
                index = len((fake / "commands").read_text().splitlines())
                result = done(start(item["id"])["id"])
                assert result["state"] == "succeeded", result
                commands = [json.loads(line)[2:] for line in (fake / "commands").read_text().splitlines()[index:]]
                stop = ["shell", "am", "force-stop", "com.luna.music"]
                opened = [command for command in commands if command[:2] == ["shell", "monkey"]]
                if restart:
                    assert commands.count(stop) == 1 and len(opened) == 1, commands
                    assert commands.index(stop) < commands.index(opened[0]), commands
                else:
                    assert stop not in commands and not opened, commands
            Model.route_images = {}
            print("通过：默认保留前台应用，仅 restart=true 先结束再启动", flush=True)

            def prevalidated_start(mode, budget=5):
                nonlocal workflow
                workflow = {**workflow, "id": "vision-" + mode + str(time.monotonic_ns()), "model_limit": budget}
                Model.mode = "valid"
                call("/workflows", "POST", workflow)
                call(f"/workflows/{workflow['id']}@1/validate", "POST")
                # 正常启动需重新验证截图样本，运行开始后才注入故障。
                Model.calls = []
                old = Model.do_POST
                sample_count = len(workflow["tests"])
                def injected(handler):
                    Model.mode = mode if len(Model.calls) >= sample_count else "valid"
                    if len(Model.calls) == sample_count:
                        Model.calls = []
                        Model.mode = mode
                        Model.do_POST = old
                    old(handler)
                Model.do_POST = injected
                return start(workflow["id"])

            for mode in ["outside", "unknown", "shift", "budget"]:
                baseline = taps()
                run = prevalidated_start(mode, 3 if mode == "budget" else 5)
                result = done(run["id"])
                assert result["state"] == "needs_help" and taps() == baseline, (mode, result)
                assert result["model_calls"] <= result["workflow"]["model_limit"]
            print("通过：越界、未知、定位漂移和预算不足均不点击", flush=True)

            Model.started.clear()
            Model.release.clear()
            baseline = taps()
            run = prevalidated_start("blocked")
            assert Model.started.wait(5)
            newer = {**model, "model": "changed-after-start"}
            call("/settings/model", "PUT", newer)
            call(f"/runs/{run['id']}/pause", "POST")
            result = done(run["id"])
            assert result["state"] == "paused" and result["model_settings"] == model
            Model.release.set()
            time.sleep(0.3)
            assert taps() == baseline
            call(f"/runs/{run['id']}/stop", "POST")
            wait(lambda: record(run["id"])["state"] == "cancelled")
            print("通过：模型等待可暂停，迟到结果不点击，修改设置不改运行快照", flush=True)
        finally:
            Model.release.set()
            process.terminate()
            process.wait(timeout=20)
            server.shutdown()
            server.server_close()
            logfile.close()
    print("纯视觉隔离检查全部通过。", flush=True)


if __name__ == "__main__":
    main()
