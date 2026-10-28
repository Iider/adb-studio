#!/usr/bin/env python3
"""隔离检查投屏转发、访问限制、页面接管与清理；不操作真机。"""

import base64
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import tempfile
import time
import urllib.request
import urllib.error
import runpy

from check_support import executable, python_program, environment as test_environment, stop

ROOT = Path(__file__).resolve().parents[1]
PORT = 17843


def wait(check):
    for _ in range(100):
        try:
            if check():
                return
        except OSError:
            pass
        time.sleep(0.1)
    raise AssertionError("等待超时")


def exact(stream, count):
    value = b""
    while len(value) < count:
        block = stream.recv(count - len(value))
        assert block, "连接意外结束"
        value += block
    return value


def send(stream, opcode, payload=b""):
    assert len(payload) < 126
    mask = os.urandom(4)
    stream.sendall(bytes([0x80 | opcode, 0x80 | len(payload)]) + mask + bytes(
        value ^ mask[index % 4] for index, value in enumerate(payload)))


def frame(stream):
    while True:
        header = exact(stream, 2)
        size = header[1] & 127
        if size == 126:
            size = struct.unpack(">H", exact(stream, 2))[0]
        elif size == 127:
            size = struct.unpack(">Q", exact(stream, 8))[0]
        payload = exact(stream, size)
        opcode = header[0] & 15
        if opcode == 9:
            send(stream, 10, payload)
        else:
            return opcode, payload


def connect(token="", device="mock-phone", expected=101, origin=None):
    stream = socket.create_connection(("127.0.0.1", PORT), timeout=15)
    key = base64.b64encode(os.urandom(16)).decode()
    headers = f"GET /api/devices/{device}/mirror HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\nAuthorization: Bearer {token}\r\n"
    if origin:
        headers += f"Origin: {origin}\r\n"
    stream.sendall((headers + "\r\n").encode())
    response = b""
    while not response.endswith(b"\r\n\r\n"):
        response += exact(stream, 1)
    assert int(response.split(b" ")[1]) == expected, response.split(b"\r\n")[0]
    return stream


def touch(action, x=100):
    return struct.pack(">BBQIIHHHII",2,action,0,x,200,400,800,0 if action in (1,3) else 65535,1 if action in (0,1) else 0,0 if action in (1,3) else 1)


def keys(key):
    return struct.pack(">BBIII",0,0,key,0,0)+struct.pack(">BBIII",0,1,key,0,0)

def initial_power(stream, enabled):
    opcode, payload = frame(stream)
    assert opcode == 1 and json.loads(payload) == {"type":"display_power","enabled":enabled,"initial":True}


