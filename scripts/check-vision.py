#!/usr/bin/env python3
"""隔离验证只读模型检查、坐标边界与 HTTPS 来源；不操作真机。"""

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

from check_support import executable, python_program, environment, stop as stop_server

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))
png, wait = helpers["png"], helpers["wait"]
BASE = "http://127.0.0.1:17844"


class Model(http.server.BaseHTTPRequestHandler):
    mode = "valid"
    calls = []
    negative = b""

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
        if Model.mode == "outside":
            answer["target"] = [9999, 200]
        if Model.mode == "missing":
            answer.pop("target")
        if Model.mode == "foreign":
            answer["choice"] = "run-shell"
        if Model.mode == "low":
            answer["confidence"] = 0.2
        raw = "invalid-json" if Model.mode == "malformed" else json.dumps(answer)
        data = {"output": [{"content": [{"text": raw}]}]} if "input" in body else {"choices": [{"message": {"content": raw}}]}
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
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
        adb = python_program(adb)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env = {**os.environ, **environment(data), "ADB_STUDIO_ROOT": str(ROOT), "ADB_STUDIO_DATA": str(data),
               "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": "17844", "FAKE_ADB_DIR": str(fake),
               "TEST_VISION_KEY": "test-only", "ADB_STUDIO_ORIGINS": "https://studio.test:8443"}
        logfile = (data / "server.log").open("w")
        process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=env, stdout=logfile, stderr=logfile)

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

        def inspect(image=positive, **query):
            return call("/recognition/check", "POST", {"image_base64": base64.b64encode(image).decode(), "query": {"condition": "before", **query}})

        def taps():
            return (fake / "taps").read_text().count("\n") if (fake / "taps").exists() else 0

        try:
            wait(lambda: urllib.request.urlopen(BASE + "/api/session", timeout=2))
            wait(lambda: any(d["id"] == "mock-phone" for d in call("/devices")))
            model = {"endpoint": f"http://127.0.0.1:{server.server_port}", "model": "mock-vision", "api": "responses", "key_env": "TEST_VISION_KEY"}
            call("/settings/model", "PUT", model)
            reply = call("/settings", extra={"Host": "studio.test:8443", "Origin": "https://studio.test:8443"})
            assert reply["platform"] in {"macos", "linux", "windows"}
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

            for api in ["responses", "chat_completions"]:
                model["api"] = api
                call("/settings/model", "PUT", model)
                for mode in ["outside", "missing", "foreign", "low", "malformed"]:
                    Model.mode = mode
                    answer = inspect(target="button")
                    assert answer["result"] is None and answer["error"] and answer["calls"] == 2, (mode, answer)
                Model.mode = "valid"
                assert inspect(target="button")["result"]["target"] == [100, 200]
            assert taps() == 0 and call("/runs") == []
            print("通过：两类接口、越界与无效结果拒绝、检查不操作手机", flush=True)
        finally:
            stop_server(process, data)
            process.wait(timeout=20)
            server.shutdown()
            server.server_close()
            logfile.close()
    print("模型只读检查全部通过。", flush=True)


if __name__ == "__main__":
    main()
