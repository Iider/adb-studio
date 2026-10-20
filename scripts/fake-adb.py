#!/usr/bin/env python3
"""集成检查使用的模拟设备；不会调用真实 ADB。"""

import json
import os
from pathlib import Path
import sys
import time

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
