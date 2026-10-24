#!/usr/bin/env python3
"""用隔离数据和模拟 ADB 检查后台行为，不操作真机。"""

import datetime as dt
import http.server
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import zlib

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


def workflow(id, count=2, interval=0.2):
    return {
        "schema_version": 1,
        "id": id,
        "version": 1,
        "name": "模拟滑动",
        "description": "集成检查",
        "devices": [],
        "entry": "swipe",
        "model_limit": 1,
        "nodes": [
            {
                "id": "swipe",
                "label": "滑动",
                "type": "swipe_loop",
                "interval": interval,
                "count": count,
                "minutes": None,
                "long": False,
                "next": "end",
            },
            {"id": "end", "label": "结束", "type": "end", "result": "succeeded"},
        ],
        "tests": [{"node": "swipe", "text": "count=2", "expected": "valid"}],
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
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb.chmod(0o755)
        (data / "bin").mkdir()
        if (ROOT / "data/bin/ocr").exists():
            shutil.copy2(ROOT / "data/bin/ocr", data / "bin/ocr")
        else:
            (data / "bin/ocr").write_text('#!/usr/bin/env python3\nimport sys\nsys.stdin.buffer.read()\nprint("[]")\n')
            (data / "bin/ocr").chmod(0o755)
        env = {
            **os.environ,
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
                [str(ROOT / "target/debug/adb-studio")],
                env=env,
                stdout=logfile,
                stderr=logfile,
            )
            wait(
                lambda: urllib.request.urlopen(BASE + "/api/session", timeout=2).status
                == 200
            )
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
                status, value = e.code, json.load(e)
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
            call("/devices", expected=401, auth=False)
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


            fixed = json.loads((ROOT / "workflows/feed-duck-fixed.json").read_text())
            fixed["id"] = "fixed-test"
            fixed["devices"] = []
            call("/workflows", "POST", fixed)
            captures = (fake / "captures").read_text() if (fake / "captures").exists() else ""
            r = call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": "fixed-test", "version": 2, "trial": True, "tap": {"count": 3, "interval": 1}})
            assert done(r["id"])["state"] == "succeeded"
            assert len((fake / "taps").read_text().splitlines()) == 3
            assert all(json.loads(line)[-2:] == ["528", "1866"] for line in (fake / "taps").read_text().splitlines())
            assert ((fake / "captures").read_text() if (fake / "captures").exists() else "") == captures
            call("/runs/" + r["id"], "DELETE")
            assert not any(row["id"] == r["id"] for row in call("/runs"))
            print("通过：固定点击次数与坐标、零截图零识别、清理运行记录", flush=True)
            call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": "point-click", "version": 1, "trial": True}, expected=400)
            call("/devices/mock-phone", "PUT", {"tap_point": {"x": 400, "y": 222, "width": 400, "height": 800}}, expected=400)
            point = {"x": 111, "y": 222, "width": 400, "height": 800}
            call("/devices/mock-phone", "PUT", {"tap_point": point})
            assert next(d for d in call("/devices") if d["id"] == "mock-phone")["tap_point"] == point
            r = call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": "point-click", "version": 1, "trial": True, "tap": {"count": 2, "interval": 1}})
            assert r["workflow"]["nodes"][0]["x"] == 111
            call("/devices/mock-phone", "PUT", {"tap_point": {**point, "x": 333}})
            assert done(r["id"])["state"] == "succeeded"
            assert all(json.loads(line)[-2:] == ["111", "222"] for line in (fake / "taps").read_text().splitlines()[-2:])
            assert ((fake / "captures").read_text() if (fake / "captures").exists() else "") == captures
            print("通过：标注保存、越界拒绝、运行坐标快照、不同尺寸零识别点击", flush=True)


            call("/workflows", "POST", workflow("fixture"))
            call("/workflows", "POST", workflow("fixture"), expected=400)
            call("/workflows/fixture@1/publish", "POST", expected=400)
            r = call(
                "/runs",
                "POST",
                {
                    "device_id": "mock-phone",
                    "workflow_id": "fixture",
                    "version": 1,
                    "trial": True,
                },
            )
            call(
                "/runs",
                "POST",
                {
                    "device_id": "mock-phone",
                    "workflow_id": "fixture",
                    "version": 1,
                    "trial": True,
                },
                expected=400,
            )
            assert done(r["id"])["state"] == "succeeded" and count() == 2
            wait(
                lambda: any(
                    w["id"] == "fixture" and w["active"] for w in call("/workflows")
                )
            )
            assert call("/runs/" + r["id"] + "/export")["events"]
            print("通过：版本不可覆盖、发布门槛、同机互斥、试跑自动启用", flush=True)

            before = count()
            custom = {"device_id": "mock-phone", "workflow_id": "fixture", "swipe": {"minutes": 0.001, "interval": 1}}
            r = call("/runs", "POST", custom)
            assert r["workflow"]["nodes"][0]["count"] is None
            assert done(r["id"])["state"] == "succeeded" and count() == before
            custom["swipe"]["minutes"] = 0
            call("/runs", "POST", custom, expected=400)
            schedule = call("/schedules", "POST", {"name": "自定义滑动", "device_id": "mock-phone", "workflow_id": "fixture", "swipe": {"minutes": 2, "interval": 7}, "rule": {"kind": "interval", "minutes": 60}, "enabled": False})
            assert next(s for s in call("/schedules") if s["id"] == schedule["id"])["swipe"] == {"minutes": 2, "interval": 7}
            call("/schedules/" + schedule["id"], "DELETE")
            print("通过：自定义滑动时长、间隔、短时长不滑动、无效输入、定时保存", flush=True)

            call("/workflows", "POST", workflow("recover", count=3, interval=0.6))
            r = call(
                "/runs",
                "POST",
                {
                    "device_id": "mock-phone",
                    "workflow_id": "recover",
                    "version": 1,
                    "trial": True,
                },
            )
            wait(lambda: count() >= 3)
            (fake / "online").unlink()
            wait(lambda: record(r["id"])["state"] == "waiting_device")
            due = (
                dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=2)
            ).isoformat()
            s = call(
                "/schedules",
                "POST",
                {
                    "name": "离线跳过",
                    "device_id": "mock-phone",
                    "workflow_id": "fixture",
                    "rule": {"kind": "once", "at": due},
                },
            )
            wait(
                lambda: next(x for x in call("/schedules") if x["id"] == s["id"])[
                    "last_result"
                ]
                and "跳过"
                in next(x for x in call("/schedules") if x["id"] == s["id"])[
                    "last_result"
                ]
            )
            process.terminate()
            process.wait(timeout=15)
            process = boot()
            assert (
                next(d for d in call("/devices") if d["id"] == "mock-phone")["name"]
                == "测试机"
            )
            assert record(r["id"])["state"] in {"waiting_device", "recovering"}
            (fake / "online").write_text("1")
            call("/devices/refresh", "POST")
            assert done(r["id"])["state"] == "succeeded" and count() == 5
            print("通过：离线等待、新定时跳过、后台重启恢复、动作不重复", flush=True)

            call("/workflows", "POST", workflow("stop", count=100, interval=0.3))
            r = call(
                "/runs",
                "POST",
                {
                    "device_id": "mock-phone",
                    "workflow_id": "stop",
                    "version": 1,
                    "trial": True,
                },
            )
            (fake / "online").unlink()
            wait(lambda: record(r["id"])["state"] == "waiting_device")
            call("/runs/" + r["id"] + "/stop", "POST")
            assert done(r["id"])["state"] == "cancelled"
            (fake / "online").write_text("1")
            call("/devices/refresh", "POST")
            due = (
                dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=3)
            ).isoformat()
            baseline = len(call("/runs"))
            call(
                "/schedules",
                "POST",
                {
                    "name": "单次执行",
                    "device_id": "mock-phone",
                    "workflow_id": "fixture",
                    "rule": {"kind": "once", "at": due},
                },
            )
            wait(lambda: len(call("/runs")) == baseline + 1)
            time.sleep(3)
            assert len(call("/runs")) == baseline + 1
            print("通过：等待时可停止、单次定时只执行一次", flush=True)

            wait(lambda: all(r["state"] not in {"running", "waiting_device", "recovering", "paused"} for r in call("/runs")))

            def begin(w):
                call("/workflows", "POST", w)
                return call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": w["id"], "version": 1, "trial": True})

            def paused(id):
                return wait(lambda: record(id) if record(id)["state"] == "paused" else None)

            before = count()
            stopped = begin(workflow("stopped-resume", count=8, interval=0.4))
            wait(lambda: record(stopped["id"])["checkpoint"].get("done", 0) >= 1 and record(stopped["id"])["pending"] is None)
            call(f"/runs/{stopped['id']}/stop", "POST")
            held = done(stopped["id"])
            progress = held["checkpoint"].copy()
            assert held["state"] == "cancelled" and held["ended_at"] and held["resume_app"] == "com.luna.music"
            assert count() - before == progress["done"] < 8
            other = begin(workflow("stopped-contender", count=100, interval=0.4))
            assert "已有其他任务" in call(f"/runs/{stopped['id']}/resume", "POST", expected=400)["error"]
            assert record(stopped["id"])["ended_at"] == held["ended_at"]
            call(f"/runs/{other['id']}/stop", "POST")
            done(other["id"])
            call("/devices/mock-phone/screen", "POST", {"action":"lock"})
            call(f"/runs/{stopped['id']}/resume", "POST", expected=400)
            call("/devices/mock-phone/screen", "POST", {"action":"wake"})
            (fake / "foreground").write_text("com.example.other")
            call(f"/runs/{stopped['id']}/resume", "POST", expected=400)
            (fake / "foreground").unlink()
            (fake / "online").unlink()
            call(f"/runs/{stopped['id']}/resume", "POST", expected=400)
            (fake / "online").touch()
            process.terminate()
            process.wait(timeout=15)
            process = boot()
            assert record(stopped["id"])["state"] == "cancelled"
            assert record(stopped["id"])["checkpoint"] == progress
            continued_at = count()
            resumed = call(f"/runs/{stopped['id']}/resume", "POST")
            assert resumed["id"] == stopped["id"] and resumed["ended_at"] is None
            assert resumed["started_at"] == stopped["started_at"] and resumed["checkpoint"] == progress
            assert done(stopped["id"])["state"] == "succeeded"
            assert count() - continued_at == 8 - progress["done"]
            call(f"/runs/{stopped['id']}/resume", "POST", expected=400)
            print("通过：停止后原记录保留进度续跑、停止不自动恢复、同机互斥、锁屏/离线/换应用拒绝继续、已完成不可继续", flush=True)

            before = count()
            r = begin(workflow("handoff", count=6, interval=0.4))
            wait(lambda: count() > before)
            call(f"/runs/{r['id']}/pause", "POST")
            held = paused(r["id"])
            assert held["ended_at"] is None and held["resume_app"] == "com.luna.music"
            progress = held["checkpoint"].copy()
            held_count = count()
            time.sleep(0.7)
            assert count() == held_count and record(r["id"])["checkpoint"] == progress
            call(f"/runs/{r['id']}", "DELETE", expected=400)
            call("/runs", "POST", {"device_id":"mock-phone", "workflow_id":"fixture"}, expected=400)
            due=(dt.datetime.now(dt.timezone.utc)+dt.timedelta(seconds=2)).isoformat()
            scheduled=call("/schedules", "POST", {"name":"接管期间避让", "device_id":"mock-phone", "workflow_id":"fixture", "rule":{"kind":"once","at":due}})
            wait(lambda: "跳过" in (next(s for s in call("/schedules") if s["id"]==scheduled["id"])["last_result"] or ""))
            assert "跳过" in next(s for s in call("/schedules") if s["id"]==scheduled["id"])["last_result"]
            assert count()==held_count
            process.terminate()
            process.wait(timeout=15)
            process=boot()
            assert record(r["id"])["state"]=="paused"
            time.sleep(0.5)
            assert count()==held_count
            call("/devices/mock-phone/screen", "POST", {"action":"lock"})
            call(f"/runs/{r['id']}/resume", "POST", expected=400)
            call("/devices/mock-phone/screen", "POST", {"action":"wake"})
            (fake/"foreground").write_text("com.example.other")
            call(f"/runs/{r['id']}/resume", "POST", expected=400)
            assert record(r["id"])["state"]=="paused" and count()==held_count
            (fake/"foreground").unlink()
            (fake/"online").unlink()
            call(f"/runs/{r['id']}/resume", "POST", expected=400)
            (fake/"online").touch()
            call(f"/runs/{r['id']}/resume", "POST")
            assert done(r["id"])["state"]=="succeeded" and count()-before==6
            print("通过：人工接管保留进度、定时避让、不可清理、重启保持暂停、锁屏/离线/换应用拒绝继续、原记录续跑", flush=True)

            w=workflow("handoff-wait",count=1,interval=0.2)
            w["entry"]="wait"
            w["nodes"].insert(0,{"id":"wait","label":"人工等待","type":"wait","seconds":20,"next":"swipe"})
            r=begin(w)
            time.sleep(0.8)
            call(f"/runs/{r['id']}/pause","POST")
            held=paused(r["id"])
            waited=held["checkpoint"]["wait_elapsed"]
            assert 0<waited<20
            call(f"/runs/{r['id']}/resume","POST")
            time.sleep(0.7)
            call(f"/runs/{r['id']}/pause","POST")
            assert paused(r["id"])["checkpoint"]["wait_elapsed"]>waited
            call(f"/runs/{r['id']}/stop","POST")
            assert done(r["id"])["state"]=="cancelled"
            print("通过：长等待可立即接管、已等待时长保留、暂停期间不累计",flush=True)

            (fake/"slow-input").touch()
            (fake/"input-started").unlink(missing_ok=True)
            before=count()
            r=begin(workflow("handoff-inflight",count=3,interval=0.2))
            wait(lambda:(fake/"input-started").exists())
            call(f"/runs/{r['id']}/pause", "POST")
            assert record(r["id"])["pause_requested"] and record(r["id"])["state"]!="paused"
            held=paused(r["id"])
            assert count()-before==1 and held["pending"] is None
            (fake/"slow-input").unlink()
            call(f"/runs/{r['id']}/resume", "POST")
            assert done(r["id"])["state"]=="succeeded" and count()-before==3
            print("通过：执行中的命令结束后才允许接管，不中断输入也不重复输入", flush=True)

            for flag in ["reject-input","timeout-input"]:
                (fake/flag).touch()
                before=count()
                r=begin(workflow("input-"+flag,count=3,interval=0.2))
                held=paused(r["id"])
                assert held["pending"]=="swipe" and held["ended_at"] is None and held["error_detail"]
                assert count()==before
                (fake/flag).unlink()
                call(f"/runs/{r['id']}/resume", "POST")
                assert done(r["id"])["state"]=="succeeded" and count()-before==2
            print("通过：触控拒绝与ADB超时保留暂停，继续不补发未确认滑动", flush=True)

            r=call("/runs","POST",{"device_id":"mock-phone","workflow_id":"point-click","version":1,"trial":True,"tap":{"count":10,"interval":1}})
            wait(lambda: record(r["id"])["checkpoint"].get("done",0)>=1)
            call(f"/runs/{r['id']}/pause","POST")
            held=paused(r["id"])
            assert held["pending"] is None and held["resume_size"]==[400,800]
            call("/devices/mock-phone","PUT",{"tap_point":{"x":10,"y":10,"width":800,"height":400}})
            (fake/"screen.png").write_bytes(png(800,400))
            call(f"/runs/{r['id']}/resume","POST",expected=400)
            assert record(r["id"])["state"]=="paused"
            (fake/"screen.png").write_bytes(png())
            call(f"/runs/{r['id']}/resume","POST")
            call(f"/runs/{r['id']}/stop","POST")
            assert done(r["id"])["state"]=="cancelled"
            call("/devices/mock-phone","PUT",{"tap_point":point})
            print("通过：继续使用启动时的屏幕尺寸，旋转后重新标注也不能覆盖任务快照",flush=True)

            (fake/"reject-input").touch()
            r=call("/runs","POST",{"device_id":"mock-phone","workflow_id":"point-click","version":1,"trial":True,"tap":{"count":2,"interval":1}})
            held=paused(r["id"])
            assert held["pending"]=="fixed_tap"
            (fake/"reject-input").unlink()
            taps=(fake/"taps").read_text()
            call(f"/runs/{r['id']}/resume","POST",expected=400)
            assert (fake/"taps").read_text()==taps
            call(f"/runs/{r['id']}/stop","POST")
            assert done(r["id"])["state"]=="cancelled"
            call(f"/runs/{r['id']}/resume","POST",expected=400)
            print("通过：不重发未确认点击、暂停可停止、停止后的未确认点击也不可继续",flush=True)

            class ModelHandler(http.server.BaseHTTPRequestHandler):
                choice = "outside-allowed-branches"
                calls = 0

                def log_message(self, *args):
                    pass

                def do_POST(self):
                    self.rfile.read(int(self.headers["Content-Length"]))
                    ModelHandler.calls += 1
                    answer = json.dumps(
                        {
                            "choice": ModelHandler.choice,
                            "reason": "模拟画面验证",
                            "confidence": 1,
                        }
                    )
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(
                        json.dumps(
                            {
                                "output": [
                                    {
                                        "type": "message",
                                        "content": [
                                            {"type": "output_text", "text": answer}
                                        ],
                                    }
                                ]
                            }
                        ).encode()
                    )

            model_server = http.server.HTTPServer(("127.0.0.1", 0), ModelHandler)
            thread = threading.Thread(target=model_server.serve_forever, daemon=True)
            thread.start()
            try:
                wait(
                    lambda: all(
                        r["state"] not in {"running", "waiting_device", "recovering", "paused"}
                        for r in call("/runs")
                    )
                )
                call(
                    "/settings/model",
                    "PUT",
                    {
                        "endpoint": f"http://127.0.0.1:{model_server.server_port}",
                        "model": "mock",
                        "api": "responses",
                        "key_env": "TEST_MODEL_KEY",
                    },
                )
                w = workflow("model-bounds")
                w["entry"] = "branch"
                w["nodes"][0] = {
                    "id": "branch",
                    "label": "识别分支",
                    "type": "branch",
                    "cases": [{"contains": "任务完成", "next": "end"}],
                    "otherwise": "end",
                }
                w["tests"] = [{"node": "branch", "text": "任务完成", "expected": "end"}]
                call("/workflows", "POST", w)
                r = call(
                    "/runs",
                    "POST",
                    {
                        "device_id": "mock-phone",
                        "workflow_id": w["id"],
                        "version": 1,
                        "trial": True,
                    },
                )
                result = done(r["id"])
                assert (
                    result["state"] == "needs_help"
                    and result["model_calls"] == 1
                    and ModelHandler.calls == 1
                )
                assert not next(
                    w for w in call("/workflows") if w["id"] == "model-bounds"
                )["active"]
                print(
                    "通过：模型不能选择未声明分支、调用预算不可越过、失败试跑不发布",
                    flush=True,
                )

                plateau = ROOT / "data/calibration/qishui/coin-0.png"
                if plateau.exists():
                    shutil.copy2(plateau, fake / "screen.png")
                    ModelHandler.choice = "finished"
                    w = workflow("qishui-plateau")
                    w["entry"] = "earn"
                    w["nodes"][0] = {
                        "id": "earn",
                        "label": "金币停滞复查",
                        "type": "qishui_loop",
                        "coin_region": [0.818, 0.163, 0.961, 0.233],
                        "identity_region": [0.25, 0.815, 0.67, 0.89],
                        "warmup": 1,
                        "stall": 1,
                        "finish_stall": 3,
                        "finish_swipes": 3,
                        "next": "end",
                    }
                    w["tests"] = [
                        {
                            "node": "earn",
                            "text": "39591 金币 已暂停",
                            "expected": "video",
                        },
                        {"node": "earn", "text": "网络异常", "expected": "blocked"},
                    ]
                    call("/workflows", "POST", w)
                    baseline = count()
                    r = call(
                        "/runs",
                        "POST",
                        {
                            "device_id": "mock-phone",
                            "workflow_id": w["id"],
                            "version": 1,
                            "trial": True,
                        },
                    )
                    result = done(r["id"])
                    assert result["state"] == "succeeded" and count() - baseline == 3
                    assert result["model_calls"] == 1 and result["last_shot"]
                    print(
                        "通过：真实金币暂停样本、三次上滑复查、完成截图与模型终检",
                        flush=True,
                    )
            finally:
                model_server.shutdown()
                model_server.server_close()
        finally:
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
            logfile.close()
    print("集成检查全部通过。", flush=True)


if __name__ == "__main__":
    main()
