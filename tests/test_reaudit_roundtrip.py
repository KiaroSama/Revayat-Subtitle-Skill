"""Authored donor-conversion and transactional-publication regressions."""
from __future__ import annotations

import copy
from contextlib import contextmanager
import json
import logging
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / 'skills/revayat-subtitle/scripts'
sys.path.insert(0, str(SCRIPTS))
import runtime
import workflow
import publication
import markup
from subtitle_formats import (ASS_FIELDS, STYLE_FIELDS, DEFAULT_STYLE, Cue,
                              parse, serialize, srt_to_ass, visible)

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'


def ass_source(text='Hello.', headers=''):
    return ('[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n' + headers +
            '\n[V4+ Styles]\nFormat: ' + STYLE_FIELDS + '\nStyle: ' + DEFAULT_STYLE +
            '\n[Events]\nFormat: ' + ASS_FIELDS +
            '\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,' + text + '\n')


class RoundtripTests(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / '.scratch/checks'
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='subtitle-roundtrip-', dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        logging.info('Roundtrip regression case=%s', self.id())

    def source(self, name='input.srt', text='Hello.'):
        path = self.root / name
        path.write_text('1\n00:00:01,000 --> 00:00:03,000\n' + text + '\n', encoding='utf-8')
        return path

    def workspace(self, count=1):
        sources = [self.source(f'input{i}.srt', f'Hello {i}.') for i in range(1, count + 1)]
        work = self.root / 'work'
        workflow.prepare(sources, work, 'Fixture', 1, 'utf-8', None, 'en')
        project = runtime.read_json(work / 'project.json')
        project['episodes'] = [{'id': f'S01E{i:02}', 'base': f's{i:04}',
                                'comparison': 'Only authored source'} for i in range(1, count + 1)]
        runtime.write_json(work / 'project.json', project)
        for i in range(1, count + 1):
            path = work / f'worksheets/s{i:04}.json'
            rows = runtime.read_json(path)
            for row in rows:
                row.update(reviewed=True, action='preserve')
            runtime.write_json(path, rows)
        runtime.write_json(work / 'glossary.json', {'series': 'Fixture', 'terms_reviewed': True,
            'terms': [], 'research': [{'url': 'https://example.org', 'note': 'Authored fixture, not anime research'}]})
        return work

    @contextmanager
    def write_fault(self, folder, mode, nth=1):
        real_open, writes = Path.open, 0
        class Faulty:
            def __init__(self, handle):
                self.handle = handle
            def __getattr__(self, name):
                return getattr(self.handle, name)
            def __enter__(self):
                return self
            def __exit__(self, *args):
                self.handle.close()
                if mode == 'close':
                    raise OSError('Injected close failure')
            def write(self, data):
                if mode == 'short':
                    return self.handle.write(data[:13])
                if mode == 'corrupt':
                    altered = bytes([data[0] ^ 1]) + data[1:]
                    self.handle.write(altered)
                    return len(data)
                return self.handle.write(data)
            def flush(self):
                if mode == 'flush':
                    raise OSError('Injected flush failure')
                return self.handle.flush()
        def opening(path, mode='r', *args, **kwargs):
            nonlocal writes
            handle = real_open(path, mode, *args, **kwargs)
            if mode in {'wb', 'xb'} and folder in path.parts:
                writes += 1
                if writes == nth:
                    return Faulty(handle)
            return handle
        with patch.object(Path, 'open', opening):
            yield

    def test_unknown_ascii_markup_donors_are_explicitly_refused(self):
        for tag in ('small', 'foo', '3', 'break', 'ruby', 'span', '_name', 'foo/bar'):
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, 'markup'):
                srt_to_ass(Cue('c000001', 1000, 3000, 'A<' + tag + '>B</' + tag + '>C'))

    def test_unknown_closing_tag_donor_is_refused(self):
        with self.assertRaisesRegex(ValueError, 'markup'):
            srt_to_ass(Cue('c000001', 1000, 3000, 'A</unknown>B'))

    def test_attributed_and_space_prefixed_controls_require_adaptation(self):
        for text in ('A<i class="x">B</i>C', 'A< b>B</b>', 'A<font face="Arial">B</font>',
                     'A<font color="red">B</font>', 'A<font\tface="Arial">B</font>'):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, 'markup'):
                srt_to_ass(Cue('c000001', 1000, 3000, text))

    def test_closing_break_variants_become_ass_linebreaks(self):
        for tag in ('</br>', '</BR>', '</br/>', '</br >', '</br/ >', '</br a="b">'):
            with self.subTest(tag=tag):
                source = 'A' + tag + 'B'
                self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, source)).text, r'A\NB')
                self.assertEqual(visible(source, 'srt'), 'A\nB')

    def test_closing_breaks_receive_independent_persian_direction_marks(self):
        source = 'سلام</br>دنیا OVA'
        normalized = markup.rtl(source, 'srt')
        self.assertEqual(normalized.count(markup.RLM), 4)
        self.assertEqual(visible(normalized, 'srt'), 'سلام\nدنیا OVA')
        self.assertEqual(markup.rtl(normalized, 'srt'), normalized)

    def test_literal_angles_and_entities_stay_literal(self):
        for source in ('x < 5 > y', 'x <۵> y', 'A<<foo>>B', '&amp; &#65;', 'A<\tb>B',
                       'A<br\t>B', 'A<br\u00a0>B', 'A<unfinished'):
            with self.subTest(source=source):
                self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, source)).text, source)

    def test_supported_binary_formatting_is_not_changed_to_a_stack(self):
        source = 'A<b>B<b>C</b>D</b>E'
        expected = r'A{\b1}B{\b1}C{\b0}D{\b0}E'
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, source)).text, expected)
        for tag in ('i', 'u', 's'):
            text = f'<{tag}>A</{tag}>'
            expected = '{\\' + tag + '1}A{\\' + tag + '0}'
            self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, text)).text, expected)

    def test_direct_donor_conversion_removes_only_completed_comments(self):
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, 'A<!-- hidden\ncomment -->B')).text, 'AB')
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, 'A<!-- unfinished')).text, 'A<!-- unfinished')

    def test_empty_decoder_tags_require_explicit_adaptation(self):
        for source in ('A<>B', 'A</>B', 'A<//>B'):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, 'markup'):
                srt_to_ass(Cue('c000001', 1000, 3000, source))
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, 'A<<>>B')).text, 'A<<>>B')

    def test_completed_comment_controls_do_not_block_conversion(self):
        source = r'A<!-- {\b1} hidden \N -->B'
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, source)).text, 'AB')
        with self.assertRaisesRegex(ValueError, 'control syntax'):
            srt_to_ass(Cue('c000001', 1000, 3000, r'A<!-- unfinished {\b1}'))

    def test_native_tag_byte_limit_leaves_long_breaks_literal(self):
        for prefix in ('<br ', '</br ', '<br/ '):
            for attributes in ('x' * 128, 'ی' * 64):
                source = 'A' + prefix + attributes + '>B'
                with self.subTest(prefix=prefix, unicode=attributes.startswith('ی')):
                    self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, source)).text, source)
                    self.assertEqual(visible(source, 'srt'), source)
        self.assertEqual(srt_to_ass(Cue('c000001', 1000, 3000, 'A<br ' + 'x' * 124 + '>B')).text, r'A\NB')

    def test_plain_visibility_contract_is_not_silently_broadened(self):
        self.assertEqual(visible('A<small>B</small>C', 'srt'), 'A<small>B</small>C')
        self.assertEqual(visible('A<break>B', 'srt'), 'A<break>B')

    def test_unsupported_donor_refusal_preserves_source_input(self):
        cue = Cue('c000001', 1001, 3009, 'A<small>B</small>C')
        before = copy.deepcopy(cue)
        with self.assertRaises(ValueError):
            srt_to_ass(cue)
        self.assertEqual(cue, before)

    def test_timing_quantization_and_control_syntax_guards_remain(self):
        cue = srt_to_ass(Cue('c000001', 1001, 3009, 'A\r\nB'))
        self.assertEqual((cue.start, cue.end, cue.text), (1000, 3000, r'A\NB'))
        with self.assertRaisesRegex(ValueError, 'collapses'):
            srt_to_ass(Cue('c000001', 1001, 1009, 'A'))
        for source in (r'{\b1}A', r'A\N B'):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, 'control syntax'):
                srt_to_ass(Cue('c000001', 1000, 3000, source))

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'POSIX named-pipe regression')
    def test_explicit_zip_fifo_is_refused_without_waiting(self):
        pipe = self.root / 'input.zip'
        os.mkfifo(pipe)
        code = ('import sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);from workflow import inputs;'
                '\ntry: list(inputs([Path(sys.argv[2])]))'
                '\nexcept ValueError as e: sys.exit(0 if "regular file" in str(e) else 4)'
                '\nelse: sys.exit(3)')
        runtime.run([sys.executable, '-S', '-c', code, str(SCRIPTS), str(pipe)],
                    timeout=2, idle_timeout=2, max_output=65536)

    def test_special_zip_refused_before_zipfile_is_opened(self):
        source = self.root / 'input.zip'
        source.write_bytes(b'authored fixture')
        real = Path.lstat
        def special(path, *args, **kwargs):
            result = real(path, *args, **kwargs)
            if path == source:
                from types import SimpleNamespace
                return SimpleNamespace(st_mode=stat.S_IFCHR | 0o600,
                                       st_file_attributes=getattr(result, 'st_file_attributes', 0))
            return result
        with patch.object(Path, 'lstat', special), patch.object(workflow.zipfile, 'ZipFile') as archive:
            with self.assertRaisesRegex(ValueError, 'regular file'):
                list(workflow.inputs([source]))
        archive.assert_not_called()

    def test_supported_zip_compressions_preserve_bytes(self):
        raw = self.source().read_bytes()
        for compression in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2, zipfile.ZIP_LZMA):
            with self.subTest(compression=compression):
                source = self.root / f'stored-{compression}.zip'
                with zipfile.ZipFile(source, 'w', compression=compression) as archive:
                    archive.writestr('nested/episode.srt', raw)
                work = self.root / f'work-{compression}'
                workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
                self.assertEqual((work / 'sources/s0001.srt').read_bytes(), raw)
                self.assertEqual(workflow.load(work)[1]['s0001'].cues[0].text, 'Hello.')

    def test_archive_path_and_budget_guards_remain(self):
        source = self.root / 'unsafe.zip'
        with zipfile.ZipFile(source, 'w') as archive:
            archive.writestr('../escape.srt', self.source().read_bytes())
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            list(workflow.inputs([source]))
        safe = self.root / 'safe.zip'
        with zipfile.ZipFile(safe, 'w') as archive:
            archive.writestr('safe.srt', self.source().read_bytes())
        with patch.object(workflow, 'MAX_FILE', 10), self.assertRaisesRegex(ValueError, 'limits'):
            list(workflow.inputs([safe]))

    def test_short_source_write_refused_before_workspace_publication_and_retry(self):
        source, work = self.source(), self.root / 'work'
        original = source.read_bytes()
        with self.write_fault('sources', 'short'), self.assertRaisesRegex(OSError, 'incomplete'):
            workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
        self.assertFalse(work.exists())
        self.assertEqual(source.read_bytes(), original)
        workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
        self.assertEqual((work / 'sources/s0001.srt').read_bytes(), original)
        workflow.load(work)

    def test_corrupt_source_readback_is_refused(self):
        source, work = self.source(), self.root / 'work'
        with self.write_fault('sources', 'corrupt'), self.assertRaisesRegex(OSError, 'bytes changed'):
            workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
        self.assertFalse(work.exists())

    def test_late_source_failure_does_not_publish_earlier_sources(self):
        sources, work = [self.source('a.srt'), self.source('b.srt')], self.root / 'work'
        with self.write_fault('sources', 'short', nth=2), self.assertRaises(OSError):
            workflow.prepare(sources, work, 'Fixture', 1, 'utf-8', None, 'en')
        self.assertFalse(work.exists())
        self.assertEqual(list(self.root.glob('.subtitle-import-*')), [])

    def test_import_flush_and_close_failures_preserve_retryability(self):
        source = self.source()
        for failure in ('flush', 'close'):
            with self.subTest(failure=failure):
                work = self.root / failure
                with self.write_fault('sources', failure), self.assertRaisesRegex(OSError, 'Injected'):
                    workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
                self.assertFalse(work.exists())
                workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
                workflow.load(work)

    def test_import_file_fsync_failure_is_not_reported_success(self):
        source, work = self.source(), self.root / 'work'
        with patch.object(publication.os, 'fsync', side_effect=OSError('Injected fsync')), self.assertRaisesRegex(OSError, 'fsync'):
            workflow.prepare([source], work, 'Fixture', 1, 'utf-8', None, 'en')
        self.assertFalse(work.exists())

    def test_short_edition_write_refused_and_retry_preserves_reviews(self):
        work = self.workspace()
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob('*') if p.is_file()}
        with self.write_fault('Sub', 'short'), self.assertRaisesRegex(OSError, 'incomplete'):
            workflow.build(work)
        self.assertEqual(list((work / 'builds').iterdir()), [])
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})
        result = workflow.build(work)
        self.assertEqual(workflow.build(work)['identity'], result['identity'])
        raw = (Path(result['build']) / 'Sub/S01E01.srt').read_bytes()
        self.assertEqual(runtime.digest(raw), result['episodes'][0]['sha256'])

    def test_edition_readback_corruption_is_refused(self):
        work = self.workspace()
        with self.write_fault('Sub', 'corrupt'), self.assertRaisesRegex(OSError, 'bytes changed'):
            workflow.build(work)
        self.assertEqual(list((work / 'builds').iterdir()), [])

    def test_late_episode_failure_keeps_entire_edition_unpublished(self):
        work = self.workspace(count=2)
        with self.write_fault('Sub', 'short', nth=2), self.assertRaises(OSError):
            workflow.build(work)
        self.assertEqual(list((work / 'builds').iterdir()), [])
        result = workflow.build(work)
        self.assertEqual(len(result['episodes']), 2)

    def test_edition_flush_and_close_failures_preserve_retryability(self):
        work = self.workspace()
        for failure in ('flush', 'close'):
            with self.subTest(failure=failure):
                with self.write_fault('Sub', failure), self.assertRaisesRegex(OSError, 'Injected'):
                    workflow.build(work)
                self.assertEqual(list((work / 'builds').iterdir()), [])
        result = workflow.build(work)
        self.assertEqual(workflow.build(work)['identity'], result['identity'])

    def test_existing_workspace_and_edition_are_not_replaced(self):
        work = self.workspace()
        original = (work / 'sources/s0001.srt').read_bytes()
        with self.assertRaisesRegex(ValueError, 'already exists'):
            workflow.prepare([self.source('extra.srt')], work, 'Fixture', 1, 'utf-8', None, 'en')
        result = workflow.build(work)
        path = Path(result['build']) / 'Sub/S01E01.srt'
        before = path.read_bytes()
        self.assertEqual(workflow.build(work)['identity'], result['identity'])
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual((work / 'sources/s0001.srt').read_bytes(), original)

    def test_unsupported_donor_refusal_leaves_no_added_default_style(self):
        base = parse(ass_source(), 'ass')
        base.styles['Base'] = base.styles.pop('Default')
        base.styles['Base'][base.style_fields.index('name')] = 'Base'
        base.cues[0].fields['style'] = 'Base'
        before = copy.deepcopy(base)
        donor = parse('1\n00:00:01,000 --> 00:00:03,000\n<font face="Arial">A</font>\n', 'srt')
        with self.assertRaises(ValueError):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(base, before)

    def test_undefined_reset_refusal_preserves_both_input_objects(self):
        base, donor = parse(ass_source(), 'ass'), parse(ass_source(r'{\rMissing}Hi'), 'ass')
        before_base, before_cues = copy.deepcopy(base), copy.deepcopy(donor.cues)
        with self.assertRaisesRegex(ValueError, 'undefined style'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(base, before_base)
        self.assertEqual(donor.cues, before_cues)
        donor.cues[0].text = 'Corrected'
        merged = workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(merged[0].fields['style'], 's0002__Default')

    def test_font_conflict_refusal_does_not_leave_namespaced_styles(self):
        base, donor = parse(ass_source(), 'ass'), parse(ass_source(), 'ass')
        base.sections.append(('[Fonts]', ['fontname: a.ttf', 'AAAA']))
        donor.sections.append(('[Fonts]', ['fontname: b.ttf', 'BBBB']))
        before = copy.deepcopy(base)
        with self.assertRaisesRegex(ValueError, 'font sets'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(base, before)
        merged = workflow.merge_donor(base, donor, donor.cues, 's0002', False)
        self.assertEqual(merged[0].fields['style'], 's0002__Default')

    def test_successful_merge_does_not_mutate_donor_cues(self):
        base, donor = parse(ass_source(), 'ass'), parse(ass_source(r'{\rDefault}Hi'), 'ass')
        before = copy.deepcopy(donor)
        result = workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(donor, before)
        self.assertEqual(result[0].text, r'{\rs0002__Default}Hi')
        self.assertIn('s0002__Default', base.styles)
        parse(serialize(base, base.cues + result), 'ass')

    def test_recipe_upgrade_preserves_old_edition_and_editorial_work(self):
        work = self.workspace()
        with patch.dict(workflow.GENERATION_RECIPE, {'version': 7, 'normalization': 7}):
            old = Path(workflow.build(work)['build'])
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob('*') if p.is_file()}
        new = Path(workflow.build(work)['build'])
        self.assertNotEqual(old, new)
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})
        self.assertFalse((new / 'renders').exists())

    def test_unterminated_break_whitespace_does_not_rescan_partitions(self):
        code = ('import sys;sys.path.insert(0,sys.argv[1]);import markup;'
                'text="<br"+" "*50000+"X";'
                'assert markup.SRT_BREAK.search(text) is None;'
                'assert markup.SRT_BREAK.fullmatch(text) is None;'
                'assert markup.uncomment(text,"srt")==text')
        runtime.run([sys.executable, '-S', '-c', code, str(SCRIPTS)],
                    timeout=8, idle_timeout=8, max_output=65536)

    def test_break_dialect_still_accepts_ascii_space_attributes_only(self):
        for count in (1, 2, 10, 100):
            for opening in ('<br', '</br', '<BR/', '</BR/'):
                text = opening + ' ' * count + 'data="authored"' + '>'
                with self.subTest(count=count, opening=opening):
                    self.assertIsNotNone(markup.SRT_BREAK.fullmatch(text))
        for text in ('<br\t>', '<br\u00a0>', '</br\t>', '</br\u00a0>'):
            self.assertIsNone(markup.SRT_BREAK.fullmatch(text))

    def test_middle_closing_break_is_in_default_render_coverage(self):
        from render import sample_plan
        from subtitle_formats import Document
        doc = Document('srt', [Cue('a', 1000, 2000, 'A'),
                               Cue('b', 3000, 4000, 'B</br>C'),
                               Cue('c', 5000, 6000, 'D')])
        self.assertIn(3.5, [sample['time_seconds'] for sample in sample_plan(doc, False)])

    def test_failed_late_style_collision_keeps_base_unmodified(self):
        base, donor = parse(ass_source(), 'ass'), parse(ass_source(), 'ass')
        donor.styles['Other'] = donor.styles['Default'].copy()
        donor.styles['Other'][donor.style_fields.index('name')] = 'Other'
        base.styles['s0002__Other'] = base.styles['Default'].copy()
        base.styles['s0002__Other'][base.style_fields.index('name')] = 's0002__Other'
        before = copy.deepcopy(base)
        with self.assertRaisesRegex(ValueError, 'collision'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertEqual(base, before)

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg integration tier')
    def test_real_mixed_build_render_qa_and_zip_after_donor_repair(self):
        import render
        first, second = self.root / 'base.ass', self.source('donor.srt', 'A<small>B</small>C')
        first.write_text(ass_source('BASE'), encoding='utf-8')
        work = self.root / 'work'
        workflow.prepare([first, second], work, 'Fixture', 1, 'utf-8', None, 'en')
        originals = {p.name: p.read_bytes() for p in (work / 'sources').iterdir()}
        project = runtime.read_json(work / 'project.json')
        project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'alternates': ['s0002'],
                                'comparison': 'Authored base plus retained donor content'}]
        runtime.write_json(work / 'project.json', project)
        runtime.write_json(work / 'glossary.json', {'series': 'Fixture', 'terms_reviewed': True,
            'terms': [], 'research': [{'url': 'https://example.org', 'note': 'Authored mechanical test'}]})
        for key in ('s0001', 's0002'):
            path = work / f'worksheets/{key}.json'
            rows = runtime.read_json(path)
            for row in rows:
                row.update(reviewed=True, action='preserve')
            runtime.write_json(path, rows)
        with self.assertRaisesRegex(ValueError, 'markup'):
            workflow.build(work)
        self.assertFalse((work / 'builds').exists())
        path = work / 'worksheets/s0002.json'
        rows = runtime.read_json(path)
        rows[0].update(action='edit', text='ABC',
                       structure_note='Explicit source-to-renderer reconciliation in an authored fixture')
        runtime.write_json(path, rows)
        result = workflow.build(work)
        edition = Path(result['build'])
        self.assertEqual(result['episodes'][0]['cues'], 2)
        self.assertEqual(originals, {p.name: p.read_bytes() for p in (work / 'sources').iterdir()})
        review = render.render(edition, 'S01E01', None, None, None, True)
        with self.assertRaisesRegex(ValueError, 'Inspect every'):
            render.qa(edition)
        path = Path(review['review'])
        evidence = runtime.read_json(path)
        for frame in evidence['frames']:
            frame.update(reviewed=True, note='Automated gate fixture; not human visual signoff')
        runtime.write_json(path, evidence)
        self.assertTrue(render.qa(edition)['ok'])
        output = self.root / 'Sub.zip'
        render.package(edition, output)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(archive.namelist(), ['Sub/S01E01.ass'])
            self.assertEqual(runtime.digest(archive.read('Sub/S01E01.ass')), result['episodes'][0]['sha256'])
        copy_output = self.root / 'Sub-again.zip'
        render.package(edition, copy_output)
        self.assertEqual(output.read_bytes(), copy_output.read_bytes())
        changed = edition / evidence['frames'][0]['file']
        changed.write_bytes(b'not an image')
        with self.assertRaisesRegex(ValueError, 'image changed'):
            render.package(edition, self.root / 'invalid.zip')
        self.assertFalse((self.root / 'invalid.zip').exists())

    def native_conversion(self, source):
        executable = os.environ.get('REVAYAT_FFMPEG') or shutil.which('ffmpeg')
        self.assertTrue(executable, 'Explicit rendering tier requires FFmpeg')
        input_path = self.source('native.srt', source)
        output = runtime.run([executable, '-hide_banner', '-loglevel', 'error', '-nostdin', '-y',
            '-i', str(input_path), '-c:s', 'ass', '-f', 'ass', 'pipe:1'],
            timeout=15, idle_timeout=15, max_output=1024 * 1024)
        return parse(output.decode('utf-8'), 'ass')

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg differential tier')
    def test_real_decoder_markup_differential_corpus(self):
        examples = ['A<small>B</small>C', 'A<foo>B</foo>C', 'A<3>B', 'A<break>B', 'A</foo>B',
                    'A</br>B', 'A</BR/>B', 'A<br x="y">B', 'x < 5 > y', 'x <۵> y',
                    'A<<foo>>B', 'A<<b>B', 'A<b>B<b>C</b>D</b>E', 'A<i>B</i>C',
                    'A<u>B</u>C', 'A<s>B</s>C', 'A<br\t>B', 'A<br\u00a0>B', 'A<\tb>B',
                    '&amp; &#65;', 'Hello.', 'سلام OVA', 'A<>B', 'A</>B',
                    'A<br ' + 'x' * 124 + '>B', 'A<br ' + 'x' * 125 + '>B',
                    'A</br ' + 'x' * 124 + '>B', 'A<br ' + 'ی' * 64 + '>B']
        for source in examples:
            with self.subTest(source=source):
                oracle = self.native_conversion(source)
                try:
                    actual = srt_to_ass(Cue('c000001', 1000, 3000, source))
                except ValueError as error:
                    self.assertIn('markup', str(error))
                    continue
                self.assertEqual(actual.text, oracle.cues[0].text)

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg differential tier')
    def test_real_closing_break_pixels_match_native_decoder(self):
        oracle = self.native_conversion('A</br>B')
        converted = srt_to_ass(Cue('c000001', 1000, 3000, 'A</br>B'))
        converted.fields = dict(oracle.cues[0].fields)
        converted.start, converted.end = oracle.cues[0].start, oracle.cues[0].end
        executable = os.environ.get('REVAYAT_FFMPEG') or shutil.which('ffmpeg')
        def pixels(name, cues):
            (self.root / (name + '.ass')).write_text(serialize(oracle, cues), encoding='utf-8')
            output = runtime.run([executable, '-hide_banner', '-loglevel', 'error', '-nostdin',
                '-f', 'lavfi', '-i', 'color=s=320x180:r=1:d=1', '-filter_threads', '1', '-vf',
                f'setpts=PTS+1.5/TB,ass={name}.ass', '-frames:v', '1', '-threads', '1',
                '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], cwd=self.root,
                timeout=15, idle_timeout=15, max_output=320 * 180 * 3 + 65536)
            self.assertEqual(len(output), 320 * 180 * 3)
            self.assertGreater(max(output), 32)
            return runtime.digest(output)
        self.assertEqual(pixels('native', oracle.cues), pixels('converted', [converted]))


if __name__ == '__main__':
    logging.basicConfig(level=logging.INFO, format='[%(asctime)s UTC] [%(levelname)s] %(message)s')
    logging.Formatter.converter = time.gmtime
    logging.info('Starting authored regression suite; fault tests intentionally emit errors')
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(RoundtripTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    logging.log(logging.INFO if result.wasSuccessful() else logging.ERROR,
                'Suite finished tests=%d failures=%d errors=%d skipped=%d',
                result.testsRun, len(result.failures), len(result.errors), len(result.skipped))
    sys.exit(0 if result.wasSuccessful() else 1)
