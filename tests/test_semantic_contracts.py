"""Authored regressions for ASCII subtitle grammar, stable bidi and typed metadata."""

from __future__ import annotations

import copy
import json
import logging
import os
from pathlib import Path
import random
import sys
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "skills/revayat-subtitle/scripts"
sys.path.insert(0, str(SCRIPTS))

import markup
import render
import runtime
import workflow
from check import WorkspaceCase
from subtitle_formats import Cue, Document, parse, serialize, srt_to_ass, structure, timecode, visible

RENDER = "--render" in sys.argv or os.environ.get("REVAYAT_TEST_RENDER") == "1"


class SemanticWorkspace(WorkspaceCase):
    def make_edition(self, text="Authored fixture."):
        source = self.root / "input.srt"
        source.write_text("1\n00:00:01,000 --> 00:00:03,000\n" + text + "\n", encoding="utf-8")
        workflow.prepare([source], self.work, "Fixture Series", 1, "utf-8", None, "fa")
        project = runtime.read_json(self.work / "project.json")
        project["episodes"] = [{"id": "S01E01", "base": "s0001", "comparison": "Only authored source"}]
        runtime.write_json(self.work / "project.json", project)
        rows = runtime.read_json(self.work / "worksheets/s0001.json")
        for row in rows:
            row.update(reviewed=True, action="preserve")
        runtime.write_json(self.work / "worksheets/s0001.json", rows)
        runtime.write_json(self.work / "glossary.json", {
            "series": "Fixture Series", "terms_reviewed": True, "terms": [],
            "research": [{"url": "https://example.org/fixture", "note": "Authored mechanical test, not anime research"}]})
        return workflow.build(self.work)


