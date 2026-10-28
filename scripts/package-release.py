#!/usr/bin/env python3
"""构建当前平台的发行包，不包含运行数据和密钥。"""

import argparse
import hashlib
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true", help="使用已完成的本机 release 构建")
    args = parser.parse_args()
    system = platform.system()
    if system not in {"Darwin", "Linux", "Windows"}:
        raise SystemExit("目前支持 macOS、Linux 和 Windows。")
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64", "AMD64": "x86_64"}.get(platform.machine())
    if not arch:
        raise SystemExit("发行包目前支持 x86_64 和 aarch64。")
    if system == "Windows" and arch != "x86_64":
        raise SystemExit("Windows 发行包目前只支持 x86_64。")
    if not args.skip_build:
        npm = shutil.which("npm.cmd" if system == "Windows" else "npm")
        subprocess.run([npm, "ci"], cwd=ROOT / "web", check=True)
        subprocess.run([npm, "run", "check"], cwd=ROOT / "web", check=True)
        subprocess.run([npm, "run", "build"], cwd=ROOT / "web", check=True)
        subprocess.run(["cargo", "build", "--locked", "--release"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, "scripts/setup-mirror.py"], cwd=ROOT, check=True)
    if system == "Windows":
        subprocess.run([sys.executable, "scripts/setup-adb.py"], cwd=ROOT, check=True)
    label = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[system]
    name = f"adb-studio-{label}-{arch}"
    binary = "adb-studio.exe" if system == "Windows" else "adb-studio"
    output = ROOT / "dist"
    output.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="adb-studio-release-") as tmp:
        folder = Path(tmp) / name
        (folder / "bin").mkdir(parents=True)
        shutil.copy2(ROOT / "target/release" / binary, folder / binary)
        shutil.copytree(ROOT / "web/build", folder / "web/build")
        (folder / "scripts").mkdir()
        for script in ["install-service.py", "studio.py", "setup-mirror.py"]:
            shutil.copy2(ROOT / "scripts" / script, folder / "scripts" / script)
        if system == "Windows":
            shutil.copy2(ROOT / "scripts/windows_service.py", folder / "scripts/windows_service.py")
            adb_folder = ROOT / "data/bin/platform-tools"
            for file in ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"]:
                shutil.copy2(adb_folder / file, folder / "bin" / file)
        shutil.copy2(ROOT / "data/bin/scrcpy-server-v3.1", folder / "bin/scrcpy-server-v3.1")
        if system == "Darwin":
            shutil.copy2(ROOT / "scripts/ocr.swift", folder / "scripts/ocr.swift")
            subprocess.run(["swiftc", "-O", str(ROOT / "scripts/ocr.swift"), "-o", str(folder / "bin/ocr")], check=True)
            adb = shutil.which("adb") or str(ROOT / "../platform-tools/adb")
            subprocess.run([adb, "version"], check=True, stdout=subprocess.DEVNULL)
            shutil.copy2(adb, folder / "bin/adb")
        shutil.copy2(ROOT / "THIRD_PARTY_NOTICES.md", folder / "THIRD_PARTY_NOTICES.md")
        launch = r".\adb-studio.exe" if system == "Windows" else "./adb-studio"
        (folder / "README.md").write_text(f"# ADB 工作台\n\n进入解压目录运行 `{launch}`，打开 http://127.0.0.1:17841 。\n\n安装、自启、环境变量与迁移见 [部署说明](docs/deployment.md)。\n\n[模型看图检查](docs/vision-recognition.md) · [投屏与操作](docs/realtime-screen.md) · [工作流接口](docs/workflows.md)\n", encoding="utf-8")
        (folder / "docs").mkdir()
        for file in ["deployment.md", "feiniu-native.md", "vision-recognition.md", "realtime-screen.md", "workflows.md", "screenshot-performance.md", "ui-components.md"]:
            shutil.copy2(ROOT / "docs" / file, folder / "docs" / file)
        shutil.copytree(ROOT / "licenses", folder / "licenses")
        if system == "Windows":
            shutil.copy2(adb_folder / "NOTICE.txt", folder / "licenses/adb-NOTICE")
        if system == "Darwin":
            adb_folder = Path(adb).resolve().parent
            if (adb_folder / "lib64").is_dir():
                shutil.copytree(adb_folder / "lib64", folder / "bin/lib64")
            for notice in [adb_folder / "NOTICE.txt", adb_folder / "NOTICE"]:
                if notice.is_file():
                    shutil.copy2(notice, folder / "licenses/adb-NOTICE")
                    break
        if system == "Windows":
            archive = Path(shutil.make_archive(str(output / name), "zip", root_dir=tmp, base_dir=name))
        else:
            archive = output / f"{name}.tar.gz"
            with tarfile.open(archive, "w:gz") as tar:
                tar.add(folder, arcname=name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(archive.suffix + ".sha256").write_text(f"{digest}  {archive.name}\n")
    print(archive)


if __name__ == "__main__":
    main()
