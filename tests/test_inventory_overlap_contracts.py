"""Owned import inventory and native ASS overlap-order regression contracts."""
from __future__ import annotations

import copy
import logging
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / 'skills/revayat-subtitle/scripts'
sys.path.insert(0, str(SCRIPTS))
import render
import runtime
import subtitle_formats as formats
import workflow

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'
SRT = '1\n00:00:01,000 --> 00:00:02,000\nAuthored input\n'


def vector(colour):
    return r'{\an7\pos(60,60)\p1\1c&H' + colour + '&}m 0 0 l 60 0 60 60 0 60'


def ass(events):
    header = ('[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n'
              '[V4+ Styles]\nFormat: ' + formats.STYLE_FIELDS + '\nStyle: ' +
              formats.DEFAULT_STYLE + '\n[Events]\nFormat: ' + formats.ASS_FIELDS + '\n')
    return header + ''.join('Dialogue: ' + str(layer) + ',' + formats.timecode(start, 'ass') + ',' +
        formats.timecode(end, 'ass') + ',Default,,0,0,0,,' + text + '\n' for start, end, text, layer in events)


def complete(work, *, merged=False):
    project = runtime.read_json(work / 'project.json')
    if merged:
        project['episodes'] = [{'id': 'S01E01', 'base': project['sources'][0]['id'],
            'alternates': [source['id'] for source in project['sources'][1:]],
            'comparison': 'Retain each authored vector in declared source order'}]
    else:
        project['episodes'] = [{'id': f'S01E{i:02}', 'base': source['id'],
            'comparison': 'One authored source per controlled episode'}
            for i, source in enumerate(project['sources'], 1)]
    runtime.write_json(work / 'project.json', project)
    for source in project['sources']:
        sheet = work / 'worksheets' / (source['id'] + '.json')
        rows = runtime.read_json(sheet)
        for row in rows:
            row.update(reviewed=True, action='preserve')
        runtime.write_json(sheet, rows)
    runtime.write_json(work / 'glossary.json', {'series': 'Authored audit', 'terms_reviewed': True,
        'terms': [], 'research': [{'url': 'https://example.org/authored',
                                  'note': 'Mechanical fixture; no anime research claim'}]})