class GrammarTests(SemanticWorkspace):
    def test_unicode_lookalikes_remain_literal(self):
        for letter in ("ſ", "İ", "ı"):
            for tag in (f"<{letter}>", f"</{letter}>"):
                with self.subTest(tag=tag):
                    self.assertEqual(visible(tag, "srt"), tag)
                    self.assertEqual(list(markup.pieces(tag, "srt")), [("text", tag)])

    def test_lookalikes_are_not_converted_to_ass_controls(self):
        for letter in ("ſ", "İ", "ı"):
            source = f"A<{letter}>B</{letter}>C"
            with self.subTest(letter=letter):
                self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, source)).text, source)

    def test_non_ascii_or_non_space_separators_are_literal(self):
        for name in ("i", "I", "b", "s", "u", "font", "FONT"):
            for space in ("\t", "\r", "\n", "\v", "\f", "\u00a0", "\u2009"):
                source = "A<" + name + space + ">B"
                with self.subTest(name=name, separator=ord(space)):
                    self.assertEqual(visible(source, "srt"), source)

    def test_literal_whitespace_tag_donors_are_preserved(self):
        for separator in ("\t", "\u00a0", "\u2009"):
            source = "A<i" + separator + ">B"
            self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, source)).text, source)

    def test_visible_lookalike_cannot_be_discarded_as_empty(self):
        for text in ("<ſ>", "<İ>", "<ı>", "<i\t>", "<i\u00a0>"):
            doc = Document("srt", [Cue("c000001", 1000, 3000, text)])
            row = {"id": "c000001", "source_text": text, "reviewed": True,
                   "start_ms": 1000, "end_ms": 3000, "action": "empty"}
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "nonempty"):
                workflow.reviewed_cues(doc, [row])

    def test_genuine_empty_formatting_remains_excludable(self):
        text = "<I></I>"
        doc = Document("srt", [Cue("c000001", 1000, 3000, text)])
        row = {"id": "c000001", "source_text": text, "reviewed": True,
               "start_ms": 1000, "end_ms": 3000, "action": "empty"}
        self.assertEqual(workflow.reviewed_cues(doc, [row])[0], [])

    def test_ascii_case_variants_still_convert_as_binary_markers(self):
        for tag in "bisu":
            for name in (tag, tag.upper()):
                expected = "{\\" + tag + "1}Hi{\\" + tag + "0}"
                self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, f"<{name}>Hi</{name}>")).text, expected)
        self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, "<i><i>A</i>B</i>")).text,
                         r"{\i1}{\i1}A{\i0}B{\i0}")

    def test_unsupported_real_markup_still_requires_adaptation(self):
        for text in ('<font face="Example">A</font>', '<i x="1">A</i>', '<small>A</small>', '< i>A'):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "adapt"):
                srt_to_ass(Cue("c000001", 1000, 3000, text))

    def test_oversized_and_literal_angle_sequences_survive(self):
        for text in ("A<i " + "é" * 70 + ">B", "x < 5 > y", "A &amp; B", "A<K>B"):
            self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, text)).text, text)
            self.assertEqual(visible(text, "srt"), text)

    def test_real_font_requests_do_not_include_literal_tag_text(self):
        doc = Document("srt", [Cue("c000001", 1000, 3000, '<font\tface="Wrong">A')])
        self.assertEqual(render.requested_fonts(doc), [])
        doc.cues[0].text = '<FONT face="Actual Family">A</FONT>'
        self.assertEqual(render.requested_fonts(doc), ["Actual Family"])

    def test_break_dialect_and_comment_policy_are_not_broadened(self):
        for text in ("A<br>B", "A</BR/>B", 'A<br x="1">B'):
            self.assertEqual(srt_to_ass(Cue("c000001", 1000, 3000, text)).text, r"A\NB")
        self.assertEqual(visible("A<br\t>B", "srt"), "A<br\t>B")
        self.assertEqual(markup.uncomment("A<!--existing cleanup policy-->B", "srt"), "AB")

    @unittest.skipUnless(RENDER, "Explicit FFmpeg integration tier")
    def test_native_conversion_of_literal_and_real_tags(self):
        texts = [f"A<{letter}>B</{letter}>C" for letter in ("ſ", "İ", "ı", "K")]
        texts += ["A<" + name + separator + ">B" for name in ("i", "s", "font")
                  for separator in ("\t", "\u00a0", "\u2009")]
        texts += [f"A<{tag}>B</{tag}>C" for tag in "bBiIsSuU"]
        texts += ["A<br>B", "A</BR/>B", "x < 5 > y", "A &amp; B"]
        source = self.root / "corpus.srt"
        source.write_text("".join(f"{i + 1}\n{timecode(i * 3000, 'srt')} --> {timecode(i * 3000 + 2000, 'srt')}\n{text}\n\n"
                                  for i, text in enumerate(texts)), encoding="utf-8")
        target = self.root / "native.ass"
        runtime.run([render.ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                     "-i", str(source), str(target)], timeout=30)
        native = parse(target.read_text(encoding="utf-8"), "ass")
        self.assertEqual(len(native.cues), len(texts))
        for i, (text, reference) in enumerate(zip(texts, native.cues)):
            converted = srt_to_ass(Cue("c000001", i * 3000, i * 3000 + 2000, text))
            with self.subTest(corpus_index=i):
                self.assertEqual(visible(converted.text, "ass"), visible(reference.text, "ass"))
                self.assertEqual(structure(converted.text, "ass"), structure(reference.text, "ass"))

    @unittest.skipUnless(RENDER, "Explicit FFmpeg integration tier")
    def test_native_literal_raster_is_not_silently_formatted(self):
        text = "A<ſ>B</ſ>C"
        source = self.root / "one.srt"
        source.write_text("1\n00:00:01,000 --> 00:00:03,000\n" + text + "\n", encoding="utf-8")
        native = self.root / "native.ass"
        runtime.run([render.ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                     "-i", str(source), str(native)], timeout=20)
        document = parse(native.read_text(encoding="utf-8"), "ass")
        converted = srt_to_ass(Cue("c000001", 1000, 3000, text))
        (self.root / "converted.ass").write_text(serialize(document, [converted]), encoding="utf-8")
        def pixels(name):
            raw = runtime.run([render.ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin",
                "-f", "lavfi", "-i", "color=s=320x180:r=1:d=1", "-filter_threads", "1", "-vf",
                f"setpts=PTS+1.5/TB,ass={name}.ass", "-frames:v", "1", "-threads", "1",
                "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"], cwd=self.root, timeout=20)
            self.assertEqual(len(raw), 320 * 180 * 3)
            self.assertGreater(max(raw), 32)
            return raw
        self.assertEqual(pixels("native"), pixels("converted"))


