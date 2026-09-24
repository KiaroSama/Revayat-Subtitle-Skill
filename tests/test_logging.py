"""Opt-in diagnostics are usable without exposing private tool data."""

import logging
import os
import shutil
import subprocess
import contextlib
import io
import unittest
from unittest.mock import patch

from check import ROOT, WorkspaceCase
from runtime import operational_log
from runtime import LogConfigurationError


class LoggingTests(WorkspaceCase):
    def test_invalid_level_is_rejected_without_exposing_its_value(self):
        with patch.dict(os.environ, {"REVAYAT_LOG_LEVEL": "sensitive-fixture-body"}):
            with self.assertRaises(LogConfigurationError) as caught:
                with operational_log("invalid"):
                    self.fail("Invalid configuration must stop before the operation")
        self.assertNotIn("sensitive-fixture-body", str(caught.exception))

    def test_unwritable_log_destination_uses_formatted_fallback(self):
        destination = self.root / "not-a-directory"
        destination.write_text("preserve", encoding="utf-8")
        stream = io.StringIO()
        with patch.dict(os.environ, {"REVAYAT_LOG_DIR": str(destination)}), contextlib.redirect_stderr(stream):
            with operational_log("fallback"):
                logging.warning("Controlled warning")
        self.assertIn("[WARNING] [fallback]", stream.getvalue())
        self.assertIn("File logging unavailable", stream.getvalue())
        self.assertEqual(destination.read_text(encoding="utf-8"), "preserve")

    def test_write_failure_falls_back_and_handlers_close(self):
        stream = io.StringIO()
        class FailedStream:
            def write(self, value):
                raise OSError("Injected log write failure")
            def flush(self):
                pass
            def close(self):
                pass
        with contextlib.redirect_stderr(stream):
            with operational_log("write-failure") as handler:
                original = handler.stream
                handler.stream = FailedStream()
                original.close()
                logging.error("Controlled failure")
        self.assertIn("File logging failed", stream.getvalue())
        self.assertIn("Controlled failure", stream.getvalue())
        self.assertNotIn("Logging error", stream.getvalue())
        self.assertNotIn(handler, logging.getLogger().handlers)

    def test_same_second_runs_do_not_overwrite_logs(self):
        with patch("runtime.time.strftime", return_value="2026-01-01_00-00-00_UTC"):
            for index in range(2):
                with operational_log("collision"):
                    logging.info("Run %d", index)
        files = list((self.root / "logs").glob("collision_*.log"))
        self.assertEqual(len(files), 2)
        self.assertNotEqual(files[0].read_bytes(), files[1].read_bytes())

    def test_native_bootstrap_logs_missing_python(self):
        binaries = self.root / "fake interpreters"
        binaries.mkdir()
        for name in ("python", "python3", "py"):
            fake = binaries / (name + ".cmd" if os.name == "nt" else name)
            fake.write_text("@exit /b 1\n" if os.name == "nt" else "#!/bin/sh\nexit 1\n", encoding="utf-8")
            if os.name != "nt":
                fake.chmod(0o755)
        environment = {**os.environ, "PATH": str(binaries) + os.pathsep + os.environ.get("PATH", ""),
                       "REVAYAT_LOG_LEVEL": "INFO"}
        if os.name == "nt":
            command = [shutil.which("pwsh") or "powershell", "-NoProfile", "-File", str(ROOT / "install/install.ps1")]
        else:
            command = [shutil.which("bash"), str(ROOT / "install/install.sh")]
        result = subprocess.run(command, env=environment, cwd=self.root, capture_output=True,
                                text=True, encoding="utf-8", timeout=15,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("Python 3.10+ is required", result.stderr)
        files = list((self.root / "logs").glob("install-bootstrap_*.log"))
        self.assertEqual(len(files), 1)
        self.assertIn("[ERROR] [install-bootstrap]", files[0].read_text(encoding="utf-8"))

    def test_debug_level_reaches_the_per_run_file(self):
        with patch.dict(os.environ, {"REVAYAT_LOG_LEVEL": "DEBUG"}):
            with operational_log("debug-probe"):
                logging.debug("Bounded diagnostic event")
        files = list((self.root / "logs").glob("debug-probe_*.log"))
        self.assertEqual(len(files), 1)
        text = files[0].read_text(encoding="utf-8")
        self.assertIn("[DEBUG]", text)
        self.assertIn("Bounded diagnostic event", text)


if __name__ == "__main__":
    unittest.main()
