#!/usr/bin/env python3
"""安装、查看或移除 Mac／Linux 当前用户的后台服务。"""

import argparse
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import sqlite3
import subprocess

ROOT = Path(__file__).resolve().parents[1]
LABEL = "local.adb-studio"


def locations(system):
    if system == "Darwin":
        return (Path.home() / "Library/Application Support/ADB Studio",
                Path.home() / "Library/Logs/ADB Studio",
                Path.home() / "Library/LaunchAgents" / (LABEL + ".plist"))
    return (Path(os.getenv("XDG_DATA_HOME", Path.home() / ".local/share")) / "adb-studio",
            Path(os.getenv("XDG_STATE_HOME", Path.home() / ".local/state")) / "adb-studio",
            Path(os.getenv("XDG_CONFIG_HOME", Path.home() / ".config")) / "systemd/user/adb-studio.service")


def unit_quote(value, quoted=True):
    if "\n" in str(value) or "\r" in str(value):
        raise ValueError("服务路径不能包含换行")
    escaped = str(value).replace("\\", "\\\\").replace('"', '\\"').replace("%", "%%")
    return '"' + escaped + '"' if quoted else escaped


def linux_unit(binary, runtime, adb):
    return "\n".join([
        "[Unit]", "Description=ADB Studio", "After=network.target", "", "[Service]",
        f"ExecStart={unit_quote(binary)}", f"WorkingDirectory={unit_quote(runtime, quoted=False)}",
        f"Environment={unit_quote('ADB_STUDIO_ROOT=' + str(runtime))}",
        f"Environment={unit_quote('ADB_STUDIO_ADB=' + str(adb))}",
        'Environment="ADB_STUDIO_HOST=0.0.0.0"', "UMask=0077", "Restart=on-failure",
        "RestartSec=5", "TimeoutStopSec=30", "", "[Install]", "WantedBy=default.target", "",
    ])