class NormalizerTests(SemanticWorkspace):
    def test_rtl_hidden_comment_whitespace_is_idempotent(self):
        source = "سلام{note} "
        once = markup.rtl(source, "ass")
        self.assertEqual(markup.rtl(once, "ass"), once)

    def test_leading_trailing_and_multiple_comment_boundaries(self):
        for source in (" {note}سلام", "سلام{note}\t", " {a}سلام{b} ", "سلام{a}{b} "):
            with self.subTest(source=source):
                result = markup.rtl(source, "ass")
                self.assertEqual(markup.rtl(result, "ass"), result)
                self.assertEqual(visible(result, "ass"), visible(source, "ass"))

    def test_srt_comment_boundaries_stabilize_after_one_pass(self):
        for source in ("سلام<!--note--> ", " <!--note-->سلام", "سلام<!--a--><!--b--> OVA "):
            result = markup.rtl(source, "srt")
            self.assertEqual(markup.rtl(result, "srt"), result)
            self.assertEqual(visible(result, "srt"), visible(source, "srt"))

    def test_force_ltr_and_rtl_do_not_accumulate_wrappers(self):
        for direction in ("ltr", "rtl"):
            result = markup.rtl(" {note}Latin{note} ", "ass", direction, force=True)
            for _ in range(5):
                self.assertEqual(markup.rtl(result, "ass", direction, force=True), result)

    def test_existing_scopes_and_real_formatting_remain_balanced(self):
        sources = [markup.RLE + markup.RLM + "{note}سلام" + markup.RLM + markup.PDF,
                   "\u2067سلام{note} OVA\\Nدنیا\u2069",
                   r"{\i1}سلام{note} OVA{\i0} "]
        for source in sources:
            result = markup.rtl(source, "ass")
            markup.validate_bidi(result, "ass")
            self.assertEqual(markup.rtl(result, "ass"), result)
            self.assertEqual(visible(source, "ass"), visible(result, "ass"))

    def test_real_override_and_drawing_boundaries_are_not_merged(self):
        source = r"{\p1}m 0 0 l 10 0 10 10{note}m 20 0 l 30 0 30 10{\p0}سلام{\i1} OVA{\i0}"
        result = markup.rtl(source, "ass")
        self.assertEqual(structure(source, "ass"), structure(result, "ass"))
        self.assertEqual(markup.rtl(result, "ass"), result)
        self.assertIn("{}", result)

    def test_seeded_comment_boundary_property(self):
        rng = random.Random(1004)
        for _ in range(500):
            kind = rng.choice(("ass", "srt"))
            comment = "{note}" if kind == "ass" else "<!--note-->"
            source = rng.choice((" ", "\t", "")) + comment + rng.choice(("سلام", "سلام OVA", "علی"))
            source += comment + rng.choice((" ", "\t", " OVA", ""))
            result = markup.rtl(source, kind)
            self.assertEqual(markup.rtl(result, kind), result)
            self.assertEqual(visible(source, kind), visible(result, kind))

    def test_many_adjacent_fragments_use_bounded_coalescing(self):
        source = ("x{note}" * 3000) + "سلام{note} "
        result = markup.rtl(source, "ass")
        self.assertEqual(visible(result, "ass"), ("x" * 3000) + "سلام")
        self.assertEqual(markup.rtl(result, "ass"), result)


