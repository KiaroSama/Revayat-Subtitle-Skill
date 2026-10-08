"""Bounded independent comment/newline/literal grammar families."""
import unittest

from hypothesis import given, settings, strategies as st

from markup import uncomment
from subtitle_formats import Cue, Document, parse, serialize
from workflow import reviewed_cues

BOUNDED = settings(max_examples=100, deadline=1000, derandomize=True, database=None)
PROSE = st.text(alphabet="abcXYZسلام012", min_size=1, max_size=20)


class StructuralPropertyTests(unittest.TestCase):
    @BOUNDED
    @given(PROSE, PROSE)
    def test_ordinary_comments_coalesce_exact_literal_prose(self, left, right):
        self.assertEqual(uncomment(left + "<!--authored-->" + right, "srt"), left + right)

    @BOUNDED
    @given(st.sampled_from(["b", "/b", "br", "small", "foo"]))
    def test_removed_boundary_never_activates_tag_family(self, tag):
        with self.assertRaisesRegex(ValueError, "activat"):
            uncomment("A<<!--authored-->" + tag + ">B", "srt")

    @BOUNDED
    @given(st.text(alphabet="abcXYZ012", min_size=1, max_size=20),
           st.text(alphabet="abcXYZ012", min_size=1, max_size=20), st.sampled_from(["\n", "\r\n", "\r"]))
    def test_single_reviewed_newline_has_independent_literal_roundtrip(self, left, right, newline):
        source = Cue("c000001", 1000, 2000, "Source")
        row = {"id": source.id, "source_text": "Source", "start_ms": 1000, "end_ms": 2000,
               "action": "edit", "text": left + newline + right, "reviewed": True,
               "links": [], "note": "", "structure_note": "Line layout", "timing_note": ""}
        cues, _ = reviewed_cues(Document("srt", [source]), [row], "en")
        expected = left + "\n" + right
        self.assertEqual(cues[0].text, expected)
        self.assertEqual(parse(serialize(Document("srt", [source]), cues), "srt").cues[0].text, expected)


    def test_mixed_direction_has_independent_ltr_wrapper_oracle(self):
        cue = Cue("c000001", 1000, 2000, "Source")
        row = {"id": cue.id, "source_text": "Source", "start_ms": 1000, "end_ms": 2000,
               "action": "edit", "text": "س\na", "reviewed": True, "links": [], "structure_note": "Line layout"}
        cues, _ = reviewed_cues(Document("srt", [cue]), [row], "en")
        self.assertEqual(cues[0].text, "‎س‎\na")

    @BOUNDED
    @given(st.sampled_from([r"\{literal}", r"x\hY", r"{\p1}m 0 0 l 1 1{note}{\p0}X"]))
    def test_ass_escapes_and_drawing_transitions_have_literal_oracles(self, text):
        expected = text.replace("{note}", "{}")
        self.assertEqual(uncomment(text, "ass"), expected)

    @BOUNDED
    @given(st.sampled_from([" A ", "\tA\t", "A  B"]), st.sampled_from(["b", "i", "u", "s"]))
    def test_supported_formatting_and_significant_interior_whitespace(self, prose, tag):
        original = f"<{tag}>{prose}<!--note-->Z</{tag}>"
        self.assertEqual(uncomment(original, "srt"), f"<{tag}>{prose}Z</{tag}>")

    def test_literal_comment_oracle_is_sensitive_to_dropped_prose(self):
        expected = "AZ"
        actual = uncomment("A<!--note-->Z", "srt")
        self.assertEqual(actual, expected)
        # Mutate the normalization seam only inside this fixture; the identical
        # authored oracle must detect dropped prose rather than certify the mutation.
        from unittest.mock import patch
        # Extend the actual removed span by one character: this mutates the
        # boundary rule used by real uncomment, not the result/oracle itself.
        with patch('markup.block_spans', return_value=iter([(1, 13)])):
            with self.assertRaises(AssertionError):
                self.assertEqual(uncomment('A<!--note-->Z', 'srt'), expected)


if __name__ == "__main__":
    unittest.main()