def copy_runtime(source, runtime, adb, system):
    runtime.mkdir(parents=True, exist_ok=True, mode=0o700)
    binary = runtime / "adb-studio"
    if source.resolve() != binary.resolve():
        staged = binary.with_suffix(".new")
        shutil.copy2(source, staged)
        staged.replace(binary)
    shutil.copytree(ROOT / "web/build", runtime / "web/build", dirs_exist_ok=True)
    (runtime / "scripts").mkdir(exist_ok=True)
    if system == "Darwin":
        shutil.copy2(ROOT / "scripts/ocr.swift", runtime / "scripts/ocr.swift")
    (runtime / "data/bin").mkdir(parents=True, exist_ok=True)
    for name in ["scrcpy-server-v3.1", "ocr"]:
        if name == "ocr" and system != "Darwin":
            continue
        for folder in [ROOT / "bin", ROOT / "data/bin"]:
            file = folder / name
            if file.exists():
                dest = runtime / "data/bin" / name
                if file.resolve() != dest.resolve():
                    shutil.copy2(file, dest)
                break
    if system == "Darwin":
        dest = runtime / "data/bin/adb"
        if adb.resolve() != dest.resolve():
            shutil.copy2(adb, dest)
        if (adb.parent / "lib64").is_dir() and adb.parent.resolve() != dest.parent.resolve():
            shutil.copytree(adb.parent / "lib64", dest.parent / "lib64", dirs_exist_ok=True)
        adb = dest
    for name in ["access-token"]:
        if not (runtime / "data" / name).exists() and (ROOT / "data" / name).exists():
            shutil.copy2(ROOT / "data" / name, runtime / "data" / name)
            (runtime / "data" / name).chmod(0o600)
    database = runtime / "data/data.sqlite"
    if not database.exists() and (ROOT / "data/data.sqlite").exists():
        with sqlite3.connect(ROOT / "data/data.sqlite") as source_db, sqlite3.connect(database) as dest_db:
            source_db.backup(dest_db)
    if not (runtime / ".env").exists():
        names = {"MODEL_KEY"}
        if database.exists():
            with sqlite3.connect(database) as db:
                row = db.execute("SELECT body FROM records WHERE kind='settings' AND id='model'").fetchone()
                if row:
                    names.add(json.loads(row[0])["key_env"])
        for env_file in [ROOT / ".env", ROOT / "../.env"]:
            if env_file.exists():
                values = [line for line in env_file.read_text().splitlines()
                          if line.partition("=")[0].strip() in names]
                if values:
                    (runtime / ".env").write_text("\n".join(values) + "\n")
                    (runtime / ".env").chmod(0o600)
                    break
    return binary, adb


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["install", "remove", "status"])
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--adb", type=Path)
    args = parser.parse_args()
    system = platform.system()
    if system not in {"Darwin", "Linux"}:
        raise SystemExit("目前支持 macOS 和 Linux。")
    runtime, logs, service = locations(system)
    runtime = (args.runtime or runtime).resolve()
    domain = f"gui/{os.getuid()}"
    if args.action == "status":
        subprocess.run(["launchctl", "print", domain + "/" + LABEL] if system == "Darwin"
                       else ["systemctl", "--user", "status", "adb-studio.service"], check=True)
        return
    if args.action == "remove":
        subprocess.run(["launchctl", "bootout", domain + "/" + LABEL] if system == "Darwin"
                       else ["systemctl", "--user", "disable", "--now", "adb-studio.service"], check=False)
        service.unlink(missing_ok=True)
        if system == "Linux":
            subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
        print("后台服务已关闭，运行数据保留。")
        return
    source = args.binary or next((p for p in [ROOT / "adb-studio", ROOT / "target/release/adb-studio", ROOT / "target/debug/adb-studio"] if p.is_file()), None)
    adb = args.adb or next((p for p in [ROOT / "bin/adb", runtime / "data/bin/adb"] if p.is_file()), None)
    if adb is None:
        adb = Path(shutil.which("adb")) if shutil.which("adb") else (ROOT / "../platform-tools/adb" if system == "Darwin" else None)
    if source is None or not source.is_file() or not (ROOT / "web/build/index.html").is_file():
        raise SystemExit("先构建后台和网页，或使用完整发行包。")
    if adb is None or not adb.is_file():
        raise SystemExit("请先安装当前平台的 ADB，或使用 --adb 指定路径。")
    if system == "Linux":
        subprocess.run(["systemctl", "--user", "show-environment"], check=True, stdout=subprocess.DEVNULL)
    previous_env = {}
    if system == "Darwin" and service.exists():
        previous_env = plistlib.loads(service.read_bytes()).get("EnvironmentVariables", {})
    subprocess.run(["launchctl", "bootout", domain + "/" + LABEL] if system == "Darwin"
                   else ["systemctl", "--user", "stop", "adb-studio.service"], capture_output=True)
    binary, adb = copy_runtime(source, runtime, adb, system)
    service.parent.mkdir(parents=True, exist_ok=True)
    if system == "Darwin":
        logs.mkdir(parents=True, exist_ok=True)
        config = {
            "Label": LABEL, "ProgramArguments": [str(binary)], "WorkingDirectory": str(runtime),
            "RunAtLoad": True, "KeepAlive": True, "ThrottleInterval": 10,
            "EnvironmentVariables": {"ADB_STUDIO_HOST": "0.0.0.0", **previous_env, "ADB_STUDIO_ROOT": str(runtime),
                                     "ADB_STUDIO_ADB": str(adb), "PATH": os.environ.get("PATH", "/usr/bin:/bin")},
            "StandardOutPath": str(logs / "service.log"), "StandardErrorPath": str(logs / "service-error.log"),
        }
        service.write_bytes(plistlib.dumps(config))
        service.chmod(0o600)
        subprocess.run(["launchctl", "bootstrap", domain, str(service)], check=True)
    else:
        service.write_text(linux_unit(binary, runtime, adb))
        subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
        subprocess.run(["systemctl", "--user", "enable", "--now", "adb-studio.service"], check=True)
    (ROOT / "data").mkdir(exist_ok=True)
    (ROOT / "data/service-root").write_text(str(runtime))
    print(f"后台已安装：{runtime}。http://127.0.0.1:17841")
    if system == "Linux":
        print("无人登录时持续运行：由管理员为当前用户开启 loginctl enable-linger，或使用 Docker。")


if __name__ == "__main__":
    main()
