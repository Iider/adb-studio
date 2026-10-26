"""隔离检查的跨平台程序路径与停止操作。"""

import os
from pathlib import Path
import sys


def executable(path):
    return Path(str(path) + ".exe") if os.name == "nt" else path


def python_program(path):
    if os.name == "nt":
        target = path.with_suffix(".py")
        path.rename(target)
        return target
    path.chmod(0o755)
    return path


def environment(data):
    return {"ADB_STUDIO_STOP_FILE": str(data / "stop-request"),
            "ADB_STUDIO_PYTHON": sys.executable, "PYTHONUTF8": "1"}


def stop(process, data):
    if os.name == "nt" and process.poll() is None:
        (data / "stop-request").touch()
        try:
            process.wait(timeout=40)
        finally:
            (data / "stop-request").unlink(missing_ok=True)
    else:
        process.terminate()
