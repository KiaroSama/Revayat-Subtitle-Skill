"""Literal text and real formatting stay distinct through subtitle editing."""

import unittest

from check import FIXTURES, WorkspaceCase
from markup import clean_empty_lines, remap_resets, validate_bidi
from subtitle_formats import Cue, Document, RLE, RLM, effective_times, parse, rtl, srt_to_ass, visible
from workflow import merge_donor, reviewed_cues


class FormatTests(WorkspaceCase):
    def test_bidi_only_blank_lines_keep_their_balanced_scope(self):
        source = "Hello\\N\u2067\\Nسلام\u2069"
        doc = Document("ass", [Cue("c000001", 1000, 2000, source)])
        row = {"id": "c000001", "source_text": source, "text": source, "reviewed": True,
               "action": "edit", "start_ms": 1000, "end_ms": 2000, "links": []}
        kept, _ = reviewed_cues(doc, [row])
        self.assertEqual(visible(kept[0].text, "ass"), "Hello\nسلام")
        self.assertEqual(kept[0].text.count("\u2067"), 1)
        self.assertEqual(kept[0].text.count("\u2069"), 1)
        validate_bidi(kept[0].text, "ass")

    def test_resets_change_only_real_override_arguments(self):
        doc = parse((FIXTURES / "episode.ass").read_text(encoding="utf-8"), "ass")
        donor = parse((FIXTURES / "episode.ass").read_text(encoding="utf-8"), "ass")
        cue = Cue("c000001", 1000, 2000, r"{\rDefault}Use the literal \rDefault", {"style": "Default"})
        result = merge_donor(doc, donor, [cue], "s0002", False)[0]
        self.assertEqual(result.text, r"{\rs0002__Default}Use the literal \rDefault")
        self.assertEqual(remap_resets(r"{\t(0,100,\rDefault)\r}x", {"Default": "Mapped"}),
                         r"{\t(0,100,\rMapped)\r}x")
        with self.assertRaisesRegex(ValueError, "undefined"):
            remap_resets(r"{\rUnknown}x", {"Default": "Mapped"})

    def test_interior_direction_marks_survive_idempotent_normalization(self):
        source = "سلام " + RLM + "OVA برگشت."
        normalized = rtl(source, "ass")
        self.assertIn("سلام " + RLM + "OVA", normalized)
        self.assertEqual(rtl(normalized, "ass"), normalized)
        for source in (" " + RLM + "سلام" + RLM, RLM * 2 + "Hello" + RLM * 2):
            normalized = rtl(source, "ass")
            self.assertEqual(rtl(normalized, "ass"), normalized)
        for source in ("علی", "Meet علی in London."):
            normalized = rtl(source, "srt", "ltr")
            self.assertNotIn(RLE, normalized)
            self.assertEqual(rtl(normalized, "srt", "ltr"), normalized)
        with self.assertRaisesRegex(ValueError, "Unclosed"):
            rtl("\u2067سلام", "srt")
        validate_bidi("\u2067سلام\u2069", "srt")

    def test_blank_prose_lines_are_removed_without_losing_controls(self):
        source = r"\Nسلام\N{\rDefault}\Nدنیا\N"
        doc = Document("ass", [Cue("c000001", 1000, 2000, source)])
        row = {"id": "c000001", "source_text": source, "text": source, "reviewed": True,
               "action": "edit", "start_ms": 1000, "end_ms": 2000, "links": []}
        kept, _ = reviewed_cues(doc, [row])
        self.assertEqual(visible(kept[0].text, "ass"), "سلام\nدنیا")
        self.assertIn(r"{\rDefault}", kept[0].text)
        row.update(preserve_empty_lines=True, structure_note="Intentional layout spacing")
        kept, _ = reviewed_cues(doc, [row])
        self.assertIn("\n\n", visible(kept[0].text, "ass"))
        for text in (r"{\p1}m 0 0\N\N l 1 1{\p0}سلام", r"{\k10}سلام\N\Nدنیا", r"سلام\n\nدنیا"):
            self.assertEqual(clean_empty_lines(text, "ass"), text)

    def test_ltr_target_does_not_receive_paragraph_rtl(self):
        source = "Hello"
        doc = Document("srt", [Cue("c000001", 1000, 2000, source)])
        row = {"id": "c000001", "source_text": source, "text": "Meet علی in London.",
               "reviewed": True, "action": "edit", "start_ms": 1000, "end_ms": 2000}
        kept, _ = reviewed_cues(doc, [row], "en")
        self.assertNotIn(RLE, kept[0].text)
        self.assertEqual(visible(kept[0].text, "srt"), "Meet علی in London.")
        row["direction"] = "rtl"
        with self.assertRaisesRegex(ValueError, "direction_note"):
            reviewed_cues(doc, [row], "en")

    def test_supported_tags_and_literal_entities_do_not_erase_text(self):
        self.assertEqual(visible("<i>Hi</i> <!--hidden-->x < 5 > y &amp;", "srt"), "Hi x < 5 > y &amp;")
        self.assertEqual(srt_to_ass(Cue("c000001", 1001, 2009, "x < 5 > y")).text, "x < 5 > y")
        self.assertEqual(effective_times(59999, 60010, "ass"), (59990, 60010))
        with self.assertRaisesRegex(ValueError, "collapses"):
            srt_to_ass(Cue("c000001", 1001, 1009, "Hello"))
        with self.assertRaisesRegex(ValueError, "markup"):
            srt_to_ass(Cue("c000001", 1000, 2000, '<font color="red">Hi</font>'))

    def test_srt_angle_bracket_numbers_remain_visible(self):
        for text in ("< 5 >", "< ۵ >", "x < 5 > y"):
            with self.subTest(text=text):
                self.assertEqual(visible(text, "srt"), text)


if __name__ == "__main__":
    unittest.main()
