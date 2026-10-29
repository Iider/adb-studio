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
                samples.mkdir(exist_ok=True)
                image = str(uuid.uuid4()) + ".png"
                png = helpers["png"]()
                (samples / image).write_bytes(png)
                # Add optional sample references to an exported editor task.
                # Import validates files and stores new image names atomically.
                file = call(export_path)[0]
                file["content"]["workflow"]["tests"] = [
                    {"node":"wait", "image":image, "expected":"unknown"},
                    {"node":"wait", "image":image, "expected":"unknown"}]
                file["assets"] = [{"id":image, "mime":"image/png", "base64":base64.b64encode(png).decode()}]

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
                assert target['id'] != workflow['id'] and target['name'] == payload['name']
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

                raw_request = request_file(file['content']['workflow'])
                call('/workflow-import/preview', 'POST', raw_request, expected=400)
                call('/workflow-import', 'POST', raw_request, expected=400)
                old = copy.deepcopy(file)
                old['content']['workflow']['nodes'][0]['type'] = 'vision_check'
                call('/workflow-import', 'POST', request_file(old), expected=400)
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
                print('通过：拒绝旧任务与裸文件、未完成草稿、画布布局及本体与草稿差异保留', flush=True)

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
                variable_id = str(uuid.uuid4())
                variable_draft = copy.deepcopy(draft)
                variable_draft.update(id=str(uuid.uuid4()), revision=0, inputs=[{"id":variable_id,"name":"delay","alias":"等待时长","type":"number","default":0.2}])
                variable_draft["nodes"][0]["config"]["seconds"] = {"input":variable_id}
                variable_path = '/workflow-drafts/' + variable_draft['id']
                saved_variable = call(variable_path,'PUT',variable_draft)[0]
                variable_workflow = call(variable_path + '/save','POST',{'revision':saved_variable['revision']})[0]
                saved_variable['inputs'][0]['default'] = 0.4
                saved_variable = call(variable_path,'PUT',saved_variable)[0]
                exported, headers = call('/workflows/' + variable_workflow['id'] + '/export')
                assert exported['format_revision'] == 2 and headers['X-Workflow-Draft-State'] == 'modified'
                assert exported['content']['workflow']['inputs'][0]['default'] == 0.2
                assert exported['content']['draft']['inputs'][0]['default'] == 0.4
                assert 'unit' not in exported['content']['workflow']['inputs'][0]
                legacy = copy.deepcopy(exported)
                legacy['content']['workflow']['inputs'][0]['unit'] = 'milliseconds'
                legacy['content']['draft']['inputs'][0]['unit'] = 'seconds'
                legacy_result = call('/workflow-import','POST',request_file(legacy))[0]
                legacy_draft = call('/workflow-drafts/' + legacy_result['draft_id'])[0]
                legacy_workflow = next(w for w in call('/workflows')[0] if w['id'] == legacy_result['workflow_id'])
                assert legacy_draft['inputs'] == saved_variable['inputs']
                assert legacy_workflow['inputs'] == variable_workflow['inputs']
                assert legacy_workflow['nodes'][0]['step']['config']['seconds'] == {'input':variable_id}
                legacy_export = call('/workflows/' + legacy_workflow['id'] + '/export')[0]
                assert 'unit' not in legacy_export['content']['workflow']['inputs'][0]
                assert 'unit' not in legacy_export['content']['draft']['inputs'][0]
                imported_variables = call('/workflow-import','POST',request_file(exported))[0]
                imported_draft = call('/workflow-drafts/' + imported_variables['draft_id'])[0]
                imported_workflow = next(w for w in call('/workflows')[0] if w['id'] == imported_variables['workflow_id'])
                assert imported_draft['inputs'] == exported['content']['draft']['inputs']
                assert imported_workflow['inputs'] == variable_workflow['inputs']
                assert imported_workflow['inputs'][0]['alias'] == '等待时长'
                assert imported_workflow['nodes'][0]['step']['config']['seconds'] == {'input':variable_id}
                variable_run = call('/runs','POST',{'device_id':'mock-phone','workflow_id':imported_workflow['id'],'inputs':{variable_id:0}})[0]
                assert done(variable_run['id'])['state'] == 'succeeded'
                invalid_version = copy.deepcopy(exported); invalid_version['format_revision'] = 1
                call('/workflow-import/preview','POST',request_file(invalid_version),expected=400)
                invalid_reference = copy.deepcopy(exported)
                invalid_reference['content']['workflow']['nodes'][0]['step']['config']['seconds'] = {'input':str(uuid.uuid4())}
                call('/workflow-import/preview','POST',request_file(invalid_reference),expected=400)
                saved_variable['inputs'][0]['default'] = None
                saved_variable = call(variable_path,'PUT',saved_variable)[0]
                unfinished = call(variable_path + '/export?revision=' + str(saved_variable['revision']))[0]
                unfinished_copy = call('/workflow-import','POST',request_file(unfinished))[0]
                assert unfinished_copy['workflow_id'] is None
                print('通过：变量与引用往返、旧变量单位丢弃且数值不换算、草稿和本体默认值分别保留、导入后临时填写运行、未完成变量草稿保留、伪造版本与失效本体引用拒绝', flush=True)
            finally:
                stop(process, data)
                process.wait(timeout=15)


if __name__ == "__main__":
    main()
