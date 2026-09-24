"""Regression and control checks for the September 2026 follow-up audit."""

from __future__ import annotations

import logging
import os
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "skills/revayat-subtitle/scripts"
sys.path.insert(0, str(SCRIPTS))

import markup
import png_validation
import render
import validation
import workflow
from publication import publish_bytes
from runtime import digest, operational_log, read_json, run, write_json
from subtitle_formats import Cue, Document, DEFAULT_STYLE, STYLE_FIELDS, ASS_FIELDS
from subtitle_formats import parse, serialize, srt_to_ass, timestamp, timecode, visible

CLI = SCRIPTS / "revayat-subtitle.py"


def ass_source(text="Hello.", start="0:00:01.00", end="0:00:03.00"):
    return ("[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n"
            "[V4+ Styles]\nFormat: " + STYLE_FIELDS + "\nStyle: " + DEFAULT_STYLE +
            "\n[Events]\nFormat: " + ASS_FIELDS +
            f"\nDialogue: 0,{start},{end},Default,,0,0,0,,{text}\n")


def png_chunk(kind, body):
    return (struct.pack(">I", len(body)) + kind + body +
            struct.pack(">I", zlib.crc32(body, zlib.crc32(kind))))


def one_pixel_png(palettes=0, after_image=False):
    palette = png_chunk(b"PLTE", b"\0\0\0") * palettes
    header = b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    image = png_chunk(b"IDAT", zlib.compress(b"\0\0\0\0"))
    return header + (image + palette if after_image else palette + image) + png_chunk(b"IEND", b"")


