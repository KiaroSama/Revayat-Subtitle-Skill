"""Public workspace contracts reject malformed editable data without losing sources."""

import unittest
import copy
import io
import json
from pathlib import Path
import struct
import subprocess
import sys
import zipfile
from unittest.mock import patch

from check import CLI, WorkspaceCase, completed
from runtime import read_json, write_json
from workflow import build, load, prepare
from subtitle_formats import parse, visible
from publication import rename_noreplace
from check import FIXTURES


class ContractTests(WorkspaceCase):
    def test_concurrent_empty_workspace_and_edition_are_not_replaced(self):
        created = []
        def competitor(source, destination):
            destination.mkdir()
            created.append((destination, destination.stat().st_ino))
            return rename_noreplace(source, destination)

        with patch("workflow.rename_noreplace", competitor):
            with self.assertRaises(OSError):
                prepare([FIXTURES / "episode.ass"], self.work, "Fixture Series", 1,
                        "utf-8-sig", None, "fa")
        self.assertEqual(created[0][0], self.work)
        self.assertEqual(self.work.stat().st_ino, created[0][1])
        self.assertEqual(list(self.work.iterdir()), [])

        self.work.rmdir()
        self.import_work()
        completed(self.work)
        with patch("workflow.rename_noreplace", competitor):
            with self.assertRaises(OSError):
                build(self.work)
        edition, original_inode = created[1]
        self.assertEqual(edition.stat().st_ino, original_inode)
        self.assertEqual(list(edition.iterdir()), [])

    def test_aggregate_workspace_bounds_apply_when_loading_existing_work(self):
        self.import_work()
        for limit, value, message in (("MAX_TOTAL_CUES", 1, "cue limits"),
                                      ("MAX_TOTAL", 1, "byte/cue limits"),
                                      ("MAX_WORKSPACE", 1, "byte limit")):
            with self.subTest(limit=limit), patch("workflow." + limit, value):
                with self.assertRaisesRegex(ValueError, message):
                    load(self.work)

    def test_wrong_record_types_report_the_field(self):
        self.import_work()
        completed(self.work)
        path = self.work / "project.json"
        original = read_json(path)
        for field, value in (("season", True), ("target_language", []), ("sources", [None]), ("episodes", [None])):
            with self.subTest(field=field):
                changed = copy.deepcopy(original)
                changed[field] = value
                write_json(path, changed)
                with self.assertRaisesRegex(ValueError, field):
                    load(self.work)
        write_json(path, original)
        write_json(self.work / "worksheets/s0001.json", [None])
        with self.assertRaises(ValueError) as caught:
            load(self.work)
        self.assertIn("worksheets/s0001.json[0]", str(caught.exception))

    def test_duplicate_keys_and_nonfinite_json_are_rejected(self):
        path = self.root / "metadata.json"
        for body in ('{"x":1,"x":2}', '{"x":NaN}', '{"x":Infinity}', '{"x":1e9999}', '{"x":'):
            with self.subTest(body=body):
                path.write_text(body, encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "metadata.json"):
                    read_json(path)

    def test_hostless_or_credentialed_research_is_rejected(self):
        self.import_work()
        completed(self.work)
        path = self.work / "glossary.json"
        terms = read_json(path)
        for url in ("https:", "https://", "https://u:p@example.org", "https://example.org:99999", "https://bad host"):
            terms["research"][0]["url"] = url
            write_json(path, terms)
            with self.assertRaisesRegex(ValueError, "research"):
                build(self.work)

    def test_unsupported_archive_has_controlled_cli_failure(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("episode.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello\n")
        data = bytearray(buffer.getvalue())
        struct.pack_into("<H", data, 8, 99)
        struct.pack_into("<H", data, data.index(b"PK\x01\x02") + 10, 99)
        path = self.root / "unsupported.zip"
        path.write_bytes(data)
        result = subprocess.run([sys.executable, "-B", str(CLI), "prepare", str(path), "--work",
                                 str(self.work), "--series", "Fixture Series", "--season", "1"],
                                capture_output=True, text=True, encoding="utf-8", timeout=15,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.assertEqual(result.returncode, 2)
        self.assertIn("Unsupported ZIP compression", result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        self.assertFalse(self.work.exists())

    def test_canonical_episode_range_and_kind(self):
        self.import_work()
        completed(self.work)
        path = self.work / "project.json"
        project = read_json(path)
        for name in ("S01OVA001", "S01E۰۱", "S01E０１", "S01E00", "S02E01"):
            project["episodes"][0]["id"] = name
            write_json(path, project)
            with self.assertRaisesRegex(ValueError, "Episode"):
                build(self.work)
        project["episodes"][0]["id"] = "S01E100"
        write_json(path, project)
        self.assertEqual(build(self.work)["episodes"][0]["id"], "S01E100")

    def test_duplicate_basenames_have_distinct_source_origins(self):
        roots = [self.root / "english", self.root / "persian"]
        for root in roots:
            root.mkdir()
            (root / "episode.srt").write_text("1\n00:00:01,000 --> 00:00:02,000\nHello\n", encoding="utf-8")
        result = prepare(roots, self.work, "Fixture Series", 1, "utf-8", None, "fa")
        sources = result["sources"]
        self.assertNotEqual(sources[0]["name"], sources[1]["name"])
        self.assertEqual(sources[0]["origin"]["relative_path"], "episode.srt")
        self.assertNotEqual(sources[0]["origin"]["root_id"], sources[1]["origin"]["root_id"])

    def test_donor_timing_and_emitted_provenance_are_recorded(self):
        self.import_work()
        completed(self.work)
        sheet_path = self.work / "worksheets/s0002.json"
        rows = read_json(sheet_path)
        rows[2].update(start_ms=1001, end_ms=2009, timing_note="Authored timing conversion case")
        write_json(sheet_path, rows)
        result = build(self.work)
        episode = result["episodes"][0]
        from pathlib import Path
        doc = parse((Path(result["build"]) / episode["file"]).read_text(encoding="utf-8"), "ass")
        index = next(i for i, cue in enumerate(doc.cues, 1) if "دروازه" in visible(cue.text, "ass"))
        record = next(item for item in episode["provenance"] if item["source"] == "s0002")
        self.assertEqual(record["emitted_index"], index)
        self.assertEqual(record["timing"]["reviewed"], [1001, 2009])
        self.assertEqual(record["timing"]["emitted"], [1000, 2000])
        self.assertEqual(record["timing"]["conversion"], "floor-centisecond")

    def test_episode_alias_is_rejected_before_publication(self):
        self.import_work()
        completed(self.work)
        path = self.work / "project.json"
        project = read_json(path)
        project["episodes"][0]["id"] = "S01E001"
        write_json(path, project)
        with self.assertRaisesRegex(ValueError, "Episode"):
            build(self.work)
        self.assertFalse((self.work / "builds").exists())

    def test_project_root_type_is_controlled(self):
        self.work.mkdir()
        write_json(self.work / "project.json", [])
        with self.assertRaisesRegex(ValueError, "project.json"):
            load(self.work)


if __name__ == "__main__":
    unittest.main()
