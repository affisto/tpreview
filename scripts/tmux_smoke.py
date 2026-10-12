#!/usr/bin/env python3
"""Isolated real tmux smoke test. Does not read or change the user's tmux configuration."""
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/tpreview").resolve())
socket = f"tpreview-test-{os.getpid()}"

def tmux(*args):
    return subprocess.check_output(["tmux", "-L", socket, "-f", "/dev/null", *args], stderr=subprocess.STDOUT).decode()

def wait_for(text):
    end = time.monotonic() + 8
    while time.monotonic() < end:
        screen = tmux("capture-pane", "-p", "-t", "smoke")
        if text in screen:
            return screen
        time.sleep(.08)
    raise AssertionError(f"{text!r} not found in tmux:\n{screen}")

with tempfile.TemporaryDirectory(prefix="tpreview-tmux-") as temp:
    root = Path(temp)
    md = root / "one.md"
    other = root / "two.md"
    initial = "# First\n\n[Go second](two.md#target)\n\n## Diagram\n\n```mermaid\nflowchart LR\nA[Before] --> B[After]\n```\n\n## Search\n\nneedle first\n\n" + ("filler paragraph\n\n" * 40) + "needle last\n"
    md.write_text(initial)
    other.write_text("# Second document\n\n## Target\n\nLinked destination\n")
    command = shlex.join([binary, "--watch", "--render", "auto", str(md)]) + "; printf '\\nRESTORED_SHELL\\n'; sleep 2"
    try:
        tmux("new-session", "-d", "-s", "smoke", "-x", "100", "-y", "32", command)
        screen = wait_for("Before")
        assert "text" in screen and "watch:on" in screen
        tmux("send-keys", "-t", "smoke", "Tab", "Enter")
        wait_for("Linked destination")
        tmux("send-keys", "-t", "smoke", "b")
        wait_for("Go second")
        tmux("send-keys", "-t", "smoke", "/", "needle", "Enter", "n")
        screen = wait_for("needle last")
        # Update a different file: current file must stay put.
        other.write_text("# Do not force navigation\n")
        time.sleep(.3)
        assert "one.md" in tmux("capture-pane", "-p", "-t", "smoke")
        # Insertion above reading position preserves the current paragraph.
        md.write_text(initial.replace("# First", "# First\n\nInserted above"))
        wait_for("updated")
        screen = tmux("capture-pane", "-p", "-t", "smoke")
        assert "needle last" in screen
        tmux("send-keys", "-t", "smoke", "t")
        wait_for("CONTENTS")
        tmux("send-keys", "-t", "smoke", "g", "Escape", "q")
        wait_for("RESTORED_SHELL")
        print("PASS tmux: text fallback, relative link + fragment + back, search next, position preservation, independent-file edit, TOC, shell restored")
    finally:
        try:
            tmux("kill-server")
        except subprocess.CalledProcessError:
            pass
