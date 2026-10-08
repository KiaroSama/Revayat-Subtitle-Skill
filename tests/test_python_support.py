"""Selected Python minimum is enforced before executable work starts."""
import contextlib
import importlib.util
import io
import unittest
from unittest.mock import patch

from check import ROOT, WorkspaceCase
from test_installation import installer


class PythonSupportTests(WorkspaceCase):
    def test_cli_and_installer_refuse_310_with_actionable_message(self):
        spec = importlib.util.spec_from_file_location("subtitle_cli", ROOT / "skills/revayat-subtitle/scripts/revayat-subtitle.py")
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        for module, argv in ((cli, ["doctor"]), (installer(), ["--dry-run"])):
            stream = io.StringIO()
            with self.subTest(module=module.__name__), patch.object(module.sys, "version_info", (3, 10, 0)), contextlib.redirect_stderr(stream):
                self.assertEqual(module.execute(argv), 2)
            self.assertIn("Python 3.11+", stream.getvalue())
        self.assertIn("3.11", (ROOT / "skills/revayat-subtitle/SKILL.md").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
