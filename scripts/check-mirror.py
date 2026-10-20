#!/usr/bin/env python3
"""隔离检查投屏转发、访问限制、重复连接与清理；不操作真机。"""

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


def main():
    with tempfile.TemporaryDirectory(prefix="adb-mirror-check-") as directory:
        data = Path(directory)
        fake = data / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (data / "bin").mkdir()
        (data / "bin/scrcpy-server-v3.1").write_bytes(b"simulation")
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb.chmod(0o755)
        environment = {**os.environ, "ADB_STUDIO_ROOT": str(ROOT), "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": str(PORT), "FAKE_ADB_DIR": str(fake)}
        with (data / "server.log").open("w") as log:
            process = subprocess.Popen([str(ROOT / "target/debug/adb-studio")], env=environment, stdout=log, stderr=log)
            try:
                wait(lambda: urllib.request.urlopen(f"http://127.0.0.1:{PORT}/api/session"))
                token = (data / "access-token").read_text()
                connect(expected=401).close()
                connect(token, origin="https://example.com", expected=401).close()
                connect(token, device="unknown", expected=400).close()
                first = connect(token)
                assert frame(first) == (2, b"studio-video-test")
                duplicate = connect(token)
                assert "其他页面" in json.loads(frame(duplicate)[1])["error"]
                duplicate.close()
                send(first, 1, b"touch")
                assert "仅供观看" in json.loads(frame(first)[1])["error"]
                first.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                again = connect(token)
                assert frame(again) == (2, b"studio-video-test")
                again.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                interrupted = connect(token)
                assert frame(interrupted) == (2, b"studio-video-test")
                (fake / "end-mirror").touch()
                assert "手机投屏已结束" in json.loads(frame(interrupted)[1])["error"]
                interrupted.close()
                wait(lambda: not list(fake.glob("localabstract-scrcpy_*")))
                (fake / "end-mirror").unlink()
                (fake / "slow-mirror").touch()
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
                active = connect(token)
                assert frame(active) == (2, b"studio-video-test")
                process.terminate()
                process.wait(timeout=10)
                active.close()
                assert not list(fake.glob("localabstract-scrcpy_*"))
                commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()]
                assert not any("input" in command for command in commands)
                assert any("control=false" in command and "power_on=false" in command for command in commands)
                print("通过：鉴权、跨站拒绝、手机核验、视频转发、重复连接、只读限制、断开重开、手机中断、启动取消、息屏提示、换机拒绝、退出清理；未发送触控命令")
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
