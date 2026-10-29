#!/usr/bin/env python3
"""Agent 使用的本机工作台命令工具。"""

import argparse
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def request(method, path, body=None, base="http://127.0.0.1:17841", timings=False, timeout=150):
    if urllib.parse.urlparse(base).hostname not in {"127.0.0.1", "localhost"}:
        raise ValueError("工作台只允许本机地址")
    data = Path(os.getenv("ADB_STUDIO_DATA", ROOT / "data"))
    pointer = ROOT / "data/service-root"
    if "ADB_STUDIO_DATA" not in os.environ and pointer.exists():
        data = Path(pointer.read_text(encoding="utf-8").strip()) / "data"
    token = (data / "access-token").read_text().strip()
    req = urllib.request.Request(
        base + "/api" + path,
        method=method,
        headers={
            "Authorization": "Bearer " + token,
            "Content-Type": "application/json",
        },
        data=None if body is None else json.dumps(body, ensure_ascii=False).encode(),
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            if timings and response.headers.get("Server-Timing"):
                print(response.headers["Server-Timing"], file=sys.stderr)
            return (
                response.read()
                if response.headers.get_content_type().startswith("image/")
                else json.load(response)
            )
    except urllib.error.HTTPError as exc:
        raise RuntimeError(json.load(exc).get("error", str(exc))) from None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:17841")
    parser.add_argument("--timeout", type=float, default=150, help="请求等待上限，秒")
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ["devices", "workflows", "runs", "schedules", "settings"]:
        sub.add_parser(name)
    call = sub.add_parser("request", help="调用文档中的接口")
    call.add_argument("method", choices=["GET", "POST", "PUT", "DELETE"])
    call.add_argument("path", help="例如 /workflows 或 /runs")
    call.add_argument("--file", type=Path, help="请求内容 JSON 文件")
    call.add_argument("--json", help="请求内容 JSON 文本")
    call.add_argument("--output", type=Path)
    call.add_argument("--timings", action="store_true", help="显示截图各环节耗时，单位毫秒")
    run = sub.add_parser("run")
    run.add_argument("workflow")
    run.add_argument("--device", required=True)
    run.add_argument("--inputs-file", help="本次运行变量 JSON 文件（变量编号到数字的映射）")
    for command in ["stop", "pause", "resume"]:
        control = sub.add_parser(command)
        control.add_argument("run_id")
    args = parser.parse_args()
    if not 0 < args.timeout <= 3600:
        parser.error("--timeout 需大于 0 且不超过 3600 秒")
    body = None
    if args.command == "request":
        if args.file and args.json:
            parser.error("--file 和 --json 只能使用一个")
        body = (
            json.loads(args.file.read_text(encoding="utf-8") if args.file else args.json)
            if args.file or args.json
            else None
        )
        method, path = args.method, args.path
    elif args.command == "run":
        method, path = "POST", "/runs"
        body = {
            "inputs": json.loads(Path(args.inputs_file).read_text()) if args.inputs_file else {},
            "device_id": args.device,
            "workflow_id": args.workflow,
        }
    elif args.command in {"stop", "pause", "resume"}:
        method, path = (
            "POST",
            "/runs/" + urllib.parse.quote(args.run_id, safe="") + "/" + args.command,
        )
    else:
        method, path = "GET", "/" + args.command
    data = request(method, path, body, args.url, getattr(args, "timings", False), args.timeout)
    if isinstance(data, bytes):
        if not getattr(args, "output", None):
            raise ValueError("截图请指定 --output 文件路径")
        args.output.write_bytes(data)
        print(str(args.output))
        return
    result = json.dumps(data, ensure_ascii=False, indent=2)
    if getattr(args, "output", None):
        args.output.write_text(result + "\n")
    else:
        print(result)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, OSError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
