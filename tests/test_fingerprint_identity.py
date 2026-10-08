"""Opened-object fingerprint identity; ordinary files, not hostile filesystem claims."""
from __future__ import annotations

import hashlib
from pathlib import Path
import unittest
from unittest.mock import patch

from check import WorkspaceCase
from runtime import file_fingerprint


class FingerprintIdentityTests(WorkspaceCase):
    def test_opened_same_size_substitution_is_refused(self):
        declared = self.root / "declared.bin"
        alternate = self.root / "alternate.bin"
        declared.write_bytes(b"original")
        alternate.write_bytes(b"replaced")
        original_open = Path.open

        def substituted(path, *args, **kwargs):
            return original_open(alternate if path == declared else path, *args, **kwargs)

        with patch.object(Path, "open", substituted):
            with self.assertRaisesRegex(ValueError, "Fingerprint input changed"):
                file_fingerprint(declared)
        self.assertEqual(declared.read_bytes(), b"original")
        self.assertEqual(alternate.read_bytes(), b"replaced")

    def test_stable_unicode_file_keeps_public_result(self):
        path = self.root / "stable فارسی.bin"
        data = "سلام OVA\n".encode("utf-8")
        path.write_bytes(data)
        self.assertEqual(file_fingerprint(path, len(data)), {
            "name": path.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})

    def test_invalid_caps_and_nonregular_inputs_refuse(self):
        path = self.root / "input.bin"
        path.write_bytes(b"AB")
        for maximum in (-1, True, 1.5, "2", None):
            with self.subTest(maximum=maximum), self.assertRaisesRegex(ValueError, "limit"):
                file_fingerprint(path, maximum)
        with self.assertRaisesRegex(ValueError, "limit"):
            file_fingerprint(path, 1)
        with self.assertRaisesRegex(ValueError, "regular"):
            file_fingerprint(self.root)


    def test_grow_shrink_during_read_and_final_path_binding_refuse(self):
        path = self.root / 'changing.bin'
        original_open = Path.open
        for changed in (b'A', b'ABC'):
            path.write_bytes(b'AB')
            class Reader:
                def __init__(self, handle):
                    self.handle = handle
                    self.changed = False
                def __enter__(self): return self
                def __exit__(self, *args): self.handle.close()
                def fileno(self): return self.handle.fileno()
                def read(self, maximum):
                    if not self.changed:
                        self.changed = True
                        with original_open(path, 'wb') as writer:
                            writer.write(changed)
                    return self.handle.read(maximum)
            def open_(target, *args, **kwargs):
                handle = original_open(target, *args, **kwargs)
                return Reader(handle) if target == path and args == ('rb',) else handle
            with self.subTest(size=len(changed)), patch.object(Path, 'open', open_):
                with self.assertRaisesRegex(ValueError, 'changed|grew'):
                    file_fingerprint(path, 2)
        path.write_bytes(b'AB')
        original_stat = Path.stat
        calls = 0
        replacement = self.root / 'other.bin'
        replacement.write_bytes(b'XY')
        def stat_(target, *args, **kwargs):
            nonlocal calls
            if target == path:
                calls += 1
                if calls == 2:
                    return original_stat(replacement)
            return original_stat(target, *args, **kwargs)
        with patch.object(Path, 'stat', stat_), self.assertRaisesRegex(ValueError, 'changed'):
            file_fingerprint(path)


if __name__ == "__main__":
    unittest.main()
