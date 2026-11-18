"""Small source-policy assertions complement real actionlint and GitHub event evidence."""
import re
import unittest
from check import ROOT


class CiPolicyTests(unittest.TestCase):
    def test_project_validation_starts_only_by_manual_dispatch(self):
        for name, event in (("ci.yml", "workflow_dispatch"), ("workflow-lint.yml", "workflow_call")):
            with self.subTest(workflow=name):
                text = (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")
                self.assertIn(f"\non:\n  {event}:\n\n", text)
                self.assertNotIn("pull_request_target", text)
                self.assertIn("persist-credentials: false", text)
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("uses: ./.github/workflows/workflow-lint.yml", ci)
        self.assertIn("python: '3.11'", ci)
        self.assertNotIn("'3.10'", ci)
        self.assertIn("cf430e030ddbb5b0abf93d22962f4752f3646cd9", ci)

    def test_codeql_steps_share_a_pin_and_dependabot_group(self):
        workflow = (ROOT / '.github/workflows/codeql.yml').read_text(encoding='utf-8')
        pins = re.findall(r'uses: github/codeql-action/(init|analyze)@([0-9a-f]{40})', workflow)
        self.assertEqual(sorted(name for name, _ in pins), ['analyze', 'init'])
        self.assertEqual(len({sha for _, sha in pins}), 1)
        dependabot = (ROOT / '.github/dependabot.yml').read_text(encoding='utf-8')
        self.assertIn("'github/codeql-action/*'", dependabot)


if __name__ == "__main__":
    unittest.main()
