"""Authored renderer and transaction regressions for the preservation audit."""
from __future__ import annotations

import copy
import logging
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / 'skills/revayat-subtitle/scripts'
sys.path.insert(0, str(SCRIPTS))
import markup
import runtime
import workflow
from subtitle_formats import (ASS_FIELDS, STYLE_FIELDS, DEFAULT_STYLE, Cue, Document,
                              has_drawing, parse, serialize, structure, visible)

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'


def ass_source(text='Hello.', headers=''):
    return ('[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n' + headers +
            '\n[V4+ Styles]\nFormat: ' + STYLE_FIELDS + '\nStyle: ' +
            DEFAULT_STYLE.replace('Arial', 'Arial' if os.name == 'nt' else 'DejaVu Sans') +
            '\n[Events]\nFormat: ' + ASS_FIELDS +
            '\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,' + text + '\n')


def row(source, **changes):
    return {'id': 'c000001', 'source_text': source, 'text': None, 'action': 'preserve',
            'reviewed': True, 'start_ms': 1000, 'end_ms': 3000, **changes}


class PreservationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='subtitle-preservation-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        logging.info('Preservation regression case=%s', self.id())

    def workspace(self, kind='ass', text='سلام OVA', complete=True):
        source = self.root / ('input.' + kind)
        source.write_text(ass_source(text) if kind == 'ass' else
                          '1\n00:00:01,000 --> 00:00:03,000\n' + text + '\n', encoding='utf-8')
        work = self.root / 'work'
        workflow.prepare([source], work, 'Authored fixture', 1, 'utf-8', None, 'fa')
        if complete:
            project = runtime.read_json(work / 'project.json')
            project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'comparison': 'Only authored source'}]
            runtime.write_json(work / 'project.json', project)
            rows = runtime.read_json(work / 'worksheets/s0001.json')
            for item in rows:
                item.update(reviewed=True, action='preserve')
            runtime.write_json(work / 'worksheets/s0001.json', rows)
            runtime.write_json(work / 'glossary.json', {'series': 'Authored fixture', 'terms_reviewed': True,
                'terms': [], 'research': [{'url': 'https://example.org', 'note': 'Authored test, not anime research'}]})
        return work

    def pixels(self, name, source):
        executable = os.environ.get('REVAYAT_FFMPEG') or shutil.which('ffmpeg')
        if not executable:
            self.fail('The selected renderer tier requires FFmpeg')
        (self.root / (name + '.ass')).write_text(source, encoding='utf-8')
        logging.debug('Rendering authored oracle fixture=%s', name)
        pixels = runtime.run([executable, '-hide_banner', '-loglevel', 'error', '-nostdin',
            '-f', 'lavfi', '-i', 'color=s=320x180:r=1:d=1', '-filter_threads', '1',
            '-vf', f'setpts=PTS+1.5/TB,ass={name}.ass', '-frames:v', '1', '-threads', '1',
            '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], cwd=self.root,
            timeout=20, idle_timeout=20, max_output=320 * 180 * 3 + 65536)
        self.assertEqual(len(pixels), 320 * 180 * 3)
        self.assertGreater(max(pixels), 32, 'Authored oracle must contain visible raster content')
        return runtime.digest(pixels)

    def test_escaped_opening_braces_are_visible_prose(self):
        for source, expected in ((r'\{Important\}', '{Important}'),
                                  (r'A\{Important}B', 'A{Important}B'),
                                  (r'A\\{Important}B', 'A\\{Important}B')):
            with self.subTest(source=source):
                self.assertEqual(visible(source, 'ass'), expected)
                self.assertEqual(markup.uncomment(source, 'ass'), source)

    def test_preserve_action_retains_escaped_literal_text(self):
        source = r'A\{Important}B'
        kept, _ = workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source)])
        self.assertEqual(kept[0].text, source)

    def test_nested_openings_close_at_first_closing_brace(self):
        for source, expected in (('A{comment{inner}VISIBLE}B', 'AVISIBLE}B'),
                                  ('{comment{inner}VISIBLE}', 'VISIBLE}'),
                                  ('A{comment{inner}VISIBLE{hidden}B', 'AVISIBLEB')):
            with self.subTest(source=source):
                self.assertEqual(visible(source, 'ass'), expected)
                self.assertEqual(markup.uncomment(source, 'ass'), expected)
                self.assertEqual(markup.uncomment(markup.uncomment(source, 'ass'), 'ass'), expected)

    def test_nested_comment_cannot_hide_a_nonempty_cue(self):
        source = '{comment{inner}VISIBLE}'
        with self.assertRaisesRegex(ValueError, 'nonempty'):
            workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source, action='empty')])

    def test_escaped_literal_reset_is_not_rewritten(self):
        source = r'\{\rLiteral} then {\rDefault}Hi'
        self.assertEqual(markup.remap_resets(source, {'Default': 'Donor'}),
                         r'\{\rLiteral} then {\rDonor}Hi')

    def test_tags_in_nested_opening_block_keep_first_close_semantics(self):
        source = r'A{note{\rDefault}VISIBLE}B'
        self.assertEqual(visible(source, 'ass'), 'AVISIBLE}B')
        self.assertEqual(markup.remap_resets(source, {'Default': 'Donor'}),
                         r'A{note{\rDonor}VISIBLE}B')

    def test_unclosed_braces_and_ordinary_comments_remain_stable(self):
        for source, expected in (('A{unfinished', 'A{unfinished'),
                                  ('A{hidden}B', 'AB'),
                                  (r'{\i1}A{note}B{\i0}', r'{\i1}AB{\i0}')):
            self.assertEqual(markup.uncomment(source, 'ass'), expected)
            self.assertEqual(markup.rtl(expected, 'ass'), expected)

    def test_drawing_mode_survives_bare_named_and_parenthesized_reset(self):
        for reset in (r'\r', r'\rDefault', r'\r(Default)', r'\rIgnored(Default)'):
            with self.subTest(reset=reset):
                source = r'{\p1}m 0 0 l 20 0 20 20{' + reset + '}m 30 0 l 50 0 50 20'
                self.assertTrue(has_drawing(source, 'ass'))
                self.assertEqual(visible(source, 'ass'), '')
                self.assertEqual(sum(kind == 'drawing' for kind, _ in structure(source, 'ass')), 2)

    def test_vector_changes_after_reset_require_structure_note(self):
        for reset in (r'\r', r'\rDefault', r'\r(Default)'):
            with self.subTest(reset=reset):
                source = r'{\p1}m 0 0 l 20 0 20 20{' + reset + '}m 30 0 l 50 0 50 20'
                changed = source.replace('l 50', 'l 70')
                with self.assertRaisesRegex(ValueError, 'tags/drawings'):
                    workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source, action='edit', text=changed)])

    def test_transform_drawing_assignments_are_structural(self):
        for tag in (r'\t(\p1)', r'\t(0,100,\p1)', r'\t(2,\p(1))', r'\t(0,100,2,\p1)'):
            with self.subTest(tag=tag):
                source = '{' + tag + '}m 0 0 l 20 0 20 20'
                self.assertTrue(has_drawing(source, 'ass'))
                self.assertEqual(visible(source, 'ass'), '')
                with self.assertRaisesRegex(ValueError, 'tags/drawings'):
                    workflow.reviewed_cues(parse(ass_source(source), 'ass'),
                                          [row(source, action='edit', text=source.replace('l 20', 'l 70'))])

    def test_rejected_transform_does_not_clear_drawing_mode(self):
        for arguments in ('0,1,2,3,', '0,1,2,3,4,', '0,,1,2,3,'):
            with self.subTest(arguments=arguments):
                source = r'{\p1\t(' + arguments + r'\p0)}m 0 0 l 20 0 20 20'
                self.assertTrue(has_drawing(source, 'ass'))
                self.assertEqual(visible(source, 'ass'), '')
                with self.assertRaisesRegex(ValueError, 'tags/drawings'):
                    workflow.reviewed_cues(parse(ass_source(source), 'ass'),
                        [row(source, action='edit', text=source.replace('l 20', 'l 70'))])
        for arguments in ('0,,100,', '0,100,2,0'):
            source = r'{\p1\t(' + arguments + r'\p0)}Hello'
            self.assertEqual(visible(source, 'ass'), 'Hello')

    def test_explicit_p0_returns_to_prose_after_resets_and_transforms(self):
        for tag in (r'\p0', r'\t(\p0)', r'\t(0,100,\p(0))'):
            source = r'{\p1}m 0 0 l 20 0 20 20{\r}{' + tag + '}Hello'
            self.assertEqual(visible(source, 'ass'), 'Hello')
            self.assertTrue(has_drawing(source, 'ass'))

    def test_justified_vector_edit_is_not_blocked(self):
        source = r'{\p1}m 0 0 l 20 0 20 20{\r}m 30 0 l 50 0 50 20'
        changed = source.replace('l 50', 'l 70')
        kept, _ = workflow.reviewed_cues(parse(ass_source(source), 'ass'),
            [row(source, action='edit', text=changed, structure_note='Authored vector adjustment requires visual review')])
        self.assertEqual(kept[0].text, changed)

    def test_drawings_are_not_empty_even_with_transform_only_mode(self):
        source = r'{\t(\p1)}m 0 0 l 20 0 20 20'
        with self.assertRaisesRegex(ValueError, 'nonempty'):
            workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source, action='empty')])

    def test_spaced_and_positive_signed_drawing_controls_require_review(self):
        for tag in (r'\t(\ p1)', r'\t(\p+1)'):
            source = '{' + tag + '}m 0 0 l 20 0 20 20'
            self.assertTrue(has_drawing(source, 'ass'))
            with self.assertRaisesRegex(ValueError, 'tags/drawings'):
                workflow.reviewed_cues(parse(ass_source(source), 'ass'),
                    [row(source, action='edit', text=source.replace('l 20', 'l 70'))])

    def test_drawing_comment_boundary_is_preserved(self):
        source = r'{\p1}m 0 0 l 20 0{comment}l 20 20 0 20{\p0}'
        expected = source.replace('{comment}', '{}')
        self.assertEqual(markup.uncomment(source, 'ass'), expected)
        kept, _ = workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source)])
        self.assertEqual(kept[0].text, expected)
        self.assertEqual(markup.uncomment(expected, 'ass'), expected)

    def test_drawing_reader_does_not_escape_override_opening(self):
        source = r'{\p1}m 0 0 l 20 0 20 20\{\p0}Hello'
        self.assertEqual(visible(source, 'ass'), 'Hello')
        changed = source.replace('Hello', 'سلام')
        kept, _ = workflow.reviewed_cues(parse(ass_source(source), 'ass'),
            [row(source, action='edit', text=changed)])
        self.assertIn('سلام', visible(kept[0].text, 'ass'))

    def test_srt_comments_break_dialect_and_entities_are_preserved(self):
        self.assertEqual(markup.uncomment('A<!--x\ny-->B', 'srt'), 'AB')
        self.assertEqual(visible('A<!--x-->B<br a="b">C', 'srt'), 'AB\nC')
        self.assertEqual(visible('A<br\t>B<br\u00a0>C &amp; x < 5 > y', 'srt'),
                         'A<br\t>B<br\u00a0>C &amp; x < 5 > y')
        self.assertEqual(markup.uncomment('A<!--open<b>B</b>', 'srt'), 'A<!--open<b>B</b>')

    def test_many_unclosed_delimiters_have_bounded_processing(self):
        # A generous process timeout guards accidental quadratic rescanning.
        code = ('import sys;sys.path.insert(0,sys.argv[1]);import markup;'
                'text="<!--"*50000+"tail";assert markup.uncomment(text,"srt")==text;'
                'text="{"*50000+"tail";assert markup.uncomment(text,"ass")==text;'
                'text="<b"+" "*50000;assert markup.uncomment(text,"srt")==text')
        try:
            result = subprocess.run([sys.executable, '-S', '-c', code, str(SCRIPTS)],
                                    capture_output=True, timeout=8)
        except subprocess.TimeoutExpired:
            self.fail('Unclosed delimiter processing exceeded the bounded regression budget')
        self.assertEqual(result.returncode, 0, 'Delimiter invariant failed')

    def test_seeded_plain_comment_removal_preserves_renderer_text(self):
        rng = random.Random(1003)
        for _ in range(250):
            left, right = rng.choice(('A', 'سلام', 'OVA')), rng.choice(('B', 'دنیا', '123'))
            source = left + '{hidden{inner}' + right + '}'
            expected = left + right + '}'
            self.assertEqual(visible(source, 'ass'), expected)
            normalized = markup.rtl(source, 'ass')
            self.assertEqual(visible(normalized, 'ass'), expected)
            self.assertEqual(markup.rtl(normalized, 'ass'), normalized)

    def test_cleanup_error_preserves_primary_exception_object(self):
        primary = ValueError('Primary authored refusal')
        with patch.object(runtime.shutil, 'rmtree', side_effect=PermissionError('Injected cleanup')), self.assertLogs(level='WARNING'):
            with self.assertRaises(ValueError) as caught:
                with runtime.staging_directory(self.root, '.owned-'):
                    raise primary
        self.assertIs(caught.exception, primary)

    def test_cleanup_error_preserves_cancellation(self):
        primary = KeyboardInterrupt('Authored cancellation')
        with patch.object(runtime.shutil, 'rmtree', side_effect=PermissionError('Injected cleanup')), self.assertLogs(level='WARNING'):
            with self.assertRaises(KeyboardInterrupt) as caught:
                with runtime.staging_directory(self.root, '.owned-'):
                    raise primary
        self.assertIs(caught.exception, primary)

    def test_successful_import_is_successful_despite_staging_cleanup_error(self):
        with patch.object(runtime.shutil, 'rmtree', side_effect=PermissionError('Injected cleanup')), self.assertLogs(level='WARNING'):
            work = self.workspace('srt', 'Hello', complete=False)
        self.assertEqual(len(workflow.load(work)[1]['s0001'].cues), 1)
        self.assertTrue(list(self.root.glob('.subtitle-import-*')))

    def test_successful_build_is_reusable_despite_staging_cleanup_error(self):
        work = self.workspace()
        with patch.object(runtime.shutil, 'rmtree', side_effect=PermissionError('Injected cleanup')), self.assertLogs(level='WARNING'):
            built = workflow.build(work)
        self.assertEqual(workflow.build(work)['identity'], built['identity'])
        self.assertTrue((Path(built['build']) / 'Sub/S01E01.ass').is_file())

    def test_replaced_stage_is_preserved_not_deleted(self):
        with self.assertLogs(level='WARNING'):
            with runtime.staging_directory(self.root, '.owned-') as stage:
                moved = self.root / 'original-stage'
                stage.rename(moved)
                stage.mkdir()
                (stage / 'unrelated.txt').write_text('Preserve me', encoding='utf-8')
        self.assertEqual((stage / 'unrelated.txt').read_text(encoding='utf-8'), 'Preserve me')
        self.assertTrue(moved.is_dir())

    def test_normal_staging_cleanup_remains_effective(self):
        with runtime.staging_directory(self.root, '.owned-') as stage:
            (stage / 'data').write_text('Authored payload', encoding='utf-8')
        self.assertFalse(stage.exists())

    def test_donor_global_mismatch_refuses_before_mutation(self):
        fields = ('Kerning', 'LayoutResX', 'LayoutResY', 'Language', 'YCbCr Matrix', 'Collisions', 'Timer')
        for field in fields:
            with self.subTest(field=field):
                base = parse(ass_source(headers=field + ': one'), 'ass')
                donor = parse(ass_source(headers=field + ': two'), 'ass')
                original = copy.deepcopy(base)
                with self.assertRaisesRegex(ValueError, 'settings') as caught:
                    workflow.merge_donor(base, donor, copy.deepcopy(donor.cues), 's0002', True)
                self.assertIn(field.lower(), str(caught.exception))
                self.assertEqual(base, original)

    def test_noncanonical_global_shadow_refuses_before_mutation(self):
        for shadow in ('kerning: no', 'Kerning : no', 'KERNING: no'):
            base = parse(ass_source(headers='Kerning: yes\n' + shadow), 'ass')
            donor = parse(ass_source(headers='Kerning: no'), 'ass')
            original, cues = copy.deepcopy(base), copy.deepcopy(donor.cues)
            with self.assertRaisesRegex(ValueError, 'settings'):
                workflow.merge_donor(base, donor, cues, 's0002', True)
            self.assertEqual(base, original)
            self.assertEqual(cues, donor.cues)
        base = parse(ass_source(headers='Kerning: yes'), 'ass')
        donor = parse(ass_source(headers='kerning: yes'), 'ass')
        original = copy.deepcopy(base)
        with self.assertRaisesRegex(ValueError, 'settings'):
            workflow.merge_donor(base, donor, copy.deepcopy(donor.cues), 's0002', True)
        self.assertEqual(base, original)

    def test_matching_globals_and_empty_donor_are_accepted(self):
        headers = 'Kerning: yes\nLayoutResX: 320\nLayoutResY: 180\nLanguage: fa\n'
        base, donor = parse(ass_source(headers=headers), 'ass'), parse(ass_source(headers=headers), 'ass')
        result = workflow.merge_donor(base, donor, copy.deepcopy(donor.cues), 's0002', True)
        self.assertEqual(result[0].fields['style'], 's0002__Default')
        donor = parse(ass_source(headers='Kerning: no'), 'ass')
        self.assertEqual(workflow.merge_donor(base, donor, [], 's0003', True), [])

    def test_old_recipe_preserves_sources_decisions_and_edition(self):
        work = self.workspace()
        with patch.dict(workflow.GENERATION_RECIPE, {'version': 4, 'normalization': 4}):
            old = Path(workflow.build(work)['build'])
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob('*') if p.is_file()}
        new = Path(workflow.build(work)['build'])
        self.assertNotEqual(old, new)
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})
        self.assertFalse((new / 'renders').exists())

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg integration tier')
    def test_real_preservation_has_identical_pixels(self):
        for index, source in enumerate((r'A\{Important}B', 'A{comment{inner}VISIBLE}B',
                r'\{\rLiteral} then {\rDefault}Hi', r'{\p1}m 0 0 l 20 0 20 20{\r}m 30 0 l 50 0 50 20',
                r'{\t(\p1)}m 0 0 l 20 0 20 20',
                r'{\p1}m 0 0 l 20 0{comment}l 20 20 0 20{\p0}',
                r'{\p1}m 0 0 l 20 0 20 20\{\p0}Hello',
                r'{\t(\ p1)}m 0 0 l 20 0 20 20',
                r'{\t(\p+1)}m 0 0 l 20 0 20 20')):
            with self.subTest(index=index):
                doc = parse(ass_source(source), 'ass')
                kept, _ = workflow.reviewed_cues(doc, [row(source)])
                self.assertEqual(self.pixels('source' + str(index), ass_source(source)),
                                 self.pixels('output' + str(index), serialize(doc, kept)))

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg integration tier')
    def test_real_vector_mutations_change_pixels_and_require_review(self):
        for index, source in enumerate((r'{\p1}m 0 0 l 20 0 20 20{\r}m 30 0 l 50 0 50 20',
                                        r'{\t(\p1)}m 0 0 l 20 0 20 20',
                                        r'{\p1\t(0,1,2,3,\p0)}m 0 0 l 20 0 20 20')):
            changed = source.replace('l 50', 'l 70') if r'{\r}' in source else source.replace('l 20', 'l 70')
            self.assertNotEqual(self.pixels('before' + str(index), ass_source(source)),
                                self.pixels('after' + str(index), ass_source(changed)))
            with self.assertRaisesRegex(ValueError, 'tags/drawings'):
                workflow.reviewed_cues(parse(ass_source(source), 'ass'), [row(source, action='edit', text=changed)])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg integration tier')
    def test_real_kerning_difference_cannot_be_silently_merged(self):
        original = ass_source('AVAVAV', 'Kerning: no')
        other = ass_source('AVAVAV', 'Kerning: yes')
        # Use the same native font families as the existing equipped renderer lanes.
        self.assertNotEqual(self.pixels('kerning_no', original), self.pixels('kerning_yes', other))
        base, donor = parse(other, 'ass'), parse(original, 'ass')
        with self.assertRaisesRegex(ValueError, 'kerning'):
            workflow.merge_donor(base, donor, copy.deepcopy(donor.cues), 's0002', True)

    def test_layout_resolution_mismatch_requires_deliberate_alignment(self):
        first = ass_source(r'{\blur3}Text', 'LayoutResX: 320\nLayoutResY: 180')
        second = ass_source(r'{\blur3}Text', 'LayoutResX: 640\nLayoutResY: 360')
        base, donor = parse(first, 'ass'), parse(second, 'ass')
        with self.assertRaisesRegex(ValueError, 'layoutres'):
            workflow.merge_donor(base, donor, copy.deepcopy(donor.cues), 's0002', True)


if __name__ == '__main__':
    with runtime.operational_log('audit-preservation'):
        unittest.main(verbosity=2)
