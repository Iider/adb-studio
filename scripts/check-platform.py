#!/usr/bin/env python3
"""检查跨平台安装路径、配置引用与升级保留，不启停真实服务。"""

import importlib.util
from contextlib import closing
import os
from pathlib import Path
import sqlite3
import shutil
import subprocess
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("installer", ROOT / "scripts/install-service.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


def main():
    with tempfile.TemporaryDirectory(prefix="adb-platform-check-") as tmp:
        folder = Path(tmp)
        with patch.object(Path, "home", return_value=folder), patch.dict(os.environ, {"XDG_DATA_HOME": str(folder / "xdg"), "XDG_STATE_HOME": str(folder / "state")}):
            assert "Library" in str(installer.locations("Darwin")[0])
            assert installer.locations("Linux")[0] == folder / "xdg/adb-studio"
        unit = installer.linux_unit(folder / '目录 % " test/adb-studio', folder / "runtime", folder / "adb")
        assert '%%' in unit and '\\"' in unit and "launchctl" not in unit and "UMask=0077" in unit
        assert f"WorkingDirectory={installer.unit_quote(folder / 'runtime', quoted=False)}\n" in unit
        if shutil.which("systemd-analyze"):
            service = folder / "path-check.service"
            service.write_text(installer.linux_unit(Path("/bin/true"), folder / '目录 % " test', Path("/bin/true")))
            subprocess.run(["systemd-analyze", "--user", "verify", str(service)], check=True)
        try:
            installer.unit_quote("bad\npath")
            raise AssertionError("换行路径应拒绝")
        except ValueError:
            pass
        source = folder / "source"
        (source / "web/build").mkdir(parents=True)
        (source / "web/build/index.html").write_text("test")
        (source / "scripts").mkdir()
        (source / "scripts/ocr.swift").write_text("mac-only")
        (source / "data/bin").mkdir(parents=True)
        (source / "data/bin/ocr").write_text("mac-binary")
        (source / "data/bin/scrcpy-server-v3.1").write_text("server")
        (source / "data/access-token").write_text("source-token")
        (source / "app").write_text("binary")
        (source / "adb").write_text("adb")
        with patch.object(installer, "ROOT", source):
            runtime = folder / "linux"
            binary, adb = installer.copy_runtime(source / "app", runtime, source / "adb", "Linux")
            assert binary.read_text() == "binary" and adb == source / "adb"
            assert not (runtime / "data/bin/ocr").exists() and not (runtime / "scripts/ocr.swift").exists()
            assert (runtime / "data/bin/scrcpy-server-v3.1").is_file()
            if os.name != "nt":
                assert (runtime / "data/access-token").stat().st_mode & 0o777 == 0o600
            (runtime / "data/access-token").write_text("keep-token")
            (runtime / ".env").write_text("MODEL_KEY=keep-key\n")
            with closing(sqlite3.connect(runtime / "data/data.sqlite")) as db, db:
                db.execute("CREATE TABLE marker (value TEXT)")
                db.execute("INSERT INTO marker VALUES ('keep-db')")
            installer.copy_runtime(source / "app", runtime, source / "adb", "Linux")
            assert (runtime / "data/access-token").read_text() == "keep-token"
            assert "keep-key" in (runtime / ".env").read_text()
            with closing(sqlite3.connect(runtime / "data/data.sqlite")) as db:
                assert db.execute("SELECT value FROM marker").fetchone()[0] == "keep-db"
            mac = folder / "mac"
            installer.copy_runtime(source / "app", mac, source / "adb", "Darwin")
            assert (mac / "data/bin/ocr").is_file() and (mac / "scripts/ocr.swift").is_file()
            assert (mac / "data/bin/adb").is_file()
            import windows_service
            for name in ["windows_service.py", "install-service.py", "studio.py"]:
                (source / "scripts" / name).write_text("test")
            for name in ["AdbWinApi.dll", "AdbWinUsbApi.dll"]:
                (source / name).write_text(name)
            windows = folder / "Windows 目录 with space"
            with patch.object(windows_service, "private_directory"):
                binary, adb = installer.copy_runtime(source / "app", windows, source / "adb", "Windows")
                assert binary.name == "adb-studio.exe" and adb.name == "adb.exe"
                assert all((adb.parent / name).is_file() for name in ["AdbWinApi.dll", "AdbWinUsbApi.dll"])
                assert not (windows / "data/bin/ocr").exists()
                (windows / ".env").write_text("MODEL_KEY=keep-windows-key\n")
                (windows / "data/access-token").write_text("keep-windows-token")
                installer.copy_runtime(source / "app", windows, source / "adb", "Windows")
                assert (windows / "data/access-token").read_text() == "keep-windows-token"
                assert "keep-windows-key" in (windows / ".env").read_text()
            with patch.dict(os.environ, {"LOCALAPPDATA": str(folder / "local"), "APPDATA": str(folder / "roaming")}):
                assert installer.locations("Windows")[0] == folder / "local/ADB Studio"
                assert installer.locations("Windows")[2].name == "ADB Studio.lnk"
    print("通过：Mac／Linux／Windows 路径、服务引用、平台组件分离、升级保留数据库与密钥。")


if __name__ == "__main__":
    main()
