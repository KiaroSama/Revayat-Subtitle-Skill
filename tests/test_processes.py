"""Owned descendants cannot survive their leader's exit or a bounded tool call."""

import json
from pathlib import Path
import subprocess
import sys
import time
import unittest
import os
import threading
import _thread
from unittest.mock import patch

import psutil

from check import WorkspaceCase
from runtime import run


def active(process):
    try:
        return process.is_running() and process.status() not in (psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD)
    except psutil.NoSuchProcess:
        return False


class ProcessTests(WorkspaceCase):
    def test_success_still_cleans_a_grandchild_with_closed_capture_pipes(self):
        marker = self.root / "grandchild.json"
        grandchild = ("import json,threading,psutil; from pathlib import Path; p=psutil.Process(); "
                      f"Path({str(marker)!r}).write_text(json.dumps({{'pid':p.pid,'created':p.create_time()}}),encoding='utf-8'); "
                      "threading.Event().wait(20)")
        child = ("import subprocess,sys; subprocess.Popen([sys.executable,'-B','-c'," + repr(grandchild) + "],"
                 "stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))")
        parent = ("import subprocess,sys,time,threading; from pathlib import Path; "
                  "subprocess.Popen([sys.executable,'-B','-c'," + repr(child) + "],stdout=subprocess.DEVNULL,"
                  "stderr=subprocess.DEVNULL,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0)); "
                  "deadline=time.monotonic()+3\n"
                  f"while not Path({str(marker)!r}).exists() and time.monotonic()<deadline: threading.Event().wait(.01)\n")
        try:
            run([sys.executable, "-B", "-c", parent], timeout=5)
            self.assertTrue(marker.exists(), "Grandchild must start before success cleanup is tested")
            info = json.loads(marker.read_text(encoding="utf-8"))
            try:
                process = psutil.Process(info["pid"])
            except psutil.NoSuchProcess:
                return
            self.assertAlmostEqual(process.create_time(), info["created"], places=2)
            deadline = time.monotonic() + 3
            while active(process) and time.monotonic() < deadline:
                threading.Event().wait(0.02)
            self.assertFalse(active(process))
        finally:
            if marker.exists():
                info = json.loads(marker.read_text(encoding="utf-8"))
                try:
                    process = psutil.Process(info["pid"])
                    if abs(process.create_time() - info["created"]) < 0.01 and active(process):
                        process.kill()
                        process.wait(timeout=3)
                except psutil.NoSuchProcess:
                    pass

    def test_output_flood_is_bounded_and_private_output_is_not_reported(self):
        with self.assertRaisesRegex(ValueError, "byte limit"):
            run([sys.executable, "-B", "-c", "import os\nwhile True: os.write(1,b'x'*65536)"],
                timeout=3, max_output=1024)
        with self.assertRaises(ValueError) as caught:
            run([sys.executable, "-B", "-c", "import sys; print('sensitive-fixture-body'); sys.exit(9)"], timeout=3)
        self.assertNotIn("sensitive-fixture-body", str(caught.exception))
        self.assertIn("exit 9", str(caught.exception))

    def test_idle_timeout_is_independent_of_wall_timeout(self):
        began = time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            run([sys.executable, "-B", "-c", "import threading; threading.Event().wait(20)"],
                timeout=10, idle_timeout=0.5)
        self.assertLess(time.monotonic() - began, 8)

    def test_cancellation_preserves_exception_and_cleans_target(self):
        marker = self.root / "cancel.json"
        code = ("import json,threading,psutil; from pathlib import Path; p=psutil.Process(); "
                f"Path({str(marker)!r}).write_text(json.dumps({{'pid':p.pid,'created':p.create_time()}}),encoding='utf-8'); "
                "threading.Event().wait(20)")
        stop = threading.Event()
        def interrupt_when_ready():
            deadline = time.monotonic() + 4
            while not stop.is_set() and time.monotonic() < deadline:
                if marker.exists():
                    if not stop.is_set():
                        _thread.interrupt_main()
                    return
                stop.wait(0.02)
        interrupter = threading.Thread(target=interrupt_when_ready)
        interrupter.start()
        try:
            with self.assertRaises(KeyboardInterrupt):
                run([sys.executable, "-B", "-c", code], timeout=6)
            self.assertTrue(marker.exists())
            info = json.loads(marker.read_text(encoding="utf-8"))
            try:
                process = psutil.Process(info["pid"])
            except psutil.NoSuchProcess:
                process = None
            self.assertTrue(process is None or not active(process))
        finally:
            stop.set()
            interrupter.join(timeout=5)
            self.assertFalse(interrupter.is_alive())
            if marker.exists():
                info = json.loads(marker.read_text(encoding="utf-8"))
                try:
                    process = psutil.Process(info["pid"])
                    if abs(process.create_time() - info["created"]) < 0.01 and active(process):
                        process.kill()
                        process.wait(timeout=3)
                except psutil.NoSuchProcess:
                    pass

    def test_exited_leader_does_not_leave_pipe_owning_child(self):
        marker = self.root / "child.json"
        child = self.root / "owned_child.py"
        child.write_text(
            "import json,sys,threading,psutil\nfrom pathlib import Path\n"
            "p=psutil.Process()\n"
            "Path(sys.argv[1]).write_text(json.dumps({'pid':p.pid,'created':p.create_time()}),encoding='utf-8')\n"
            "threading.Event().wait(20)\n", encoding="utf-8")
        parent = self.root / "exiting_parent.py"
        parent.write_text(
            "import subprocess,sys,time,threading\nfrom pathlib import Path\n"
            "subprocess.Popen([sys.executable,'-B',sys.argv[1],sys.argv[2]],"
            "creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))\n"
            "deadline=time.monotonic()+3\n"
            "while not Path(sys.argv[2]).exists() and time.monotonic()<deadline: threading.Event().wait(.01)\n",
            encoding="utf-8")
        began = time.time()
        process = None
        try:
            try:
                run([sys.executable, "-B", str(parent), str(child), str(marker)], timeout=2)
            except subprocess.TimeoutExpired:
                pass  # POSIX may keep the capture pipe open; Windows may close it early.
            self.assertTrue(marker.exists(), "Child must actually start before cleanup is tested")
            info = json.loads(marker.read_text(encoding="utf-8"))
            try:
                process = psutil.Process(info["pid"])
            except psutil.NoSuchProcess:
                return
            self.assertGreaterEqual(info["created"], began - 1)
            self.assertAlmostEqual(process.create_time(), info["created"], places=2)
            deadline = time.monotonic() + 3
            while active(process) and time.monotonic() < deadline:
                threading.Event().wait(0.02)
            self.assertFalse(active(process), "Owned child survived after its parent exited")
        finally:
            if marker.exists():
                info = json.loads(marker.read_text(encoding="utf-8"))
                try:
                    process = psutil.Process(info["pid"])
                    if abs(process.create_time() - info["created"]) < 0.01 and active(process):
                        process.kill()
                        process.wait(timeout=3)
                except psutil.NoSuchProcess:
                    pass