class ImmutableTests(SemanticWorkspace):
    def test_loading_boolean_manifest_count_is_rejected(self):
        edition = Path(self.make_edition()["build"])
        path = edition / "manifest.json"
        manifest = runtime.read_json(path)
        manifest["episodes"][0]["cues"] = True
        runtime.write_json(path, manifest)
        with self.assertRaisesRegex(ValueError, "modified"):
            render.load_build(edition)

    def test_rebuild_rejects_coerced_review_count(self):
        edition = Path(self.make_edition()["build"])
        path = edition / "manifest.json"
        manifest = runtime.read_json(path)
        for alias in (True, 1.0):
            with self.subTest(alias_type=type(alias).__name__):
                changed = copy.deepcopy(manifest)
                changed["reviewed_cues"] = alias
                runtime.write_json(path, changed)
                with self.assertRaisesRegex(ValueError, "modified"):
                    workflow.build(self.work)

    def test_nested_provenance_and_recipe_require_original_types(self):
        edition = Path(self.make_edition()["build"])
        path = edition / "manifest.json"
        original = runtime.read_json(path)
        changes = [(["episodes", 0, "provenance", 0, "emitted_index"], True),
                   (["episodes", 0, "provenance", 0, "emitted_index"], 1.0),
                   (["episodes", 0, "provenance", 0, "structure_changed"], 0),
                   (["generation_recipe", "version"], float(workflow.GENERATION_RECIPE["version"]))]
        for trail, value in changes:
            with self.subTest(field=trail[-1], value_type=type(value).__name__):
                changed = copy.deepcopy(original)
                pointer = changed
                for key in trail[:-1]:
                    pointer = pointer[key]
                pointer[trail[-1]] = value
                runtime.write_json(path, changed)
                with self.assertRaisesRegex(ValueError, "modified"):
                    render.load_build(edition)
                with self.assertRaisesRegex(ValueError, "modified"):
                    workflow.build(self.work)

    def test_saved_glossary_booleans_cannot_be_replaced_by_numbers(self):
        edition = Path(self.make_edition()["build"])
        path = edition / "glossary.json"
        glossary = runtime.read_json(path)
        glossary["terms_reviewed"] = 1
        runtime.write_json(path, glossary)
        with self.assertRaisesRegex(ValueError, "glossary"):
            workflow.build(self.work)
        with self.assertRaisesRegex(ValueError, "glossary"):
            render.load_build(edition)

    def test_json_layout_and_dictionary_order_are_not_identity(self):
        result = self.make_edition()
        edition = Path(result["build"])
        for name in ("manifest.json", "glossary.json"):
            path = edition / name
            value = runtime.read_json(path)
            path.write_text(json.dumps(dict(reversed(list(value.items()))), ensure_ascii=True), encoding="utf-8")
        self.assertEqual(workflow.build(self.work)["identity"], result["identity"])
        self.assertEqual(render.load_build(edition)[0]["identity"], result["identity"])

    def test_typed_equality_keeps_json_types_and_list_order(self):
        self.assertTrue(runtime.same_json({"a": [1, True, 1.5], "b": None}, {"b": None, "a": [1, True, 1.5]}))
        for first, second in ((True, 1), (False, 0), (1, 1.0), ([1], [True]),
                              ({"a": False}, {"a": 0}), ([1, 2], [2, 1]), ({"a": None}, {})):
            with self.subTest(first_type=type(first).__name__, second_type=type(second).__name__):
                self.assertFalse(runtime.same_json(first, second))

    def test_typed_equality_handles_deep_json_without_python_recursion(self):
        first, second = 1, 1
        for _ in range(1500):
            first, second = [first], [second]
        self.assertTrue(runtime.same_json(first, second))

    def test_previous_recipe_history_and_reviews_are_preserved(self):
        with patch.dict(workflow.GENERATION_RECIPE, {"version": 8, "normalization": 8}):
            old = Path(self.make_edition()["build"])
        before = {p.relative_to(self.work): p.read_bytes() for p in self.work.rglob("*") if p.is_file()}
        new = Path(workflow.build(self.work)["build"])
        self.assertNotEqual(old, new)
        self.assertEqual(before, {p: (self.work / p).read_bytes() for p in before})
        self.assertFalse((new / "renders").exists())

    @unittest.skipUnless(RENDER, "Explicit FFmpeg integration tier")
    def test_complete_delivery_refuses_metadata_type_drift(self):
        result = self.make_edition("A<ſ>B</ſ>C سلام")
        edition = Path(result["build"])
        original_source = (self.work / "sources/s0001.srt").read_bytes()
        evidence = render.render(edition, "S01E01", None, None, None, False)
        with self.assertRaisesRegex(ValueError, "Inspect every"):
            render.qa(edition)
        path = Path(evidence["review"])
        review = runtime.read_json(path)
        for frame in review["frames"]:
            frame.update(reviewed=True, note="Automated mechanical gate fixture, not human visual approval.")
        runtime.write_json(path, review)
        self.assertTrue(render.qa(edition)["ok"])
        output = self.root / "complete.zip"
        render.package(edition, output)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(archive.namelist(), ["Sub/S01E01.srt"])
            self.assertEqual(runtime.digest(archive.read("Sub/S01E01.srt")), result["episodes"][0]["sha256"])
        manifest_path = edition / "manifest.json"
        original = manifest_path.read_bytes()
        manifest = runtime.read_json(manifest_path)
        manifest["episodes"][0]["provenance"][0]["emitted_index"] = True
        runtime.write_json(manifest_path, manifest)
        refused = self.root / "refused.zip"
        with self.assertRaisesRegex(ValueError, "modified"):
            render.package(edition, refused)
        self.assertFalse(refused.exists())
        manifest_path.write_bytes(original)
        self.assertTrue(render.qa(edition)["ok"])
        self.assertEqual((self.work / "sources/s0001.srt").read_bytes(), original_source)


if __name__ == "__main__":
    with runtime.operational_log("semantic-audit"):
        unittest.main(verbosity=2)
