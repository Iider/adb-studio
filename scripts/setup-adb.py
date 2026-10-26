"""从 Google 官方下载 Windows ADB，保存在本项目 data/。"""

import io
from pathlib import Path
import subprocess
import sys
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
URL = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"


def main():
    if sys.platform != "win32":
        raise SystemExit("此脚本只用于 Windows；Mac／Linux 请安装对应平台的 ADB。")
    target = ROOT / "data/bin/platform-tools"
    if not all((target / name).is_file() for name in ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"]):
        with urllib.request.urlopen(URL, timeout=90) as response:
            archive = response.read(40 * 1024 * 1024)
        with zipfile.ZipFile(io.BytesIO(archive)) as package:
            files = {name: package.read("platform-tools/" + name) for name in
                     ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll", "NOTICE.txt", "source.properties"]}
        target.mkdir(parents=True, exist_ok=True)
        for name, content in files.items():
            (target / name).write_bytes(content)
    subprocess.run([str(target / "adb.exe"), "version"], check=True)
    print(f"Windows ADB 已就绪：{target}")


if __name__ == "__main__":
    main()
