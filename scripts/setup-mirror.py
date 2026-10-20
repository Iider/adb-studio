#!/usr/bin/env python3
"""下载并核对 scrcpy 3.1 投屏组件。"""

import hashlib
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
VERSION = "3.1"
SHA256 = "958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0"
URL = f"https://github.com/Genymobile/scrcpy/releases/download/v{VERSION}/scrcpy-server-v{VERSION}"


def main():
    target = ROOT / f"data/bin/scrcpy-server-v{VERSION}"
    if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() == SHA256:
        print("投屏组件已就绪。")
        return
    with urllib.request.urlopen(URL, timeout=45) as response:
        content = response.read(1024 * 1024)
    if hashlib.sha256(content).hexdigest() != SHA256:
        raise SystemExit("投屏组件校验失败，未保存。")
    target.parent.mkdir(parents=True, exist_ok=True)
    staged = target.with_suffix(".download")
    staged.write_bytes(content)
    staged.replace(target)
    print("投屏组件已下载并通过校验。")


if __name__ == "__main__":
    main()
