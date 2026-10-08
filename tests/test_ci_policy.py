"""Small source-policy assertions complement real actionlint and GitHub event evidence."""
import unittest
from check import ROOT


class CiPolicyTests(unittest.TestCase):
    def test_main_push_pr_manual_and_minimum_lane_are_explicit(self):
        for name in ("ci.yml", "workflow-lint.yml"):
            text = (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")
            self.assertIn("  push:\n    branches: [main]\n  pull_request:\n  workflow_dispatch:", text)
            self.assertNotIn("pull_request_target", text)
            self.assertIn("persist-credentials: false", text)
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("python: '3.11'", ci)
        self.assertNotIn("'3.10'", ci)
        self.assertIn("cf430e030ddbb5b0abf93d22962f4752f3646cd9", ci)


if __name__ == "__main__":
    unittest.main()
