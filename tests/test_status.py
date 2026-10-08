"""Read-only status validates pending, current and stale workspace identities."""
import json
import os
from pathlib import Path
import sys
import unittest

from check import CLI, WorkspaceCase, completed
from runtime import digest, read_json, write_json
from status import status
from process_helpers import run_child
from workflow import build


class StatusTests(WorkspaceCase):
    def snapshot(self):
        return {path.relative_to(self.work).as_posix(): digest(path.read_bytes())
                for path in self.work.rglob("*") if path.is_file()}

    def test_pending_overview_and_cli_leave_workspace_unchanged(self):
        self.import_work()
        before = self.snapshot()
        result = status(self.work)
        self.assertFalse(result["ready"])
        self.assertEqual(result["version"], 1)
        self.assertEqual(result["first_pending"], {"source": "s0001", "cue": "c000001"})
        self.assertEqual(result["unassigned_sources"], ["s0001", "s0002"])
        self.assertNotIn("source_text", json.dumps(result))
        cli = run_child([sys.executable, "-B", str(CLI), "status", "--work", str(self.work)], timeout=15)
        self.assertEqual(cli.returncode, 1, cli.stderr.decode("utf-8"))
        self.assertEqual(json.loads(cli.stdout)["pending_cues"], result["pending_cues"])
        self.assertEqual(self.snapshot(), before)

    def test_current_build_missing_render_is_pending_and_tamper_invalid(self):
        self.import_work()
        completed(self.work)
        edition = Path(build(self.work)["build"])
        result = status(self.work)
        self.assertFalse(result["ready"])
        self.assertEqual(result["editions"][0]["state"], "pending")
        self.assertEqual(result["editions"][0]["visual_pending"], 1)
        (edition / "Sub/S01E01.ass").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "changed"):
            status(self.work)

    def test_stale_source_and_changed_row_mapping_refuse(self):
        self.import_work()
        sheet = self.work / "worksheets/s0001.json"
        rows = read_json(sheet)
        rows.reverse()
        write_json(sheet, rows)
        with self.assertRaisesRegex(ValueError, "coverage/order"):
            status(self.work)
        rows.reverse()
        write_json(sheet, rows)
        (self.work / "sources/s0001.ass").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "Source changed"):
            status(self.work)


    @unittest.skipUnless('--render' in sys.argv or os.environ.get('REVAYAT_TEST_RENDER') == '1', 'Actual rendered status evidence tier')
    def test_render_pending_complete_and_corrupt_cli_states_are_read_only(self):
        import render
        self.import_work()
        completed(self.work)
        edition = Path(build(self.work)['build'])
        evidence = render.render(edition, 'S01E01', None, None, None, False)
        before = self.snapshot()
        result = run_child([sys.executable, '-B', str(CLI), 'status', '--work', str(self.work)], timeout=20)
        self.assertEqual(result.returncode, 1, result.stderr.decode('utf-8'))
        self.assertEqual(self.snapshot(), before)
        review_path = Path(evidence['review'])
        record = read_json(review_path)
        for frame in record['frames']:
            frame.update(reviewed=True, note='Authored mechanical gate fixture, not human visual review.')
        write_json(review_path, record)
        before = self.snapshot()
        result = run_child([sys.executable, '-B', str(CLI), 'status', '--work', str(self.work)], timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8'))
        self.assertTrue(json.loads(result.stdout)['ready'])
        self.assertEqual(self.snapshot(), before)
        receipt = edition / record['receipt_file']
        original_receipt = receipt.read_bytes()
        changed_receipt = read_json(receipt)
        changed_receipt['recipe']['sampler'] = 0
        write_json(receipt, changed_receipt)
        record['receipt_sha256'] = digest(receipt.read_bytes())
        write_json(review_path, record)
        output = run_child([sys.executable, '-B', str(CLI), 'status', '--work', str(self.work)], timeout=20)
        self.assertEqual(output.returncode, 2)
        self.assertNotIn(b'Traceback', output.stderr)
        self.assertIn(b'sampling/profile', output.stderr)
        receipt.write_bytes(original_receipt)
        record['receipt_sha256'] = digest(original_receipt)
        write_json(review_path, record)
        image = edition / record['frames'][0]['file']
        image.write_bytes(image.read_bytes() + b'changed')
        before = self.snapshot()
        result = run_child([sys.executable, '-B', str(CLI), 'status', '--work', str(self.work)], timeout=20)
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(b'Traceback', result.stderr)
        self.assertEqual(self.snapshot(), before)

    def test_linked_status_input_refuses_controlled_cli_without_mutation(self):
        self.import_work()
        path = self.work / 'glossary.json'
        original = path.read_bytes()
        real = self.root / 'glossary-copy.json'
        real.write_bytes(original)
        path.unlink()
        try:
            path.symlink_to(real)
        except OSError:
            path.write_bytes(original)
            self.skipTest('Native symbolic-link permission unavailable; Windows junction coverage remains in retained tests')
        result = run_child([sys.executable, '-B', str(CLI), 'status', '--work', str(self.work)], timeout=15)
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(b'Traceback', result.stderr)
        self.assertEqual(real.read_bytes(), original)
        self.assertTrue(path.is_symlink())

    def test_old_edition_is_stale_not_selected_after_glossary_edit(self):
        self.import_work()
        completed(self.work)
        build(self.work)
        path = self.work / 'glossary.json'
        glossary = read_json(path)
        glossary['research'][0]['note'] += ' Revised authored context.'
        write_json(path, glossary)
        before = self.snapshot()
        report = status(self.work)
        self.assertFalse(report['ready'])
        self.assertEqual(report['editions'][0]['state'], 'stale')
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
