"""The owned test adapter preserves status/env and never hides ownership failures."""
import os
import subprocess
import sys
import unittest
from unittest.mock import patch

from check import WorkspaceCase
import runtime
from process_control import ToolFailure
from process_helpers import run_child


class TestChildOwnershipTests(WorkspaceCase):
    def test_explicit_environment_status_and_utf8_streams(self):
        env = {"SystemRoot": os.environ.get("SystemRoot", ""), "REVAYAT_CASE": "سلام"}
        code = "import os,sys;sys.stdout.buffer.write(os.environ['REVAYAT_CASE'].encode('utf-8'));sys.stderr.buffer.write(b'diagnostic');sys.exit(7)"
        result = run_child([sys.executable, "-I", "-S", "-c", code], env=env, cwd=self.root,
                           timeout=8, idle_timeout=5)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, "سلام".encode("utf-8"))
        self.assertEqual(result.stderr, b"diagnostic")
        self.assertNotIn("REVAYAT_CASE", os.environ)

    def test_default_bytes_and_tool_failure_unchanged(self):
        self.assertEqual(runtime.run([sys.executable, "-I", "-S", "-c", "print('ok')"], timeout=8), b"ok\r\n" if os.name == "nt" else b"ok\n")
        with self.assertRaises(ToolFailure) as caught:
            runtime.run([sys.executable, "-I", "-S", "-c", "import sys;sys.exit(3)"], timeout=8)
        self.assertEqual(caught.exception.returncode, 3)

    def test_check_false_does_not_swallow_timeout_or_cleanup_errors(self):
        error = subprocess.TimeoutExpired("owned fixture", 1)
        with patch("process_helpers.run", side_effect=error):
            with self.assertRaises(subprocess.TimeoutExpired) as caught:
                run_child([sys.executable], timeout=1)
        self.assertIs(caught.exception, error)
        error = OSError("owned cleanup failure")
        with patch("process_helpers.run", side_effect=error):
            with self.assertRaises(OSError) as caught:
                run_child([sys.executable], timeout=1)
        self.assertIs(caught.exception, error)

    def test_environment_validation_precedes_spawn(self):
        for env in ({"bad=key": "x"}, {"x": 1}, {"x": "\x00"}, {"x": "x" * (1024 * 1024 + 1)}):
            with self.subTest(env_size=len(env)), self.assertRaisesRegex(ValueError, "environment"):
                runtime.run([sys.executable], env=env)


if __name__ == "__main__":
    unittest.main()
