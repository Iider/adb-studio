#!/usr/bin/env python3
"""用隔离数据库和模拟 ADB 验证草稿保存，不连接真实手机。"""

from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid
import runpy

ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:17844"


def main():
    with tempfile.TemporaryDirectory(prefix="adb-editor-check-") as folder:
        data = Path(folder)
        fake = data / "fake"
        fake.mkdir()
        helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))
        (fake / "screen.png").write_bytes(helpers["png"]())
        (data / "bin").mkdir()
        ocr = data / "bin/ocr"
        ocr.write_text('#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\nsys.stdin.buffer.read()\np=Path(os.environ["FAKE_ADB_DIR"])/"words"\nprint(p.read_text() if p.exists() else "[]")\n')
        ocr.chmod(0o755)
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb.chmod(0o755)
        env = {
            **os.environ,
            "ADB_STUDIO_ROOT": str(ROOT),
            "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb),
            "ADB_STUDIO_PORT": "17844",
            "FAKE_ADB_DIR": str(fake),
        }

        def call(path, method="GET", body=None, expected=200, auth=True):
            headers = {"Content-Type": "application/json"}
            if auth:
                headers["Authorization"] = "Bearer " + (data / "access-token").read_text().strip()
            request = urllib.request.Request(
                BASE + "/api" + path, method=method, headers=headers,
                data=None if body is None else json.dumps(body).encode(),
            )
            try:
                with urllib.request.urlopen(request, timeout=5) as response:
                    status, result = response.status, json.load(response)
            except urllib.error.HTTPError as error:
                status, result = error.code, json.load(error)
            if expected is not None:
                assert status == expected, (path, status, result)
            return status, result

        def boot(log):
            process = subprocess.Popen([str(ROOT / "target/debug/adb-studio")], env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError("检查后台未能启动")
                try:
                    call("/session", auth=False)
                    return process
                except (OSError, urllib.error.URLError):
                    time.sleep(0.1)
            process.terminate()
            process.wait(timeout=10)
            raise AssertionError("检查后台启动超时")

        with (data / "server.log").open("w") as log:
            process = boot(log)
            try:
                call("/workflow-drafts", expected=401, auth=False)
                initial_workflows = call("/workflows")[1]
                draft = {
                    "id": str(uuid.uuid4()), "revision": 0, "name": "未接完的草稿",
                    "description": "", "entry": "wait", "viewport": [0, 0, 1],
                    "nodes": [{"id": "wait", "kind": "wait", "label": "等待",
                               "config": {"seconds": 2}, "targets": {}, "position": [100, 100]}],
                }
                path = "/workflow-drafts/" + draft["id"]
                saved = call(path, "PUT", draft)[1]
                assert saved["revision"] == 1
                call(path, "PUT", draft, expected=409)
                invalid = {**saved, "nodes": [{**saved["nodes"][0], "targets": {"next": "absent"}}]}
                call(path, "PUT", invalid, expected=400)
                with ThreadPoolExecutor(max_workers=2) as pool:
                    results = list(pool.map(lambda _: call(path, "PUT", saved, expected=None)[0], range(2)))
                assert sorted(results) == [200, 409], results
                latest = call(path)[1]
                assert latest["revision"] == 2
                process.terminate()
                process.wait(timeout=10)
                process = boot(log)
                assert call(path)[1] == latest
                assert call("/workflows")[1] == initial_workflows
                assert call("/runs")[1] == []
                call(path + "?revision=1", "DELETE", expected=409)
                call(path + "?revision=2", "DELETE")
                call(path, expected=404)
                call(path, "PUT", latest, expected=409)
                assert call("/workflow-drafts")[1] == []
                commands = (fake / "commands").read_text()
                assert '"input"' not in commands
                print("通过：未完成草稿保存、身份验证、非法连线拒绝、并发冲突、重启保留、删除冲突、草稿不参与执行")
            finally:
                process.terminate()
                process.wait(timeout=10)


if __name__ == "__main__":
    main()
