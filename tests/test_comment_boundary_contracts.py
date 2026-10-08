"""Comment removal may coalesce prose but must not activate literal delimiters."""
import os
from pathlib import Path
import sys
import unittest

from check import WorkspaceCase
from runtime import read_json, write_json, run
from markup import uncomment
from workflow import build, prepare

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'


class CommentBoundaryTests(unittest.TestCase):
    def test_completed_comments_cannot_create_active_srt_tags(self):
        for text in ("A<<!--note-->b>B", "A</<!--note-->b>B", "A<<!--note-->br>B", "A<<!--note-->small>B", "A<<!--note-->foo>B", "A<<!--note-->123>B", "A<<!--note-->_foo>B", r"A{\a<!--note-->n8}B"):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "comment.*activat"):
                uncomment(text, "srt")

    def test_ordinary_comment_and_existing_tag_boundaries_remain(self):
        for text, expected in (("A<!--note-->B", "AB"), ("<b>A<!--note-->B</b>", "<b>AB</b>"),
                               ("A<!--unfinished", "A<!--unfinished"), ("A<sm<!--note-->all!>B", "A<small!>B"),
                               ("A< <!--note-->foo>B", "A< foo>B"), ("A<<<!--note-->foo>B", "A<<foo>B")):
            with self.subTest(text=text):
                self.assertEqual(uncomment(text, "srt"), expected)
                self.assertEqual(uncomment(expected, "srt"), expected)

    def test_ass_literal_escape_and_drawing_object_boundary_remain(self):
        self.assertEqual(uncomment(r"A\{literal}B", "ass"), r"A\{literal}B")
        self.assertEqual(uncomment(r"{\p1}m 0 0{note}m 1 1{\p0}X", "ass"),
                         r"{\p1}m 0 0{}m 1 1{\p0}X")


class CommentBuildTests(WorkspaceCase):
    def test_public_build_refusal_preserves_source_then_explicit_retry(self):
        source = self.root / 'input.srt'
        raw = b'1\n00:00:01,000 --> 00:00:02,000\nA<<!--note-->b>B\n'
        source.write_bytes(raw)
        prepare([source], self.work, 'Authored', 1, 'utf-8', None, 'en')
        project = read_json(self.work / 'project.json')
        project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'comparison': 'Authored source'}]
        write_json(self.work / 'project.json', project)
        write_json(self.work / 'glossary.json', {'series': 'Authored', 'terms_reviewed': True,
                   'terms': [], 'research': [{'url': 'https://example.org', 'note': 'Authored fixture'}]})
        sheet = self.work / 'worksheets/s0001.json'
        rows = read_json(sheet)
        rows[0].update(reviewed=True, action='preserve', structure_note='Cannot authorize activation')
        write_json(sheet, rows)
        with self.assertRaisesRegex(ValueError, 'activat'):
            build(self.work)
        self.assertFalse((self.work / 'builds').exists())
        rows[0].update(action='edit', text='A less than b greater than B', structure_note='Explicit literal adaptation')
        write_json(sheet, rows)
        self.assertEqual(build(self.work)['episodes'][0]['cues'], 1)
        self.assertEqual(source.read_bytes(), raw)
        self.assertEqual((self.work / 'sources/s0001.srt').read_bytes(), raw)

    @unittest.skipUnless(RENDER, 'Actual FFmpeg subtitle decoder tier')
    def test_native_original_and_activated_control_are_distinct(self):
        from render import ffmpeg_path
        outputs = []
        for index, text in enumerate(('A<<!--note-->b>B', 'A<b>B', 'A less than b greater than B',
                                      'A<<!--note-->123>B', 'A<123>B',
                                      'A<<!--note-->_foo>B', 'A<_foo>B', 'A<small!>B', 'A< foo>B', 'A<<foo>B')):
            source = self.root / f'native-{index}.srt'
            source.write_text('1\n00:00:01,000 --> 00:00:02,000\n' + text + '\n', encoding='utf-8')
            output = self.root / f'native-{index}.ass'
            run([ffmpeg_path(), '-nostdin', '-hide_banner', '-loglevel', 'error', '-i', str(source),
                 '-c:s', 'ass', str(output)], timeout=20, idle_timeout=15)
            data = output.read_text(encoding='utf-8')
            dialogue = next(line for line in data.splitlines() if line.startswith('Dialogue:'))
            outputs.append(dialogue.split(',', 9)[-1])
        self.assertNotEqual(outputs[0], outputs[1], 'Reject the hypothesis if original already activates the same tag')
        self.assertIn('A', outputs[0])
        self.assertIn('B', outputs[0])
        self.assertEqual(outputs[2], 'A less than b greater than B')
        self.assertNotEqual(outputs[3], outputs[4])
        self.assertNotEqual(outputs[5], outputs[6])
        self.assertEqual(outputs[4], 'AB')
        self.assertEqual(outputs[6], 'AB')
        self.assertIn('<small!>', outputs[7])
        self.assertIn('< foo>', outputs[8])
        self.assertIn('<<foo>', outputs[9])


if __name__ == "__main__":
    unittest.main()
