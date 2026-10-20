#!/usr/bin/env python3
"""安装、查看或移除当前用户的登录自启服务。"""

import argparse
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sqlite3

ROOT = Path(__file__).resolve().parents[1]
LABEL = "local.adb-studio"
PLIST = Path.home() / "Library/LaunchAgents" / (LABEL + ".plist")
DOMAIN = "gui/" + str(os.getuid())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["install", "remove", "status"])
    args = parser.parse_args()
    if args.action == "status":
        subprocess.run(["launchctl", "print", DOMAIN + "/" + LABEL], check=True)
        return
    if args.action == "remove":
        subprocess.run(
            ["launchctl", "bootout", DOMAIN + "/" + LABEL], capture_output=True
        )
        PLIST.unlink(missing_ok=True)
        print("已关闭登录自启。")
        return
    source = ROOT / "target/debug/adb-studio"
    if not source.exists() or not (ROOT / "web/build/index.html").exists():
        raise SystemExit("先运行 cargo build 和网页 npm run build。")
    runtime = Path.home() / "Library/Application Support/ADB Studio"
    runtime.mkdir(parents=True, exist_ok=True, mode=0o700)
    binary = runtime / "data/bin/adb-studio"
    binary.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["launchctl", "bootout", DOMAIN + "/" + LABEL], capture_output=True)
    # 替换文件而非覆盖旧文件，避免 macOS 沿用旧可执行文件的签名缓存。
    staged_binary = binary.with_suffix(".new")
    shutil.copy2(source, staged_binary)
    staged_binary.replace(binary)
    shutil.copytree(ROOT / "web/build", runtime / "web/build", dirs_exist_ok=True)
    (runtime / "scripts").mkdir(exist_ok=True)
    shutil.copy2(ROOT / "scripts/ocr.swift", runtime / "scripts/ocr.swift")
    shutil.copy2(ROOT / "../platform-tools/adb", runtime / "data/bin/adb")
    if (ROOT / "data/bin/ocr").exists():
        shutil.copy2(ROOT / "data/bin/ocr", runtime / "data/bin/ocr")
    if (ROOT / "data/bin/scrcpy-server-v3.1").exists():
        shutil.copy2(ROOT / "data/bin/scrcpy-server-v3.1", runtime / "data/bin/scrcpy-server-v3.1")
    for name in ["access-token"]:
        if not (runtime / "data" / name).exists() and (ROOT / "data" / name).exists():
            shutil.copy2(ROOT / "data" / name, runtime / "data" / name)
            (runtime / "data" / name).chmod(0o600)
    if (
        not (runtime / "data/data.sqlite").exists()
        and (ROOT / "data/data.sqlite").exists()
    ):
        with sqlite3.connect(ROOT / "data/data.sqlite") as source_db, sqlite3.connect(
            runtime / "data/data.sqlite"
        ) as dest_db:
            source_db.backup(dest_db)
    if not (runtime / ".env").exists():
        for env_file in [ROOT / ".env", ROOT / "../.env"]:
            if env_file.exists():
                values = [
                    line
                    for line in env_file.read_text().splitlines()
                    if line.partition("=")[0].strip() == "MODEL_KEY"
                ]
                if values:
                    (runtime / ".env").write_text("\n".join(values) + "\n")
                    (runtime / ".env").chmod(0o600)
                    break
    (ROOT / "data").mkdir(exist_ok=True)
    (ROOT / "data/service-root").write_text(str(runtime))
    config = {
        "Label": LABEL,
        "ProgramArguments": [str(binary)],
        "WorkingDirectory": str(Path.home()),
        "RunAtLoad": True,
        "KeepAlive": True,
        "ThrottleInterval": 10,
        "EnvironmentVariables": {
            "ADB_STUDIO_ROOT": str(runtime),
            "ADB_STUDIO_HOST": "0.0.0.0",
            "ADB_STUDIO_ADB": str(runtime / "data/bin/adb"),
            "PATH": "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        },
        "StandardOutPath": str(Path.home() / "Library/Logs/ADB Studio/service.log"),
        "StandardErrorPath": str(
            Path.home() / "Library/Logs/ADB Studio/service-error.log"
        ),
    }
    (Path.home() / "Library/Logs/ADB Studio").mkdir(parents=True, exist_ok=True)
    PLIST.parent.mkdir(parents=True, exist_ok=True)
    PLIST.write_bytes(plistlib.dumps(config))
    PLIST.chmod(0o600)
    subprocess.run(["launchctl", "bootstrap", DOMAIN, str(PLIST)], check=True)
    print("自启服务已安装，登录 Mac 后会自动运行。http://127.0.0.1:17841")


if __name__ == "__main__":
    main()
