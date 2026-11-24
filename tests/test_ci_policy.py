"""Small source-policy assertions complement real actionlint and GitHub event evidence."""
import re
import unittest
from check import ROOT


class CiPolicyTests(unittest.TestCase):
    def test_project_validation_covers_current_pushes_and_pull_requests(self):
        for name in ("ci.yml", "workflow-lint.yml"):
            with self.subTest(workflow=name):
                text = (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")
                self.assertNotIn("pull_request_target", text)
                self.assertIn("persist-credentials: false", text)
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        triggers = ci.split("\non:\n", 1)[1].split("\npermissions:", 1)[0]
        for event in ("push", "pull_request", "merge_group", "workflow_dispatch"):
            self.assertRegex(triggers, rf"(?m)^  {event}:$")
        self.assertNotIn("paths:", triggers)
        self.assertNotIn("paths-ignore:", triggers)
        self.assertIn("branches: [main]", triggers)
        self.assertIn("uses: ./.github/workflows/workflow-lint.yml", ci)
        self.assertIn("python: '3.11'", ci)
        self.assertNotIn("'3.10'", ci)
        self.assertIn("cf430e030ddbb5b0abf93d22962f4752f3646cd9", ci)

    def test_independent_releases_do_not_cancel_each_others_validation(self):
        ci = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
        release = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
        self.assertIn("inputs.validation_id || ''", ci)
        self.assertIn('validation_id: ${{ github.run_id }}', release)
        self.assertIn('needs: validate', release)

    def test_codeql_steps_share_a_pin_and_dependabot_group(self):
        workflow = (ROOT / '.github/workflows/codeql.yml').read_text(encoding='utf-8')
        pins = re.findall(r'uses: github/codeql-action/(init|analyze)@([0-9a-f]{40})', workflow)
        self.assertEqual(sorted(name for name, _ in pins), ['analyze', 'init'])
        self.assertEqual(len({sha for _, sha in pins}), 1)
        dependabot = (ROOT / '.github/dependabot.yml').read_text(encoding='utf-8')
        self.assertIn("'github/codeql-action/*'", dependabot)


if __name__ == "__main__":
    unittest.main()
