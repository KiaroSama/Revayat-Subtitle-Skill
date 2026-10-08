"""Acknowledged editable JSON saves preserve old state until verified publication."""
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from check import WorkspaceCase
import runtime


class JsonPersistenceTests(WorkspaceCase):
    def test_sync_failure_preserves_exact_old_bytes(self):
        path = self.root / "state.json"
        old = b'{"old":true}\n'
        path.write_bytes(old)
        error = OSError("authored sync failure")
        with patch.object(runtime.os, "fsync", side_effect=error):
            with self.assertRaises(OSError) as caught:
                runtime.write_json(path, {"text": "سلام"})
        self.assertIs(caught.exception, error)
        self.assertEqual(path.read_bytes(), old)
        self.assertEqual(list(self.root.glob(".write-*")), [])

    def test_replace_completed_then_raised_reports_committed(self):
        path = self.root / "state.json"
        path.write_bytes(b'{"old":true}\n')
        replace = os.replace
        error = OSError("authored after-effect failure")
        def after_effect(source, destination):
            replace(source, destination)
            raise error
        with patch.object(runtime.os, "replace", after_effect), self.assertLogs(level="ERROR") as logs:
            with self.assertRaises(OSError) as caught:
                runtime.write_json(path, {"text": "سلام"})
        self.assertIs(caught.exception, error)
        self.assertEqual(runtime.read_json(path), {"text": "سلام"})
        self.assertTrue(any("committed=True" in line for line in logs.output))

    def test_completed_replace_then_corruption_is_never_reported_unchanged(self):
        path = self.root / "state.json"
        path.write_bytes(b'{"old":true}\n')
        replace = os.replace
        def corrupt(source, destination):
            replace(source, destination)
            Path(destination).write_bytes(b"corrupt")
        with patch.object(runtime.os, "replace", corrupt), self.assertLogs(level="ERROR") as logs:
            with self.assertRaisesRegex(OSError, "Published JSON"):
                runtime.write_json(path, {"new": True})
        self.assertEqual(path.read_bytes(), b"corrupt")
        self.assertTrue(any("committed=True" in line for line in logs.output))

    def test_staged_readback_failure_preserves_old(self):
        path = self.root / "state.json"
        path.write_bytes(b'{"old":true}\n')
        original = runtime.read_limited
        def altered(target, maximum):
            return b"changed" if target.name.startswith(".write-") else original(target, maximum)
        with patch.object(runtime, "read_limited", altered):
            with self.assertRaisesRegex(OSError, "Staged JSON"):
                runtime.write_json(path, {"new": True})
        self.assertEqual(path.read_bytes(), b'{"old":true}\n')

    def test_unicode_roundtrip_and_no_staging_residue(self):
        path = self.root / "state فارسی.json"
        value = {"text": "سلام OVA", "ready": False}
        runtime.write_json(path, value)
        self.assertEqual(json.loads(path.read_bytes().decode("utf-8")), value)
        self.assertEqual(list(self.root.glob(".write-*")), [])


    def test_flush_and_close_failures_preserve_exact_old_and_exception(self):
        path = self.root / 'state.json'
        original_fdopen = os.fdopen
        for phase in ('flush', 'close'):
            path.write_bytes(b'{"old":true}\n')
            error = OSError('authored ' + phase)
            class Writer:
                def __init__(self, handle): self.handle = handle
                def __enter__(self): return self
                def write(self, data): return self.handle.write(data)
                def fileno(self): return self.handle.fileno()
                def flush(self):
                    if phase == 'flush': raise error
                    return self.handle.flush()
                def __exit__(self, *args):
                    self.handle.close()
                    if phase == 'close': raise error
            with self.subTest(phase=phase), patch.object(runtime.os, 'fdopen', lambda fd, mode: Writer(original_fdopen(fd, mode))):
                with self.assertRaises(OSError) as caught:
                    runtime.write_json(path, {'new': True})
                self.assertIs(caught.exception, error)
            self.assertEqual(path.read_bytes(), b'{"old":true}\n')

    def test_after_effect_foreign_replacement_reports_unknown_not_unchanged(self):
        path = self.root / 'state.json'
        path.write_bytes(b'{"old":true}\n')
        replacement = self.root / 'foreign.json'
        replacement.write_bytes(b'foreign')
        replace = os.replace
        error = OSError('after effect with foreign state')
        def after(source, destination):
            replace(source, destination)
            replace(replacement, destination)
            raise error
        with patch.object(runtime.os, 'replace', after), self.assertLogs(level='ERROR') as logs:
            with self.assertRaises(OSError) as caught:
                runtime.write_json(path, {'new': True})
        self.assertIs(caught.exception, error)
        self.assertEqual(path.read_bytes(), b'foreign')
        self.assertTrue(any('committed=None' in line for line in logs.output))


if __name__ == "__main__":
    unittest.main()
