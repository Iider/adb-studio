#!/usr/bin/env python3
"""将 Tesseract TSV 转成工作台需要的文字和位置。输入为 stdin PNG。"""

import csv
import io
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    runtime = Path(os.environ.get("ADB_STUDIO_OCR_RUNTIME", Path(__file__).resolve().parents[1] / "ocr-runtime"))
    binary = runtime / "usr/bin/tesseract"
    env = os.environ.copy()
    libraries = runtime / "usr/lib/x86_64-linux-gnu"
    env["LD_LIBRARY_PATH"] = str(libraries) + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
    env["TESSDATA_PREFIX"] = str(runtime / "usr/share/tesseract-ocr/5/tessdata")
    result = subprocess.run(
        [str(binary), "stdin", "stdout", "-l", "chi_sim+eng", "--psm", "11", "tsv"],
        input=sys.stdin.buffer.read(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        timeout=10,
        check=True,
    )

    words = []
    lines = {}
    for row in csv.DictReader(io.StringIO(result.stdout.decode()), delimiter="\t"):
        if row["level"] != "5" or not row["text"].strip():
            continue
        confidence = float(row["conf"]) / 100
        if confidence < 0.5:
            continue
        x, y, width, height = (int(row[k]) for k in ("left", "top", "width", "height"))
        word = {"text": row["text"].strip(), "x": x + width // 2, "y": y + height // 2, "confidence": confidence}
        words.append(word)
        key = tuple(row[k] for k in ("page_num", "block_num", "par_num", "line_num"))
        lines.setdefault(key, []).append((x, y, width, height, word))

    for parts in lines.values():
        if len(parts) < 2:
            continue
        parts.sort(key=lambda part: part[0])
        joined = "".join(part[4]["text"] for part in parts)
        if joined in {part[4]["text"] for part in parts}:
            continue
        left = min(part[0] for part in parts)
        top = min(part[1] for part in parts)
        right = max(part[0] + part[2] for part in parts)
        bottom = max(part[1] + part[3] for part in parts)
        words.append({"text": joined, "x": (left + right) // 2, "y": (top + bottom) // 2,
                      "confidence": min(part[4]["confidence"] for part in parts)})
    json.dump(words, sys.stdout, ensure_ascii=False)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"本地文字识别失败：{error}", file=sys.stderr)
        sys.exit(1)
