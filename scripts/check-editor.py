#!/usr/bin/env python3
"""用隔离数据库和模拟 ADB 验证画布草稿与节点执行，不连接真实手机。"""

from concurrent.futures import ThreadPoolExecutor
import json
import datetime as dt
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

from check_support import executable, python_program, environment, stop

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
        ocr = python_program(ocr)
        adb = data / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        env = {
            **os.environ, **environment(data),
            "ADB_STUDIO_ROOT": str(ROOT),
            "ADB_STUDIO_DATA": str(data),
            "ADB_STUDIO_ADB": str(adb),
            "ADB_STUDIO_PORT": "17844",
            "FAKE_ADB_DIR": str(fake),
        }

        def call(path, method="GET", body=None, expected=200, auth=True, extra_headers=None):
            headers = {"Content-Type": "application/json", **(extra_headers or {})}
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
            process = subprocess.Popen([str(executable(ROOT / "target/debug/adb-studio"))], env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError("检查后台未能启动")
                try:
                    call("/session", auth=False)
                    call("/settings/model", "PUT", {"endpoint":"http://127.0.0.1:1", "model":"unused", "api":"responses", "key_env":"UNCONFIGURED_EDITOR_TEST_KEY"})
                    return process
                except (OSError, urllib.error.URLError):
                    time.sleep(0.1)
            stop(process, data)
            process.wait(timeout=10)
            raise AssertionError("检查后台启动超时")

        with (data / "server.log").open("w") as log:
            process = boot(log)
            try:
                call("/workflow-drafts", expected=401, auth=False)
                initial_workflows = call("/workflows")[1]
                assert initial_workflows == []
                assert all("archived" not in item for item in initial_workflows)
                call("/workflows/swipe/archive", "PUT", expected=404)
                call("/workflows/swipe/archive", "DELETE", expected=404)
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
                stop(process, data)
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
                (fake / "online").touch()
                (fake / "stay_on_while_plugged_in").write_text("3")
                (fake / "screen_off_timeout").write_text("600000")
                call("/devices/refresh", "POST")

                def wait_value(fn, timeout=35):
                    deadline = time.monotonic() + timeout
                    while time.monotonic() < deadline:
                        value = fn()
                        if value:
                            return value
                        time.sleep(0.1)
                    raise AssertionError("等待编排任务超时")

                def node(id, kind, targets=None, **config):
                    return {"id":id,"kind":kind,"label":id,"config":config,"targets":targets or {},"position":[0,0]}

                def save_workflow(nodes, entry, expected=200):
                    body = {"id":str(uuid.uuid4()),"revision":0,"name":"编排执行检查","description":"","entry":entry,"viewport":[0,0,1],"nodes":nodes}
                    saved = call("/workflow-drafts/" + body["id"], "PUT", body)[1]
                    return call("/workflow-drafts/" + body["id"] + "/save", "POST", {"revision":saved["revision"]},expected=expected)[1], saved

                def run_workflow(workflow, device="mock-phone"):
                    return call("/runs","POST",{"device_id":device,"workflow_id":workflow["id"]})[1]

                def record(id):
                    return next(row for row in call("/runs")[1] if row["id"] == id)

                def finish(id):
                    return wait_value(lambda: (row if row["state"] not in {"running","recovering","waiting_device","paused"} else None) if (row := record(id)) else None)

                def taps():
                    return len((fake / "taps").read_text().splitlines()) if (fake / "taps").exists() else 0

                profile = {"device_id":"mock-phone","width":400,"height":800,"display":"default","x":0.5,"y":0.5,"to_x":0.8,"to_y":0.2}
                flow = [
                    node("loop","loop",{"body":"tap","done":"end"},count=3,interval=0.01),
                    node("tap","tap",{"next":"wait"},count=2,interval=0.01,profiles=[profile]),
                    node("wait","wait",{"next":"swipe"},seconds=0.01),
                    node("swipe","swipe",{"next":"loop"},count=1,interval=0,duration_ms=250,profiles=[profile]),
                    node("end","end",result="succeeded"),
                ]
                workflow, source = save_workflow(flow, "loop")
                call("/workflow-drafts/" + source["id"] + "?revision=" + str(source["revision"]), "DELETE", expected=409)
                first_schedule = call("/schedules", "POST", {"name":"无需预运行", "device_id":"mock-phone", "workflow_id":workflow["id"], "enabled":False,"rule":{"kind":"interval","minutes":60}})[1]
                assert first_schedule["workflow_id"] == workflow["id"]
                call("/schedules/" + first_schedule["id"], "DELETE")
                run = run_workflow(workflow)
                result = finish(run["id"])
                assert result["state"] == "succeeded", result
                assert taps() == 6
                assert len((fake / "swipes").read_text().splitlines()) == 3
                assert all(json.loads(line)[-2:] == ["200", "400"] for line in (fake / "taps").read_text().splitlines())
                assert all(json.loads(line)[-5:] == ["200", "400", "320", "160", "250"] for line in (fake / "swipes").read_text().splitlines())
                assert result["canvas_progress"]["loops"]["loop"]["ended"] == 3
                source["nodes"][0]["position"] = [300,400]
                source = call("/workflow-drafts/" + source["id"],"PUT",source)[1]
                same = call("/workflow-drafts/" + source["id"] + "/save","POST",{"revision":source["revision"]})[1]
                assert same == workflow
                assert len([row for row in call("/workflows")[1] if row["id"] == workflow["id"]]) == 1
                (fake / "screen.png").write_bytes(helpers["png"](800,400))
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and taps() == 6
                (fake / "screen.png").write_bytes(helpers["png"]())
                print("通过：循环轮数、重复点击、滑动路线、重复保存与布局修改仅保留一份任务、屏幕变化拒绝动作")

                busy_workflow, _ = save_workflow([
                    node("wait", "wait", {"next":"end"}, seconds=8),
                    node("end", "end", result="succeeded"),
                ], "wait")
                busy_run = run_workflow(busy_workflow)
                call("/workflows/" + busy_workflow["id"], "DELETE", expected=400)
                call("/runs/" + busy_run["id"] + "/pause", "POST")
                wait_value(lambda: record(busy_run["id"])["state"] == "paused")
                call("/workflows/" + busy_workflow["id"], "DELETE", expected=400)
                call("/runs/" + busy_run["id"] + "/stop", "POST")
                finish(busy_run["id"])
                blocked_shots = data / "shots" / busy_run["id"]
                blocked_shots.write_text("模拟暂时无法清理的目录")
                removed = call("/workflows/" + busy_workflow["id"], "DELETE")[1]
                assert removed["cleanup_pending"]
                call("/runs/" + busy_run["id"] + "/resume", "POST", expected=400)
                blocked_shots.unlink()
                blocked_shots.mkdir()
                (blocked_shots / "retry.png").write_bytes(helpers["png"]())
                stop(process, data)
                process.wait(timeout=10)
                process = boot(log)
                assert not blocked_shots.exists()
                assert not any(item["workflow"]["id"] == busy_workflow["id"] for item in call("/runs")[1])
                print("通过：截图暂时无法清理时明确提示，重启后完成剩余清理")

                schedule_body = {"name":"删除检查","device_id":"mock-phone","workflow_id":workflow["id"],"enabled":True,"rule":{"kind":"interval","minutes":60}}
                scheduled = call("/schedules", "POST", schedule_body)[1]
                assert scheduled["enabled"]
                history_ids = [item["id"] for item in call("/runs")[1] if item["workflow"]["id"] == workflow["id"]]
                assert len(history_ids) >= 2
                for run_id in history_ids:
                    folder = data / "shots" / run_id
                    folder.mkdir(exist_ok=True)
                    (folder / "delete-proof.png").write_bytes(helpers["png"]())
                call("/runs/" + history_ids[0], "DELETE")
                assert (data / "shots" / history_ids[0]).exists()
                assert not call("/workflows/" + workflow["id"], "DELETE")[1]["cleanup_pending"]
                assert all(not (data / "shots" / run_id).exists() for run_id in history_ids)
                assert not any(item["id"] == workflow["id"] for item in call("/workflows")[1])
                assert not any(item["id"] == scheduled["id"] for item in call("/schedules")[1])
                call("/workflow-drafts/" + source["id"], expected=404)
                call("/workflow-drafts/" + source["id"], "PUT", source, expected=409)
                assert not any(item["workflow"]["id"] == workflow["id"] for item in call("/runs")[1])
                assert call("/runs/" + result["id"] + "/events")[1] == []
                call("/runs/" + result["id"] + "/export", expected=400)
                assert not (data / "shots" / result["id"]).exists()
                print("通过：运行或暂停时拒绝删除；草稿、定时、记录和截图一并清理；删除后不能续跑")

                samples = [{"text":"","app":"","round":2,"expected":"yes"},{"text":"","app":"","round":1,"expected":"no"}]
                for control, expected in [("continue",2),("break",1)]:
                    before = taps()
                    flow = [
                        node("loop","loop",{"body":"condition","done":"end"},count=3,interval=0.01),
                        node("condition","condition",{"yes":"control","no":"tap"},mode="all",conditions=[{"source":"round","operator":"eq","value":"2","guard":""}]),
                        node("control",control,loop="loop"),
                        node("tap","tap",{"next":"loop"},count=1,interval=0,profiles=[profile]),
                        node("end","end",result="succeeded"),
                    ]
                    workflow, _ = save_workflow(flow,"loop")
                    result = finish(run_workflow(workflow)["id"])
                    assert result["state"] == "succeeded" and taps() - before == expected, result
                print("通过：轮次判断、进入下一轮、提前退出循环")

                for control, round_number, expected in [("continue", 3, 2), ("break", 1, 0)]:
                    before = taps()
                    workflow, _ = save_workflow([
                        node("loop", "loop", {"body":"check", "done":"end"}, count=3, interval=0.01),
                        node("check", "condition", {"yes":"control", "no":"tap"}, mode="all", conditions=[{"source":"round", "operator":"eq", "value":str(round_number), "guard":""}], samples=[{"round":round_number,"expected":"yes"},{"round":2 if round_number == 1 else 1,"expected":"no"}]),
                        node("control", control, loop="loop"),
                        node("tap", "tap", {"next":"loop"}, count=1, interval=0, profiles=[profile]),
                        node("end", "end", result="succeeded"),
                    ], "loop")
                    result = finish(run_workflow(workflow)["id"])
                    assert result["state"] == "succeeded" and taps() - before == expected
                    if control == "continue":
                        assert result["canvas_progress"]["loops"]["loop"]["ended"] == 3
                print("通过：最后一轮跳过直接完成、第一轮提前退出")

                before = taps()
                workflow, _ = save_workflow([node("tap","tap",{"next":"end"},count=2,interval=0,profiles=[profile]),node("end","end",result="succeeded")],"tap")
                (fake / "reject-input").touch()
                run = run_workflow(workflow)
                wait_value(lambda: record(run["id"])["state"] == "paused")
                call("/runs/" + run["id"] + "/resume","POST",expected=400)
                call("/runs/" + run["id"] + "/stop","POST")
                wait_value(lambda: record(run["id"])["state"] == "cancelled")
                assert taps() == before
                (fake / "reject-input").unlink()
                print("通过：触控拒绝保留暂停，未确认动作不重放")
                text_condition = [{"source":"text","operator":"contains","value":"领取","guard":"任务页"}]
                text_samples = [{"text":"任务页 领取","app":"","round":1,"expected":"yes"},{"text":"任务页","app":"","round":1,"expected":"no"},{"text":"其他页面","app":"","round":1,"expected":"unknown"}]
                def words(text):
                    (fake / "words").write_text(json.dumps([{"text":text,"x":200,"y":400,"confidence":1}]))

                for kind in ["condition", "wait_condition"]:
                    for guard in [None, ""]:
                        condition = {"source":"text", "operator":"contains", "value":"奖励"}
                        if guard is not None:
                            condition["guard"] = guard
                        workflow, _ = save_workflow([
                            node("check", kind, {"yes":"yes", "no" if kind == "condition" else "timeout":"no"}, mode="all", conditions=[condition], interval=0.1, timeout=2),
                            node("yes", "end", result="succeeded"), node("no", "end", result="succeeded"),
                        ], "check")
                        for text, target in [("奖励", "yes"), ("其他页面", "no")]:
                            words(text)
                            result = finish(run_workflow(workflow)["id"])
                            assert result["state"] == "succeeded" and result["node"] == target, result
                print("通过：条件判断与等待条件不填页面依据即可保存、运行，正常分流与超时")

                workflow, _ = save_workflow([
                    node("await","wait_condition",{"yes":"end","timeout":"timeout"},mode="all",conditions=text_condition,samples=text_samples,interval=0.1,timeout=2),
                    node("end","end",result="succeeded"),node("timeout","end",result="needs_help")
                ],"await")
                words("任务页 领取")
                assert finish(run_workflow(workflow)["id"])["state"] == "succeeded"
                words("任务页")
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and result["node"] == "timeout", result
                words("陌生页面")
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and result["node"] == "await", result
                print("通过：等待条件立即满足、已知页面超时、陌生页面停止")

                (fake / "words").write_text("invalid OCR output")
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and result["node"] == "await"
                (fake / "words").write_text(json.dumps([{"text":"任务页 领取", "x":200, "y":400, "confidence":0.1}]))
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and result["node"] == "await"
                print("通过：识别失败、低可信文字不会误走超时或符合分支")

                app_condition = [{"source":"app", "operator":"is", "value":"com.example.ready", "guard":""}]
                app_samples = [{"app":"com.example.ready", "expected":"yes"}, {"app":"com.luna.music", "expected":"no"}]
                workflow, _ = save_workflow([
                    node("await", "wait_condition", {"yes":"end", "timeout":"timeout"}, mode="all", conditions=app_condition, samples=app_samples, interval=0.2, timeout=2),
                    node("end", "end", result="succeeded"), node("timeout", "end", result="needs_help"),
                ], "await")
                started = time.monotonic()
                run = run_workflow(workflow)
                wait_value(lambda: record(run["id"])["checkpoint"].get("condition_elapsed", 0) >= 0.2)
                call("/runs/" + run["id"] + "/pause", "POST")
                wait_value(lambda: record(run["id"])["state"] == "paused")
                elapsed = record(run["id"])["checkpoint"]["condition_elapsed"]
                time.sleep(0.4)
                assert record(run["id"])["checkpoint"]["condition_elapsed"] == elapsed
                call("/runs/" + run["id"] + "/resume", "POST")
                result = finish(run["id"])
                assert result["node"] == "timeout" and time.monotonic() - started >= 2.4
                (fake / "foreground").write_text("com.example.ready")
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "succeeded"
                (fake / "foreground").unlink()
                print("通过：当前应用条件、等待条件暂停期间不计时")

                workflow, _ = save_workflow([
                    node("back","back",{"next":"wait"}),
                    node("wait","wait",{"next":"home"},seconds=2),
                    node("home","home",{"next":"recents"}),
                    node("recents","recents",{"next":"end"}),
                    node("end","end",result="succeeded")
                ],"back")
                offset = len((fake / "commands").read_text().splitlines())
                started = time.monotonic()
                run = run_workflow(workflow)
                wait_value(lambda: record(run["id"])["node"] == "wait" and record(run["id"])["checkpoint"].get("wait_elapsed", 0) >= 0.5)
                call("/runs/" + run["id"] + "/pause", "POST")
                wait_value(lambda: record(run["id"])["state"] == "paused")
                checkpoint = record(run["id"])["checkpoint"]
                time.sleep(0.3)
                assert record(run["id"])["checkpoint"] == checkpoint
                commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()[offset:]]
                assert [command[-1] for command in commands if command[2:5] == ["shell", "input", "keyevent"]] == ["KEYCODE_BACK"]
                call("/runs/" + run["id"] + "/resume", "POST")
                assert finish(run["id"])["state"] == "succeeded"
                assert time.monotonic() - started >= 2.3
                commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()[offset:]]
                assert [command[-1] for command in commands if command[2:5] == ["shell", "input", "keyevent"]] == ["KEYCODE_BACK", "KEYCODE_HOME", "KEYCODE_APP_SWITCH"]
                print("通过：三种按键顺序与次数、等待时长、暂停续接不补按、首次直接运行")

                workflow, _ = save_workflow([node("back", "back", {"next":"end"}), node("end", "end", result="succeeded")], "back")
                (fake / "uncertain-key").touch()
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "needs_help" and result["pending"] == "canvas_key"
                call("/runs/" + result["id"] + "/resume", "POST", expected=400)
                (fake / "uncertain-key").unlink()
                print("通过：按键未获确认时保留证据，不继续后续动作")

                before = taps()
                workflow, _ = save_workflow([
                    node("loop","loop",{"body":"tap","done":"end"},count=2,interval=0.01),
                    node("tap","tap",{"next":"wait"},count=1,interval=0,profiles=[profile]),
                    node("wait","wait",{"next":"loop"},seconds=2),
                    node("end","end",result="succeeded")
                ],"loop")
                run = run_workflow(workflow)
                wait_value(lambda: record(run["id"])["node"] == "wait" and record(run["id"])["checkpoint"].get("wait_elapsed",0) >= 0.5)
                call("/runs/" + run["id"] + "/pause","POST")
                wait_value(lambda: record(run["id"])["state"] == "paused")
                paused = record(run["id"])
                stop(process, data)
                process.wait(timeout=10)
                process = boot(log)
                restored = record(run["id"])
                assert restored["state"] == "paused"
                assert restored["canvas_progress"] == paused["canvas_progress"]
                assert restored["checkpoint"] == paused["checkpoint"]
                call("/runs/" + run["id"] + "/resume","POST")
                result = finish(run["id"])
                assert result["state"] == "succeeded" and taps() - before == 2, result
                assert result["canvas_progress"]["loops"]["loop"]["ended"] == 2
                print("通过：循环中暂停、后台重启保留轮次、恢复后不重做已完成动作")

                for kind in ["tap", "swipe"]:
                    action_log = fake / ("taps" if kind == "tap" else "swipes")
                    for count in [1, 3]:
                        before = len(action_log.read_text().splitlines())
                        workflow, _ = save_workflow([
                            node("action", kind, {"next":"end"}, count=count, interval=0,
                                 after_wait=2, duration_ms=100, profiles=[profile]),
                            node("end", "end", result="succeeded"),
                        ], "action")
                        run = run_workflow(workflow)
                        waiting = wait_value(lambda: row if (row := record(run["id"]))["checkpoint"].get("wait_elapsed", 0) >= 0.5 else None)
                        assert waiting["node"] == "action" and waiting["checkpoint"]["done"] == count, waiting
                        assert len(action_log.read_text().splitlines()) - before == count
                        if count == 3:
                            call("/runs/" + run["id"] + "/pause", "POST")
                            paused = wait_value(lambda: row if (row := record(run["id"]))["state"] == "paused" else None)
                            stop(process, data)
                            process.wait(timeout=10)
                            process = boot(log)
                            restored = record(run["id"])
                            assert restored["state"] == "paused"
                            assert restored["checkpoint"] == paused["checkpoint"]
                            waiting = restored
                            call("/runs/" + run["id"] + "/resume", "POST")
                        resumed = time.monotonic()
                        result = finish(run["id"])
                        assert result["state"] == "succeeded", result
                        assert time.monotonic() - resumed >= 2 - waiting["checkpoint"]["wait_elapsed"] - 0.2
                        assert len(action_log.read_text().splitlines()) - before == count
                    for config in [{}, {"after_wait":0}]:
                        workflow, _ = save_workflow([
                            node("action", kind, {"next":"end"}, count=1, interval=86400,
                                 duration_ms=100, profiles=[profile], **config),
                            node("end", "end", result="succeeded"),
                        ], "action")
                        assert finish(run_workflow(workflow)["id"])["state"] == "succeeded"
                    for seconds in [-1, 86401]:
                        rejected, _ = save_workflow([
                            node("action", kind, {"next":"end"}, count=1, interval=0,
                                 after_wait=seconds, duration_ms=100, profiles=[profile]),
                            node("end", "end", result="succeeded"),
                        ], "action", expected=400)
                        assert "结束后等待" in rejected["error"], rejected
                print("通过：点击和滑动全部动作后只等待一次、单次生效、旧配置与 0 秒兼容、范围校验、暂停重启续等且不重做动作")
                right_half = {**profile,"x":0.5,"y":0,"to_x":1,"to_y":1}
                (fake / "words").write_text(json.dumps([
                    {"text":"任务页","x":300,"y":200,"confidence":1},
                    {"text":"领取","x":100,"y":400,"confidence":1},
                ]))
                workflow, _ = save_workflow([
                    node("check","condition",{"yes":"unexpected","no":"end"},mode="all",conditions=[{**text_condition[0],"regions":[right_half]}],samples=text_samples),
                    node("end","end",result="succeeded"),node("unexpected","end",result="needs_help")
                ],"check")
                result = finish(run_workflow(workflow)["id"])
                assert result["state"] == "succeeded" and result["node"] == "end", result
                screen = call("/editor/observe/mock-phone")[1]
                assert screen["screen"]["width"] == 400 and screen["image"].startswith("data:image/png;base64,")
                rejected, _ = save_workflow([node("wait","wait",{"next":"wait"},seconds=0),node("end","end",result="succeeded")],"wait",expected=400)
                assert "回线" in rejected["error"]
                rejected, _ = save_workflow([node("loop","loop",{"body":"wait","done":"end"},count=10000,interval=0),node("wait","wait",{"next":"loop"},seconds=0),node("end","end",result="succeeded")],"loop",expected=400)
                assert "10000" in rejected["error"]
                print("通过：按手机框选文字区域、标注原图接口、非法回线和超大循环拒绝保存")

                locations = [
                    {**profile, "x":0.25, "y":0.75},
                    {**profile, "device_id":"mock-phone-2", "width":800, "height":400, "x":0.75, "y":0.25},
                ]
                workflow, _ = save_workflow([
                    node("tap", "tap", {"next":"end"}, count=1, interval=0, profiles=locations),
                    node("end", "end", result="succeeded"),
                ], "tap")
                assert finish(run_workflow(workflow)["id"])["state"] == "succeeded"
                assert json.loads((fake / "taps").read_text().splitlines()[-1])[-2:] == ["100", "600"]
                (fake / "identity").write_text("mock-phone-2")
                (fake / "screen.png").write_bytes(helpers["png"](800,400))
                call("/devices/refresh", "POST")
                assert finish(run_workflow(workflow, "mock-phone-2")["id"])["state"] == "succeeded"
                assert json.loads((fake / "taps").read_text().splitlines()[-1])[-2:] == ["600", "100"]
                (fake / "identity").unlink()
                (fake / "screen.png").write_bytes(helpers["png"]())
                call("/devices/refresh", "POST")
                print("通过：不同手机与尺寸使用独立坐标，各台手机均可首次直接运行")

                (fake / "displays").write_text('\n'.join([
                    'DisplayDeviceInfo{uniqueId="local:100", type INTERNAL, state OFF,}',
                    'DisplayDeviceInfo{uniqueId="local:200", type INTERNAL, state ON,}',
                    'mDisplayId=0', 'DisplayInfo{uniqueId="local:100"}',
                    'mDisplayId=3', 'DisplayInfo{uniqueId="local:200"}',
                ]))
                (fake / "expected-display").write_text("200")
                observed = call("/editor/observe/mock-phone")[1]
                assert observed["screen"]["display"] == "200" and observed["screen"]["input_display"] == 3
                workflow, _ = save_workflow([
                    node("tap", "tap", {"next":"end"}, count=1, interval=0, profiles=[{**profile,"display":"200"}]),
                    node("end", "end", result="succeeded"),
                ], "tap")
                offset = len((fake / "commands").read_text().splitlines())
                assert finish(run_workflow(workflow)["id"])["state"] == "succeeded"
                commands = [json.loads(line) for line in (fake / "commands").read_text().splitlines()[offset:]]
                assert any(command[2:] == ["shell","input","-d","3","tap","200","400"] for command in commands)
                (fake / "displays").write_text((fake / "displays").read_text().replace('mDisplayId=3\n', ''))
                before = taps()
                assert finish(run_workflow(workflow)["id"])["state"] == "needs_help" and taps() == before
                (fake / "displays").unlink()
                (fake / "expected-display").unlink()
                print("通过：截图与点击使用同一显示屏，无法确认触控编号时拒绝动作")
                rounds_id, wait_id = str(uuid.uuid4()), str(uuid.uuid4())
                variable_draft = {
                    "id":str(uuid.uuid4()), "revision":0, "name":"运行变量检查", "description":"", "entry":"loop", "viewport":[0,0,1],
                    "inputs":[
                        {"id":rounds_id,"name":"执行轮数","type":"integer","default":3,"min":1,"max":10},
                        {"id":wait_id,"name":"结束后等待","type":"number","default":0.05},
                    ],
                    "nodes":[
                        node("loop","loop",{"body":"tap","done":"end"},count={"input":rounds_id},interval=0),
                        node("tap","tap",{"next":"loop"},count=1,interval=1,after_wait={"input":wait_id},profiles=[profile]),
                        node("end","end",result="succeeded"),
                    ],
                }
                variable_path = "/workflow-drafts/" + variable_draft["id"]
                saved_variables = call(variable_path, "PUT", variable_draft)[1]
                variable_flow = call(variable_path + "/save", "POST", {"revision":saved_variables["revision"]})[1]
                assert variable_flow["schema_version"] == 2
                def variable_run(values):
                    return call("/runs", "POST", {"workflow_id":variable_flow["id"],"device_id":"mock-phone","inputs":values})[1]
                before = taps()
                result = finish(variable_run({rounds_id:2,wait_id:0})["id"])
                assert result["state"] == "succeeded" and taps() - before == 2, result
                assert result["inputs"] == {rounds_id:2,wait_id:0}
                assert result["workflow"]["nodes"][0]["step"]["config"]["count"] == 2
                assert next(w for w in call("/workflows")[1] if w["id"] == variable_flow["id"]) == variable_flow
                before = taps()
                result = finish(variable_run({})["id"])
                assert taps() - before == 3 and result["inputs"][rounds_id] == 3
                existing_runs = len(call("/runs")[1])
                for values in [{rounds_id:0},{rounds_id:1.5},{rounds_id:11},{wait_id:-1},{"missing":1}]:
                    call("/runs", "POST", {"workflow_id":variable_flow["id"],"device_id":"mock-phone","inputs":values}, expected=400)
                assert len(call("/runs")[1]) == existing_runs
                before = taps()
                run = variable_run({rounds_id:2,wait_id:2})
                wait_value(lambda: record(run["id"])["checkpoint"].get("done") == 1)
                call("/runs/" + run["id"] + "/pause", "POST")
                wait_value(lambda: record(run["id"])["state"] == "paused")
                saved_variables["inputs"][0]["default"] = 4
                saved_variables = call(variable_path,"PUT",saved_variables)[1]
                call(variable_path + "/save","POST",{"revision":saved_variables["revision"]})
                stop(process, data); process.wait(timeout=10); process = boot(log)
                restored = record(run["id"])
                assert restored["state"] == "paused" and restored["inputs"] == {rounds_id:2,wait_id:2}
                call("/runs/" + run["id"] + "/resume","POST")
                result = finish(run["id"])
                assert result["state"] == "succeeded" and taps() - before == 2, result
                assert result["workflow"]["inputs"][0]["default"] == 3
                print("通过：变量默认值、临时覆盖与零值、执行次数、非法输入零启动；暂停重启后不受新默认值影响", flush=True)
                before = taps()
                run = variable_run({rounds_id:2,wait_id:1})
                wait_value(lambda: record(run["id"])["checkpoint"].get("done") == 1)
                (fake / "online").unlink()
                call("/devices/refresh", "POST")
                wait_value(lambda: record(run["id"])["state"] == "waiting_device")
                assert record(run["id"])["inputs"] == {rounds_id:2,wait_id:1}
                (fake / "online").touch()
                call("/devices/refresh", "POST")
                assert finish(run["id"])["state"] == "succeeded" and taps() - before == 2
                before = taps()
                previous = {row['id'] for row in call('/runs')[1]}
                call('/schedules','POST',{'name':'变量定时','workflow_id':variable_flow['id'],'device_id':'mock-phone','enabled':True,'rule':{'kind':'once','at':(dt.datetime.now(dt.timezone.utc)+dt.timedelta(seconds=3)).isoformat()}})
                scheduled = wait_value(lambda: next((row for row in call('/runs')[1] if row['id'] not in previous),None))
                assert finish(scheduled['id'])['state'] == 'succeeded'
                assert record(scheduled['id'])['inputs'][rounds_id] == 4 and taps() - before == 4
                assert record(scheduled['id'])['input_overrides'] == {}
                print('通过：变量断线恢复保持原值，定时使用当前默认值且不沿用手动输入', flush=True)
            finally:
                stop(process, data)
                process.wait(timeout=10)


if __name__ == "__main__":
    main()
