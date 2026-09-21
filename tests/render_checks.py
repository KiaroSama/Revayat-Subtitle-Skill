"""Native renderer checks; loaded only by the explicit render CI bucket."""

import os
from pathlib import Path
import shutil

from check import WorkspaceCase, completed
from render import ffmpeg_path, render
from runtime import read_json, run, write_json
from workflow import build, prepare


class ControlledFontRender(WorkspaceCase):
    def test_literal_srt_delimiters_render_as_visible_content(self):
        source = self.root / "literal.srt"
        source.write_text("1\n00:00:01,001 --> 00:00:02,009\nx < 5 and y > 2\n", encoding="utf-8")
        prepare([source], self.work, "Literal Fixture", 1, "utf-8", None, "fa")
        project = read_json(self.work / "project.json")
        project["episodes"] = [{"id": "S01E01", "base": "s0001", "alternates": [], "comparison": "Only authored candidate"}]
        write_json(self.work / "project.json", project)
        write_json(self.work / "glossary.json", {"series": "Literal Fixture", "terms_reviewed": True,
                   "research": [{"url": "https://example.org/fixture", "note": "Authored structural fixture"}], "terms": []})
        worksheet = self.work / "worksheets/s0001.json"
        rows = read_json(worksheet)
        rows[0].update(text="x < 5 و y > 2 رو نگه دار.", reviewed=True)
        write_json(worksheet, rows)
        result = build(self.work)
        edition = Path(result["build"])
        evidence = render(edition, "S01E01", None, None, None, True)
        self.assertIn("x < 5", (edition / "Sub/S01E01.srt").read_text(encoding="utf-8"))
        frame = evidence["frames"][0]
        pixels = run([ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-i",
                      str(edition / frame["file"]), "-frames:v", "1", "-pix_fmt", "gray",
                      "-f", "rawvideo", "pipe:1"], timeout=15)
        self.assertGreater(len(set(pixels)), 10)

    def test_default_samples_include_inline_font_and_reset_pixels(self):
        font = (Path(os.environ["WINDIR"]) / "Fonts/arial.ttf" if os.name == "nt"
                else Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
        self.assertTrue(font.is_file(), "The render CI lane must provide its controlled font")
        fonts = self.root / "fonts"
        fonts.mkdir()
        shutil.copyfile(font, fonts / font.name)
        family = "Arial" if os.name == "nt" else "DejaVu Sans"
        self.import_work()
        completed(self.work)
        worksheet = self.work / "worksheets/s0001.json"
        rows = read_json(worksheet)
        rows[2]["text"] = r"{\rResetOnly\fn" + family + r"\fsp0\fe-1}من دیروز OVA رو دیدم؛ اسمش «امید» بود."
        rows[2]["structure_note"] = "Authored renderer fixture for reset, inline font and mixed text"
        write_json(worksheet, rows)
        result = build(self.work)
        edition = Path(result["build"])
        evidence = render(edition, "S01E01", None, None, fonts, False)
        record = read_json(Path(evidence["review"]))
        receipt = read_json(edition / record["receipt_file"])
        self.assertEqual(receipt["recipe"]["fonts"][0]["name"], font.name)
        self.assertIn(family, receipt["recipe"]["requested_fonts"])
        # This proves sampling and visible raster output, not correct Persian shaping.
        frames = record["frames"]
        self.assertTrue(frames)
        populated = 0
        for frame in frames:
            pixels = run([ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin",
                          "-i", str(edition / frame["file"]), "-frames:v", "1", "-pix_fmt", "gray",
                          "-f", "rawvideo", "pipe:1"], timeout=15, idle_timeout=10)
            populated += len(set(pixels)) > 10
        self.assertGreater(populated, 0)
