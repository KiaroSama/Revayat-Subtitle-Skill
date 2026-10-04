"""Preserve ASS overlap read order while keeping independent groups chronological."""
from __future__ import annotations

import copy
import logging
import os
from pathlib import Path
import random
import sys
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'skills/revayat-subtitle/scripts'))
import render
import runtime
import subtitle_formats as formats
import workflow
from test_ass_track_contracts import TrackWorkspace, source

RENDER = '--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1'


def cue(start, end, text='VISIBLE', index=1, layer='0'):
    fields = dict(zip([name.strip().lower() for name in formats.ASS_FIELDS.split(',')],
                      [layer, '', '', 'Default', '', '0', '0', '0', '', text]))
    return formats.Cue(f'c{index:06}', start, end, text, fields)


def authored(cues):
    # Do not use the serializer being tested to construct the source oracle.
    header = source().split('Dialogue:', 1)[0]
    rows = []
    for item in cues:
        rows.append('Dialogue: ' + ','.join((item.fields['layer'], formats.timecode(item.start, 'ass'),
            formats.timecode(item.end, 'ass'), item.fields['style'], '', '0', '0', '0', '', item.text)))
    return header + '\n'.join(rows) + '\n'


def overlapping(*, positioned=True, identical=True):
    position = r'{\pos(160,90)}' if positioned else ''
    return [cue(1000, 3000, position + r'{\1c&H0000FF&}' + ('VISIBLE' if identical else 'LATER'), 1),
            cue(0, 3000, position + r'{\1c&H00FF00&}' + ('VISIBLE' if identical else 'EARLIER'), 2)]


