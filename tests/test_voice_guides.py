"""The subtitle workflow and both distribution modes ship the same voice guides."""
import json
from pathlib import Path
import unittest

from test_installation import ROOT, installer


class VoiceGuideTests(unittest.TestCase):
    def test_bundled_guides_are_discoverable_in_both_payloads(self):
        install = installer()
        manifest = json.loads((ROOT / '.claude-plugin/plugin.json').read_text(encoding='utf-8'))
        policy = 'references/target-voice.md'
        required = {
            'persian': ('fa', 'NOTICE', 'scripts/persian_lint.py', 'scripts/data/idioms.tsv'),
            'great-russian': ('ru', 'references/typography.md'),
            'humanizer': ('en',),
        }
        standalone = dict((dest.as_posix(), src) for src, dest in install.payload(False))
        plugin = dict((dest.as_posix(), src) for src, dest in install.payload(True))
        self.assertIn(policy, standalone)
        routing = standalone[policy].read_text(encoding='utf-8')
        for name, (language, *resources) in required.items():
            with self.subTest(language=language):
                self.assertIn(
                    f'| `{language}` | [{name}](../voice/{name}/SKILL.md) | `revayat-subtitle:{name}` |',
                    routing,
                )
                self.assertIn(f'./skills/revayat-subtitle/voice/{name}', manifest['skills'])
                for resource in ('SKILL.md', 'LICENSE', *resources):
                    path = f'voice/{name}/{resource}'
                    self.assertEqual(standalone[path], plugin[f'skills/revayat-subtitle/{path}'])
                    self.assertTrue(Path(standalone[path]).is_file())
        self.assertIn(f'({policy})', (install.SKILL / 'SKILL.md').read_text(encoding='utf-8'))


if __name__ == '__main__':
    unittest.main()
