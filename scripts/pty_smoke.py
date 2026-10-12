#!/usr/bin/env python3
"""Development-only integration tests. Python is NOT a tpreview runtime dependency.
Uses a real PTY; Kitty responses are simulated, never called a real-terminal image test.
"""
import errno
import fcntl
import json
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

BIN = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/tpreview").resolve())

class Session:
    def __init__(self, args, stdin=None, image=False, tmux=False):
        self.master, self.slave = os.openpty()
        self.initial = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 34, 100, 800, 544))
        self.image = image
        self.responded = False
        self.data = b""
        self.done = False
        pipe = os.pipe() if stdin is not None else None
        report_read, report_write = os.pipe()
        self.report_read = report_read
        self.pid = os.fork()
        if self.pid == 0:
            try:
                os.close(report_read)
                os.setsid()
                fcntl.ioctl(self.slave, termios.TIOCSCTTY, 0)
                os.dup2(pipe[0] if pipe else self.slave, 0)
                os.dup2(self.slave, 1)
                os.dup2(self.slave, 2)
                os.close(self.master)
                if pipe:
                    os.close(pipe[1])
                env = os.environ.copy()
                env.pop("TMUX", None)
                env["TERM"] = "xterm-kitty" if not image else "unrecognized-terminal"
                if tmux:
                    env["TMUX"] = "/tmp/simulated,1,0"
                baseline = termios.tcgetattr(1)
                app = subprocess.Popen([BIN, *args], env=env)
                os.write(report_write, (str(app.pid) + "\n").encode())
                status = app.wait()
                restored = termios.tcgetattr(1)
                # Darwin may set transient PENDIN when canonical mode is restored.
                restored[3] &= ~getattr(termios, "PENDIN", 0)
                baseline[3] &= ~getattr(termios, "PENDIN", 0)
                os.write(report_write, json.dumps({"status": status, "restored": restored == baseline, "before": repr(baseline), "after": repr(restored)}).encode())
                os.close(report_write)
                os._exit(0)
            finally:
                os._exit(127)
        os.close(report_write)
        pid_bytes = b""
        while not pid_bytes.endswith(b"\n"):
            pid_bytes += os.read(report_read, 1)
        self.app_pid = int(pid_bytes)
        if pipe:
            os.close(pipe[0])
            os.write(pipe[1], stdin)
            os.close(pipe[1])

    def pump(self, duration=.12):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            if select.select([self.master], [], [], .025)[0]:
                try:
                    part = os.read(self.master, 65536)
                except OSError as e:
                    if e.errno == errno.EIO:
                        break
                    raise
                self.data += part
                if self.image and not self.responded and b"\x1b[16t" in self.data:
                    self.responded = True
                    # Probe response interleaving / fragmentation is covered by Rust decoder tests.
                    os.write(self.master, b"\x1b_Gi=31;OK\x1b\\\x1b[6;16;8t")
            found, status = os.waitpid(self.pid, os.WNOHANG)
            if found:
                self.done = True
                self.status = status
                break
        return self.data

    def until(self, predicate, seconds=8):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.pump()
            if predicate(self.data):
                return
            if self.done:
                break
        raise AssertionError(f"PTY expectation failed; tail={self.data[-2500:]!r}")

    def send(self, data):
        os.write(self.master, data)
        self.pump(.15)

    def finish(self, key=b"q", signum=None):
        if signum:
            os.kill(self.app_pid, signum)
        elif not self.done:
            os.write(self.master, key)
        end = time.monotonic() + 6
        while not self.done and time.monotonic() < end:
            self.pump()
        if not self.done:
            os.kill(self.app_pid, signal.SIGKILL)
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
            raise AssertionError("UI did not exit")
        assert os.waitstatus_to_exitcode(self.status) == 0, self.status
        report = json.loads(os.read(self.report_read, 4096))
        os.close(self.report_read)
        assert report["status"] == 0, report
        assert report["restored"], report
        assert b"\x1b[?1049l" in self.data, "missing alternate-screen exit"
        os.close(self.master)
        os.close(self.slave)