if os.name == "nt":
    class WindowsOwnershipTests(WorkspaceCase):
        def test_gate_ignores_caller_sitecustomize_and_reports_target_failure(self):
            site = self.root / "sitecustomize.py"
            marker = self.root / "site-executed"
            site.write_text(f"from pathlib import Path; Path({str(marker)!r}).touch()", encoding="utf-8")
            logs = self.root / "logs"
            with patch.dict(os.environ, {"PYTHONPATH": str(self.root), "REVAYAT_LOG_DIR": str(logs)}):
                with self.assertRaisesRegex(ValueError, "exit 7"):
                    run([sys.executable, "-I", "-S", "-c", "raise SystemExit(7)"], timeout=4)
            self.assertFalse(marker.exists())
            actual = list(logs.glob("process-supervisor*.log"))
            self.assertEqual(len(actual), 1)
            self.assertIn("status=failed", actual[0].read_text(encoding="utf-8"))

        def test_assignment_failure_never_starts_the_target(self):
            from process_control import WindowsJob
            marker = self.root / "must-not-exist"
            with patch.object(WindowsJob, "assign", side_effect=OSError("Injected assignment failure")):
                with self.assertRaisesRegex(OSError, "assignment"):
                    run([sys.executable, "-B", "-c", f"from pathlib import Path; Path({str(marker)!r}).touch()"], timeout=3)
            self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