class AuditCase(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / '.scratch/inventory-overlap'
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='case-', dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.work = self.root / 'work'
        environment = patch.dict(os.environ, {'REVAYAT_LOG_DIR': str(self.root / 'logs')})
        environment.start()
        self.addCleanup(environment.stop)
        logging.debug('Inventory/overlap regression case=%s', self.id())

    def prepared(self, text, kind='ass'):
        path = self.root / ('source.' + kind)
        path.write_text(text, encoding='utf-8')
        workflow.prepare([path], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        complete(self.work)
        return self.work

    def raster(self, name, text, seconds=2.5):
        path = self.root / (name + '.ass')
        path.write_text(text, encoding='utf-8')
        raw = runtime.run([render.ffmpeg_path(), '-hide_banner', '-loglevel', 'error', '-nostdin',
            '-f', 'lavfi', '-i', 'color=s=320x180:r=1:d=1', '-filter_threads', '1', '-vf',
            f'setpts=PTS+{seconds}/TB,ass={path.name}', '-frames:v', '1', '-threads', '1',
            '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], cwd=self.root, timeout=20)
        self.assertEqual(len(raw), 320 * 180 * 3)
        self.assertGreater(max(raw), 100, 'Oracle must render nonblank vectors')
        return raw


class InventoryTests(AuditCase):
    def test_later_parent_root_never_imports_own_staged_source(self):
        source = self.root / 'input.srt'
        source.write_text(SRT, encoding='utf-8')
        result = workflow.prepare([source, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2, 'Each caller root contributes the original, never generated copies')
        self.assertEqual([item['origin']['relative_path'] for item in result['sources']], ['input.srt'] * 2)
        self.assertEqual(source.read_text(encoding='utf-8'), SRT)
        self.assertEqual(len(workflow.load(self.work)[1]), 2)

    def test_repeated_directory_root_does_not_grow_the_inventory(self):
        (self.root / 'input.srt').write_text(SRT, encoding='utf-8')
        result = workflow.prepare([self.root] * 3, self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 3)
        self.assertEqual([s['origin']['root_id'] for s in result['sources']], ['input001', 'input002', 'input003'])

    def test_zip_then_parent_root_keeps_archive_provenance(self):
        archive = self.root / 'input.zip'
        with zipfile.ZipFile(archive, 'w') as handle:
            handle.writestr('nested/input.srt', SRT)
        result = workflow.prepare([archive, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2)
        self.assertEqual([s['origin']['member'] for s in result['sources']], ['nested/input.srt'] * 2)
        self.assertFalse(any('.subtitle-import-' in s['name'] for s in result['sources']))

    def test_unrelated_staging_looking_directory_is_not_filtered_by_name(self):
        authored = self.root / '.subtitle-import-authored'
        authored.mkdir()
        source = authored / 'keep.srt'
        source.write_text(SRT, encoding='utf-8')
        result = workflow.prepare([source, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2)
        self.assertIn('.subtitle-import-authored/keep.srt', result['sources'][1]['name'])

    def test_owned_exclusion_is_a_path_boundary_not_a_prefix(self):
        owned = self.root / 'stage'
        neighbor = self.root / 'stage-original'
        for directory in (owned, neighbor):
            directory.mkdir()
            (directory / 'keep.srt').write_text(SRT, encoding='utf-8')
        self.assertEqual(workflow.input_candidates(self.root, excluded_root=owned), [neighbor / 'keep.srt'])
        self.assertEqual(workflow.input_candidates(owned, excluded_root=owned), [])

    def test_relative_roots_match_the_owned_absolute_stage(self):
        source = self.root / 'input.srt'
        source.write_text(SRT, encoding='utf-8')
        relative = Path(os.path.relpath(self.root, Path.cwd()))
        result = workflow.prepare([relative / source.name, relative], self.work,
                                  'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2)

    def test_parent_segments_do_not_expose_owned_staging(self):
        (self.root / 'nested').mkdir()
        source = self.root / 'input.srt'
        source.write_text(SRT, encoding='utf-8')
        alias = self.root / 'nested' / '..'
        result = workflow.prepare([source, alias], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2)
        self.assertEqual([s['origin']['relative_path'] for s in result['sources']], ['input.srt'] * 2)

    def test_generated_stage_does_not_consume_external_traversal_budget(self):
        source = self.root / 'input.srt'
        source.write_text(SRT, encoding='utf-8')
        with patch.object(workflow, 'MAX_ENTRIES', 1):
            result = workflow.prepare([source, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 2)

    def test_late_source_failure_preserves_originals_and_retry_inventory(self):
        good, bad = self.root / 'a.srt', self.root / 'z.srt'
        good.write_text(SRT, encoding='utf-8')
        bad.write_text('Malformed authored fixture', encoding='utf-8')
        with self.assertRaises(ValueError):
            workflow.prepare([good, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertFalse(self.work.exists())
        self.assertEqual(good.read_text(encoding='utf-8'), SRT)
        bad.write_text(SRT, encoding='utf-8')
        result = workflow.prepare([good, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 3)
        self.assertEqual(list(self.root.glob('.subtitle-import-*')), [])

    def test_duplicate_basenames_and_overlapping_roots_keep_declared_origins(self):
        nested = self.root / 'nested'
        nested.mkdir()
        (self.root / 'input.srt').write_text(SRT, encoding='utf-8')
        (nested / 'input.srt').write_text(SRT.replace('input', 'other'), encoding='utf-8')
        result = workflow.prepare([nested, self.root], self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        self.assertEqual(len(result['sources']), 3)
        self.assertEqual({s['origin']['relative_path'] for s in result['sources']}, {'input.srt', 'nested/input.srt'})
        self.assertEqual(len({s['name'] for s in result['sources']}), 3)

    def test_cli_inventory_contains_no_generated_candidates(self):
        source = self.root / 'input.srt'
        source.write_text(SRT, encoding='utf-8')
        output = runtime.run([sys.executable, '-X', 'utf8', str(SCRIPTS / 'revayat-subtitle.py'),
            'prepare', str(source), str(self.root), '--work', str(self.work), '--series',
            'Authored audit', '--season', '1'], timeout=20)
        import json
        self.assertEqual(len(json.loads(output)['sources']), 2)
        self.assertEqual(len(workflow.load(self.work)[1]), 2)


class OrderTests(AuditCase):
    def cues(self, intervals):
        return [formats.Cue(f'c{i:06}', start, end, f'cue-{i}', {'style': 'Default'})
                for i, (start, end) in enumerate(intervals, 1)]

    def test_serialize_retains_unsorted_overlap_order(self):
        document = formats.parse(ass([(2000, 4000, vector('0000FF'), 0),
                                     (1000, 3000, vector('FF0000'), 0)]), 'ass')
        output = formats.parse(formats.serialize(document, document.cues), 'ass')
        self.assertEqual([c.text for c in output.cues], [c.text for c in document.cues])
        self.assertEqual([c.start for c in output.cues], [2000, 1000])

    def test_disjoint_components_are_still_chronological(self):
        cues = self.cues([(8000, 9000), (3000, 5000), (2000, 4000), (0, 1000)])
        self.assertEqual(formats.presentation_order(cues, 'ass'), [3, 1, 2, 0])

    def test_transitive_overlap_bridge_retains_original_component(self):
        cues = self.cues([(5000, 7000), (1000, 3000), (2500, 6000), (8000, 9000)])
        self.assertEqual(formats.presentation_order(cues, 'ass'), [0, 1, 2, 3])

    def test_touching_boundaries_do_not_form_an_overlap(self):
        self.assertEqual(formats.presentation_order(self.cues([(2000, 3000), (1000, 2000)]), 'ass'), [1, 0])

    def test_same_start_order_and_fields_are_preserved_without_mutation(self):
        cues = self.cues([(1000, 4000)] * 3)
        for index, cue in enumerate(cues):
            cue.fields.update(layer=str(index), marginl=str(index), effect='')
        old = copy.deepcopy(cues)
        self.assertEqual(formats.presentation_order(cues, 'ass'), [0, 1, 2])
        self.assertEqual(cues, old)

    def test_emitted_centiseconds_determine_component_membership(self):
        cues = self.cues([(2011, 3000), (1000, 2019)])
        self.assertEqual(formats.presentation_order(cues, 'ass'), [1, 0])
        self.assertEqual(formats.presentation_order(self.cues([(2011, 3000), (1000, 2020)]), 'ass'), [0, 1])

    def test_collapsed_timing_is_not_hidden_by_ordering(self):
        with self.assertRaisesRegex(ValueError, 'collapses'):
            formats.presentation_order(self.cues([(1001, 1009)]), 'ass')

    def test_srt_remains_chronological_even_for_overlaps(self):
        cues = self.cues([(2000, 4000), (1000, 3000)])
        self.assertEqual(formats.presentation_order(cues, 'srt'), [1, 0])
        output = formats.parse(formats.serialize(formats.Document('srt', cues), cues), 'srt')
        self.assertEqual([c.text for c in output.cues], ['cue-2', 'cue-1'])

    def test_order_is_idempotent_and_preserves_every_active_pair(self):
        rng = random.Random(10412)
        for _ in range(250):
            intervals = [(start := rng.randrange(0, 10000), start + rng.randrange(10, 4000)) for _ in range(20)]
            cues = self.cues(intervals)
            order = formats.presentation_order(cues, 'ass')
            self.assertEqual(sorted(order), list(range(len(cues))))
            positions = {old: new for new, old in enumerate(order)}
            times = [formats.effective_times(c.start, c.end, 'ass') for c in cues]
            for i, (start, end) in enumerate(times):
                for j in range(i + 1, len(cues)):
                    if max(start, times[j][0]) < min(end, times[j][1]):
                        self.assertLess(positions[i], positions[j])
            ordered = [cues[i] for i in order]
            self.assertEqual(formats.presentation_order(ordered, 'ass'), list(range(len(cues))))

    def test_large_transitive_component_has_bounded_processing(self):
        code = ('import sys;sys.path.insert(0,sys.argv[1]);from subtitle_formats import Cue,presentation_order;'
                'c=[Cue(str(i),i*10,i*10+20,"x") for i in reversed(range(100000))];'
                'assert presentation_order(c,"ass")==list(range(100000))')
        result = subprocess.run([sys.executable, '-S', '-c', code, str(SCRIPTS)],
                                capture_output=True, timeout=12)
        self.assertEqual(result.returncode, 0, 'Bounded component-order regression failed')

    def test_sampler_covers_timeline_edges_in_nonchronological_rows(self):
        document = formats.Document('ass', self.cues([(5000, 20000), (1000, 20000),
                                                    (9000, 20000), (2000, 20000)]))
        plan = render.sample_plan(document, False)
        self.assertEqual({i for frame in plan for i in frame['cue_indices']}, {1, 2, 3, 4})
        self.assertEqual([frame['time_seconds'] for frame in plan], sorted(frame['time_seconds'] for frame in plan))

    def test_empty_sampler_has_a_controlled_error(self):
        with self.assertRaisesRegex(ValueError, 'at least one cue'):
            render.sample_plan(formats.Document('srt', []), False)

    def test_build_provenance_follows_physical_emitted_order(self):
        work = self.prepared(ass([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)]))
        result = workflow.build(work)
        episode = result['episodes'][0]
        output = formats.parse((Path(result['build']) / episode['file']).read_text(encoding='utf-8'), 'ass')
        self.assertEqual([c.start for c in output.cues], [2000, 1000])
        self.assertEqual([(p['emitted_index'], p['cue']) for p in episode['provenance']], [(1, 'c000001'), (2, 'c000002')])
        self.assertEqual([p['timing']['emitted'] for p in episode['provenance']], [[2000, 4000], [1000, 3000]])
        self.assertEqual(workflow.build(work)['identity'], result['identity'])

    def test_retained_donors_follow_declared_source_order_in_overlap(self):
        sources = []
        for i, event in enumerate([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)]):
            path = self.root / f'source-{i}.ass'
            path.write_text(ass([event]), encoding='utf-8')
            sources.append(path)
        workflow.prepare(sources, self.work, 'Authored audit', 1, 'utf-8', None, 'en')
        complete(self.work, merged=True)
        result = workflow.build(self.work)
        self.assertEqual([p['source'] for p in result['episodes'][0]['provenance']], ['s0001', 's0002'])
        output = formats.parse((Path(result['build']) / 'Sub/S01E01.ass').read_text(encoding='utf-8'), 'ass')
        self.assertEqual([c.fields['style'] for c in output.cues], ['Default', 's0002__Default'])

    def test_worksheet_retiming_preserves_read_order_and_timing_notes(self):
        work = self.prepared(ass([(1000, 3000, vector('0000FF'), 0), (4000, 6000, vector('FF0000'), 0)]))
        sheet = work / 'worksheets/s0001.json'
        rows = runtime.read_json(sheet)
        rows[0].update(start_ms=4500, end_ms=6500, timing_note='Authored overlap timing correction')
        runtime.write_json(sheet, rows)
        result = workflow.build(work)
        self.assertEqual([p['cue'] for p in result['episodes'][0]['provenance']], ['c000001', 'c000002'])
        self.assertEqual(result['episodes'][0]['provenance'][0]['timing']['original'], [1000, 3000])

    def test_recipe_upgrade_preserves_all_existing_bytes(self):
        work = self.prepared(ass([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)]))
        # This is a recipe-identity fixture, not an assertion of old renderer pixels.
        with patch.dict(workflow.GENERATION_RECIPE, {'version': 10, 'normalization': 10}):
            old = workflow.build(work)
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob('*') if p.is_file()}
        new = workflow.build(work)
        self.assertNotEqual(old['identity'], new['identity'])
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})
        self.assertFalse((Path(new['build']) / 'renders').exists())

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg overlap-order oracle')
    def test_native_unsorted_overlap_keeps_the_same_raster(self):
        text = ass([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)])
        document = formats.parse(text, 'ass')
        rendered = self.raster('original', text)
        self.assertEqual(rendered, self.raster('preserved', formats.serialize(document, document.cues)))
        wrong = ass([(1000, 3000, vector('FF0000'), 0), (2000, 4000, vector('0000FF'), 0)])
        self.assertNotEqual(rendered, self.raster('control-wrong-order', wrong))

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg overlap-order corpus')
    def test_native_components_layers_and_phase_corpus(self):
        cases = [([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)], [1.5, 2.5, 3.5]),
                 ([(2000, 4000, vector('0000FF'), 1), (1000, 3000, vector('FF0000'), 0)], [2.5]),
                 ([(3000, 6000, vector('00FF00'), 0), (1000, 2500, vector('FF0000'), 0),
                   (2000, 4000, vector('0000FF'), 0)], [2.25, 3.5, 5.0])]
        for case, (events, times) in enumerate(cases):
            text = ass(events)
            document = formats.parse(text, 'ass')
            output = formats.serialize(document, document.cues)
            for i, seconds in enumerate(times):
                self.assertEqual(self.raster(f'before-{case}-{i}', text, seconds),
                                 self.raster(f'after-{case}-{i}', output, seconds))

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg build/render/QA/ZIP integration')
    def test_complete_overlap_delivery_and_old_sampler_refusal(self):
        work = self.prepared(ass([(2000, 4000, vector('0000FF'), 0), (1000, 3000, vector('FF0000'), 0)]))
        result = workflow.build(work)
        edition = Path(result['build'])
        evidence = render.render(edition, 'S01E01', None, None, None, True)
        with self.assertRaisesRegex(ValueError, 'Inspect every'):
            render.qa(edition)
        review_path = Path(evidence['review'])
        record = runtime.read_json(review_path)
        for frame in record['frames']:
            frame.update(reviewed=True, note='Mechanical QA gate fixture; not human editorial or visual approval')
        runtime.write_json(review_path, record)
        self.assertTrue(render.qa(edition)['ok'])
        with patch.object(render, 'SAMPLER_VERSION', 11):
            with self.assertRaisesRegex(ValueError, 'recipe changed'):
                render.qa(edition)
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob('*') if p.is_file()}
        output = self.root / 'delivery.zip'
        render.package(edition, output)
        with zipfile.ZipFile(output) as archive:
            self.assertIsNone(archive.testzip())
            self.assertEqual(archive.namelist(), ['Sub/S01E01.ass'])
            self.assertEqual(runtime.digest(archive.read('Sub/S01E01.ass')), result['episodes'][0]['sha256'])
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})


if __name__ == '__main__':
    with runtime.operational_log('inventory-overlap-tests'):
        unittest.main(verbosity=2)
