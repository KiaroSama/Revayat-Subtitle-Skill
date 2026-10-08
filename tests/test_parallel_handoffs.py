"""Revision-bound proposal validation leaves coordinator-owned workspace bytes intact."""
import copy
from pathlib import Path
import unittest

from check import CLI, WorkspaceCase, completed
from process_helpers import run_child
import sys
from handoff import handoff
from runtime import digest, read_json, write_json


class ParallelHandoffTests(WorkspaceCase):
    def fixture(self):
        self.import_work()
        completed(self.work)
        project = read_json(self.work / "project.json")
        rows = read_json(self.work / "worksheets/s0001.json")
        assignment = {"version": 1, "id": "worker-a", "source": "s0001", "episode": "S01E01",
                      "source_sha256": project["sources"][0]["sha256"],
                      "glossary_sha256": digest((self.work / "glossary.json").read_bytes()),
                      "assigned_ids": [rows[0]["id"]], "context_ids": [rows[1]["id"]]}
        result = {"version": 1, "assignment_id": "worker-a", "source_sha256": assignment["source_sha256"],
                  "glossary_sha256": assignment["glossary_sha256"], "rows": [rows[0]], "status": "complete"}
        ap, rp = self.root / "assignment.json", self.root / "result.json"
        write_json(ap, assignment)
        write_json(rp, result)
        return ap, rp, assignment, result

    def test_valid_complete_and_pending_never_import_or_certify(self):
        ap, rp, _, result = self.fixture()
        before = {path.relative_to(self.work).as_posix(): digest(path.read_bytes()) for path in self.work.rglob("*") if path.is_file()}
        report = handoff(self.work, [ap], [rp])
        self.assertTrue(report["ready"])
        self.assertTrue(report["coordinator_review_required"])
        self.assertFalse(report["linguistic_quality_certified"])
        result["rows"][0].update(reviewed=False, text=None)
        result["status"] = "pending"
        write_json(rp, result)
        report = handoff(self.work, [ap], [rp])
        self.assertFalse(report["ready"])
        self.assertEqual(report["pending_rows"], 1)
        self.assertEqual(before, {path.relative_to(self.work).as_posix(): digest(path.read_bytes()) for path in self.work.rglob("*") if path.is_file()})

    def test_stale_extra_missing_context_and_unknown_fields_refuse(self):
        ap, rp, assignment, result = self.fixture()
        mutations = []
        stale = copy.deepcopy(result)
        stale["glossary_sha256"] = "0" * 64
        mutations.append(stale)
        missing = copy.deepcopy(result)
        missing["rows"] = []
        mutations.append(missing)
        context = copy.deepcopy(result)
        context["rows"][0]["id"] = assignment["context_ids"][0]
        mutations.append(context)
        foreign = copy.deepcopy(result)
        foreign["auto_approve"] = True
        mutations.append(foreign)
        source = copy.deepcopy(result)
        source["rows"][0]["source_text"] = "changed private body"
        mutations.append(source)
        for changed in mutations:
            with self.subTest(fields=list(changed)):
                write_json(rp, changed)
                with self.assertRaises(ValueError) as caught:
                    handoff(self.work, [ap], [rp])
                self.assertNotIn("changed private body", str(caught.exception))

    def test_overlapping_assignments_refuse_before_results(self):
        ap, rp, assignment, _ = self.fixture()
        assignment["id"] = "worker-b"
        second = self.root / "second.json"
        write_json(second, assignment)
        with self.assertRaisesRegex(ValueError, "overlap"):
            handoff(self.work, [ap, second], [rp])


    def test_public_cli_complete_pending_invalid_types_are_controlled(self):
        ap, rp, _, result = self.fixture()
        command = [sys.executable, '-B', str(CLI), 'handoff', '--work', str(self.work), '--assignment', str(ap), '--result', str(rp)]
        self.assertEqual(run_child(command, timeout=15).returncode, 0)
        result['rows'][0].update(reviewed=False, text=None)
        result['status'] = 'pending'
        write_json(rp, result)
        self.assertEqual(run_child(command, timeout=15).returncode, 1)
        for value in ([], {}, True):
            result['status'] = value
            write_json(rp, result)
            output = run_child(command, timeout=15)
            self.assertEqual(output.returncode, 2)
            self.assertNotIn(b'Traceback', output.stderr)
            self.assertNotIn(result['rows'][0]['source_text'].encode('utf-8'), output.stderr)

    def test_reviewed_alternate_links_require_effective_retained_targets(self):
        ap, rp, _, result = self.fixture()
        row = result['rows'][0]
        row.update(action='alternate', note='Duplicate content', text=None)
        for link in ('s0001:c000001', 's0001:c000004', 's0002:c000001'):
            row['links'] = [link]
            write_json(rp, result)
            with self.subTest(link=link), self.assertRaisesRegex(ValueError, 'itself|retained'):
                handoff(self.work, [ap], [rp])
        row['links'] = ['s0001:c000002']
        write_json(rp, result)
        self.assertTrue(handoff(self.work, [ap], [rp])['ready'])
        worksheet = self.work / 'worksheets/s0001.json'
        rows = read_json(worksheet)
        rows[1].update(reviewed=False, text=None)
        write_json(worksheet, rows)
        report = handoff(self.work, [ap], [rp])
        self.assertFalse(report['ready'])
        self.assertEqual(report['pending_rows'], 1)

    def test_complete_cross_proposal_retained_link_overrides_old_exclusion(self):
        ap, rp, assignment, result = self.fixture()
        rows = read_json(self.work / 'worksheets/s0001.json')
        second_assignment = copy.deepcopy(assignment)
        second_assignment.update(id='worker-b', assigned_ids=[rows[1]['id']], context_ids=[])
        second_result = {'version': 1, 'assignment_id': 'worker-b', 'source_sha256': assignment['source_sha256'],
                         'glossary_sha256': assignment['glossary_sha256'], 'rows': [copy.deepcopy(rows[1])], 'status': 'complete'}
        ap2, rp2 = self.root / 'assignment-b.json', self.root / 'result-b.json'
        result['rows'][0].update(action='alternate', links=['s0001:c000002'], note='Proposed retained duplicate')
        rows[1].update(action='credit', note='Old excluded decision')
        write_json(self.work / 'worksheets/s0001.json', rows)
        for path, value in ((ap, assignment), (rp, result), (ap2, second_assignment), (rp2, second_result)):
            write_json(path, value)
        self.assertTrue(handoff(self.work, [ap, ap2], [rp, rp2])['ready'])
        second_result['rows'][0].update(reviewed=False, text=None)
        second_result['status'] = 'pending'
        write_json(rp2, second_result)
        self.assertFalse(handoff(self.work, [ap, ap2], [rp, rp2])['ready'])

    def test_duplicate_results_rows_and_missing_complete_notes_refuse(self):
        ap, rp, _, result = self.fixture()
        with self.assertRaisesRegex(ValueError, 'duplicated'):
            handoff(self.work, [ap], [rp, rp])
        result['rows'].append(copy.deepcopy(result['rows'][0]))
        write_json(rp, result)
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            handoff(self.work, [ap], [rp])
        result['rows'].pop()
        for changed in ({'action': 'credit', 'note': ''}, {'end_ms': 0}, {'start_ms': 1100, 'timing_note': ''}):
            original = copy.deepcopy(result['rows'][0])
            result['rows'][0].update(changed)
            write_json(rp, result)
            with self.subTest(change=changed), self.assertRaises(ValueError):
                handoff(self.work, [ap], [rp])
            result['rows'][0] = original


if __name__ == "__main__":
    unittest.main()
