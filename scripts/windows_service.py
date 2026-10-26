"""Windows 当前用户后台：登录启动、单实例、正常停止。"""

import argparse
import base64
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import traceback
import urllib.request


def private_directory(path):
    path.mkdir(parents=True, exist_ok=True)
    identity = subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"])
    sid = re.search(rb"S-1-[0-9-]+", identity).group().decode("ascii")
    subprocess.run(["icacls", str(path), "/inheritance:r", "/grant:r", f"*{sid}:(OI)(CI)F"],
                   check=True, stdout=subprocess.DEVNULL)


def running(runtime):
    import msvcrt
    lock = runtime / "data/service.lock"
    if not lock.exists():
        return False
    with lock.open("r+b") as handle:
        try:
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError:
            return True
        msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
    return False


def stop(runtime):
    request = runtime / "data/stop-request"
    if running(runtime):
        request.touch()
        deadline = time.monotonic() + 40
        while running(runtime):
            if time.monotonic() > deadline:
                raise RuntimeError("后台未能在 40 秒内停止；请检查日志，不会强制终止任务。")
            time.sleep(0.2)
    request.unlink(missing_ok=True)


def shortcut(service, runtime, python):
    def quote(value):
        return "'" + str(value).replace("'", "''") + "'"
    arguments = subprocess.list2cmdline([str(runtime / "scripts/windows_service.py"), str(runtime)])
    script = ("$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';"
              "$s=(New-Object -ComObject WScript.Shell).CreateShortcut(" + quote(service) + ");"
              "$s.TargetPath=" + quote(python) + ";$s.Arguments=" + quote(arguments) + ";"
              "$s.WorkingDirectory=" + quote(runtime) + ";$s.WindowStyle=7;$s.Save()")
    encoded = base64.b64encode(script.encode("utf-16le")).decode("ascii")
    service.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded], check=True)


def start(runtime, service):
    python = Path(sys.executable).with_name("pythonw.exe")
    if not python.is_file():
        raise RuntimeError("Windows 后台需要 Python 安装目录中的 pythonw.exe。")
    if running(runtime):
        return
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        probe.bind(("127.0.0.1", 17841))
    shortcut(service, runtime, python)
    (runtime / "data/stop-request").unlink(missing_ok=True)
    subprocess.Popen([str(python), str(runtime / "scripts/windows_service.py"), str(runtime)],
                     cwd=runtime, creationflags=subprocess.CREATE_NO_WINDOW)
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        if running(runtime):
            try:
                token = (runtime / "data/access-token").read_text(encoding="utf-8").strip()
                request = urllib.request.Request("http://127.0.0.1:17841/api/devices",
                                                 headers={"Authorization": "Bearer " + token})
                with urllib.request.urlopen(request, timeout=1) as response:
                    if response.status == 200:
                        return
            except OSError:
                pass
        time.sleep(0.2)
    raise RuntimeError(f"后台启动失败，请查看 {runtime / 'logs/service-error.log'}")


def run(runtime):
    import msvcrt
    data = runtime / "data"
    logs = runtime / "logs"
    logs.mkdir(exist_ok=True)
    with (data / "service.lock").open("a+b") as lock:
        lock.seek(0)
        try:
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError:
            return
        request = data / "stop-request"
        request.unlink(missing_ok=True)
        env = {**os.environ, "ADB_STUDIO_ROOT": str(runtime), "ADB_STUDIO_DATA": str(data),
               "ADB_STUDIO_ADB": str(runtime / "data/bin/adb.exe"),
               "ADB_STUDIO_HOST": "127.0.0.1", "ADB_STUDIO_PORT": "17841",
               "ADB_STUDIO_STOP_FILE": str(request), "PYTHONUTF8": "1"}
        with (logs / "service.log").open("ab") as out, (logs / "service-error.log").open("ab") as error:
            result = subprocess.run([str(runtime / "adb-studio.exe")], cwd=runtime, env=env,
                                    stdout=out, stderr=error, creationflags=subprocess.CREATE_NO_WINDOW)
        request.unlink(missing_ok=True)
        sys.exit(result.returncode)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime", type=Path)
    runtime = parser.parse_args().runtime.resolve()
    try:
        run(runtime)
    except Exception:
        with (runtime / "logs/service-error.log").open("a", encoding="utf-8") as error:
            traceback.print_exc(file=error)
        sys.exit(1)
