#!/usr/bin/env python3
"""安装、查看或移除 Mac／Linux／Windows 当前用户的后台服务。"""

import argparse
from contextlib import closing
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
    if system == "Windows":
        runtime = Path(os.environ["LOCALAPPDATA"]) / "ADB Studio"
        return (runtime, runtime / "logs", Path(os.environ["APPDATA"]) /
                "Microsoft/Windows/Start Menu/Programs/Startup/ADB Studio.lnk")
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
    if system == "Windows":
        from windows_service import private_directory
        private_directory(runtime)
    binary = runtime / ("adb-studio.exe" if system == "Windows" else "adb-studio")
    if source.resolve() != binary.resolve():
        staged = binary.with_suffix(".new")
        shutil.copy2(source, staged)
        staged.replace(binary)
    if (ROOT / "web/build").resolve() != (runtime / "web/build").resolve():
        shutil.copytree(ROOT / "web/build", runtime / "web/build", dirs_exist_ok=True)
    (runtime / "scripts").mkdir(exist_ok=True)
    if system == "Windows":
        for name in ["windows_service.py", "install-service.py", "studio.py"]:
            if (ROOT / "scripts" / name).resolve() != (runtime / "scripts" / name).resolve():
                shutil.copy2(ROOT / "scripts" / name, runtime / "scripts" / name)
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
    if system in {"Darwin", "Windows"}:
        dest = runtime / "data/bin" / ("adb.exe" if system == "Windows" else "adb")
        if system == "Windows":
            for name in ["AdbWinApi.dll", "AdbWinUsbApi.dll"]:
                dll = adb.parent / name
                if not dll.is_file():
                    raise RuntimeError(f"ADB 缺少 {name}，请使用完整的 Windows platform-tools。")
                if dll.resolve() != (dest.parent / name).resolve():
                    shutil.copy2(dll, dest.parent / name)
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
        with closing(sqlite3.connect(ROOT / "data/data.sqlite")) as source_db, closing(sqlite3.connect(database)) as dest_db:
            source_db.backup(dest_db)
    if not (runtime / ".env").exists():
        names = {"MODEL_KEY"}
        if database.exists():
            with closing(sqlite3.connect(database)) as db:
                row = db.execute("SELECT body FROM records WHERE kind='settings' AND id='model'").fetchone()
                if row:
                    names.add(json.loads(row[0])["key_env"])
        for env_file in [ROOT / ".env", ROOT / "../.env"]:
            if env_file.exists():
                values = [line for line in env_file.read_text(encoding="utf-8").splitlines()
                          if line.partition("=")[0].strip() in names]
                if values:
                    (runtime / ".env").write_text("\n".join(values) + "\n", encoding="utf-8")
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
    if system not in {"Darwin", "Linux", "Windows"}:
        raise SystemExit("目前支持 macOS、Linux 和 Windows。")
    runtime, logs, service = locations(system)
    runtime = (args.runtime or runtime).resolve()
    if system == "Windows":
        install_windows(args, runtime, service)
        return
    domain = f"gui/{os.getuid()}" if system == "Darwin" else None
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
    (ROOT / "data/service-root").write_text(str(runtime), encoding="utf-8")
    print(f"后台已安装：{runtime}。http://127.0.0.1:17841")
    if system == "Linux":
        print("无人登录时持续运行：由管理员为当前用户开启 loginctl enable-linger，或使用 Docker。")


def install_windows(args, runtime, service):
    from windows_service import running, start, stop
    if args.action == "status":
        print(f"后台：{'运行中' if running(runtime) else '已停止'}；登录自启：{'开启' if service.is_file() else '关闭'}。")
        print(f"安装目录：{runtime}")
        return
    if args.action == "remove":
        stop(runtime)
        service.unlink(missing_ok=True)
        print("后台已停止，登录自启已关闭，运行数据保留。")
        return
    source = args.binary or next((p for p in [ROOT / "adb-studio.exe", ROOT / "target/release/adb-studio.exe", ROOT / "target/debug/adb-studio.exe"] if p.is_file()), None)
    adb = args.adb or next((p for p in [ROOT / "bin/adb.exe", runtime / "data/bin/adb.exe", ROOT / "data/bin/platform-tools/adb.exe"] if p.is_file()), None)
    if adb is None and shutil.which("adb"):
        adb = Path(shutil.which("adb"))
    if source is None or not source.is_file() or not (ROOT / "web/build/index.html").is_file():
        raise SystemExit("先构建后台和网页，或使用完整发行包。")
    if adb is None or not adb.is_file():
        raise SystemExit("请先安装 Windows platform-tools，或使用 --adb 指定 adb.exe。")
    # 在停止已有服务之前核对安装输入。
    for name in ["AdbWinApi.dll", "AdbWinUsbApi.dll"]:
        if not (adb.parent / name).is_file():
            raise SystemExit(f"ADB 缺少 {name}，请使用完整的 Windows platform-tools。")
    subprocess.run([str(adb), "version"], check=True, stdout=subprocess.DEVNULL)
    stop(runtime)
    copy_runtime(source.resolve(), runtime, adb.resolve(), "Windows")
    start(runtime, service)
    (ROOT / "data").mkdir(exist_ok=True)
    (ROOT / "data/service-root").write_text(str(runtime), encoding="utf-8")
    print(f"后台已安装并开启登录自启：{runtime}。http://127.0.0.1:17841")


if __name__ == "__main__":
    main()
