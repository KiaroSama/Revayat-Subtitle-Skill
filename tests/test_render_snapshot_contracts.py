"""Verified render snapshots, cleanup outcome, and exact track-value contracts."""
from __future__ import annotations

import copy
from contextlib import contextmanager
import logging
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'skills/revayat-subtitle/scripts'))
import publication
import render
import runtime
import subtitle_formats as formats
import workflow
from test_ass_track_contracts import TrackWorkspace, source

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'


class SnapshotTests(TrackWorkspace):
    def build_one(self):
        return Path(self.edition([source(text='VISIBLE AVAVAV')])['build'])

    def episode(self, edition):
        return runtime.read_json(edition / 'manifest.json')['episodes'][0]

    def inspect_fixture(self, evidence):
        record_path = Path(evidence['review'])
        record = runtime.read_json(record_path)
        for frame in record['frames']:
            frame.update(reviewed=True, note='Authored mechanical test only; not human visual or language approval.')
        runtime.write_json(record_path, record)
        return record

    def render_one(self, edition):
        return render.render(edition, 'S01E01', None, None, None, False)

    def fail_render_cleanup(self, function):
        def wrapped(path, *args, **kwargs):
            if Path(path).name.startswith('.render-'):
                raise OSError('Injected render scratch cleanup failure')
            return function(path, *args, **kwargs)
        return wrapped

    @contextmanager
    def cleanup_fault(self):
        # tempfile can bind rmtree at import time; cover that boundary as well
        # as the shared helper without depending on a CPython minor version.
        with patch.object(render.shutil, 'rmtree', side_effect=self.fail_render_cleanup(render.shutil.rmtree)), patch.object(
                render.tempfile.TemporaryDirectory, '_rmtree',
                side_effect=self.fail_render_cleanup(render.tempfile.TemporaryDirectory._rmtree)):
            yield

    def test_snapshot_accepts_exact_ass_and_srt_bytes(self):
        for kind, data in [('ass', source(text='سلام OVA').encode('utf-8')),
                           ('srt', '1\n00:00:01,000 --> 00:00:02,000\nسلام OVA\n'.encode('utf-8'))]:
            with self.subTest(kind=kind):
                path = self.root / 'Sub' / ('S01E01.' + kind)
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(data)
                episode = {'file': path.relative_to(self.root).as_posix(), 'sha256': runtime.digest(data)}
                self.assertEqual(render.subtitle_snapshot(self.root, episode), data)

    def test_snapshot_refuses_hash_drift_without_echoing_content(self):
        edition = self.build_one()
        episode = self.episode(edition)
        path = edition / episode['file']
        path.write_bytes(b'private-sentinel-caption')
        with self.assertRaisesRegex(ValueError, 'snapshot') as caught:
            render.subtitle_snapshot(edition, episode)
        self.assertNotIn('private-sentinel-caption', str(caught.exception))

    def test_snapshot_has_an_explicit_read_budget(self):
        edition = self.build_one()
        episode = self.episode(edition)
        original = (edition / episode['file']).read_bytes()
        with patch.object(render, 'read_limited', wraps=runtime.read_limited) as read:
            self.assertEqual(render.subtitle_snapshot(edition, episode), original)
        read.assert_called_once_with((edition / episode['file']).resolve(), render.MAX_SUBTITLE_BYTES)

    def test_snapshot_refuses_oversize_and_preserves_source(self):
        edition = self.build_one()
        episode = self.episode(edition)
        before = (edition / episode['file']).read_bytes()
        with patch.object(render, 'MAX_SUBTITLE_BYTES', 2):
            with self.assertRaisesRegex(ValueError, 'byte limit'):
                render.subtitle_snapshot(edition, episode)
        self.assertEqual((edition / episode['file']).read_bytes(), before)

    def test_snapshot_refuses_directory_instead_of_subtitle(self):
        edition = self.build_one()
        episode = self.episode(edition)
        path = edition / episode['file']
        path.unlink()
        path.mkdir()
        with self.assertRaisesRegex(ValueError, 'regular'):
            render.subtitle_snapshot(edition, episode)

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'POSIX named-pipe snapshot boundary')
    def test_snapshot_refuses_fifo_before_open(self):
        edition = self.build_one()
        episode = self.episode(edition)
        path = edition / episode['file']
        path.unlink()
        os.mkfifo(path)
        with self.assertRaisesRegex(ValueError, 'regular'):
            render.subtitle_snapshot(edition, episode)

    def test_snapshot_refuses_workspace_escape(self):
        with self.assertRaisesRegex(ValueError, 'canonical'):
            render.subtitle_snapshot(self.root, {'file': '../outside.ass', 'sha256': '0' * 64})

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg snapshot integration tier')
    def test_unverified_input_copy_cannot_change_attested_caption(self):
        edition = self.build_one()
        expected = self.render_one(edition)
        original_copy = render.shutil.copyfile
        def corrupt_old_copy(src, dst, *args, **kwargs):
            result = original_copy(src, dst, *args, **kwargs)
            if Path(dst).name == 'input.ass':
                payload = Path(dst).read_bytes()
                self.assertIn(b'VISIBLE', payload)
                Path(dst).write_bytes(payload.replace(b'VISIBLE', b'ALTERED'))
            return result
        with patch.object(render.shutil, 'copyfile', side_effect=corrupt_old_copy):
            actual = self.render_one(edition)
        self.assertEqual(actual['frames'][0]['sha256'], expected['frames'][0]['sha256'],
                         'A receipt must not attest a changed staged caption as the immutable subtitle.')
        self.assertEqual(runtime.digest((edition / self.episode(edition)['file']).read_bytes()),
                         self.episode(edition)['sha256'])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg snapshot integration tier')
    def test_unverified_frame_copy_cannot_create_an_invalid_receipt(self):
        edition = self.build_one()
        original_copy = render.shutil.copyfile
        def corrupt_old_copy(src, dst, *args, **kwargs):
            result = original_copy(src, dst, *args, **kwargs)
            path = Path(dst)
            if path.name.startswith('frame-') and path.parent.parent.name == 'renders':
                path.write_bytes(path.read_bytes() + b'corrupt-copy')
            return result
        with patch.object(render.shutil, 'copyfile', side_effect=corrupt_old_copy):
            evidence = self.render_one(edition)
        for frame in evidence['frames']:
            raw = (edition / frame['file']).read_bytes()
            self.assertEqual(runtime.digest(raw), frame['sha256'])
            self.assertEqual(render.decode_png(raw), (frame['width'], frame['height']))

    def test_staging_hash_drift_stops_before_render_invocation(self):
        edition = self.build_one()
        episode = self.episode(edition)
        path = edition / episode['file']
        original = path.read_bytes()
        frame_commands = []
        def simulate_version_then_change(command, **kwargs):
            if '-version' in command:
                path.write_bytes(original.replace(b'VISIBLE', b'ALTERED'))
                return b'authored renderer fixture\n'
            frame_commands.append(command)
            self.fail('Changed subtitle was passed to a renderer')
        try:
            with patch.object(render, 'run', side_effect=simulate_version_then_change):
                with self.assertRaisesRegex(ValueError, 'snapshot'):
                    render.render(edition, 'S01E01', sys.executable, None, None, False)
        finally:
            path.write_bytes(original)
        self.assertEqual(frame_commands, [])
        self.assertFalse((edition / 'renders/S01E01.json').exists())
        self.assertEqual(workflow.build(self.work)['identity'], edition.name)

    def test_subtitle_publication_short_write_is_refused(self):
        edition = self.build_one()
        original_open = Path.open
        injected = []
        class ShortWriter:
            def __init__(self, handle):
                self.handle = handle
            def __enter__(self):
                return self
            def __getattr__(self, name):
                return getattr(self.handle, name)
            def write(self, data):
                injected.append(True)
                return self.handle.write(data[:-1])
            def __exit__(self, *args):
                self.handle.close()
        def open_with_short_write(path, mode='r', *args, **kwargs):
            handle = original_open(path, mode, *args, **kwargs)
            if mode == 'xb' and path.parent.name.startswith('.render-'):
                return ShortWriter(handle)
            return handle
        with patch.object(Path, 'open', open_with_short_write), patch.object(render, 'run', return_value=b'authored fixture\n'):
            with self.assertRaisesRegex(OSError, 'incomplete'):
                render.render(edition, 'S01E01', sys.executable, None, None, False)
        self.assertEqual(len(injected), 1)
        self.assertFalse((edition / 'renders/S01E01.json').exists())

    def test_subtitle_publication_readback_corruption_is_refused(self):
        edition = self.build_one()
        original_read = Path.read_bytes
        injected = []
        def corrupt_readback(path):
            data = original_read(path)
            if path.name.startswith('.publish-') and path.parent.name.startswith('.render-'):
                injected.append(True)
                return data + b'injected'
            return data
        with patch.object(Path, 'read_bytes', corrupt_readback), patch.object(render, 'run', return_value=b'authored fixture\n'):
            with self.assertRaisesRegex(OSError, 'Staged publication bytes changed'):
                render.render(edition, 'S01E01', sys.executable, None, None, False)
        self.assertEqual(len(injected), 1)
        self.assertFalse((edition / 'renders/S01E01.json').exists())

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg cleanup integration tier')
    def test_successful_render_survives_cleanup_failure(self):
        edition = self.build_one()
        with self.cleanup_fault():
            try:
                evidence = self.render_one(edition)
            except OSError as error:
                self.fail('Scratch cleanup replaced a successful rendering outcome: ' + type(error).__name__)
        self.assertTrue(list(edition.glob('.render-*')), 'Cleanup warning must retain a recoverable directory.')
        with self.assertRaisesRegex(ValueError, 'Inspect every'):
            render.qa(edition)
        self.inspect_fixture(evidence)
        self.assertTrue(render.qa(edition)['ok'])

    def test_cleanup_preserves_primary_exception_object(self):
        edition = self.build_one()
        primary = ValueError('Authored rendering failure')
        def fail_frame(command, **kwargs):
            if '-version' in command:
                return b'authored renderer fixture\n'
            raise primary
        caught = None
        with self.cleanup_fault(), patch.object(render, 'run', side_effect=fail_frame):
            try:
                render.render(edition, 'S01E01', sys.executable, None, None, False)
            except BaseException as error:
                caught = error
        self.assertIs(caught, primary)
        self.assertFalse((edition / 'renders/S01E01.json').exists())

    def test_cleanup_preserves_cancellation_object(self):
        edition = self.build_one()
        primary = KeyboardInterrupt('Authored cancellation')
        def fail_frame(command, **kwargs):
            if '-version' in command:
                return b'authored renderer fixture\n'
            raise primary
        caught = None
        with self.cleanup_fault(), patch.object(render, 'run', side_effect=fail_frame):
            try:
                render.render(edition, 'S01E01', sys.executable, None, None, False)
            except BaseException as error:
                caught = error
        self.assertIs(caught, primary)

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg snapshot integration tier')
    def test_failed_snapshot_keeps_previous_review_and_retry_works(self):
        edition = self.build_one()
        evidence = self.render_one(edition)
        self.inspect_fixture(evidence)
        before = {p.relative_to(edition): p.read_bytes() for p in edition.rglob('*') if p.is_file()}
        with patch.object(render, 'subtitle_snapshot', side_effect=ValueError('Invalid subtitle snapshot')):
            with self.assertRaisesRegex(ValueError, 'snapshot'):
                self.render_one(edition)
        self.assertEqual({p: (edition / p).read_bytes() for p in before}, before)
        self.assertTrue(render.qa(edition)['ok'])
        fresh = self.render_one(edition)
        self.assertNotEqual(fresh['frames'][0]['file'], evidence['frames'][0]['file'])
        self.assertTrue(all(frame['reviewed'] is False for frame in fresh['frames']))
        with self.assertRaisesRegex(ValueError, 'Inspect every'):
            render.qa(edition)

    def test_font_staging_uses_bounded_verified_bytes(self):
        edition = self.build_one()
        fonts = self.root / 'provided fonts'
        fonts.mkdir()
        font = fonts / 'authored.ttf'
        font_data = b'Authored font-byte fixture; not a real distributable font'
        font.write_bytes(font_data)
        primary = ValueError('Stop after verifying staging; no font rendering claimed')
        def inspect_stage(command, **kwargs):
            if '-version' in command:
                return b'authored renderer fixture\n'
            self.assertEqual((Path(kwargs['cwd']) / 'fonts/authored.ttf').read_bytes(), font_data)
            raise primary
        with patch.object(render, 'read_limited', wraps=runtime.read_limited) as read, patch.object(render, 'run', side_effect=inspect_stage):
            with self.assertRaises(ValueError) as caught:
                render.render(edition, 'S01E01', sys.executable, None, fonts, False)
        self.assertIs(caught.exception, primary)
        self.assertIn(((font, 32 * 1024 * 1024), {}), [(call.args, call.kwargs) for call in read.call_args_list])
        self.assertEqual(font.read_bytes(), font_data)

    def test_font_change_after_fingerprint_is_refused(self):
        edition = self.build_one()
        fonts = self.root / 'provided fonts'
        fonts.mkdir()
        font = fonts / 'authored.ttf'
        font.write_bytes(b'authored font snapshot')
        original_fingerprint = render.file_fingerprint
        def fingerprint_then_change(path, *args, **kwargs):
            info = original_fingerprint(path, *args, **kwargs)
            if path == font:
                font.write_bytes(b'changed font snapshot')
            return info
        with patch.object(render, 'file_fingerprint', side_effect=fingerprint_then_change), patch.object(render, 'run', return_value=b'authored renderer fixture\n') as commands:
            with self.assertRaisesRegex(ValueError, 'Font changed'):
                render.render(edition, 'S01E01', sys.executable, None, fonts, False)
        self.assertEqual(commands.call_count, 1, 'A changed font must not reach the frame renderer.')
        self.assertFalse((edition / 'renders/S01E01.json').exists())

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg frame-publication integration tier')
    def test_failed_frame_publication_keeps_previous_receipt(self):
        edition = self.build_one()
        evidence = self.render_one(edition)
        self.inspect_fixture(evidence)
        record_path = Path(evidence['review'])
        before = record_path.read_bytes()
        original_publish = render.publish_bytes
        def fail_frame(path, data):
            if Path(path).name.startswith('frame-'):
                raise OSError('Injected frame publication failure')
            return original_publish(path, data)
        with patch.object(render, 'publish_bytes', side_effect=fail_frame):
            with self.assertRaisesRegex(OSError, 'frame publication'):
                self.render_one(edition)
        self.assertEqual(record_path.read_bytes(), before)
        self.assertTrue(render.qa(edition)['ok'])
        self.inspect_fixture(self.render_one(edition))
        self.assertTrue(render.qa(edition)['ok'])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg snapshot integration tier')
    def test_older_sampler_requires_fresh_receipt_not_new_source_edits(self):
        edition = self.build_one()
        evidence = self.render_one(edition)
        record = self.inspect_fixture(evidence)
        receipt_path = edition / record['receipt_file']
        receipt = runtime.read_json(receipt_path)
        receipt['recipe']['sampler'] = render.SAMPLER_VERSION - 1
        runtime.write_json(receipt_path, receipt)
        record['receipt_sha256'] = runtime.digest(receipt_path.read_bytes())
        runtime.write_json(Path(evidence['review']), record)
        with self.assertRaisesRegex(ValueError, 'recipe changed'):
            render.qa(edition)
        before = {p.relative_to(self.work): p.read_bytes() for folder in ('sources', 'worksheets') for p in (self.work / folder).iterdir()}
        self.assertEqual(workflow.build(self.work)['identity'], edition.name)
        self.inspect_fixture(self.render_one(edition))
        self.assertTrue(render.qa(edition)['ok'])
        self.assertEqual({p: (self.work / p).read_bytes() for p in before}, before)

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg delivery integration tier')
    def test_package_uses_bounded_snapshot_and_exact_reviewed_bytes(self):
        edition = self.build_one()
        self.inspect_fixture(self.render_one(edition))
        output = self.root / 'verified.zip'
        with patch.object(render, 'subtitle_snapshot', wraps=render.subtitle_snapshot) as snapshot:
            render.package(edition, output)
        self.assertEqual(snapshot.call_count, 1)
        with zipfile.ZipFile(output) as archive:
            episode = self.episode(edition)
            self.assertEqual(archive.namelist(), [episode['file']])
            self.assertEqual(runtime.digest(archive.read(episode['file'])), episode['sha256'])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg delivery integration tier')
    def test_package_rechecks_source_before_archive_write(self):
        edition = self.build_one()
        self.inspect_fixture(self.render_one(edition))
        original_check = render.check_delivery
        episode = self.episode(edition)
        path = edition / episode['file']
        before = path.read_bytes()
        def change_after_qa(build):
            result = original_check(build)
            path.write_bytes(before.replace(b'VISIBLE', b'ALTERED'))
            return result
        output = self.root / 'refused.zip'
        try:
            with patch.object(render, 'check_delivery', side_effect=change_after_qa), patch.object(zipfile.ZipFile, 'writestr') as write:
                with self.assertRaisesRegex(ValueError, 'snapshot'):
                    render.package(edition, output)
                write.assert_not_called()
        finally:
            path.write_bytes(before)
        self.assertFalse(output.exists())
        render.package(edition, output)
        self.assertTrue(output.is_file())