class OrderTests(TrackWorkspace):
    def test_serializer_preserves_same_layer_overlap_read_order(self):
        original = authored(overlapping())
        doc = formats.parse(original, 'ass')
        result = formats.parse(formats.serialize(doc, doc.cues), 'ass')
        self.assertEqual([c.text for c in result.cues], [c.text for c in doc.cues])
        self.assertEqual([c.start for c in result.cues], [1000, 0])

    def test_independent_ass_components_remain_chronological(self):
        items = [cue(5000, 6000, 'LATE', 1), cue(1000, 3000, 'TOP', 2),
                 cue(0, 2000, 'BOTTOM', 3), cue(3000, 4000, 'NEXT', 4)]
        doc = formats.parse(authored(items), 'ass')
        result = formats.parse(formats.serialize(doc, doc.cues), 'ass')
        self.assertEqual([c.text for c in result.cues], ['TOP', 'BOTTOM', 'NEXT', 'LATE'])

    def test_transitive_component_uses_maximum_end_not_previous_end(self):
        # A long middle-listed cue connects short otherwise disjoint events.
        items = [cue(7000, 8000, 'LAST-TIME', 1), cue(0, 10000, 'BRIDGE', 2),
                 cue(1000, 2000, 'SHORT', 3), cue(4000, 5000, 'MIDDLE', 4)]
        self.assertEqual(formats.presentation_order(items, 'ass'), [0, 1, 2, 3])

    def test_touching_half_open_intervals_are_independent(self):
        items = [cue(1000, 2000), cue(0, 1000)]
        self.assertEqual(formats.presentation_order(items, 'ass'), [1, 0])

    def test_quantized_intervals_determine_ass_overlap(self):
        items = [cue(1009, 2000), cue(0, 1009)]
        self.assertEqual(formats.presentation_order(items, 'ass'), [1, 0])
        self.assertEqual(formats.presentation_order([cue(1009, 2000), cue(0, 1010)], 'ass'), [0, 1])

    def test_equal_start_order_and_layers_are_unchanged(self):
        items = [cue(1000, 3000, index=1, layer='2'), cue(1000, 2000, index=2, layer='0')]
        before = copy.deepcopy(items)
        self.assertEqual(formats.presentation_order(items, 'ass'), [0, 1])
        doc = formats.parse(authored(items), 'ass')
        result = formats.parse(formats.serialize(doc, doc.cues), 'ass')
        self.assertEqual([c.fields['layer'] for c in result.cues], ['2', '0'])
        self.assertEqual(items, before)

    def test_input_objects_and_source_identifiers_are_not_mutated(self):
        items = [cue(8000, 9000), cue(1000, 3000), cue(0, 3000)]
        original = copy.deepcopy(items)
        self.assertEqual(formats.presentation_order(items, 'ass'), [1, 2, 0])
        self.assertEqual(items, original)
        self.assertTrue(all(item.id == 'c000001' for item in items), 'IDs are not ordering keys.')

    def test_empty_single_and_unsupported_format_boundaries(self):
        self.assertEqual(formats.presentation_order([], 'ass'), [])
        self.assertEqual(formats.presentation_order([cue(0, 10)], 'ass'), [0])
        with self.assertRaisesRegex(ValueError, 'format'):
            formats.presentation_order([], 'ssa')

    def test_invalid_or_collapsed_timing_is_still_refused(self):
        for start, end in [(1, 9), (10, 10), (20, 10), (-1, 10), (True, 10)]:
            with self.subTest(start=start, end=end), self.assertRaises(ValueError):
                formats.presentation_order([cue(start, end)], 'ass')

    def test_srt_keeps_existing_stable_time_order(self):
        items = [formats.Cue('c000001', 1000, 3000, 'late'),
                 formats.Cue('c000002', 0, 3000, 'first'),
                 formats.Cue('c000003', 0, 2000, 'second')]
        doc = formats.Document('srt', items)
        self.assertEqual(formats.presentation_order(items, 'srt'), [1, 2, 0])
        result = formats.parse(formats.serialize(doc, items), 'srt')
        self.assertEqual([c.text for c in result.cues], ['first', 'second', 'late'])

    def test_second_serialization_keeps_the_same_bytes(self):
        items = [cue(5000, 6000, 'FINAL'), *overlapping()]
        doc = formats.parse(authored(items), 'ass')
        first = formats.serialize(doc, doc.cues)
        again = formats.parse(first, 'ass')
        self.assertEqual(formats.serialize(again, again.cues), first)

    def test_seeded_pair_order_permutation_and_component_invariants(self):
        rng = random.Random(9483392)
        for case in range(400):
            items = []
            for index in range(rng.randrange(1, 30)):
                start = rng.randrange(0, 80) * 10
                items.append(cue(start, start + rng.randrange(1, 25) * 10, str(index), index + 1))
            before = copy.deepcopy(items)
            order = formats.presentation_order(items, 'ass')
            self.assertEqual(sorted(order), list(range(len(items))), f'Permutation case={case}')
            rank = {index: offset for offset, index in enumerate(order)}
            for i, first in enumerate(items):
                for j in range(i + 1, len(items)):
                    second = items[j]
                    if max(first.start, second.start) < min(first.end, second.end):
                        self.assertLess(rank[i], rank[j], f'Overlap read order case={case}')
            sorted_items = [items[i] for i in order]
            self.assertEqual(formats.presentation_order(sorted_items, 'ass'), list(range(len(items))))
            self.assertEqual(items, before)

    def test_hundred_thousand_cue_budget_is_not_pairwise(self):
        code = ('import sys;sys.path.insert(0,sys.argv[1]);'
                'from subtitle_formats import Cue,presentation_order;'
                'n=100000;items=[Cue(str(i),i*20,i*20+10,"x") for i in range(n-1,-1,-1)];'
                'assert presentation_order(items,"ass")==list(range(n-1,-1,-1));'
                'items=[Cue(str(i),i,200000,"x") for i in range(n-1,-1,-1)];'
                'assert presentation_order(items,"ass")==list(range(n))')
        logging.debug('Running bounded 100000-cue ordering probes')
        runtime.run([sys.executable, '-X', 'utf8', '-S', '-c', code,
            str(ROOT / 'skills/revayat-subtitle/scripts')], timeout=15, idle_timeout=10,
            max_output=2 * 1024 * 1024)

    def test_assemble_orders_cues_and_provenance_together(self):
        edition = self.edition([authored([cue(5000, 6000, 'FINAL', 1), *overlapping()])])
        episode = edition['episodes'][0]
        doc = formats.parse((Path(edition['build']) / episode['file']).read_text(encoding='utf-8'), 'ass')
        self.assertEqual([item['cue'] for item in episode['provenance']], ['c000002', 'c000003', 'c000001'])
        self.assertEqual([c.start for c in doc.cues], [1000, 0, 5000])
        for offset, (item, emitted) in enumerate(zip(episode['provenance'], doc.cues), 1):
            self.assertEqual(item['emitted_index'], offset)
            self.assertEqual(item['timing']['emitted'], [emitted.start, emitted.end])
        self.assertEqual(workflow.build(self.work)['identity'], edition['identity'])

    def test_donor_duplicate_ids_preserve_source_precedence_and_provenance(self):
        self.edition([authored([cue(1000, 3000, 'BASE')]), authored([cue(0, 3000, 'DONOR')])])
        project_path = self.work / 'project.json'
        project = runtime.read_json(project_path)
        project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'alternates': ['s0002'],
                                'comparison': 'Authored missing donor content; base precedes donor in overlap.'}]
        runtime.write_json(project_path, project)
        edition = workflow.build(self.work)
        record = edition['episodes'][0]
        self.assertEqual([(p['source'], p['cue']) for p in record['provenance']],
                         [('s0001', 'c000001'), ('s0002', 'c000001')])
        doc = formats.parse((Path(edition['build']) / record['file']).read_text(encoding='utf-8'), 'ass')
        self.assertEqual([c.text for c in doc.cues], ['BASE', 'DONOR'])
        self.assertEqual(doc.cues[1].fields['style'], 's0002__Default')

    def test_reviewed_timing_changes_use_the_same_ordering_contract(self):
        self.edition([authored([cue(0, 1000, 'FIRST'), cue(2000, 3000, 'SECOND')])])
        path = self.work / 'worksheets/s0001.json'
        rows = runtime.read_json(path)
        rows[0].update(start_ms=2501, end_ms=3509, timing_note='Authored retime into overlap')
        runtime.write_json(path, rows)
        edition = workflow.build(self.work)
        records = edition['episodes'][0]['provenance']
        self.assertEqual([r['cue'] for r in records], ['c000001', 'c000002'])
        self.assertEqual(records[0]['timing']['reviewed'], [2501, 3509])
        self.assertEqual(records[0]['timing']['emitted'], [2500, 3500])
        self.assertEqual(records[0]['timing']['conversion'], 'floor-centisecond')

    def test_recipe_upgrade_preserves_all_existing_files(self):
        with patch.dict(workflow.GENERATION_RECIPE, {'version': 10, 'normalization': 10}):
            old = self.edition([authored(overlapping())])
        before = {p.relative_to(self.work): p.read_bytes() for p in self.work.rglob('*') if p.is_file()}
        new = workflow.build(self.work)
        self.assertNotEqual(new['identity'], old['identity'])
        self.assertEqual(new['input_identity'], old['input_identity'])
        self.assertEqual(new['generation_recipe']['normalization'], 10)
        self.assertEqual({p: (self.work / p).read_bytes() for p in before}, before)
        self.assertFalse((Path(new['build']) / 'renders').exists())

    def test_default_samples_cover_temporal_edges_in_unsorted_overlap(self):
        items = [cue(3000, 10000, index=1), cue(0, 10000, index=2),
                 cue(9000, 10000, index=3), cue(2000, 10000, index=4)]
        doc = formats.parse(authored(items), 'ass')
        plan = render.sample_plan(doc, False)
        selected = {i for sample in plan for i in sample['cue_indices']}
        self.assertTrue({2, 3} <= selected, 'File-edge cues are not necessarily temporal-edge cues.')
        for sample in plan:
            for index in sample['cue_indices']:
                item = doc.cues[index - 1]
                self.assertLessEqual(item.start / 1000, sample['time_seconds'])
                self.assertLess(sample['time_seconds'], item.end / 1000)

    def test_default_samples_cover_longest_tail_and_changed_indices(self):
        items = [cue(3000, 5000, index=1), cue(1000, 12000, index=2),
                 cue(2000, 4000, index=3), cue(0, 3500, index=4)]
        plan = render.sample_plan(formats.parse(authored(items), 'ass'), False, [3])
        selected = {i for sample in plan for i in sample['cue_indices']}
        self.assertTrue({2, 3, 4} <= selected)

    def test_all_cues_and_existing_representative_dedup_remain(self):
        items = [cue(i * 1000, (i + 1) * 1000, index=i + 1) for i in range(5)]
        doc = formats.parse(authored(items), 'ass')
        selected = lambda plan: {i for sample in plan for i in sample['cue_indices']}
        self.assertEqual(selected(render.sample_plan(doc, False)), {1, 5})
        self.assertEqual(selected(render.sample_plan(doc, True)), {1, 2, 3, 4, 5})

    def test_equal_effective_starts_keep_original_order(self):
        items = [cue(1009, 3000, 'FIRST'), cue(1000, 2000, 'SECOND')]
        self.assertEqual(formats.presentation_order(items, 'ass'), [0, 1])
        doc = formats.parse(authored(items), 'ass')
        self.assertEqual([c.text for c in formats.parse(formats.serialize(doc, doc.cues), 'ass').cues],
                         ['FIRST', 'SECOND'])

    def test_timestamp_upper_boundary_does_not_use_float_ordering(self):
        limit = 10 ** 12
        items = [cue(limit - 1000, limit), cue(limit - 2000, limit)]
        self.assertEqual(formats.presentation_order(items, 'ass'), [0, 1])
        self.assertEqual(formats.presentation_order(items, 'srt'), [1, 0])

    def test_srt_donor_order_uses_final_ass_intervals(self):
        self.edition([authored([cue(1000, 2000, 'BASE')])])
        base_doc = formats.parse(authored([cue(1000, 2000, 'BASE')]), 'ass')
        donor = formats.parse('1\n00:00:00,000 --> 00:00:01,009\nDONOR\n', 'srt')
        converted = workflow.merge_donor(base_doc, donor, donor.cues, 's0002', True)
        self.assertEqual(converted[0].end, 1000)
        self.assertEqual(formats.presentation_order(base_doc.cues + converted, 'ass'), [1, 0])
        donor.cues[0].end = 1010
        converted = workflow.merge_donor(base_doc, donor, donor.cues, 's0002', True)
        self.assertEqual(formats.presentation_order(base_doc.cues + converted, 'ass'), [0, 1])

    def test_new_generation_has_no_inherited_render_approval(self):
        with patch.dict(workflow.GENERATION_RECIPE, {'version': 10}):
            old = self.edition([authored(overlapping())])
        receipt = Path(old['build']) / 'renders/S01E01.json'
        runtime.write_json(receipt, {'version': 2, 'note': 'Untrusted historical marker, never a real approval.'})
        previous = receipt.read_bytes()
        fresh = workflow.build(self.work)
        self.assertEqual(receipt.read_bytes(), previous)
        self.assertNotEqual(fresh['identity'], old['identity'])
        self.assertFalse((Path(fresh['build']) / 'renders/S01E01.json').exists())

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg read-order raster tier')
    def test_native_positioned_compositing_and_collision_are_preserved(self):
        for positioned, identical in [(True, True), (False, False)]:
            with self.subTest(positioned=positioned):
                original = authored(overlapping(positioned=positioned, identical=identical))
                doc = formats.parse(original, 'ass')
                baseline = self.pixels(f'source-{positioned}', original)
                changed = self.pixels(f'reversed-{positioned}', authored(list(reversed(doc.cues))))
                self.assertGreater(max(baseline), 32)
                self.assertNotEqual(baseline, changed, 'Negative control must exhibit real read-order effects.')
                result = formats.serialize(doc, doc.cues)
                self.assertTrue(self.pixels(f'fixed-{positioned}', result) == baseline,
                                'ASS serialization changed source raster bytes.')

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg read-order raster tier')
    def test_native_nonoverlap_reordering_is_visually_neutral(self):
        original = authored([cue(5000, 6000, 'LATER'), cue(0, 3000, 'VISIBLE')])
        doc = formats.parse(original, 'ass')
        normalized = formats.serialize(doc, doc.cues)
        self.assertTrue(self.pixels('independent-source', original) == self.pixels('independent-result', normalized),
                        'Independent component order must not change visible pixels.')
        self.assertEqual([c.start for c in formats.parse(normalized, 'ass').cues], [0, 5000])

    @unittest.skipUnless(RENDER, 'Explicit FFmpeg read-order delivery tier')
    def test_native_build_render_review_and_zip_keep_source_presentation(self):
        original = authored(overlapping())
        built = self.edition([original])
        edition = Path(built['build'])
        episode = built['episodes'][0]
        output = (edition / episode['file']).read_text(encoding='utf-8')
        self.assertTrue(self.pixels('delivery-source', original) == self.pixels('delivery-result', output),
                        'Built ASS lost the original overlapping presentation.')
        evidence = render.render(edition, 'S01E01', None, None, None, True)
        with self.assertRaisesRegex(ValueError, 'Inspect every'):
            render.package(edition, self.root / 'unreviewed.zip')
        record = runtime.read_json(Path(evidence['review']))
        for frame in record['frames']:
            frame.update(reviewed=True, note='Mechanical authored gate fixture, not human editorial certification.')
        runtime.write_json(Path(evidence['review']), record)
        self.assertTrue(render.qa(edition)['ok'])
        delivered = render.package(edition, self.root / 'ordered.zip')
        with zipfile.ZipFile(delivered['zip']) as archive:
            self.assertEqual(archive.namelist(), [episode['file']])
            self.assertIsNone(archive.testzip())
            self.assertEqual(runtime.digest(archive.read(episode['file'])), episode['sha256'])
        self.assertEqual((self.work / 'sources/s0001.ass').read_text(encoding='utf-8'), original)


if __name__ == '__main__':
    with runtime.operational_log('event-order-contracts'):
        logging.info('Running authored read-order and provenance regressions')
        unittest.main(verbosity=2)