class AuditCase(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="revayat-reaudit-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        logging.info("Audit case started case=%s", self.id())

    def workspace(self, content=None, kind="srt"):
        source = self.root / ("input." + kind)
        source.write_text(content or "1\n00:00:01,000 --> 00:00:03,000\nسلام OVA.\n", encoding="utf-8")
        work = self.root / "work"
        workflow.prepare([source], work, "Authored fixture", 1, "utf-8", None, "fa")
        project = read_json(work / "project.json")
        project["episodes"] = [{"id": "S01E01", "base": "s0001", "comparison": "Only authored source"}]
        write_json(work / "project.json", project)
        write_json(work / "glossary.json", {
            "series": "Authored fixture", "terms_reviewed": True, "terms": [],
            "research": [{"url": "https://example.org/fixture", "note": "Authored test, not anime research"}]})
        rows = read_json(work / "worksheets/s0001.json")
        for row in rows:
            row.update(reviewed=True, action="preserve")
        write_json(work / "worksheets/s0001.json", rows)
        return work

    def link(self, path, target, directory=False):
        try:
            path.symlink_to(target, target_is_directory=directory)
        except OSError:
            if not directory or os.name != "nt":
                self.skipTest("Native symbolic-link permission is unavailable")
            result = subprocess.run(["cmd", "/c", "mklink", "/J", str(path), str(target)],
                                    capture_output=True, timeout=10)
            if result.returncode:
                self.fail("Windows junction fixture could not be created")
        def remove():
            if path.is_symlink():
                path.unlink()
            elif os.path.lexists(path):
                os.rmdir(path)
        self.addCleanup(remove)

    def test_linked_render_root_rejected_before_external_tools_or_writes(self):
        work = self.workspace()
        build = Path(workflow.build(work)["build"])
        external = self.root / "external"
        external.mkdir()
        self.link(build / "renders", external, True)
        with patch.object(render, "run") as child:
            with self.assertRaisesRegex(ValueError, "[Ll]ink"):
                render.render(build, "S01E01", sys.executable, None, None, False)
        child.assert_not_called()
        self.assertEqual(list(external.iterdir()), [])

    def test_linked_project_record_is_not_accepted(self):
        work = self.workspace()
        original, external = work / "project.json", self.root / "project-external.json"
        original.rename(external)
        self.link(original, external)
        with self.assertRaisesRegex(ValueError, "[Ll]ink"):
            workflow.load(work)

    def test_linked_workspace_glossary_is_not_accepted(self):
        work = self.workspace()
        original, external = work / "glossary.json", self.root / "glossary-external.json"
        original.rename(external)
        self.link(original, external)
        with self.assertRaisesRegex(ValueError, "[Ll]ink"):
            workflow.build(work)
        self.assertFalse((work / "builds").exists())

    def test_linked_build_manifest_is_not_accepted(self):
        build = Path(workflow.build(self.workspace())["build"])
        original, external = build / "manifest.json", self.root / "manifest-external.json"
        original.rename(external)
        self.link(original, external)
        with self.assertRaisesRegex(ValueError, "[Ll]ink"):
            render.load_build(build)

    def test_linked_existing_subtitle_is_not_accepted_by_rebuild(self):
        work = self.workspace()
        build = Path(workflow.build(work)["build"])
        original, external = build / "Sub/S01E01.srt", self.root / "subtitle-external.srt"
        original.rename(external)
        self.link(original, external)
        with self.assertRaisesRegex(ValueError, "[Ll]ink"):
            workflow.build(work)

    def test_existing_build_glossary_must_match_on_rebuild(self):
        work = self.workspace()
        build = Path(workflow.build(work)["build"])
        write_json(build / "glossary.json", {"series": "Not the reviewed glossary"})
        with self.assertRaisesRegex(ValueError, "glossary"):
            workflow.build(work)

    def test_out_of_range_srt_fails_before_workspace_publication(self):
        source = self.root / "large.srt"
        source.write_text("1\n300000:00:00,000 --> 300000:00:01,000\nHello.\n", encoding="utf-8")
        work = self.root / "work"
        with self.assertRaisesRegex(ValueError, "timestamp"):
            workflow.prepare([source], work, "Authored fixture", 1, "utf-8", None, "fa")
        self.assertFalse(work.exists())

    def test_out_of_range_ass_fails_before_workspace_publication(self):
        source = self.root / "large.ass"
        source.write_text(ass_source(start="300000:00:00.00", end="300000:00:01.00"), encoding="utf-8")
        work = self.root / "work"
        with self.assertRaisesRegex(ValueError, "timestamp"):
            workflow.prepare([source], work, "Authored fixture", 1, "utf-8", None, "fa")
        self.assertFalse(work.exists())

    def test_timestamp_boundaries_are_shared_with_the_worksheet(self):
        maximum = 10**12
        self.assertEqual(timestamp("277777:46:40,000", "srt"), maximum)
        self.assertEqual(timestamp("277777:46:40.00", "ass"), maximum)
        for kind in ("srt", "ass"):
            with self.subTest(kind=kind):
                self.assertEqual(timestamp(timecode(maximum, kind), kind), maximum)
                with self.assertRaises(ValueError):
                    timecode(maximum + 1, kind)
        with self.assertRaises(ValueError):
            timestamp("277777:46:40,001", "srt")

    def test_timestamp_huge_and_nonascii_numbers_are_rejected(self):
        for value in ("9" * 10000 + ":00:00,000", "۰۰:۰۰:۰۱,۰۰۰"):
            with self.subTest(length=len(value)):
                with self.assertRaises(ValueError):
                    timestamp(value, "srt")

    def test_repeated_animation_is_sampled_for_each_event(self):
        for tag in (r"\move(0,0,10,10)", r"\fad(100,100)", r"\fade(255,0,255,0,100,800,1000)",
                    r"\t(0,100,\fs50)", r"\k10", r"\K10", r"\kf10", r"\ko10", r"\kt10"):
            with self.subTest(tag=tag):
                doc = Document("ass", [
                    Cue(f"c{i:06}", i * 2000 - 1000, i * 2000, "{" + tag + "}متن", {"style": "Default"})
                    for i in range(1, 4)])
                plan = render.sample_plan(doc, False)
                self.assertEqual({i for row in plan for i in row["cue_indices"]}, {1, 2, 3})
                self.assertEqual([r["time_seconds"] for r in plan if 2 in r["cue_indices"]], [3.1, 3.5, 3.9])

    def test_sampler_version_invalidates_old_sampling_receipts(self):
        self.assertGreaterEqual(render.SAMPLER_VERSION, 3)

    def test_static_duplicate_signatures_remain_bounded(self):
        doc = Document("ass", [Cue(f"c{i:06}", i * 2000 - 1000, i * 2000, "متن", {"style": "Default"})
                               for i in range(1, 101)])
        self.assertEqual(len(render.sample_plan(doc, False)), 2)

    def test_srt_cr_and_crlf_use_the_same_logical_line_model(self):
        doc = Document("srt", [Cue("c000001", 1000, 2000, "original")])
        for newline in ("\n", "\r", "\r\n"):
            with self.subTest(newline=repr(newline)):
                row = {"id": "c000001", "source_text": "original", "text": "سلام" + newline + "دنیا",
                       "reviewed": True, "action": "edit", "start_ms": 1000, "end_ms": 2000}
                kept, _ = workflow.reviewed_cues(doc, [row])
                self.assertNotIn("\r", kept[0].text)
                self.assertEqual(kept[0].text.count(markup.RLM), 4)
                self.assertEqual(parse(serialize(doc, kept), "srt").cues[0].text, kept[0].text)

    def test_srt_donor_crlf_becomes_one_ass_linebreak(self):
        cue = srt_to_ass(Cue("c000001", 1000, 2000, "<i>Hello</i>\r\nWorld"))
        self.assertNotIn("\r", cue.text)
        self.assertNotIn("\n", cue.text)
        self.assertEqual(cue.text, r"{\i1}Hello{\i0}\NWorld")

    def test_duplicate_png_palette_is_refused(self):
        with self.assertRaisesRegex(ValueError, "palette"):
            png_validation.decode_png(one_pixel_png(palettes=2))

    def test_single_optional_palette_and_plain_rgb_are_supported(self):
        for count in (0, 1):
            self.assertEqual(png_validation.decode_png(one_pixel_png(palettes=count)), (1, 1))

    def test_palette_after_image_is_refused(self):
        with self.assertRaisesRegex(ValueError, "palette"):
            png_validation.decode_png(one_pixel_png(palettes=1, after_image=True))

    def test_corrupt_deflate_gets_controlled_cli_failure(self):
        source = self.root / "corrupt.zip"
        with zipfile.ZipFile(source, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("episode.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello.\n")
        data = bytearray(source.read_bytes())
        offset = 30 + int.from_bytes(data[26:28], "little") + int.from_bytes(data[28:30], "little")
        data[offset] = 7  # Reserved BTYPE=3; the archive directory remains intact.
        source.write_bytes(data)
        work = self.root / "work"
        result = subprocess.run([sys.executable, "-S", str(CLI), "prepare", str(source), "--work", str(work),
                                 "--series", "Authored fixture", "--season", "1"],
                                capture_output=True, timeout=15,
                                env={**os.environ, "REVAYAT_LOG_DIR": str(self.root / "logs")})
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(b"Traceback", result.stderr)
        self.assertIn(b"compressed stream", result.stderr)
        self.assertFalse(work.exists())

    def test_corrupt_lzma_gets_controlled_import_failure(self):
        try:
            import lzma
        except ImportError:
            self.skipTest("Python was built without optional LZMA")
        source = self.root / "corrupt-lzma.zip"
        with zipfile.ZipFile(source, "w", compression=zipfile.ZIP_LZMA) as archive:
            archive.writestr("episode.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello.\n")
        data = bytearray(source.read_bytes())
        offset = 30 + int.from_bytes(data[26:28], "little") + int.from_bytes(data[28:30], "little")
        data[offset + 4] = 255  # Invalid LZMA property byte.
        source.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "compressed stream"):
            workflow.prepare([source], self.root / "work", "Authored fixture", 1, "utf-8", None, "fa")
        self.assertFalse((self.root / "work").exists())

    def test_supported_zip_compressions_still_import(self):
        for compression in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2, zipfile.ZIP_LZMA):
            with self.subTest(compression=compression):
                source = self.root / f"{compression}.zip"
                with zipfile.ZipFile(source, "w", compression=compression) as archive:
                    archive.writestr("episode.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello.\n")
                work = self.root / f"work-{compression}"
                workflow.prepare([source], work, "Authored fixture", 1, "utf-8", None, "fa")
                self.assertEqual(len(workflow.load(work)[1]), 1)

    def test_previous_recipe_build_is_preserved_when_upgrading(self):
        work = self.workspace()
        with patch.dict(workflow.GENERATION_RECIPE, {"version": 2, "normalization": 2}):
            old = Path(workflow.build(work)["build"])
        before = {p.relative_to(old): p.read_bytes() for p in old.rglob("*") if p.is_file()}
        new = Path(workflow.build(work)["build"])
        self.assertNotEqual(old, new)
        self.assertEqual(before, {p: (old / p).read_bytes() for p in before})
        self.assertFalse((new / "renders").exists())

    def test_prior_literal_markup_and_direction_guards_remain(self):
        self.assertEqual(visible("x < 5 > y", "srt"), "x < 5 > y")
        self.assertEqual(markup.remap_resets(r"{\rDefault}literal \rDefault", {"Default": "Donor"}),
                         r"{\rDonor}literal \rDefault")
        value = "سلام " + markup.RLM + "OVA برگشت."
        self.assertIn(value, markup.rtl(value, "ass"))
        self.assertNotIn(markup.RLE, markup.rtl("Meet علی in London.", "srt", "ltr"))
        self.assertEqual(markup.clean_empty_lines(r"سلام\N\Nدنیا", "ass"), r"سلام\Nدنیا")

    def test_seeded_rtl_normalization_is_idempotent_and_preserves_prose(self):
        rng = random.Random(924)
        tokens = ["سلام", "OVA", " ", markup.RLM, markup.LRM, r"{\i1}", r"{\i0}", r"\N", r"\n"]
        for _ in range(500):
            text = "".join(rng.choices(tokens, k=rng.randrange(1, 12)))
            result = markup.rtl(text, "ass")
            self.assertEqual(markup.rtl(result, "ass"), result)
            self.assertEqual(visible(text, "ass"), visible(result, "ass"))

    def test_source_and_invalid_episode_guards_remain(self):
        work = self.workspace()
        project = read_json(work / "project.json")
        project["episodes"][0]["id"] = "S01E001"
        write_json(work / "project.json", project)
        with self.assertRaisesRegex(ValueError, "canonical"):
            workflow.build(work)
        project["episodes"][0]["id"] = "S01E01"
        write_json(work / "project.json", project)
        source = work / "sources/s0001.srt"
        source.write_bytes(source.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "Source changed"):
            workflow.build(work)

    def test_publication_write_failure_remains_retryable(self):
        target = self.root / "archive.zip"
        with patch("publication.os.fsync", side_effect=OSError("Injected flush failure")):
            with self.assertRaises(OSError):
                publish_bytes(target, b"complete")
        self.assertFalse(target.exists())
        publish_bytes(target, b"complete")
        self.assertEqual(target.read_bytes(), b"complete")

    @unittest.skipUnless("--render" in sys.argv or os.environ.get("REVAYAT_TEST_RENDER") == "1",
                         "Real FFmpeg tier is selected explicitly")
    def test_real_animated_render_review_and_package_roundtrip(self):
        source = ass_source(r"{\move(300,600,900,600)}یک")
        source += ("Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,"
                   r"{\move(300,600,900,600)}دو" + "\n"
                   "Dialogue: 0,0:00:07.00,0:00:09.00,Default,,0,0,0,,"
                   r"{\move(300,600,900,600)}سه" + "\n")
        work = self.workspace(source, "ass")
        original = (work / "sources/s0001.ass").read_bytes()
        result = workflow.build(work)
        build = Path(result["build"])
        record = render.render(build, "S01E01", None, None, None, False)
        self.assertEqual(len(record["frames"]), 9)
        self.assertEqual({i for frame in record["frames"] for i in frame["cue_indices"]}, {1, 2, 3})
        with self.assertRaisesRegex(ValueError, "Inspect every"):
            render.qa(build)
        path = Path(record["review"])
        evidence = read_json(path)
        for frame in evidence["frames"]:
            frame.update(reviewed=True, note="Authored automated gate fixture, not human visual signoff.")
        write_json(path, evidence)
        self.assertEqual(render.qa(build)["reviewed_frames"], 9)
        output = self.root / "Sub.zip"
        render.package(build, output)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(archive.namelist(), ["Sub/S01E01.ass"])
            self.assertEqual(digest(archive.read("Sub/S01E01.ass")), result["episodes"][0]["sha256"])
        self.assertEqual((work / "sources/s0001.ass").read_bytes(), original)

    def test_json_duplicate_and_nonfinite_guards_remain(self):
        path = self.root / "bad.json"
        for text in ('{"a": 1, "a": 2}', '{"x": NaN}', '{"x": 1e999}'):
            path.write_text(text, encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "invalid JSON"):
                read_json(path)


if __name__ == "__main__":
    with operational_log("reaudit"):
        unittest.main(verbosity=2)