class GlobalValueTests(TrackWorkspace):
    def pair(self, header, first, second):
        def document(value):
            text = source(headers='Kerning: yes')
            if header in ('PlayResX', 'PlayResY'):
                text = text.replace(header + (': 320' if header == 'PlayResX' else ': 180'), header + ': ' + value)
            elif header == 'Kerning':
                text = text.replace('Kerning: yes', 'Kerning: ' + value)
            else:
                text = source(headers=header + ': ' + value)
            return formats.parse(text, 'ass')
        return document(first), document(second)

    def test_unicode_header_value_difference_cannot_be_merged(self):
        base, donor = self.pair('Kerning', 'yes', '\u00a0yes')
        with self.assertRaisesRegex(ValueError, 'track settings'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)

    def test_unicode_whitespace_matrix_cannot_alias_track_values(self):
        cases = {'PlayResX': '320', 'PlayResY': '180', 'LayoutResX': '320', 'LayoutResY': '180',
                 'WrapStyle': '2', 'ScaledBorderAndShadow': 'yes', 'Kerning': 'yes',
                 'YCbCr Matrix': 'TV.709', 'Language': 'fa', 'Collisions': 'Normal', 'Timer': '100'}
        count = 0
        for header, value in cases.items():
            for separator in ('\u00a0', '\u1680', '\u2000', '\u2007', '\u202f', '\u205f', '\u3000'):
                for first, second in ((value, separator + value), (separator + value, value)):
                    with self.subTest(header=header, codepoint=ord(separator), first_is_plain=first == value):
                        base, donor = self.pair(header, first, second)
                        before = copy.deepcopy((base, donor))
                        with self.assertRaisesRegex(ValueError, 'track settings'):
                            workflow.merge_donor(base, donor, donor.cues, 's0002', True)
                        self.assertEqual((base, donor), before)
                        count += 1
        self.assertEqual(count, 154)

    def test_ascii_padding_still_merges(self):
        for first, second in [('yes', ' \tyes\t '), ('\tyes ', 'yes')]:
            with self.subTest(first=first):
                base, donor = self.pair('Kerning', first, second)
                merged = workflow.merge_donor(base, donor, donor.cues, 's0002', True)
                self.assertEqual(len(merged), 1)
                self.assertEqual(merged[0].fields['style'], 's0002__Default')

    def test_equal_unicode_values_are_not_repaired_or_rejected(self):
        base, donor = self.pair('Kerning', '\u00a0yes', '\u00a0yes')
        cues = workflow.merge_donor(base, donor, donor.cues, 's0002', True)
        self.assertIn('Kerning: \u00a0yes', formats.serialize(base, base.cues + cues))

    def test_last_duplicate_header_value_remains_effective(self):
        base = formats.parse(source(headers='Kerning: no\nKerning: yes'), 'ass')
        donor = formats.parse(source(headers='Kerning: yes'), 'ass')
        self.assertEqual(len(workflow.merge_donor(base, donor, donor.cues, 's0002', True)), 1)

    def test_empty_donor_does_not_impose_unused_track_settings(self):
        base, donor = self.pair('Kerning', 'yes', '\u00a0yes')
        before = copy.deepcopy((base, donor))
        self.assertEqual(workflow.merge_donor(base, donor, [], 's0002', True), [])
        self.assertEqual((base, donor), before)

    def test_noncanonical_header_is_still_refused(self):
        base = formats.parse(source(headers='Kerning: yes'), 'ass')
        donor = formats.parse(source(headers='kerning: yes'), 'ass')
        with self.assertRaisesRegex(ValueError, 'canonical'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)

    def test_build_refusal_is_atomic_and_editorial_retry_works(self):
        first = self.edition([source(headers='Kerning: yes'), source(headers='Kerning: \u00a0yes')])
        project_path = self.work / 'project.json'
        project = runtime.read_json(project_path)
        project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'alternates': ['s0002'], 'comparison': 'Authored candidate comparison'}]
        runtime.write_json(project_path, project)
        before = {p.relative_to(self.work): p.read_bytes() for p in self.work.rglob('*') if p.is_file()}
        editions = list((self.work / 'builds').iterdir())
        with self.assertRaisesRegex(ValueError, 'track settings'):
            workflow.build(self.work)
        self.assertEqual(list((self.work / 'builds').iterdir()), editions)
        self.assertEqual({p: (self.work / p).read_bytes() for p in before}, before)
        sheet_path = self.work / 'worksheets/s0002.json'
        sheet = runtime.read_json(sheet_path)
        sheet[0].update(action='alternate', links=['s0001:c000001'], note='Same authored text and timing are retained in the base.')
        runtime.write_json(sheet_path, sheet)
        retried = workflow.build(self.work)
        self.assertEqual(retried['episodes'][0]['cues'], 1)
        self.assertEqual(workflow.build(self.work)['identity'], retried['identity'])
        self.assertTrue(Path(first['build']).is_dir())
        self.assertEqual((self.work / 'sources/s0002.ass').read_bytes(), before[Path('sources/s0002.ass')])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg track-value oracle')
    def test_native_kerning_difference_is_not_a_cosmetic_spelling(self):
        a = self.pixels('ascii-value', source(text='AVAVAV To To', headers='Kerning: yes'))
        b = self.pixels('unicode-value', source(text='AVAVAV To To', headers='Kerning: \u00a0yes'))
        self.assertNotEqual(a, b)
        base, donor = self.pair('Kerning', 'yes', '\u00a0yes')
        with self.assertRaisesRegex(ValueError, 'track settings'):
            workflow.merge_donor(base, donor, donor.cues, 's0002', True)


if __name__ == '__main__':
    with runtime.operational_log('render-snapshot-contracts'):
        logging.info('Running authored snapshot and track-value regression contracts')
        unittest.main(verbosity=2)
