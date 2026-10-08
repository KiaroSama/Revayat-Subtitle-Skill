"""Intentional physical SRT cue separators receive an early actionable refusal."""
import unittest

from check import WorkspaceCase
from runtime import read_json, write_json
from workflow import build, prepare


class SrtEmptyLineTests(WorkspaceCase):
    def workspace(self):
        source = self.root / "source.srt"
        source.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nOriginal\n")
        prepare([source], self.work, "Authored", 1, "utf-8", None, "fa")
        project = read_json(self.work / "project.json")
        project["episodes"] = [{"id": "S01E01", "base": "s0001", "comparison": "Authored source"}]
        write_json(self.work / "project.json", project)
        write_json(self.work / "glossary.json", {"series": "Authored", "terms_reviewed": True,
                   "terms": [], "research": [{"url": "https://example.org", "note": "Authored fixture"}]})
        return source

    def test_build_refuses_preserved_separators_before_publication_and_safe_retry(self):
        source = self.workspace()
        original = source.read_bytes()
        sheet = self.work / "worksheets/s0001.json"
        rows = read_json(sheet)
        for text in ("A\n\nB", "A\r\n\r\nB", "A\r\rB", "A\n \t\nB"):
            rows[0].update(text=text, reviewed=True, preserve_empty_lines=True, structure_note="Intentional layout")
            write_json(sheet, rows)
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "c000001.*SRT.*separator"):
                build(self.work)
            self.assertFalse((self.work / "builds").exists())
        rows[0]["text"] = "A\nB"
        write_json(sheet, rows)
        result = build(self.work)
        self.assertEqual(result["episodes"][0]["cues"], 1)
        self.assertEqual(source.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
