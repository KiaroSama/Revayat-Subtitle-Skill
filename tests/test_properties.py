"""Bounded generated examples for logical text and serialization invariants."""

import os
from pathlib import Path
import unittest

from check import ROOT
os.environ["HYPOTHESIS_STORAGE_DIRECTORY"] = str(ROOT / ".scratch" / "hypothesis")
from hypothesis import given, settings, strategies as st

from subtitle_formats import Cue, Document, parse, rtl, serialize, visible

TEXT = st.text(alphabet="سلامدنیا abcXYZ012۵۶۷<>()!?‌‏", min_size=1, max_size=100).filter(lambda text: bool(visible(text, "srt")))
BOUNDED = settings(max_examples=100, deadline=1000, derandomize=True, database=None)


class GeneratedFormatTests(unittest.TestCase):
    @BOUNDED
    @given(TEXT)
    def test_logical_text_and_idempotence(self, text):
        normalized = rtl(text, "srt")
        self.assertEqual(visible(normalized, "srt"), visible(text, "srt"))
        self.assertEqual(rtl(normalized, "srt"), normalized)

    @BOUNDED
    @given(st.lists(st.tuples(st.integers(0, 100000), TEXT), min_size=1, max_size=12))
    def test_srt_roundtrip_keeps_sorted_content(self, values):
        cues = [Cue(f"c{index:06}", start, start + 1000, text.strip())
                for index, (start, text) in enumerate(values, 1)]
        document = Document("srt", cues)
        result = parse(serialize(document, cues), "srt")
        expected = sorted(cues, key=lambda cue: cue.start)
        self.assertEqual([(cue.start, cue.end, cue.text) for cue in result.cues],
                         [(cue.start, cue.end, cue.text) for cue in expected])
