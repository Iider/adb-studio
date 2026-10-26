#!/usr/bin/env python3
"""检查当前平台发行包完整性与异地启动，不操作真机。"""

import argparse
import hashlib
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile

from check_support import executable, python_program, environment, stop

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/check-integration.py"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    expected = args.archive.with_suffix(args.archive.suffix + ".sha256").read_text().split()[0]
    assert hashlib.sha256(args.archive.read_bytes()).hexdigest() == expected
    with tempfile.TemporaryDirectory(prefix="adb-release-check-") as directory:
        temp = Path(directory)
        archive_open = zipfile.ZipFile if args.archive.suffix == ".zip" else tarfile.open
        with archive_open(args.archive) as archive:
            names = archive.namelist() if isinstance(archive, zipfile.ZipFile) else archive.getnames()
            for name in names:
                path = Path(name)
                assert not path.is_absolute() and ".." not in path.parts
                assert len(path.parts) < 2 or path.parts[1] not in {"data", ".env", "target", "node_modules"}, name
            if isinstance(archive, zipfile.ZipFile):
                archive.extractall(temp)
            elif hasattr(tarfile, "data_filter"):
                archive.extractall(temp, filter="data")
            else:
                # Debian 12 的 Python 尚无解包过滤器；本程序只接受普通文件与目录。
                assert all(member.isfile() or member.isdir() for member in archive.getmembers())
                archive.extractall(temp)
        folder = next(p for p in temp.iterdir() if p.is_dir())
        for name in ["adb-studio", "web/build/index.html", "bin/scrcpy-server-v3.1", "scripts/install-service.py", "docs/deployment.md", "licenses/scrcpy-LICENSE"]:
            assert (executable(folder / name) if name == "adb-studio" else folder / name).is_file(), name
        fake = temp / "fake"
        fake.mkdir()
        (fake / "online").touch()
        (fake / "stay_on_while_plugged_in").write_text("3")
        (fake / "screen_off_timeout").write_text("600000")
        adb = temp / "adb"
        shutil.copy2(ROOT / "scripts/fake-adb.py", adb)
        adb = python_program(adb)
        env = {**os.environ, **environment(temp / "runtime"), "ADB_STUDIO_DATA": str(temp / "runtime"), "ADB_STUDIO_ADB": str(adb), "ADB_STUDIO_PORT": "17848", "ADB_STUDIO_HOST": "127.0.0.1", "FAKE_ADB_DIR": str(fake)}
        env.pop("ADB_STUDIO_ROOT", None)
        env.pop("ADB_STUDIO_ORIGINS", None)
        with (temp / "service.log").open("w") as log:
            process = subprocess.Popen([str(executable(folder / "adb-studio"))], env=env, cwd=temp, stdout=log, stderr=log)
            try:
                def page():
                    with urllib.request.urlopen("http://127.0.0.1:17848", timeout=2) as response:
                        return response.read().decode()
                html = helpers["wait"](page)
                assert "_app/" in html and html == (folder / "web/build/index.html").read_bytes().decode("utf-8")
                assert (temp / "runtime/access-token").is_file()
            finally:
                stop(process, temp / "runtime")
                process.wait(timeout=15)
    print("通过：发行包校验、无运行数据与密钥、脱离源码目录启动、网页与组件完整。")


if __name__ == "__main__":
    main()
