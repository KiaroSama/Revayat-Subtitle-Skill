"""Behavioral regressions for installer history and protected Persian text."""

import importlib.util
import contextlib
import io
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from check import ROOT, WorkspaceCase
from test_installation import installer


def persian_linter():
    path = ROOT / "skills/revayat-subtitle/voice/persian/scripts/persian_lint.py"
    spec = importlib.util.spec_from_file_location("audit_persian_lint", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ProtectedPersianTextTests(unittest.TestCase):
    def test_indented_code_protects_only_its_own_line(self):
        linter = persian_linter()
        for newline in ("\n", "\r\n"):
            for indent in ("    ", "\t"):
                with self.subTest(newline=repr(newline), indent=repr(indent)):
                    text = f"{indent}code = 'كتاب ها' {newline}{newline}كتاب ها مي رود"
                    fixed, stats = linter.normalize(text, digits="keep")
                    self.assertEqual(fixed, f"{indent}code = 'كتاب ها' {newline}{newline}کتاب‌ها می‌رود")
                    self.assertEqual(stats, {"arabic_to_persian_letters": 2,
                                             "zwnj_mi_prefix": 1, "zwnj_plural_ha": 1})
                    self.assertEqual(linter.normalize(fixed, digits="keep"), (fixed, {}))

    def test_suggestions_continue_after_indented_code(self):
        linter = persian_linter()
        text = "    میرود code\n\nمیرود"
        suggestions = linter.suggestions(text)
        self.assertEqual([item["type"] for item in suggestions], ["maybe_glued_mi"])

    def test_literal_nul_cannot_shift_or_drop_protected_fragments(self):
        linter = persian_linter()
        cases = ("before\x00`one` after `two`", "\x00`1`\x00`2`\x00", "`one\x00two`\x00end")
        for text in cases:
            with self.subTest(text=repr(text)):
                self.assertEqual(linter.normalize(text, digits="keep"), (text, {}))

    def test_multiline_fenced_code_is_preserved_with_following_prose(self):
        linter = persian_linter()
        text = "```python\nكتاب ها\nمي رود\n```\n\nكتاب ها مي رود"
        fixed, _ = linter.normalize(text, digits="keep")
        self.assertEqual(fixed, "```python\nكتاب ها\nمي رود\n```\n\nکتاب‌ها می‌رود")

    def test_cli_diagnostics_do_not_expose_source_text(self):
        linter = persian_linter()
        output, diagnostics = io.StringIO(), io.StringIO()
        text = "sensitive-phrase میرود"
        with patch.object(linter.sys, "stdin", io.StringIO(text)), \
             patch.dict(os.environ, {"REVAYAT_LOG_LEVEL": "DEBUG"}), \
             contextlib.redirect_stdout(output), contextlib.redirect_stderr(diagnostics):
            result = linter.main(["--fix", "--digits", "keep"])
        self.assertEqual(result, 0)
        self.assertEqual(output.getvalue(), text)
        log = diagnostics.getvalue()
        self.assertIn("[DEBUG]", log)
        self.assertIn("[INFO]", log)
        self.assertIn("[WARNING]", log)
        self.assertRegex(log, r"\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC\]")
        self.assertNotIn("sensitive-phrase", log)

    def test_invalid_log_level_fails_without_processing_input(self):
        linter = persian_linter()
        output, diagnostics = io.StringIO(), io.StringIO()
        with patch.object(linter.sys, "stdin", io.StringIO("كتاب ها")), \
             patch.dict(os.environ, {"REVAYAT_LOG_LEVEL": "invalid"}), \
             contextlib.redirect_stdout(output), contextlib.redirect_stderr(diagnostics):
            self.assertEqual(linter.main(["--fix"]), 2)
        self.assertEqual(output.getvalue(), "")
        self.assertIn("[ERROR]", diagnostics.getvalue())


class InstallerRegistryCapacityTests(WorkspaceCase):
    def test_completed_history_does_not_poison_the_next_transaction(self):
        module = installer()
        from runtime import read_json, write_json
        with module.state.ownership() as root:
            entries = [{"path": str(self.root / f"past-{i}" / "journal.json"),
                        "transaction_id": ["1", f"{i + 1:x}"], "digests": ["0" * 64],
                        "active": False} for i in range(1000)]
            write_json(root / "pending.json", entries)
        target = self.root / "installed"
        module.install(target, plugin=False, force=False, recovery_dir=self.root / "recovery")
        self.assertTrue((target / "SKILL.md").is_file())
        with module.state.ownership() as root:
            current = module.state.registry(root)
            self.assertLessEqual(len(current), 1000)
            self.assertFalse(current[-1]["active"])
            journal = Path(current[-1]["path"])
            self.assertEqual(read_json(journal)["state"], "committed")
            self.assertTrue(module.state.registered(root, journal))

    def test_capacity_retirement_preserves_every_active_binding(self):
        module = installer()
        from runtime import write_json
        with module.state.ownership() as root:
            active = {"path": str(self.root / "interrupted" / "journal.json"),
                      "transaction_id": ["1", "1"], "digests": ["0" * 64], "active": True}
            entries = [active] + [dict(active, path=str(self.root / f"old-{i}" / "journal.json"),
                                      active=False) for i in range(999)]
            write_json(root / "pending.json", entries)
            journal = self.root / "new transaction" / "journal.json"
            journal.parent.mkdir()
            write_json(journal, {})
            module.state.register(root, journal)
            current = module.state.registry(root)
            self.assertEqual(len(current), 1000)
            self.assertEqual(current[0], active)
            self.assertNotIn(entries[1], current)
            self.assertTrue(module.state.registered(root, journal))

    def test_full_active_registry_is_refused_before_mutation(self):
        module = installer()
        from runtime import write_json
        with module.state.ownership() as root:
            entries = [{"path": str(self.root / f"active-{i}" / "journal.json"),
                        "transaction_id": ["1", f"{i + 1:x}"], "digests": ["0" * 64],
                        "active": True} for i in range(1000)]
            registry = root / "pending.json"
            write_json(registry, entries)
            before = registry.read_bytes()
            journal = self.root / "new transaction" / "journal.json"
            journal.parent.mkdir()
            write_json(journal, {})
            with self.assertRaisesRegex(ValueError, "registry is full"):
                module.state.register(root, journal)
            self.assertEqual(registry.read_bytes(), before)
            self.assertEqual(module.state.registry(root), entries)

    def test_legacy_overflow_recovers_before_installing_again(self):
        module = installer()
        from runtime import digest, read_json, read_limited, write_json
        with module.state.ownership() as root:
            history = [{"path": str(self.root / f"past-{i}" / "journal.json"),
                        "transaction_id": ["1", f"{i + 1:x}"], "digests": ["0" * 64],
                        "active": False} for i in range(1000)]
            write_json(root / "pending.json", history)

        def old_register(owner, journal):
            entries = module.state.registry(owner)
            entries.append({"path": str(journal), "transaction_id": module.state.identity(journal.parent),
                            "digests": [digest(read_limited(journal, module.state.MAX_FILE))], "active": True})
            write_json(owner / "pending.json", entries)
            raise OSError("Simulated older installer interruption after overflow write")

        target, recovery = self.root / "target", self.root / "recovery"
        with patch.object(module.state, "register", old_register):
            with self.assertRaisesRegex(OSError, "older installer"):
                module.install(target, plugin=False, force=False, recovery_dir=recovery)
        journal = next(recovery.glob("transaction-*/journal.json"))
        self.assertEqual(len(read_json(root / "pending.json")), 1001)
        self.assertFalse(target.exists())
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual(read_json(journal)["state"], "restored")
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(target.exists())
        module.install(target, plugin=False, force=False, recovery_dir=recovery)
        self.assertTrue((target / "SKILL.md").is_file())
        self.assertLessEqual(len(module.state.registry(root)), 1000)

    def test_committed_journals_with_unfinished_unregister_do_not_exhaust_capacity(self):
        module = installer()
        from runtime import write_json
        with patch.object(module.state, "MAX_REGISTRY", 2):
            targets = [self.root / f"target-{i}" for i in range(3)]
            recovery = self.root / "recovery"
            for target in targets[:2]:
                module.install(target, plugin=False, force=False, recovery_dir=recovery)
            with module.state.ownership() as root:
                entries = module.state.registry(root)
                self.assertEqual(len(entries), 2)
                for entry in entries:
                    entry["active"] = True
                write_json(root / "pending.json", entries)
            before = [(target / "SKILL.md").read_bytes() for target in targets[:2]]
            module.install(targets[2], plugin=False, force=False, recovery_dir=recovery)
            self.assertEqual(before, [(target / "SKILL.md").read_bytes() for target in targets[:2]])
            self.assertTrue((targets[2] / "SKILL.md").is_file())
            self.assertTrue(all(not entry["active"] for entry in module.state.registry(root)))


if __name__ == "__main__":
    unittest.main()
