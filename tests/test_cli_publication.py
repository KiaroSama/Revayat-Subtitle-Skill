"""The package CLI must preserve the publisher's exclusive output semantics."""

import sys
import unittest

from check import CLI, WorkspaceCase
from process_helpers import run_child
import test_delivery as delivery_fixtures


class CliPublicationTests(WorkspaceCase):
    def test_dangling_output_link_cannot_redirect_package_writes(self):
        edition = delivery_fixtures.DeliveryTests.reviewed_build(self)
        target = self.root / "unrequested.zip"
        output = self.root / "Sub.zip"
        try:
            output.symlink_to(target)
        except OSError:
            self.skipTest("Native symbolic-link permission unavailable")
        result = run_child(
            [sys.executable, str(CLI), "package", "--build", str(edition), "--out", str(output)],
            timeout=20,
        )
        self.assertEqual(result.returncode, 2, result.stdout.decode("utf-8"))
        self.assertTrue(output.is_symlink())
        self.assertFalse(target.exists(), "An occupied output name must not create another file")
        self.assertNotIn(b"Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