def run():
    master, slave = os.openpty()
    result = subprocess.run([BIN, "-"], input=b"# No controlling terminal\n", stdout=slave, stderr=subprocess.PIPE, start_new_session=True, timeout=5)
    output = os.read(master, 4096)
    os.close(master)
    os.close(slave)
    assert result.returncode == 0 and b"No controlling terminal" in output and b"\x1b" not in output
    print("PASS stdout TTY without controlling terminal: escape-free noninteractive fallback")
    with tempfile.TemporaryDirectory(prefix="tpreview-pty-") as tmp:
        root = Path(tmp)
        md = root / "design.md"
        initial = "# Start\n\n```mermaid\nflowchart LR\nA[Alpha] --> B[Bravo]\n```\n\n## Sequence\n\n```mermaid\nsequenceDiagram\nA->>B: Ping\n```\n"
        md.write_text(initial)
        s = Session(["--render", "auto", str(md)])
        s.until(lambda b: b"text" in b and b"Alpha" in b)
        assert b"a=T" not in s.data, "TERM name alone enabled images"
        s.send(b"/?unlikely\x1b")
        s.finish(key=b"\x03")
        print("PASS real PTY: no-response text fallback; Ctrl+C restores termios")

        s = Session(["--render", "auto", str(md)], image=True)
        s.until(lambda b: b"a=T,f=100" in b)
        assert b"image/Kitty" in s.data
        s.send(b"\t\r")
        s.send(b"+l")
        s.send(b"\x1b")
        s.send(b"j")
        s.finish()
        assert b"a=d,d=I,i=" in s.data
        assert b";iVBOR" in s.data
        print("PASS simulated Kitty ACK: PNG placement, detail/zoom/pan, owned-image cleanup")

        s = Session(["-", "--render", "text"], stdin=b"# Piped document\n\nkeyboard via controlling TTY\n")
        s.until(lambda b: b"Piped document" in b)
        s.finish()
        print("PASS piped stdin + separate /dev/tty keyboard")

        s = Session(["--watch", "--render", "text", str(md)])
        s.until(lambda b: b"Alpha" in b and "┌".encode() in b)
        invalid = initial.replace("A[Alpha] --> B[Bravo]", "A[unfinished")
        md.write_text(invalid)
        s.until(lambda b: b"previous" in b and b"valid" in b)
        # Atomic replacement and a burst of writes must converge on the last file.
        for i in range(8):
            replacement = root / "replacement.tmp"
            replacement.write_text(initial.replace("Alpha", f"Burst{i}"))
            replacement.replace(md)
        s.until(lambda b: b"Burst7" in b)
        md.unlink()
        s.until(lambda b: b"read error:" in b or b"cannot open" in b)
        md.write_text(initial.replace("Alpha", "Restored"))
        s.until(lambda b: b"Restored" in b)
        s.finish(signum=signal.SIGTERM)
        print("PASS watch: invalid draft, last-good result, burst/atomic saves, delete/recreate; SIGTERM restores")

        md.write_text(initial)
        s = Session(["--render", "image", str(md)], image=True, tmux=True)
        s.until(lambda b: b"Alpha" in b)
        assert b"a=q" not in s.data and b"a=T" not in s.data
        s.finish()
        print("PASS tmux policy: no image probe or transmission, even with --render image")

        # Resize while reading, then quit: terminal dimensions are polled from the actual PTY.
        s = Session(["--render", "text", str(md)])
        s.until(lambda b: b"Alpha" in b)
        fcntl.ioctl(s.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 32, 256, 192))
        s.send(b"G")
        s.finish()
        print("PASS PTY resize to 32×12 and normal-exit restoration")

if __name__ == "__main__":
    run()