def main():
    with tempfile.TemporaryDirectory(prefix="adb-mirror-check-") as directory:
        data = Path(directory)
        fake = data / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (fake / "screen.png").write_bytes(runpy.run_path(str(ROOT / "scripts/check-integration.py"))["png"]())
        (data / "bin").mkdir()
        (data / "bin/scrcpy-server-v3.1").write_bytes(b"simulation")
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        environment = {**os.environ, **test_environment(data), "ADB_STUDIO_ROOT": str(ROOT), "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": str(PORT), "FAKE_ADB_DIR": str(fake)}
        with (data / "server.log").open("w") as log:
            process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=environment, stdout=log, stderr=log)
            try:
                wait(lambda: urllib.request.urlopen(f"http://127.0.0.1:{PORT}/api/session"))
                token = (data / "access-token").read_text()
                connect(expected=401).close()
                connect(token, origin="https://example.com", expected=401).close()
                connect(token, device="unknown", expected=400).close()
                first = connect(token)
                initial_power(first, True)
                assert frame(first) == (2, b"studio-video-test")
                duplicate = connect(token)
                initial_power(duplicate, True)
                assert frame(duplicate) == (2, b"studio-video-test")
                assert json.loads(frame(first)[1])["type"] == "taken_over"
                first.close()
                first = duplicate
                send(first, 2, touch(0))
                assert "先开启" in json.loads(frame(first)[1])["error"]
                first.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                again = connect(token)
                initial_power(again, True)
                assert frame(again) == (2, b"studio-video-test")
                again.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                interrupted = connect(token)
                initial_power(interrupted, True)
                assert frame(interrupted) == (2, b"studio-video-test")
                (fake / "end-mirror").touch()
                assert "已结束" in json.loads(frame(interrupted)[1])["error"]
                interrupted.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                (fake / "end-mirror").unlink()
                (fake / "slow-mirror").touch()
                starting = connect(token)
                wait(lambda: list(fake.glob("localabstract-scrcpy_*")))
                replacement = connect(token)
                assert json.loads(frame(starting)[1])["type"] == "taken_over"
                starting.close()
                latest = connect(token)
                message = json.loads(frame(replacement)[1])
                if message.get("type") == "display_power":
                    assert frame(replacement) == (2, b"studio-video-test")
                    message = json.loads(frame(replacement)[1])
                assert message["type"] == "taken_over"
                replacement.close()
                initial_power(latest, True)
                assert frame(latest) == (2, b"studio-video-test")
                latest.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                pending = connect(token)
                wait(lambda: list(fake.glob("localabstract-scrcpy_*")))
                send(pending, 8)
                pending.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                (fake / "slow-mirror").unlink()
                (fake / "asleep").touch()
                asleep = connect(token)
                assert "息屏" in json.loads(frame(asleep)[1])["error"]
                asleep.close()
                (fake / "asleep").unlink()
                (fake / "identity").write_text("another-phone")
                wrong = connect(token)
                assert "手机未连接或未授权" in json.loads(frame(wrong)[1])["error"]
                wrong.close()
                (fake / "identity").unlink()
                def call(path, body=None, expected=200, method=None):
                    request = urllib.request.Request(f"http://127.0.0.1:{PORT}/api" + path,
                        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
                        data=json.dumps(body).encode() if body is not None else None, method=method)
                    try:
                        with urllib.request.urlopen(request, timeout=20) as response:
                            status, result = response.status, json.load(response)
                    except urllib.error.HTTPError as error:
                        status, result = error.code, json.load(error)
                    assert status == expected, (path,status,result)
                    return result
                def records():
                    return [bytes.fromhex(line) for line in (fake / "controls").read_text().splitlines()] if (fake / "controls").exists() else []
                def control_message(stream, enabled):
                    send(stream,1,json.dumps({"type":"control","enabled":enabled}).encode())
                    opcode, data = frame(stream)
                    assert opcode == 1
                    return json.loads(data)
                def viewer():
                    stream=connect(token)
                    initial_power(stream, not next(row for row in call("/devices") if row["id"] == "mock-phone")["display_off"])
                    assert frame(stream) == (2,b"studio-video-test")
                    return stream
                def state(run):
                    return next(row for row in call("/runs") if row["id"] == run["id"])["state"]
                def swipes():
                    return len((fake / "swipes").read_text().splitlines()) if (fake / "swipes").exists() else 0
                fixture = runpy.run_path(str(ROOT / "scripts/check-integration.py"))["workflow"]
                call("/workflows",fixture("mirror-parallel",count=100,interval=0.3))
                body={"device_id":"mock-phone","workflow_id":"mirror-parallel"}
                manual=viewer()
                assert control_message(manual,True)["enabled"]
                run = call("/runs",body)
                wait(lambda: swipes() > 0)
                assert "已有任务" in call("/runs",body,400)["error"]
                assert "手动操作" in call("/devices/mock-phone/screen",{"action":"lock"},400)["error"]
                for action in [0,2,1]: send(manual,2,touch(action))
                for key in [3,4,187]: send(manual,2,keys(key))
                wait(lambda: len(records()) == 9)
                assert [record[1] for record in records()[:3]] == [0,2,1]
                assert [int.from_bytes(record[2:6],"big") for record in records()[3::2]] == [3,4,187]
                for mode in [0,2,0]:
                    send(manual,2,bytes([10,mode]))
                    opcode, payload = frame(manual)
                    assert opcode == 1 and json.loads(payload) == {"type":"display_power","enabled":mode == 2}
                wait(lambda: len(records()) == 12)
                assert records()[-3:] == [bytes([10,0]),bytes([10,2]),bytes([10,0])]
                send(manual,2,touch(0))
                wait(lambda: len(records()) == 13)
                assert not control_message(manual,False)["enabled"]
                wait(lambda: records()[-1][1] == 3)
                assert records()[-3] == bytes([10,0])
                assert next(row for row in call("/devices") if row["id"] == "mock-phone")["display_off"]
                wakeups = sum("KEYCODE_WAKEUP" in command for command in (fake / "commands").read_text().splitlines())
                call("/devices/mock-phone", {"keep_awake": True}, method="PUT")
                call("/devices/refresh", {})
                assert sum("KEYCODE_WAKEUP" in command for command in (fake / "commands").read_text().splitlines()) == wakeups
                manual.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                (fake / "asleep").touch()
                manual = viewer()
                assert control_message(manual,True)["enabled"]
                (fake / "asleep").unlink()
                send(manual,2,bytes([10,2]))
                assert json.loads(frame(manual)[1]) == {"type":"display_power","enabled":True}
                assert not next(row for row in call("/devices") if row["id"] == "mock-phone")["display_off"]
                call("/devices/mock-phone", {"keep_awake": False}, method="PUT")
                count = swipes()
                wait(lambda: swipes() > count)
                assert state(run) == "running"
                assert control_message(manual,True)["enabled"]
                count = swipes()
                wait(lambda: swipes() > count)
                assert state(run) == "running"
                call(f"/runs/{run['id']}/pause",{})
                wait(lambda: state(run) == "paused")
                assert not control_message(manual,False)["enabled"]
                assert state(run) == "paused"
                assert control_message(manual,True)["enabled"]
                assert state(run) == "paused"
                call(f"/runs/{run['id']}/resume",{})
                count = swipes()
                wait(lambda: swipes() > count)
                assert state(run) == "running"
                count=len(records());send(manual,2,touch(0))
                wait(lambda: len(records())>count)
                manual.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                assert records()[-1][1] == 3
                count = swipes()
                wait(lambda: swipes() > count)
                assert state(run) == "running"
                call(f"/runs/{run['id']}/pause",{})
                wait(lambda: state(run) == "paused")
                denied=viewer()
                assert control_message(denied,True)["enabled"]
                (fake / "reject-control").touch()
                send(denied,2,touch(0))
                status=json.loads(frame(denied)[1])
                assert not status["enabled"] and "拒绝操作" in status["error"]
                denied.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                (fake / "reject-control").unlink()
                invalid=viewer()
                assert control_message(invalid,True)["enabled"]
                send(invalid,2,touch(0,x=400))
                assert "超出画面" in json.loads(frame(invalid)[1])["error"]
                invalid.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                call(f"/runs/{run['id']}/stop",{})
                active = viewer()
                assert control_message(active,True)["enabled"]
                count=len(records());send(active,2,touch(0))
                wait(lambda: len(records())>count)
                replacement = viewer()
                assert json.loads(frame(active)[1])["type"] == "taken_over"
                wait(lambda: records()[-1][1] == 3)
                active.close()
                active = replacement
                assert control_message(active,True)["enabled"]
                count=len(records());send(active,2,touch(0))
                wait(lambda: len(records())>count)
                send(active,2,touch(3))
                send(active,2,bytes([10,0]))
                assert json.loads(frame(active)[1]) == {"type":"display_power","enabled":False}
                stop(process, data)
                process.wait(timeout=10)
                active.close()
                assert not list(fake.glob("localabstract-scrcpy_*"))
                process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=environment, stdout=log, stderr=log)
                wait(lambda: urllib.request.urlopen(f"http://127.0.0.1:{PORT}/api/session"))
                restored = viewer()
                assert control_message(restored,True)["enabled"]
                send(restored,2,bytes([10,2]))
                assert json.loads(frame(restored)[1]) == {"type":"display_power","enabled":True}
                restored.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()]
                assert all(command[2:5] == ["shell","input","swipe"] for command in commands if "input" in command)
                assert any("control=true" in command and "power_on=false" in command and "cleanup=false" in command for command in commands)
                print("通过：投屏与触控、页面接管、启动中接管、接管取消触点、任务并行、同机互斥、息屏后断开与重连、保持常亮不误唤醒、重启恢复状态、权限与清理")
            finally:
                if process.poll() is None:
                    stop(process, data)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
