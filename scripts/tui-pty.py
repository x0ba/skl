#!/usr/bin/env python3
"""Drive skl on a PTY for scripts/smoke-tui.sh. Not product UI."""

from __future__ import annotations

import errno
import fcntl
import json
import os
import pty
import select
import struct
import sys
import termios
import time


def drain(master: int, budget: float) -> bytes:
    end = time.time() + budget
    got = b""
    while time.time() < end:
        remain = end - time.time()
        ready, _, _ = select.select([master], [], [], max(0.0, remain))
        if not ready:
            continue
        try:
            data = os.read(master, 65536)
        except OSError as exc:
            if exc.errno == errno.EIO:
                break
            raise
        if not data:
            break
        got += data
        end = time.time() + 0.05
    return got


def saw_alt(blob: bytes) -> bool:
    return b"\x1b[?1049h" in blob or b"\x1b[?1049" in blob


def main() -> int:
    if len(sys.argv) < 4:
        sys.stderr.write("usage: tui-pty.py <skl-bin> <prefix> <keys> [skl args...]\n")
        return 2
    bin_path, prefix, keys = sys.argv[1], sys.argv[2], sys.argv[3]
    argv = [bin_path] + sys.argv[4:]
    env = os.environ.copy()
    env.setdefault("SKL_NO_PROMPT", "1")
    # Cloud/CI often export TERM=dumb. Furnace degrades on dumb; force a
    # capable value so the PTY can actually Enter (override via SKL_TUI_TERM).
    term = env.get("SKL_TUI_TERM") or env.get("TERM") or "xterm-256color"
    if term.strip().lower() == "dumb":
        term = "xterm-256color"
    env["TERM"] = term

    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))

    pid = os.fork()
    if pid == 0:
        os.close(master)
        os.setsid()
        os.dup2(slave, 0)
        os.dup2(slave, 1)
        os.dup2(slave, 2)
        if slave > 2:
            os.close(slave)
        os.execvpe(argv[0], argv, env)

    deadline = time.time() + float(env.get("SKL_TUI_PTY_TIMEOUT", "12"))
    chunks: list[bytes] = []
    status = None
    alive = True
    entered = False
    keys_b = keys.encode("utf-8") if keys else b""

    while time.time() < deadline:
        chunks.append(drain(master, 0.15))
        blob = b"".join(chunks)
        if saw_alt(blob):
            entered = True
            break
        wpid, st = os.waitpid(pid, os.WNOHANG)
        if wpid != 0:
            status = st
            alive = False
            break

    if keys_b and alive:
        os.write(master, keys_b)

    timed_out = False
    while alive:
        chunks.append(drain(master, 0.2))
        wpid, status = os.waitpid(pid, os.WNOHANG)
        if wpid != 0:
            chunks.append(drain(master, 0.2))
            break
        if time.time() >= deadline:
            os.kill(pid, 9)
            _, status = os.waitpid(pid, 0)
            timed_out = True
            break

    if timed_out:
        exit_code = 124
    elif status is None:
        exit_code = 0
    elif os.WIFEXITED(status):
        exit_code = os.WEXITSTATUS(status)
    else:
        exit_code = 1

    lflag = termios.tcgetattr(slave)[3]
    cooked = bool(lflag & termios.ICANON) and bool(lflag & termios.ECHO)
    blob = b"".join(chunks)
    if saw_alt(blob):
        entered = True

    os.close(master)
    os.close(slave)

    with open(prefix + ".out", "wb") as fh:
        fh.write(blob)
    with open(prefix + ".meta.json", "w", encoding="utf-8") as fh:
        json.dump(
            {
                "exit": exit_code,
                "cooked": cooked,
                "entered_alt": entered,
                "timeout": timed_out,
                "bytes": len(blob),
            },
            fh,
        )
        fh.write("\n")
    if timed_out:
        sys.stderr.write("tui-pty: timeout waiting for %s\n" % argv)
        return 124
    return 0


if __name__ == "__main__":
    sys.exit(main())
