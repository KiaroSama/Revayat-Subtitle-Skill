"""Installation preflight and recovery are consistent across all selected targets."""

import contextlib
import importlib.util
import io
import os
import shutil
import unittest
from unittest.mock import patch

from check import ROOT, WorkspaceCase
from runtime import run


def installer():
    spec = importlib.util.spec_from_file_location("subtitle_install", ROOT / "install/install.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class InstallationTests(WorkspaceCase):
    @unittest.skipUnless(os.name == "nt", "Windows exclusive directory handles")
    def test_locked_destination_is_preserved_and_retry_succeeds(self):
        import ctypes
        from ctypes import wintypes
        module = installer()
        target = self.root / "locked target"
        target.mkdir()
        (target / "previous.txt").write_text("keep", encoding="utf-8")
        api = ctypes.WinDLL("kernel32", use_last_error=True)
        api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                   ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        api.CreateFileW.restype = wintypes.HANDLE
        api.CloseHandle.argtypes, api.CloseHandle.restype = [wintypes.HANDLE], wintypes.BOOL
        handle = api.CreateFileW(str(target), 0x80000000, 0, None, 3, 0x02000000, None)
        self.assertNotEqual(handle, ctypes.c_void_p(-1).value)
        try:
            with self.assertRaises(OSError):
                module.install(target, plugin=False, force=True)
            self.assertEqual((target / "previous.txt").read_text(encoding="utf-8"), "keep")
        finally:
            self.assertTrue(api.CloseHandle(handle))
        backup = module.install(target, plugin=False, force=True)
        self.assertEqual((backup / "previous.txt").read_text(encoding="utf-8"), "keep")
        self.assertTrue((target / "SKILL.md").is_file())

    @unittest.skipUnless(os.name == "nt", "Windows junction contract")
    def test_native_junction_target_is_rejected_and_data_preserved(self):
        real = self.root / "unrelated"
        real.mkdir()
        sentinel = real / "keep.txt"
        sentinel.write_text("keep", encoding="utf-8")
        junction = self.root / "junction"
        script = self.root / "junction.ps1"
        script.write_text("param([string]$Link,[string]$Target)\n$ErrorActionPreference='Stop'\n"
                          "New-Item -ItemType Junction -Path $Link -Target $Target | Out-Null\n", encoding="utf-8")
        run([shutil.which("pwsh") or "powershell.exe", "-NoProfile", "-File", str(script), str(junction), str(real)], timeout=10)
        with self.assertRaisesRegex(ValueError, "links or junctions"):
            installer().plan_install([junction / "skill"], plugin=False, force=False)
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_changed_ancestor_is_refused_before_staging(self):
        module = installer()
        parent = self.root / "parent"
        parent.mkdir()
        target = parent / "target"
        plan = module.plan_install([target], plugin=False, force=False)
        parent.rename(self.root / "original-parent")
        parent.mkdir()
        with self.assertRaisesRegex(ValueError, "ancestor changed"):
            module.execute_plan(plan)
        self.assertEqual(list(parent.iterdir()), [])

    def test_rollback_preserves_unrelated_empty_directory(self):
        module = installer()
        first, second = self.root / "first", self.root / "second"
        plan = module.plan_install([first, second], plugin=False, force=False)
        real = module.rename_noreplace
        def interfere(source, destination):
            if destination == second:
                (first / "unrelated-empty-directory").mkdir()
                raise OSError("Injected later failure")
            return real(source, destination)
        with patch.object(module, "rename_noreplace", interfere):
            with self.assertRaisesRegex(ValueError, "rollback incomplete"):
                module.execute_plan(plan)
        self.assertTrue((first / "unrelated-empty-directory").is_dir())

    def test_late_publication_failure_restores_all_previous_targets(self):
        module = installer()
        targets = [self.root / "first", self.root / "second"]
        for index, target in enumerate(targets):
            target.mkdir()
            (target / "previous.txt").write_text(str(index), encoding="utf-8")
        plan = module.plan_install(targets, plugin=False, force=True)
        real, fired = module.rename_noreplace, False
        def fail_once(source, destination):
            nonlocal fired
            if destination == targets[1] and not fired:
                fired = True
                raise OSError("Injected commit failure")
            return real(source, destination)
        with patch.object(module, "rename_noreplace", fail_once):
            with self.assertRaisesRegex(OSError, "Injected"):
                module.execute_plan(plan)
        for index, target in enumerate(targets):
            self.assertEqual((target / "previous.txt").read_text(encoding="utf-8"), str(index))
            self.assertFalse((target / "SKILL.md").exists())

    def test_rollback_failure_retains_recoverable_backup(self):
        module = installer()
        first, second = self.root / "first", self.root / "second"
        first.mkdir()
        (first / "previous.txt").write_text("keep", encoding="utf-8")
        plan = module.plan_install([first, second], plugin=False, force=True)
        real = module.rename_noreplace
        def fail_commit_and_restore(source, destination):
            if destination == second or (destination == first and ".backup-" in source.name):
                raise OSError("Injected recovery failure")
            return real(source, destination)
        with patch.object(module, "rename_noreplace", fail_commit_and_restore):
            with self.assertRaisesRegex(ValueError, "rollback incomplete") as caught:
                module.execute_plan(plan)
        backups = list(self.root.glob("first.backup-*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / "previous.txt").read_text(encoding="utf-8"), "keep")
        self.assertIn(str(backups[0]), str(caught.exception))

    def test_exact_destination_ignores_unused_base_and_dry_run_rejects_overlap(self):
        module = installer()
        target = self.root / "exact destination"
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = module.main(["--scope", "project", "--path", str(self.root / "missing"),
                                  "--destination", str(target)])
            refused = module.main(["--destination", str(module.SKILL), "--dry-run", "--force"])
        self.assertEqual(result, 0)
        self.assertTrue((target / "SKILL.md").is_file())
        self.assertEqual(refused, 2)

    def test_equivalent_targets_are_deduplicated(self):
        module = installer()
        target = self.root / "destination"
        plan = module.plan_install([target, target / "."], plugin=False, force=False)
        self.assertEqual(len(plan["targets"]), 1)

    def test_invalid_late_target_does_not_install_earlier_agents(self):
        base = self.root / "agents"
        (base / ".agents").mkdir(parents=True)
        (base / ".agents/skills").write_text("Occupied by an unrelated file", encoding="utf-8")
        module = installer()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = module.main(["--agent", "all", "--scope", "project", "--path", str(base)])
        self.assertEqual(result, 2)
        self.assertFalse((base / ".claude/skills/revayat-subtitle").exists())
        self.assertEqual((base / ".agents/skills").read_text(encoding="utf-8"), "Occupied by an unrelated file")


if __name__ == "__main__":
    unittest.main()
