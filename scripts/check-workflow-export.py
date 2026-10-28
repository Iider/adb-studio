#!/usr/bin/env python3
"""用临时数据和模拟 ADB 检查文件导出与副本导入，不连接真实手机。"""

import base64
import copy
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid
import sqlite3
from contextlib import closing
from concurrent.futures import ThreadPoolExecutor

from check_support import executable, python_program, environment, stop

ROOT = Path(__file__).resolve().parents[1]


def main():
    helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    base = f"http://127.0.0.1:{port}/api"
    with tempfile.TemporaryDirectory(prefix="adb-export-check-") as folder:
        data = Path(folder)
        fake = data / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (fake / "screen.png").write_bytes(helpers["png"]())
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        env = {
            **os.environ, **environment(data),
            "ADB_STUDIO_ROOT": str(ROOT), "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": str(port),
            "FAKE_ADB_DIR": str(fake),
        }

        def call(path, method="GET", body=None, expected=200, auth=True, padding=0):
            headers = {"Content-Type": "application/json"}
            if auth:
                headers["Authorization"] = "Bearer " + (data / "access-token").read_text().strip()
            request = urllib.request.Request(base + path, method=method, headers=headers,
                data=None if body is None else json.dumps(body).encode() + b" " * padding)
            try:
                with urllib.request.urlopen(request, timeout=20) as response:
                    status, value, metadata = response.status, json.load(response), response.headers
            except urllib.error.HTTPError as error:
                status, value, metadata = error.code, json.load(error), error.headers
            assert status == expected, (path, status, value)
            return value, metadata

        def done(run_id):
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                run = next(item for item in call("/runs")[0] if item["id"] == run_id)
                if run["state"] not in {"running", "waiting_device", "recovering", "paused"}:
                    assert run["state"] == "succeeded", run
                    return run
                time.sleep(0.1)
            raise AssertionError("模拟任务未结束")

        with (data / "server.log").open("w") as log:
            process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))],
                env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 15
                while True:
                    if process.poll() is not None:
                        raise AssertionError("隔离服务启动失败")
                    try:
                        call("/session", auth=False)
                        break
                    except (OSError, urllib.error.URLError):
                        assert time.monotonic() < deadline, "隔离服务启动超时"
                        time.sleep(0.1)

                call("/workflows/swipe/export", expected=401, auth=False)
                call("/workflows/missing/export", expected=404)
                before = call("/workflows")[0]
                for task in before:
                    file, headers = call(f"/workflows/{task['id']}/export")
                    assert file["format"] == "adb-studio.workflow"
                    assert file["format_revision"] == 1
                    assert "attachment" in headers["Content-Disposition"]
                    assert "archived" not in file["content"]["workflow"]
                    assert file["content"]["draft"] is None
                    assert "version" not in file["content"]["workflow"]
                    assert not any(key in file for key in ("token", "settings", "runs", "schedules"))
                assert call("/workflows")[0] == before
                print("通过：所有内置任务导出、权限、缺失任务、不改变本体、不携带状态", flush=True)

                draft = {"id": str(uuid.uuid4()), "revision": 0, "name": "导出检查",
                    "description": "", "entry": "wait", "viewport": [35, 48, 0.7], "nodes": [
                        {"id": "wait", "kind": "wait", "label": "等待", "config": {"seconds": 1},
                            "targets": {"next": "end"}, "position": [100, 200]},
                        {"id": "end", "kind": "end", "label": "完成", "config": {"result": "succeeded"},
                            "targets": {}, "position": [300, 400]},
                    ]}
                path = "/workflow-drafts/" + draft["id"]
                saved = call(path, "PUT", draft)[0]
                workflow = call(path + "/save", "POST", {"revision": saved["revision"]})[0]
                export_path = f"/workflows/{workflow['id']}/export"
                file, headers = call(export_path)
                assert headers["x-workflow-draft-state"] == "saved"
                portable = file["content"]["draft"]
                assert portable["viewport"] == draft["viewport"] and portable["nodes"] == draft["nodes"]
                assert "revision" not in portable and "updated_at" not in portable
                original = copy.deepcopy(file["content"]["workflow"])
                saved["nodes"][0]["config"]["seconds"] = 3
                saved = call(path, "PUT", saved)[0]
                file, headers = call(export_path)
                assert headers["x-workflow-draft-state"] == "modified"
                assert file["content"]["workflow"] == original
                assert file["content"]["draft"]["nodes"][0]["config"]["seconds"] == 3
                call(path + "/export?revision=1", expected=409)
                saved["nodes"][0]["targets"] = {}
                saved = call(path, "PUT", saved)[0]
                assert call(export_path)[1]["x-workflow-draft-state"] == "incomplete"
                file, _ = call(path + f"/export?revision={saved['revision']}")
                assert file["content"]["workflow"] is None
                assert file["content"]["draft"]["nodes"] == saved["nodes"]
                print("通过：画布布局、草稿与本体差异、未完成草稿、校对冲突", flush=True)

                samples = data / "vision-samples"
                samples.mkdir()
                image = str(uuid.uuid4()) + ".png"
                png = helpers["png"]()
                (samples / image).write_bytes(png)
                visual = {"schema_version": 1, "id": "visual-export", "name": "图片导出",
                    "description": "", "devices": [], "entry": "check", "model_limit": 10,
                    "recognition": "vision", "nodes": [
                        {"id": "check", "label": "判断页面", "type": "vision_check",
                         "package": "test.app", "condition": "指定页面", "next": "end"},
                        {"id": "end", "label": "完成", "type": "end", "result": "succeeded"}],
                    "tests": [{"node": "check", "image": image, "expected": "confirmed"},
                              {"node": "check", "image": image, "expected": "unknown"}]}
                call("/workflows", "POST", visual)
                file, _ = call("/workflows/visual-export/export")
                assert len(file["assets"]) == 1
                assert base64.b64decode(file["assets"][0]["base64"]) == png
                (samples / image).write_bytes(helpers["png"](6000, 3000))
                assert call("/workflows/visual-export/export")[0]["assets"]
                (samples / image).write_bytes(b"not a png")
                call("/workflows/visual-export/export", expected=400)
                (samples / image).unlink()
                assert image in call("/workflows/visual-export/export", expected=400)[0]["error"]
                visual["tests"][0]["image"] = "../access-token"
                call("/workflows", "POST", visual)
                call("/workflows/visual-export/export", expected=400)
                print("通过：图片携带与去重、现有大图兼容、缺图、损坏图片、路径拒绝", flush=True)

                point = call("/workflows/point-click/export")[0]["content"]["workflow"]
                assert point["launch_mode"] == "device_point"
                point["id"] = "point-copy"
                call("/workflows", "POST", point)
                call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": "point-copy"}, expected=400)
                call("/devices/mock-phone", "PUT", {"tap_point": {"x": 123, "y": 234, "width": 400, "height": 800}})
                run = call("/runs", "POST", {"device_id": "mock-phone", "workflow_id": "point-copy"})[0]
                assert run["resume_size"] == [400, 800]
                assert done(run["id"])["workflow"]["nodes"][0]["x"] == 123
                assert any(json.loads(line)[3:] == ["123", "234"]
                    for line in (fake / "taps").read_text().splitlines())
                print("通过：定点点击更换编号后保留标注依赖、坐标注入与恢复屏幕尺寸", flush=True)

                def request_file(content, name=None):
                    return {"operation_id": str(uuid.uuid4()), "file": copy.deepcopy(content), "name": name}

                baseline = {key: call('/' + key)[0] for key in ['workflows', 'workflow-drafts', 'runs', 'schedules']}
                payload = request_file(file)
                call('/workflow-import/preview', 'POST', payload, expected=401, auth=False)
                summary = call('/workflow-import/preview', 'POST', payload, padding=3 * 1024 * 1024)[0]
                assert summary['name'].endswith('（导入）') and summary['images'] == 1
                assert baseline == {key: call('/' + key)[0] for key in baseline}
                payload['name'] = '导入图片副本'
                with ThreadPoolExecutor(max_workers=2) as pool:
                    imported, repeated = list(pool.map(lambda _: call('/workflow-import', 'POST', payload)[0], range(2)))
                assert imported == repeated
                target = next(w for w in call('/workflows')[0] if w['id'] == imported['workflow_id'])
                assert target['id'] != visual['id'] and target['name'] == payload['name']
                new_image = target['tests'][0]['image']
                assert new_image != image and target['tests'][1]['image'] == new_image
                assert (samples / new_image).read_bytes() == png
                assert len(call('/workflows')[0]) == len(baseline['workflows']) + 1
                assert call('/runs')[0] == baseline['runs'] and call('/schedules')[0] == baseline['schedules']
                conflict = copy.deepcopy(payload); conflict['name'] = '其他名称'
                call('/workflow-import', 'POST', conflict, expected=409)
                another = call('/workflow-import', 'POST', request_file(file))[0]
                assert another['workflow_id'] != imported['workflow_id']
                print('通过：预览零写入、超过2MiB请求、图片重新编号、并发与重试不重复、同文件独立副本、不运行或创建定时', flush=True)

                raw = call('/workflows/point-click/export')[0]['content']['workflow']
                raw['devices'] = ['phone-not-registered']
                raw_request = request_file(raw)
                summary = call('/workflow-import/preview', 'POST', raw_request)[0]
                assert any('未登记' in warning for warning in summary['warnings'])
                assert any('本机标注' in warning for warning in summary['warnings'])
                raw_result = call('/workflow-import', 'POST', raw_request)[0]
                raw_copy = next(w for w in call('/workflows')[0] if w['id'] == raw_result['workflow_id'])
                assert raw_copy['launch_mode'] == 'device_point' and raw_copy['devices'] == raw['devices']
                mixed = call(export_path)[0]
                mixed_result = call('/workflow-import', 'POST', request_file(mixed))[0]
                copied_draft = call('/workflow-drafts/' + mixed_result['draft_id'])[0]
                copied_body = next(w for w in call('/workflows')[0] if w['id'] == mixed_result['workflow_id'])
                assert copied_body['id'] == 'manual-' + copied_draft['id']
                assert copied_draft['nodes'] == mixed['content']['draft']['nodes']
                assert copied_draft['viewport'] == mixed['content']['draft']['viewport']
                assert copied_body['nodes'] == mixed['content']['workflow']['nodes']
                assert copied_draft['revision'] == 1
                draft_file = call(path + f"/export?revision={saved['revision']}")[0]
                draft_result = call('/workflow-import', 'POST', request_file(draft_file))[0]
                assert draft_result['workflow_id'] is None
                assert call('/workflow-drafts/' + draft_result['draft_id'])[0]['nodes'] == draft_file['content']['draft']['nodes']
                print('通过：裸工作流、离线手机保留标注、内置运行方式、未完成草稿、画布布局及本体与草稿差异保留', flush=True)

                stable = call('/workflows')[0]
                bad_files = []
                for mutation in ['missing', 'extra', 'duplicate', 'corrupt', 'format', 'unknown', 'path', 'node']:
                    bad = copy.deepcopy(file)
                    if mutation == 'missing': bad['assets'] = []
                    if mutation == 'extra': bad['assets'].append({**bad['assets'][0], 'id': str(uuid.uuid4()) + '.png'})
                    if mutation == 'duplicate': bad['assets'].append(bad['assets'][0])
                    if mutation == 'corrupt': bad['assets'][0]['base64'] = base64.b64encode(b'invalid PNG').decode()
                    if mutation == 'format': bad['format_revision'] = 999
                    if mutation == 'unknown': bad['unexpected'] = True
                    if mutation == 'path': bad['content']['workflow']['tests'][0]['image'] = '../access-token'
                    if mutation == 'node': bad['content']['workflow']['nodes'][0]['type'] = 'run_shell'
                    bad_files.append(bad)
                for bad in bad_files:
                    call('/workflow-import/preview', 'POST', request_file(bad), expected=400)
                    call('/workflow-import', 'POST', request_file(bad), expected=400)
                call('/workflow-import', 'POST', request_file(draft_file, 'x' * 81), expected=400)
                assert call('/workflows')[0] == stable
                before_images = set(samples.iterdir())
                with closing(sqlite3.connect(data / 'data.sqlite')) as db, db:
                    db.execute("CREATE TRIGGER fail_import BEFORE INSERT ON records WHEN NEW.kind='workflow_imports' BEGIN SELECT RAISE(ABORT, 'test import failure'); END")
                call('/workflow-import', 'POST', request_file(mixed), expected=500)
                call('/workflow-import', 'POST', request_file(file), expected=500)
                with closing(sqlite3.connect(data / 'data.sqlite')) as db, db:
                    db.execute('DROP TRIGGER fail_import')
                assert call('/workflows')[0] == stable and set(samples.iterdir()) == before_images
                assert not list((data / 'import-staging').iterdir())
                print('通过：格式、路径、重复和缺失附件、坏图与非法步骤拒绝；数据库失败整体回退并清理图片', flush=True)

                staging = data / 'import-staging' / str(uuid.uuid4()); staging.mkdir(parents=True)
                orphan = str(uuid.uuid4()) + '.png'; unrelated = str(uuid.uuid4()) + '.png'
                (samples / orphan).write_bytes(png); (samples / unrelated).write_bytes(png)
                (staging / 'manifest.json').write_text(json.dumps([orphan, new_image, '../access-token']))
                stop(process, data); process.wait(timeout=15)
                process = subprocess.Popen([str(executable(ROOT / 'target/debug/adb-studio'))], env=env, stdout=log, stderr=log)
                helpers['wait'](lambda: call('/session', auth=False))
                assert not (samples / orphan).exists() and not staging.exists()
                assert (samples / new_image).exists() and (samples / unrelated).exists() and (data / 'access-token').exists()
                assert call('/workflow-import', 'POST', payload)[0] == imported
                call('/workflows/' + imported['workflow_id'], 'DELETE')
                assert call('/workflow-import', 'POST', payload)[0]['removed'] is True
                print('通过：重启后导入结果可确认、已删除副本不重建、清理中断残留只处理无引用的本次图片', flush=True)
            finally:
                stop(process, data)
                process.wait(timeout=15)


if __name__ == "__main__":
    main()
