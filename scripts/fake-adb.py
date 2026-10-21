#!/usr/bin/env python3
"""集成检查使用的模拟设备；不会调用真实 ADB。"""

import json
import os
from pathlib import Path
import sys
import time
import socket
import select

root = Path(os.environ["FAKE_ADB_DIR"])
args = sys.argv[1:]
with (root / "commands").open("a") as log:
    log.write(json.dumps(args) + "\n")
online = (root / "online").exists()
if args == ["devices", "-l"]:
    print("List of devices attached")
    if online:
        if (root / "legacy-transport").exists():
            print("mock-usb device usb:1-1 model:Test\n127.0.0.1:5555 device model:Test")
        elif (root / "wireless-only").exists():
            print("127.0.0.1:5555 device model:Test transport_id:2")
        else:
            print("mock-usb device usb:1-1 model:Test transport_id:1\n127.0.0.1:5555 device model:Test transport_id:2")
elif args == ["mdns", "services"]:
    print("List of discovered mdns services")
elif args[:1] in (["-s"], ["-t"]):
    if not online or (args[:2] == ["-t", "1"] and (root / "transport-gone").exists()):
        print("device offline", file=sys.stderr)
        sys.exit(1)
    command = args[2:]
    if command[:1] == ["reverse"]:
        if command[1] == "--remove":
            (root / command[2].replace(":", "-")).unlink(missing_ok=True)
        else:
            (root / command[1].replace(":", "-")).write_text(command[2])
        sys.exit(0)
    if command[:1] == ["shell"] and len(command) > 1 and command[1].startswith("CLASSPATH="):
        scid = next(value.split("=", 1)[1] for value in command if value.startswith("scid="))
        port = int((root / ("localabstract-scrcpy_" + scid)).read_text().split(":")[1])
        if (root / "slow-mirror").exists():
            time.sleep(2)
        with socket.create_connection(("127.0.0.1", port), timeout=10) as connection, socket.create_connection(("127.0.0.1", port), timeout=10) as control:
            connection.sendall(b"studio-video-test")
            buffered = b""
            while (root / "online").exists() and not (root / "end-mirror").exists():
                ready, _, _ = select.select([connection, control], [], [], 0.2)
                if connection in ready and not connection.recv(1):
                    break
                if control in ready:
                    part = control.recv(4096)
                    if not part:
                        break
                    buffered += part
                    while buffered:
                        size = 32 if buffered[0] == 2 else 14
                        if len(buffered) < size:
                            break
                        packet, buffered = buffered[:size], buffered[size:]
                        with (root / "control-attempts").open("a") as log:
                            log.write(packet.hex() + "\n")
                        if (root / "reject-control").exists():
                            print("java.lang.SecurityException: INJECT_EVENTS permission denied", file=sys.stderr, flush=True)
                        else:
                            with (root / "controls").open("a") as log:
                                log.write(packet.hex() + "\n")
        sys.exit(0)
    if command[:2] == ["shell", "input"] and command[2] in {"tap", "swipe"}:
        with (root / "input-attempts").open("a") as log:
            log.write(json.dumps(command) + "\n")
        if (root / "slow-input").exists():
            (root / "input-started").touch()
            time.sleep(1.5)
        if (root / "timeout-input").exists():
            time.sleep(14)
        if (root / "reject-input").exists():
            print("java.lang.SecurityException: Injecting to another application requires INJECT_EVENTS permission\n\tat android.hardware.input.InputManager.injectInputEvent(InputManager.java:886)", file=sys.stderr)
            sys.exit(1)
    if command[:2] == ["shell", "getprop"]:
        identity = (root / "identity").read_text() if (root / "identity").exists() else "mock-phone"
        print(identity if command[-1] == "ro.serialno" else "模拟手机")
    elif command == ["shell", "dumpsys", "display"]:
        print((root / "displays").read_text() if (root / "displays").exists() else "")
        if (root / "disconnect-after-display").exists():
            (root / "transport-gone").touch()
    elif command[:3] == ["shell", "input", "keyevent"]:
        if command[-1] == "KEYCODE_SLEEP":
            (root / "asleep").touch()
            (root / "locked").touch()
        elif command[-1] == "KEYCODE_WAKEUP":
            (root / "asleep").unlink(missing_ok=True)
    elif command == ["shell", "wm", "dismiss-keyguard"]:
        if not (root / "password").exists():
            (root / "locked").unlink(missing_ok=True)
    elif command == ["shell", "dumpsys", "power"]:
        print("mWakefulness=Asleep" if (root / "asleep").exists() else "mWakefulness=Awake")
    elif command == ["shell", "wm", "size"]:
        print("Physical size: 400x800")
    elif command[:3] == ["shell", "settings", "get"]:
        print((root / command[-1]).read_text())
    elif command[:3] == ["shell", "settings", "put"]:
        (root / command[-2]).write_text(command[-1])
    elif command[:3] == ["shell", "input", "tap"]:
        with (root / "taps").open("a") as f:
            f.write(json.dumps(command) + "\n")
    elif command[:3] == ["shell", "input", "swipe"]:
        with (root / "swipes").open("a") as f:
            f.write(json.dumps(command) + "\n")
    elif command[:2] == ["exec-out", "screencap"]:
        if (root / "expected-display").exists():
            expected = (root / "expected-display").read_text()
            if command[-2:] != ["-d", expected]:
                print("wrong display", file=sys.stderr)
                sys.exit(1)
        with (root / "captures").open("a") as f:
            f.write("capture\n")
        if "-p" in command:
            sys.stdout.buffer.write((root / "screen.png").read_bytes())
        else:
            sys.stdout.buffer.write((root / "screen.raw").read_bytes() if (root / "screen.raw").exists() else b"unsupported raw format")
    elif command[:3] == ["shell", "dumpsys", "activity"]:
        print("mResumedActivity: " + ((root / "foreground").read_text() if (root / "foreground").exists() else "com.luna.music") + "/MainActivity")
    elif command[:3] == ["shell", "dumpsys", "package"]:
        print("versionName=21.1.0")
    elif command[:3] == ["shell", "dumpsys", "window"]:
        print("KeyguardServiceDelegate showing=" + ("true" if (root / "locked").exists() else "false"))
    else:
        print("")
else:
    print("unsupported fake command", file=sys.stderr)
    sys.exit(1)
