#!/usr/bin/env python3
"""用隔离数据和模拟 ADB 检查后台行为，不操作真机。"""

import datetime as dt
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import zlib
import uuid

from check_support import executable, python_program, environment, stop

ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:17842"


def png(width=400, height=800):
    def chunk(name, value):
        return (
            struct.pack(">I", len(value))
            + name
            + value
            + struct.pack(">I", zlib.crc32(name + value))
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress((b"\x00" + b"\xff" * (width * 3)) * height))
        + chunk(b"IEND", b"")
    )


def wait(fn, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            value = fn()
            if value:
                return value
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.15)
    raise AssertionError("等待检查结果超时")


def workflow(name, count=2, interval=0.2):
    return {
        "id": str(uuid.uuid4()), "revision": 0, "name": name,
        "description": "模拟滑动", "entry": "swipe", "viewport": [0, 0, 1],
        "nodes": [
            {"id": "swipe", "kind": "swipe", "label": "滑动", "position": [0, 0],
             "targets": {"next": "end"}, "config": {
                 "count": count, "interval": interval, "duration_ms": 100,
                 "profiles": [{"device_id": "mock-phone", "width": 400, "height": 800,
                    "display": "default", "x": 0.5, "y": 0.8, "to_x": 0.5, "to_y": 0.2}]}},
            {"id": "end", "kind": "end", "label": "结束", "position": [300, 0],
             "targets": {}, "config": {"result": "succeeded"}},
        ],
    }


