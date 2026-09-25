"""Authored regressions for subtitle text scopes and bounded artifact IO."""
from __future__ import annotations

import logging
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "skills/revayat-subtitle/scripts"
sys.path.insert(0, str(SCRIPTS))

import markup
import runtime
import validation
import workflow
from subtitle_formats import (Cue, Document, ASS_FIELDS, STYLE_FIELDS, DEFAULT_STYLE,
                              parse, serialize, srt_to_ass, style_references, visible)


def ass_source(text="Hello."):
    return ("[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n"
            "[V4+ Styles]\nFormat: " + STYLE_FIELDS + "\nStyle: " + DEFAULT_STYLE +
            "\n[Events]\nFormat: " + ASS_FIELDS +
            "\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,," + text + "\n")


class BoundaryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="subtitle-boundaries-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        logging.info("Boundary regression case=%s", self.id())

    def test_srt_break_variants_match_display_lines(self):
        for tag in ("<br>", "<br/>", "<br />", "<BR>", "<Br  />"):
            with self.subTest(tag=tag):
                source = "سلام" + tag + "دنیا OVA"
                self.assertEqual(visible(source, "srt"), "سلام\nدنیا OVA")
                normalized = markup.rtl(source, "srt")
                self.assertEqual(normalized.count(markup.RLM), 4)
                self.assertEqual(visible(normalized, "srt"), "سلام\nدنیا OVA")
                self.assertEqual(markup.rtl(normalized, "srt"), normalized)

    def test_break_donor_conversion_does_not_emit_literal_html(self):
        for tag in ("<br>", "<br/>", "<br />", "<BR>"):
            with self.subTest(tag=tag):
                self.assertEqual(srt_to_ass(Cue("c000001", 1000, 2000, "A" + tag + "B")).text, r"A\NB")

    def test_break_only_source_is_empty_not_literal_prose(self):
        self.assertEqual(visible("<br><br/>", "srt"), "")
        self.assertEqual(markup.clean_empty_lines("A<br><br>B", "srt"), "A\nB")

    def test_angle_literals_entities_and_regular_tags_are_unchanged(self):
        text = "<i>Hi</i> x < 5 > y &amp; <break>"
        self.assertEqual(visible(text, "srt"), "Hi x < 5 > y &amp; <break>")
        self.assertEqual(srt_to_ass(Cue("c000001", 1000, 2000, "<b>Hi</b>")).text, r"{\b1}Hi{\b0}")

    def test_parenthesized_reset_is_a_reference_to_its_inner_name(self):
        self.assertEqual(style_references(Cue("c000001", 1000, 2000, r"{\r(Default)}Hi", {"style": "Default"})), {"Default"})
        self.assertEqual(markup.remap_resets(r"{\r(Default)}literal \rDefault", {"Default": "Donor"}),
                         r"{\r(Donor)}literal \rDefault")

    def test_parenthesized_scalars_preserve_argument_spans(self):
        source = r"{\fn(DejaVu Sans)\p(1)\p(0)\fs(32)}"
        values = list(markup.overrides(source))
        self.assertEqual([(name, arg) for name, arg, _, _ in values],
                         [("fn", "DejaVu Sans"), ("p", "1"), ("p", "0"), ("fs", "32")])
        for _, arg, start, end in values:
            self.assertEqual(source[start:end], arg)
        self.assertTrue(any(kind == "drawing" for kind, _ in markup.pieces(r"{\p(1)}m 0 0 l 1 1{\p(0)}Hi", "ass")))

    def test_rotation_blur_and_scale_aliases_are_not_shorter_tags(self):
        self.assertEqual([(name, arg) for name, arg, _, _ in markup.overrides(r"{\fr45\be1\fsc100}")],
                         [("fr", "45"), ("be", "1"), ("fsc", "100")])

    def test_nested_transforms_and_literal_reset_stay_compatible(self):
        self.assertEqual(markup.remap_resets(r"{\t(0,100,\rDefault)\r}x", {"Default": "Mapped"}),
                         r"{\t(0,100,\rMapped)\r}x")
        self.assertEqual(markup.remap_resets(r"{\r(Default)}x", {"Default": "Mapped"}), r"{\r(Mapped)}x")
        with self.assertRaises(ValueError):
            list(markup.overrides(r"{\t(0,100,\fs42}"))

    def test_cross_line_isolates_and_embeddings_preserve_valid_scope(self):
        for kind, newline in (("ass", r"\N"), ("ass", r"\n"), ("srt", "\n")):
            for opening, closing in (("\u2066", "\u2069"), ("\u2067", "\u2069"), ("\u2068", "\u2069"),
                                     ("\u202a", "\u202c"), ("\u202b", "\u202c")):
                for words in (("سلام OVA", "دنیا"), ("سلام", "دنیا OVA")):
                    with self.subTest(kind=kind, newline=newline, opening=ord(opening), words=words):
                        source = opening + words[0] + newline + words[1] + closing
                        result = markup.rtl(source, kind)
                        markup.validate_bidi(result, kind)
                        self.assertEqual(visible(source, kind), visible(result, kind))
                        self.assertEqual(markup.rtl(result, kind), result)
                        self.assertEqual(result.count(opening), 1)
                        self.assertEqual(result.count(closing), 1)

    def test_nested_cross_line_scopes_with_inline_styles_are_preserved(self):
        source = "\u2067سلام {\\i1}OVA\\N\u202aEnglish\\Nدنیا\u2069"
        result = markup.rtl(source, "ass")
        markup.validate_bidi(result, "ass")
        self.assertEqual(markup.rtl(result, "ass"), result)
        self.assertEqual(visible(result, "ass"), visible(source, "ass"))
        self.assertIn(r"{\i1}", result)

    def test_invalid_user_scope_is_not_silently_repaired(self):
        for value in ("\u2067سلام", "\u202cسلام", "\u2069", "\u2067سلام\u202c\u2069"):
            with self.subTest(value=repr(value)), self.assertRaises(ValueError):
                markup.rtl(value, "srt")

    def test_seeded_scope_normalization_is_stable(self):
        rng = random.Random(925)
        for _ in range(250):
            opening, closing = rng.choice((("\u2066", "\u2069"), ("\u2067", "\u2069"), ("\u202b", "\u202c")))
            source = opening + rng.choice(("سلام OVA", "علی")) + r"\N" + rng.choice(("دنیا", "Test علی")) + closing
            result = markup.rtl(source, "ass")
            self.assertEqual(markup.rtl(result, "ass"), result)
            self.assertEqual(visible(source, "ass"), visible(result, "ass"))

    def test_nontext_codecs_fail_as_validation_errors_before_publication(self):
        source = self.root / "input.srt"
        source.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nHi\n")
        for encoding in ("base64", "rot13", "hex", "bz2", "zlib"):
            with self.subTest(encoding=encoding):
                work = self.root / encoding
                with self.assertRaises(ValueError):
                    workflow.prepare([source], work, "Fixture", 1, encoding, None, "fa")
                self.assertFalse(work.exists())

    def test_supported_text_codecs_still_import(self):
        for encoding in ("utf-8", "utf-16", "utf-16-le", "cp1256"):
            with self.subTest(encoding=encoding):
                source = self.root / (encoding + ".srt")
                source.write_bytes("1\n00:00:01,000 --> 00:00:02,000\nسلام\n".encode(encoding))
                work = self.root / encoding
                workflow.prepare([source], work, "Fixture", 1, encoding, None, "fa")
                self.assertEqual(workflow.load(work)[1]["s0001"].cues[0].text, "سلام")

    @unittest.skipUnless(hasattr(os, "mkfifo"), "POSIX named-pipe boundary")
    def test_named_pipe_is_refused_without_waiting_for_a_writer(self):
        path = self.root / "record.json"
        os.mkfifo(path)
        command = [sys.executable, "-S", "-c", "import sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);"
                   "from runtime import read_json;\ntry: read_json(Path(sys.argv[2]))\n"
                   "except ValueError: sys.exit(0)\nelse: sys.exit(3)", str(SCRIPTS), str(path)]
        try:
            result = subprocess.run(command, capture_output=True, timeout=2)
        except subprocess.TimeoutExpired:
            self.fail("Artifact reader blocked on a FIFO instead of rejecting it")
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))

    def test_byte_limit_validation_precedes_io(self):
        for maximum in (-1, True, 1.5):
            with self.subTest(maximum=maximum), self.assertRaises(ValueError):
                runtime.read_limited(self.root / "missing", maximum)

    def test_json_write_limit_preserves_existing_file(self):
        path = self.root / "record.json"
        path.write_bytes(b'{"old":true}\n')
        with patch.object(runtime, "MAX_JSON_BYTES", 128, create=True):
            with self.assertRaisesRegex(ValueError, "JSON.*byte|byte.*JSON"):
                runtime.write_json(path, {"large": "x" * 1024})
        self.assertEqual(path.read_bytes(), b'{"old":true}\n')
        self.assertEqual(list(self.root.glob(".write-*")), [])

    def test_json_expansion_is_refused_before_workspace_publication(self):
        source = self.root / "input.srt"
        source.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nA" + b"\x01" * 400 + b"B\n")
        work = self.root / "work"
        with patch.object(runtime, "MAX_JSON_BYTES", 1024, create=True):
            with self.assertRaises(ValueError):
                workflow.prepare([source], work, "Fixture", 1, "utf-8", None, "fa")
        self.assertFalse(work.exists())

    def test_json_limit_is_shared_by_reader_and_writer(self):
        path = self.root / "record.json"
        with patch.object(runtime, "MAX_JSON_BYTES", 128, create=True):
            runtime.write_json(path, {"ok": True})
            self.assertEqual(runtime.read_json(path), {"ok": True})
            path.write_bytes(b'{"x":"' + b"a" * 200 + b'"}')
            with self.assertRaises(ValueError):
                runtime.read_json(path)


    def workspace(self, kind="srt", text="سلام OVA"):
        source = self.root / ("source." + kind)
        source.write_text(ass_source(text) if kind == "ass" else
                          "1\n00:00:01,000 --> 00:00:03,000\n" + text + "\n", encoding="utf-8")
        work = self.root / "work"
        workflow.prepare([source], work, "Fixture", 1, "utf-8", None, "fa")
        project = runtime.read_json(work / "project.json")
        project["episodes"] = [{"id": "S01E01", "base": "s0001", "comparison": "Authored source only"}]
        runtime.write_json(work / "project.json", project)
        rows = runtime.read_json(work / "worksheets/s0001.json")
        for row in rows:
            row.update(action="preserve", reviewed=True)
        runtime.write_json(work / "worksheets/s0001.json", rows)
        runtime.write_json(work / "glossary.json", {"series": "Fixture", "terms_reviewed": True,
                           "terms": [], "research": [{"url": "https://example.org", "note": "Authored test only"}]})
        return work

    def test_existing_metadata_also_rejects_nontext_codec(self):
        work = self.workspace()
        path = work / "project.json"
        project = runtime.read_json(path)
        project["sources"][0]["encoding"] = "base64"
        runtime.write_json(path, project)
        with self.assertRaisesRegex(ValueError, "encoding"):
            workflow.load(work)

    def test_cli_nontext_codec_is_controlled_and_private_value_is_not_echoed(self):
        source = self.root / "source.srt"
        source.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nHi\n")
        for encoding in ("base64", "rot13", "hex", "bz2", "zlib"):
            with self.subTest(encoding=encoding):
                work = self.root / encoding
                result = subprocess.run([sys.executable, "-S", str(SCRIPTS / "revayat-subtitle.py"), "prepare",
                                         str(source), "--work", str(work), "--series", "Fixture", "--season", "1",
                                         "--encoding", encoding], capture_output=True, timeout=10,
                                        env={**os.environ, "REVAYAT_LOG_DIR": str(self.root / "logs")})
                self.assertEqual(result.returncode, 2)
                self.assertNotIn(b"Traceback", result.stderr)
                self.assertIn(b"text encoding", result.stderr)
                self.assertFalse(work.exists())

    def test_rebuild_uses_a_bounded_subtitle_reader(self):
        work = self.workspace()
        output = Path(workflow.build(work)["build"])
        path = output / "Sub/S01E01.srt"
        path.write_bytes(path.read_bytes() + b"x" * 100)
        with self.assertRaisesRegex(ValueError, "byte limit") as caught:
            workflow.build(work)
        self.assertIn("modified", str(caught.exception))

    def test_json_short_write_preserves_existing_target(self):
        path = self.root / "record.json"
        path.write_bytes(b'{"old":true}\n')
        original = os.fdopen
        class ShortWriter:
            def __init__(self, stream):
                self.stream = stream
            def __enter__(self):
                return self
            def write(self, data):
                return self.stream.write(data[:3])
            def __exit__(self, *args):
                self.stream.close()
        with patch.object(runtime.os, "fdopen", lambda fd, mode: ShortWriter(original(fd, mode))):
            with self.assertRaisesRegex(OSError, "incomplete"):
                runtime.write_json(path, {"new": True})
        self.assertEqual(path.read_bytes(), b'{"old":true}\n')

    def test_json_cleanup_does_not_mask_the_primary_error(self):
        path = self.root / "record.json"
        original = Path.unlink
        def unlink(target, *args, **kwargs):
            if target.name.startswith(".write-"):
                raise OSError("cleanup")
            return original(target, *args, **kwargs)
        with patch.object(runtime.os, "replace", side_effect=PermissionError("primary")), patch.object(Path, "unlink", unlink):
            with self.assertLogs(level="WARNING"), self.assertRaisesRegex(PermissionError, "primary"):
                runtime.write_json(path, {"new": True})
        self.assertFalse(path.exists())

    def test_legacy_effects_cover_every_event_and_phase(self):
        from render import sample_plan
        for effect in ("Banner;1;0;0", "Scroll up;0;720;1", "Scroll down;0;720;1"):
            with self.subTest(effect=effect):
                doc = Document("ass", [Cue(f"c{i:06}", i * 2000 - 1000, i * 2000, "متن",
                                           {"style": "Default", "effect": effect}) for i in range(1, 4)])
                plan = sample_plan(doc, False)
                self.assertEqual({i for row in plan for i in row["cue_indices"]}, {1, 2, 3})
                self.assertEqual([row["time_seconds"] for row in plan if 2 in row["cue_indices"]], [3.1, 3.5, 3.9])

    def test_middle_layout_changes_have_distinct_sampling_signatures(self):
        from render import sample_plan
        for field in ("layer", "marginl", "marginr", "marginv"):
            doc = Document("ass", [Cue(f"c{i:06}", i * 2000 - 1000, i * 2000, "متن",
                                       {"style": "Default", field: "80" if i == 2 else "0"}) for i in range(1, 4)])
            self.assertIn(2, {i for row in sample_plan(doc, False) for i in row["cue_indices"]})

    def test_rotated_blurred_middle_event_is_sampled(self):
        from render import sample_plan
        for tag in (r"\fr45", r"\be1", r"\fsc80"):
            doc = Document("ass", [Cue(f"c{i:06}", i * 2000 - 1000, i * 2000,
                                       ("{" + tag + "}" if i == 2 else "") + "متن",
                                       {"style": "Default"}) for i in range(1, 4)])
            self.assertIn(2, {i for row in sample_plan(doc, False) for i in row["cue_indices"]})

    def test_identical_static_events_are_still_representatively_sampled(self):
        from render import sample_plan
        doc = Document("ass", [Cue(f"c{i:06}", i * 2000 - 1000, i * 2000, "متن",
                                   {"style": "Default"}) for i in range(1, 101)])
        self.assertEqual(len(sample_plan(doc, False)), 2)
        with patch("render.MAX_RENDER_FRAMES", 1), self.assertRaisesRegex(ValueError, "budget"):
            sample_plan(doc, True)

    def test_many_ass_sections_keep_order_and_case_insensitive_duplicate_guard(self):
        text = ass_source() + "".join(f"\n[Custom {i}]\nkey: value\n" for i in range(6000))
        doc = parse(text, "ass")
        self.assertEqual(len(doc.sections), 6003)
        self.assertEqual(doc.sections[-1][0], "[Custom 5999]")
        self.assertIn("[Custom 5999]", serialize(doc, doc.cues))
        with self.assertRaisesRegex(ValueError, "Repeated"):
            parse(text + "\n[CUSTOM 0]\n", "ass")

    def test_previous_generation_keeps_workspace_and_edition_bytes(self):
        work = self.workspace()
        with patch.dict(workflow.GENERATION_RECIPE, {"version": 3, "normalization": 3}):
            old = Path(workflow.build(work)["build"])
        before = {p.relative_to(work): p.read_bytes() for p in work.rglob("*") if p.is_file()}
        new = Path(workflow.build(work)["build"])
        self.assertNotEqual(old, new)
        self.assertEqual(before, {p: (work / p).read_bytes() for p in before})
        self.assertFalse((new / "renders").exists())

    @unittest.skipUnless("--render" in sys.argv or os.environ.get("REVAYAT_TEST_RENDER") == "1",
                         "Explicit FFmpeg integration tier")
    def test_real_parenthesized_reset_render_and_complete_delivery(self):
        import render
        import zipfile
        work = self.workspace("ass", r"{\r(Default)\fn(DejaVu Sans)}سلام OVA")
        result = workflow.build(work)
        edition = Path(result["build"])
        evidence = render.render(edition, "S01E01", None, None, None, False)
        path = Path(evidence["review"])
        record = runtime.read_json(path)
        with self.assertRaisesRegex(ValueError, "Inspect every"):
            render.qa(edition)
        for frame in record["frames"]:
            frame.update(reviewed=True, note="Automated gate fixture; not human visual approval.")
        runtime.write_json(path, record)
        self.assertTrue(render.qa(edition)["ok"])
        output = self.root / "Sub.zip"
        render.package(edition, output)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(runtime.digest(archive.read("Sub/S01E01.ass")), result["episodes"][0]["sha256"])

    @unittest.skipUnless("--render" in sys.argv or os.environ.get("REVAYAT_TEST_RENDER") == "1",
                         "Explicit FFmpeg integration tier")
    def test_real_srt_break_and_parenthesized_scalar_pixel_equivalence(self):
        from render import ffmpeg_path
        def pixels(name, cue):
            doc = parse(ass_source(), "ass")
            source = self.root / (name + ".ass")
            source.write_text(serialize(doc, [cue]), encoding="utf-8")
            return runtime.run([ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-f", "lavfi",
                                "-i", "color=s=320x180:r=1:d=1", "-filter_threads", "1", "-vf",
                                f"setpts=PTS+1.5/TB,ass={name}.ass", "-frames:v", "1", "-threads", "1",
                                "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"], cwd=self.root, timeout=20)
        a = srt_to_ass(Cue("c000001", 1000, 3000, "A<br>B"))
        b = srt_to_ass(Cue("c000001", 1000, 3000, "A\nB"))
        self.assertEqual(pixels("br", a), pixels("lf", b))
        a.text, b.text = r"{\r(Default)\fr(12)\be(1)}سلام OVA", r"{\rDefault\fr12\be1}سلام OVA"
        self.assertEqual(pixels("parenthesized", a), pixels("plain", b))


if __name__ == "__main__":
    with runtime.operational_log("audit-boundaries"):
        unittest.main(verbosity=2)
