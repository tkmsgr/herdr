import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="hfi-") as tmp:
    base = Path(tmp)
    env = {k: v for k, v in os.environ.items() if not k.startswith("HERDR_")}
    for name in ["cfg", "state-home", "run", "bin"]:
        (base / name).mkdir(mode=0o700)
    env.update(XDG_CONFIG_HOME=str(base / "cfg"), XDG_STATE_HOME=str(base / "state-home"),
               XDG_RUNTIME_DIR=str(base / "run"), HERDR_CONFIG_PATH=str(base / "config.toml"),
               HERDR_DISABLE_SOUND="1", SHELL="/bin/sh", TERM="xterm-256color",
               PATH=str(base / "bin") + os.pathsep + env["PATH"], FCITX_SMOKE_DIR=tmp)
    (base / "config.toml").write_text('''onboarding = false
[update]
version_check = false
manifest_check = false
[keys]
prefix = "ctrl+a"
[experimental]
switch_ascii_input_source_in_prefix = true
''')
    state = base / "ime-state"
    state.write_text("2\n")
    fake = base / "bin/fcitx5-remote"
    fake.write_text('''#!/bin/sh
if [ "$1" = "--check" ]; then shift; fi
case "$1" in
  '') read -r value < "$FCITX_SMOKE_DIR/ime-state"; echo "$value" ;;
  -c) echo 1 > "$FCITX_SMOKE_DIR/ime-state"; echo deactivate >> "$FCITX_SMOKE_DIR/ime-calls" ;;
  -o) echo 2 > "$FCITX_SMOKE_DIR/ime-state"; echo activate >> "$FCITX_SMOKE_DIR/ime-calls" ;;
  *) exit 1 ;;
esac
''')
    fake.chmod(0o700)
    children = []

    def start(*args):
        pid, fd = pty.fork()
        if pid == 0:
            os.chdir(tmp)
            os.execve(binary, [binary, "--session", "smoke", *args], env)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        children.append((pid, fd))
        return fd

    def drain(duration=0.1):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd for _, fd in children], [], [], 0.02)
            for fd in ready:
                try:
                    data = os.read(fd, 65536)
                    if b"\x1b[6n" in data:
                        os.write(fd, b"\x1b[1;1R")
                except OSError:
                    pass

    def cli(*args):
        return subprocess.run([binary, "--session", "smoke", *args], env=env, cwd=tmp,
                              capture_output=True, text=True, timeout=5)

    def expect(value):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            drain()
            if state.read_text().strip() == value:
                return
        raise AssertionError(f"expected IME {value}, got {state.read_text()!r}")

    try:
        start("server")
        deadline = time.monotonic() + 10
        while True:
            result = cli("api", "snapshot")
            if result.returncode == 0:
                break
            if time.monotonic() >= deadline:
                raise AssertionError(result.stderr)
            drain()
        result = cli("workspace", "create", "--cwd", tmp, "--label", "ime-smoke", "--focus")
        assert result.returncode == 0, result.stderr
        client = start()
        drain(2)
        os.write(client, b"\x01")
        expect("1")
        os.write(client, b"j")
        expect("2")
        os.write(client, b"\x01")
        expect("1")
        os.write(client, b"P")
        expect("2")
        os.write(client, b"\x1b")
        drain(0.3)
        os.write(client, b"\x01")
        expect("1")
        os.write(client, b"\x1b")
        expect("2")
        calls = (base / "ime-calls").read_text()
        assert calls == "deactivate\nactivate\n" * 3, calls
        state.write_text("1\n")
        os.write(client, b"\x01")
        drain(0.3)
        os.write(client, b"\x1b")
        drain(0.3)
        assert state.read_text().strip() == "1"
        assert (base / "ime-calls").read_text() == calls
        print(json.dumps({"result": "passed", "binary": binary,
                          "checks": ["prefix entry", "j command exit", "rename text entry",
                                     "escape exit", "initially inactive unchanged"],
                          "desktop_ime_touched": False}))
    finally:
        try:
            cli("server", "stop")
        except subprocess.TimeoutExpired:
            pass
        for pid, fd in reversed(children):
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            os.close(fd)
            try:
                os.waitpid(pid, 0)
            except ChildProcessError:
                pass