def main():
    process = None
    with tempfile.TemporaryDirectory(prefix="adb-studio-check-") as tmp:
        data = Path(tmp)
        fake = data / "fake"
        fake.mkdir()
        for key, value in {
            "online": "1",
            "stay_on_while_plugged_in": "3",
            "screen_off_timeout": "600000",
        }.items():
            (fake / key).write_text(value)
        (fake / "screen.png").write_bytes(png())
        (fake / "foreground-unavailable").touch()
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        (data / "bin").mkdir()
        if (ROOT / "data/bin/ocr").exists():
            shutil.copy2(ROOT / "data/bin/ocr", data / "bin/ocr")
        else:
            (data / "bin/ocr").write_text('#!/usr/bin/env python3\nimport sys\nsys.stdin.buffer.read()\nprint("[]")\n')
            python_program(data / "bin/ocr")
        env = {
            **os.environ, **environment(data),
            "ADB_STUDIO_ROOT": str(ROOT),
            "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb),
            "ADB_STUDIO_PORT": "17842",
            "FAKE_ADB_DIR": str(fake),
            "TEST_MODEL_KEY": "test-only",
        }
        logfile = (data / "server.log").open("w")

        def boot():
            p = subprocess.Popen(
                [str(executable(ROOT / "target/debug/adb-studio"))],
                env=env,
                stdout=logfile,
                stderr=logfile,
            )
            wait(
                lambda: urllib.request.urlopen(BASE + "/api/session", timeout=2).status
                == 200
            )
            call("/settings/model", "PUT", {"endpoint":"http://127.0.0.1:1", "model":"unused", "api":"responses", "key_env":"UNCONFIGURED_INTEGRATION_TEST_KEY"})
            return p

        def call(path, method="GET", body=None, expected=200, auth=True):
            headers = {"Content-Type": "application/json"}
            if auth:
                headers["Authorization"] = (
                    "Bearer " + (data / "access-token").read_text()
                )
            req = urllib.request.Request(
                BASE + "/api" + path,
                method=method,
                headers=headers,
                data=None if body is None else json.dumps(body).encode(),
            )
            try:
                with urllib.request.urlopen(req, timeout=20) as response:
                    status, value = response.status, json.load(response)
            except urllib.error.HTTPError as e:
                status = e.code
                raw = e.read().decode()
                try:
                    value = json.loads(raw)
                except json.JSONDecodeError:
                    value = {"error": raw}
            assert status == expected, (path, status, value)
            return value

        def record(id):
            return next(r for r in call("/runs") if r["id"] == id)

        def done(id):
            def finished():
                r = record(id)
                return (
                    r
                    if r["state"] not in {"running", "waiting_device", "recovering", "paused"}
                    else None
                )

            return wait(finished)

        def count():
            return (
                len((fake / "swipes").read_text().splitlines())
                if (fake / "swipes").exists()
                else 0
            )

        try:
            process = boot()
            for route in ("/", "/editor?draft=web-serving-check", "/index.html"):
                with urllib.request.urlopen(BASE + route, timeout=5) as response:
                    assert response.status == 200
                    assert response.headers["Cache-Control"] == "no-store"
                    assert response.headers["Content-Type"].startswith("text/html")
                    html = response.read().decode()
                for asset in set(re.findall(r'(?:src|href)="(/_app/[^"]+)"', html)):
                    with urllib.request.urlopen(BASE + asset, timeout=5) as response:
                        assert response.status == 200
                        assert not response.headers["Content-Type"].startswith("text/html")
            with urllib.request.urlopen(BASE + "/_app/version.json", timeout=5) as response:
                assert response.headers["Cache-Control"] == "no-store"
                assert "version" in json.load(response)
            with urllib.request.urlopen(BASE + "/", timeout=5) as response:
                modified = response.headers["Last-Modified"]
            request = urllib.request.Request(
                BASE + "/", headers={"If-Modified-Since": modified}
            )
            try:
                with urllib.request.urlopen(request, timeout=5) as response:
                    assert response.headers["Cache-Control"] == "no-store"
            except urllib.error.HTTPError as error:
                assert error.code == 304
                assert error.headers["Cache-Control"] == "no-store"
            try:
                urllib.request.urlopen(BASE + "/_app/immutable/missing.js", timeout=5)
                raise AssertionError("缺失脚本未返回 404")
            except urllib.error.HTTPError as error:
                assert error.code == 404
                assert error.headers["Cache-Control"] == "no-store"
                assert not error.headers.get("Content-Type", "").startswith("text/html")
            print("通过：网页入口不缓存、编辑器直达、资源完整、缺失脚本返回 404", flush=True)
            call("/devices", expected=401, auth=False)
            call("/devices/restart-adb", "POST", expected=401, auth=False)
            req = urllib.request.Request(
                BASE + "/api/session", headers={"Origin": "https://example.com"}
            )
            try:
                urllib.request.urlopen(req)
                raise AssertionError("跨站请求未拒绝")
            except urllib.error.HTTPError as e:
                assert e.code == 403
            rows = call("/devices")
            mock = [d for d in rows if d["id"] == "mock-phone"]
            assert len(mock) == 1 and len(mock[0]["endpoints"]) == 2
            call("/devices/mock-phone", "PUT", {"name": "测试机"})
            call("/devices/mock-phone", "PUT", {"keep_awake": True})
            assert (fake / "stay_on_while_plugged_in").read_text() == "7"
            call("/devices/mock-phone", "PUT", {"keep_awake": False})
            assert (fake / "stay_on_while_plugged_in").read_text() == "3" and (
                fake / "screen_off_timeout"
            ).read_text() == "600000"
            print("通过：访问限制、设备合并、重命名、常亮恢复", flush=True)

            def screenshot():
                req = urllib.request.Request(
                    BASE + "/api/devices/mock-phone/screenshot",
                    headers={"Authorization": "Bearer " + (data / "access-token").read_text()},
                )
                with urllib.request.urlopen(req, timeout=20) as response:
                    assert response.headers["Content-Type"] == "image/png"
                    assert response.headers["Cache-Control"] == "no-store"
                    timing = response.headers["Server-Timing"]
                    assert all(name + ";dur=" in timing for name in ("device", "display", "capture", "decode", "encode", "total"))
                    return response.read(), timing

            def commands_since(index):
                return [json.loads(line) for line in (fake / "commands").read_text().splitlines()[index:]]

            raw = struct.pack("<IIII", 400, 800, 1, 1) + bytes([255, 255, 255, 255]) * 400 * 800
            (fake / "screen.raw").write_bytes(raw)
            index = len((fake / "commands").read_text().splitlines())
            first, timing = screenshot()
            assert struct.unpack(">II", first[16:24]) == (400, 800) and 'source;desc="raw"' in timing
            assert commands_since(index).count(["-t", "1", "shell", "getprop", "ro.serialno"]) == 1
            (fake / "screen.raw").write_bytes(struct.pack("<IIII", 2, 1, 1, 1) + bytes([255, 0, 0, 255]) * 2)
            second, _ = screenshot()
            assert second != first and struct.unpack(">II", second[16:24]) == (2, 1)

            for active in ("200", "100"):
                (fake / "displays").write_text("\n".join(
                    f'DisplayDeviceInfo{{uniqueId="local:{display}", type INTERNAL, state {"ON" if display == active else "OFF"},}}'
                    for display in ("100", "200")
                ))
                (fake / "expected-display").write_text(active)
                screenshot()
            (fake / "displays").write_text((fake / "displays").read_text().replace("state ON", "state OFF"))
            captures = (fake / "captures").read_text()
            call("/devices/mock-phone/screenshot", expected=400)
            assert (fake / "captures").read_text() == captures
            (fake / "displays").unlink()
            (fake / "expected-display").unlink()

            for invalid in (raw[:-4], raw[:12] + struct.pack("<I", 2) + raw[16:]):
                (fake / "screen.raw").write_bytes(invalid)
                result, timing = screenshot()
                assert result == png() and 'source;desc="png"' in timing
            (fake / "screen.png").write_bytes(b"broken PNG")
            call("/devices/mock-phone/screenshot", expected=400)
            (fake / "screen.png").write_bytes(png())
            (fake / "screen.raw").write_bytes(raw)

            captures = (fake / "captures").read_text()
            (fake / "identity").write_text("another-phone")
            call("/devices/mock-phone/screenshot", expected=400)
            assert (fake / "captures").read_text() == captures
            (fake / "identity").unlink()
            (fake / "disconnect-after-display").touch()
            call("/devices/mock-phone/screenshot", expected=400)
            assert (fake / "captures").read_text() == captures
            (fake / "disconnect-after-display").unlink()
            (fake / "transport-gone").unlink()

            (fake / "legacy-transport").touch()
            index = len((fake / "commands").read_text().splitlines())
            screenshot()
            assert commands_since(index).count(["-s", "mock-usb", "shell", "getprop", "ro.serialno"]) >= 2
            (fake / "legacy-transport").unlink()
            (fake / "wireless-only").touch()
            index = len((fake / "commands").read_text().splitlines())
            result, timing = screenshot()
            assert result == png() and 'source;desc="png"' in timing
            assert all("-p" in command for command in commands_since(index) if "screencap" in command)
            (fake / "wireless-only").unlink()
            (fake / "screen.raw").unlink()
            print("通过：截图分段计时、原始画面、新图与旋转尺寸、内外屏选择、损坏回退、身份核验、断线不换机、旧版 ADB 与无线截图", flush=True)

            def device_connection(expected, endpoint_count, endpoint=None):
                device = next(d for d in call("/devices") if d["id"] == "mock-phone")
                return (device["connection"] == expected and len(device["endpoints"]) == endpoint_count
                        and (endpoint is None or device["endpoints"] == [endpoint]))

            wait(lambda: device_connection("USB", 2))
            (fake / "mdns-only").touch()
            wait(lambda: device_connection("无线", 1, "adb-mock-phone._adb-tls-connect._tcp"))
            (fake / "mdns-only").unlink()
            (fake / "wireless-only").touch()
            wait(lambda: device_connection("无线", 1, "127.0.0.1:5555"))
            (fake / "wireless-only").unlink()
            wait(lambda: device_connection("USB", 2))
            print("通过：mDNS 与 IP 无线端点显示无线；USB 与无线并存时保留 USB 优先", flush=True)

            for _ in range(2):
                assert call("/devices/mock-phone/screen", "POST", {"action": "lock"})["message"] == "屏幕已关闭"
                assert (fake / "asleep").exists()
            assert call("/devices/mock-phone/screen", "POST", {"action": "wake"})["message"] == "已唤醒，锁屏已解除"
            assert not (fake / "locked").exists()
            (fake / "password").touch()
            call("/devices/mock-phone/screen", "POST", {"action": "lock"})
            assert "需要在手机上" in call("/devices/mock-phone/screen", "POST", {"action": "wake"})["message"]
            assert (fake / "locked").exists()
            (fake / "password").unlink()
            call("/devices/mock-phone/screen", "POST", {"action": "wake"})
            call("/devices/mock-phone/screen", "POST", {"action": "invalid"}, expected=400)
            print("通过：无密码解锁、密码锁屏提示、重复锁屏不唤醒", flush=True)


            assert call("/workflows") == []
            # Removed endpoints must reject old submissions before any action.
            call("/workflows", "POST", {}, expected=405)
            call("/runs", "POST", {"device_id":"mock-phone", "workflow_id":"swipe", "tap":{}}, expected=422)

            def save(draft):
                path = "/workflow-drafts/" + draft["id"]
                saved = call(path, "PUT", draft)
                return call(path + "/save", "POST", {"revision":saved["revision"]}), saved

            def start_workflow(workflow):
                return call("/runs", "POST", {"device_id":"mock-phone", "workflow_id":workflow["id"]})

            flow, draft = save(workflow("工作流执行", count=4, interval=0.6))
            r = start_workflow(flow)
            call("/runs", "POST", {"device_id":"mock-phone", "workflow_id":flow["id"]}, expected=400)
            draft["name"] = "保存的新内容"
            replacement, draft = save(draft)
            assert record(r["id"])["workflow"]["name"] == "工作流执行"
            assert done(r["id"])["state"] == "succeeded" and count() == 4
            assert len(call("/workflows")) == 1
            assert start_workflow(replacement)["workflow"]["name"] == "保存的新内容"
            wait(lambda: all(not row["state"] in {"running","recovering","waiting_device"} for row in call("/runs")))
            print("通过：只允许工作流创建、运行快照、重复保存与同机互斥", flush=True)

            flow, _ = save(workflow("超时诊断", count=1, interval=0))
            before = count()
            (fake / "timeout-display").touch()
            r = start_workflow(flow)
            wait(lambda: record(r["id"])["state"] == "paused")
            paused = record(r["id"])
            assert paused["pending"] is None and count() == before
            assert "shell dumpsys display" in paused["error_detail"] and "时限 12 秒" in paused["error_detail"]
            assert "连接：-s mock-usb" in paused["error_detail"]
            log_text = (data / "server.log").read_text(encoding="utf-8")
            assert all(value in log_text for value in ["ADB 命令超时", "shell dumpsys display", "canvas_screen", "mock-phone", "adb_target=", "elapsed_ms=", "timeout_ms=12000"]), log_text
            print("超时详情示例：" + paused["error_detail"], flush=True)
            (fake / "timeout-display").unlink()
            (fake / "timeout-input").touch()
            call("/runs/" + r["id"] + "/resume", "POST")
            wait(lambda: record(r["id"])["state"] == "paused")
            paused = record(r["id"])
            assert paused["pending"] == "canvas_swipe" and count() == before
            assert "shell input swipe" in paused["error_detail"]
            assert "shell input swipe" in (data / "server.log").read_text(encoding="utf-8")
            (fake / "timeout-input").unlink()
            call("/runs/" + r["id"] + "/resume", "POST", expected=400)
            assert count() == before
            call("/runs/" + r["id"] + "/stop", "POST")
            wait(lambda: record(r["id"])["state"] == "cancelled")
            print("通过：屏幕检查与滑动超时均记录命令、连接、耗时；日志带请求手机与阶段，待确认动作不重复", flush=True)

            flow, _ = save(workflow("暂停与恢复", count=12, interval=0.5))
            r = start_workflow(flow)
            wait(lambda: record(r["id"])["checkpoint"].get("done", 0) >= 1)
            assert "未结束任务" in call("/devices/restart-adb", "POST", expected=400)["error"]
            call("/runs/" + r["id"] + "/pause", "POST")
            wait(lambda: record(r["id"])["state"] == "paused")
            assert "未结束任务" in call("/devices/restart-adb", "POST", expected=400)["error"]
            before = count()
            stop(process, data); process.wait(timeout=15); process = boot()
            assert record(r["id"])["state"] == "paused" and count() == before
            (fake / "foreground-unavailable").unlink()
            (fake / "foreground").write_text("other.app/.Main")
            call("/runs/" + r["id"] + "/resume", "POST")
            wait(lambda: count() > before)
            call("/runs/" + r["id"] + "/stop", "POST")
            wait(lambda: record(r["id"])["state"] == "cancelled")
            before = count()
            stop(process, data); process.wait(timeout=15); process = boot()
            assert record(r["id"])["state"] == "cancelled" and count() == before
            (fake / "foreground-unavailable").touch()
            call("/runs/" + r["id"] + "/resume", "POST")
            assert done(r["id"])["state"] == "succeeded"
            assert count() == 8 + 12
            (fake / "foreground").unlink()
            assert not any(json.loads(line)[-4:] == ["shell", "dumpsys", "activity", "activities"] for line in (fake / "commands").read_text().splitlines())
            print("通过：暂停与停止不自动恢复、原记录继续、不重复动作；应用未知或改变不阻断动作与继续，全程不查询应用", flush=True)

            flow, _ = save(workflow("离线恢复", count=6, interval=0.5))
            before = count()
            r = start_workflow(flow)
            wait(lambda: count() > before)
            (fake / "online").unlink()
            call("/devices/refresh", "POST")
            wait(lambda: record(r["id"])["state"] == "waiting_device")
            assert "未结束任务" in call("/devices/restart-adb", "POST", expected=400)["error"]
            assert ["kill-server"] not in [json.loads(line) for line in (fake / "commands").read_text().splitlines()]
            stop(process, data); process.wait(timeout=15); process = boot()
            (fake / "online").touch()
            call("/devices/refresh", "POST")
            assert done(r["id"])["state"] == "succeeded"
            assert count() == before + 6
            print("通过：掉线等待、重启恢复、不重复动作", flush=True)

            when = (dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=2)).isoformat()
            before = count()
            schedule = call("/schedules", "POST", {"name":"单次工作流", "device_id":"mock-phone", "workflow_id":flow["id"], "rule":{"kind":"once","at":when}})
            wait(lambda: count() == before + 6)
            wait(lambda: all(row["state"] != "running" for row in call("/runs")))
            assert not next(s for s in call("/schedules") if s["id"] == schedule["id"])["enabled"]
            print("通过：工作流定时仅触发一次", flush=True)

            call("/devices/pair", "POST", {"address": "8.8.8.8:41234", "code": "123456"}, 400)
            call("/devices/pair", "POST", {"address": "127.0.0.1:41234", "code": "12345"}, 400)
            (fake / "pair-protocol-fault").touch()
            pairing = call("/devices/pair", "POST", {"address": "127.0.0.1:41234", "code": "123456"}, 400)
            assert "配对码窗口" in pairing["error"] and "代理绕过局域网" in pairing["error"]
            (fake / "pair-protocol-fault").unlink()
            assert call("/devices/pair", "POST", {"address": "127.0.0.1:41234", "code": "123456"})["ok"]
            call("/devices/connect", "POST", {"address": "8.8.8.8:5555"}, 400)
            (fake / "connect-text-fails").touch()
            assert "连接端口可能已变化" in call(
                "/devices/connect", "POST", {"address": "127.0.0.1:5555"}, 400
            )["error"]
            (fake / "connect-text-fails").unlink()
            connected = call("/devices/connect", "POST", {"address": "127.0.0.1:5555"})
            assert connected["id"] == "mock-phone" and "127.0.0.1:5555" in connected["manual_endpoints"]
            (fake / "mdns-visible").touch()
            nearby = call("/devices/discover")
            assert {(device["kind"], device["address"]) for device in nearby} == {
                ("connect", "127.0.0.1:5555"),
                ("pair", "127.0.0.1:41234"),
            }
            (fake / "mdns-visible").unlink()
            (fake / "wireless-only").touch()
            (fake / "online").unlink()
            call("/devices/refresh", "POST")
            assert not next(d for d in call("/devices") if d["id"] == "mock-phone")["online"]
            (fake / "auto-connect").touch()
            wait(lambda: next(d for d in call("/devices") if d["id"] == "mock-phone")["online"], timeout=25)
            assert any(cmd == ["connect", "127.0.0.1:5555"] for cmd in map(json.loads, (fake / "commands").read_text().splitlines()))
            print("通过：局域网配对、手动连接、地址校验与断线重连", flush=True)

            offset = len((fake / "commands").read_text().splitlines())
            restarted = call("/devices/restart-adb", "POST")
            assert next(d for d in restarted if d["id"] == "mock-phone")["online"]
            commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()[offset:]]
            kill = commands.index(["kill-server"])
            assert commands[kill + 1] == ["start-server"], commands
            assert ["connect", "127.0.0.1:5555"] in commands[kill + 2:]
            (fake / "start-server-fails").touch()
            failed = call("/devices/restart-adb", "POST", expected=400)
            assert "启动 ADB 失败" in failed["error"], failed
            (fake / "start-server-fails").unlink()
            (fake / "auto-connect").unlink()
            restarted = call("/devices/restart-adb", "POST")
            assert not next(d for d in restarted if d["id"] == "mock-phone")["online"]
            (fake / "auto-connect").touch()
            assert next(d for d in call("/devices/restart-adb", "POST") if d["id"] == "mock-phone")["online"]
            print("通过：重启 ADB 后重连；未授权、运行、暂停与等待手机时拒绝重启；启动失败与仍离线如实返回", flush=True)
        finally:
            if process and process.poll() is None:
                stop(process, data)
                process.wait(timeout=15)
            logfile.close()
    print("集成检查全部通过。", flush=True)


if __name__ == "__main__":
    main()
