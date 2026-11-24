"""Compare real compiled extension behavior with the Python reference functions."""

import importlib
import os
import random
import unittest
from unittest.mock import patch

from check import ROOT
import markup
import text_pipeline
from test_audit_install_native_20261124 import persian_linter


def native_module(name):
    try:
        return importlib.import_module(name)
    except ImportError as error:
        if os.environ.get("REVAYAT_REQUIRE_NATIVE") == "1":
            raise AssertionError(f"Required compiled extension {name} is unavailable") from error
        raise unittest.SkipTest(f"Optional compiled extension {name} is unavailable") from error


def outcome(function, *args, **kwargs):
    try:
        return "value", function(*args, **kwargs)
    except Exception as error:
        return "error", type(error).__name__, str(error)


class NativeParityCase(unittest.TestCase):
    def compare(self, python, native, *args, **kwargs):
        with patch.dict(os.environ, {"REVAYAT_DISABLE_RUST": "1"}):
            expected = outcome(python, *args, **kwargs)
        actual = outcome(native, *args, **kwargs)
        self.assertEqual(actual, expected, f"{python.__name__}: args={args!r}, kwargs={kwargs!r}")


class MarkupNativeParityTests(NativeParityCase):
    @classmethod
    def setUpClass(cls):
        cls.native = native_module("markup_rs")

    def test_detection_font_and_drawing_apis(self):
        for text in ("", "Hello 123", "سلام ۱۲۳", "日本語", "עברית", "مرحبا", "\u200f",
                     "\u2067سلام\u2069", "𝛂×²", "<i>سلام</i>", "\U0001171e",
                     "\U0001d6c1", "\u1c89", "\U00010d40", "\U0001e4d0"):
            for name in ("has_ltr", "has_rtl"):
                with self.subTest(function=name, text=text):
                    self.compare(getattr(markup, name), getattr(self.native, name), text)
        for tag in ("<i>", "</font>", '<font face="A&amp;B" face=Tahoma>',
                    '<!--<font face=hidden>--><FONT FACE=" B Koodak "/>',
                    "<" + "é" * 64 + ">", "</" + "a" * 127 + ">",
                    '<font face="&fjlig; &NotEqualTilde; &amp=">',
                    '<font face="&copycat &notit; &unknown; &amp=">'):
            for name in ("srt_tag_fits", "srt_font_names"):
                with self.subTest(function=name, tag=tag):
                    self.compare(getattr(markup, name), getattr(self.native, name), tag)
        for block in (r"{\p1}", r"{\p0}", r"{\p-1}", r"{\p(+02)}", r"{\t(0,1,\p2)}",
                      r"{\pos(1,2}", r"{\t(0,1,2,3,\p1)}", "{comment}"):
            for drawing in (False, True):
                with self.subTest(block=block, drawing=drawing):
                    self.compare(markup.drawing_mode, self.native.drawing_mode, block, drawing)

    def test_markup_bidi_and_cleanup_apis(self):
        cases = ("", "Plain text.", "سلام!", "<i>سلام</i><br>خوب؟",
                 r"{\i1}سلام{\i0}\Nخط دوم.", r"{\p1}m 0 0 l 1 1{\p0}سلام",
                 "<!--\u202c-->سلام", "<fo<!--note-->nt>x", "<i>\n \nhello\n</i>",
                 "\u2066site\u2069 سلام", "\u2067سلام\nدنیا\u2069", "\u202cسلام",
                 r"{\k2}\NHello", r"{\i1}\Nhello", "سلام \U0001d6c1", "\U00010d40 \u1c89")
        for kind in ("ass", "srt"):
            for text in cases:
                for name in ("uncomment", "validate_bidi"):
                    with self.subTest(function=name, kind=kind, text=text):
                        self.compare(getattr(markup, name), getattr(self.native, name), text, kind)
                for preserve in (False, True):
                    with self.subTest(function="clean_empty_lines", kind=kind, preserve=preserve, text=text):
                        self.compare(markup.clean_empty_lines, self.native.clean_empty_lines,
                                     text, kind, preserve)
                for direction in ("rtl", "ltr"):
                    for force in (False, True):
                        with self.subTest(function="rtl", kind=kind, direction=direction, force=force, text=text):
                            self.compare(markup.rtl, self.native.rtl, text, kind, direction, force=force)
        for text in (r"{\rOld}Hello", r"{\t(\r( , Old ,ignored))}سلام", r"{\r}Plain"):
            for mapping in ({"Old": "جدید"}, {}):
                with self.subTest(function="remap_resets", text=text, mapping=mapping):
                    self.compare(markup.remap_resets, self.native.remap_resets, text, mapping)
        for language in ("fa", "fa-Latn", "und-Hebr", "en", "ar", "ja", "fa-Arab-LATN", ""):
            with self.subTest(function="paragraph_direction", language=language):
                self.compare(markup.paragraph_direction, self.native.paragraph_direction, language)


class PersianNativeParityTests(NativeParityCase):
    @classmethod
    def setUpClass(cls):
        cls.native = native_module("persian_lint_rs")
        cls.python = persian_linter()

    def test_normalization_suggestions_and_change_reporting(self):
        cases = ["", "كتاب ها مي رود", "سلام, دنيا 1964!", "خانه ام و بزرگ ترین",
                 "1. كتاب ها 50% و `42`,خوب", "    code = 1\n\nكتاب ها مي رود",
                 "\tcode\r\n\r\nكتاب ها", "    میرود code\n\nمیرود",
                 "before\x00`one` after `two`", "\x00`1`\x00`2`\x00",
                 "```\nمي رود\n```\nكتاب ها", "سال v1.2.3 و https://x.test/مي",
                 "زمینِ تر بود و میرود و میز", "می‌باشد و توسط او نوشته شد.",
                 'He said "hi".', "الف — ب", "foo\x1cbar\nسلام 50%"]
        generator = random.Random(20261124)
        fragments = ("كتاب ها", "می رود", "سلام,", "50%", "`ك 123`", "\x00", "—",
                     "https://x.test/ك", "خانه ام", "بزرگ تر", "\n", " ", "normal 123")
        cases.extend(" ".join(generator.choices(fragments, k=6)) for _ in range(30))
        for text in cases:
            for digits in ("persian", "latin", "keep"):
                with self.subTest(function="normalize", digits=digits, text=text):
                    self.compare(self.python.normalize, self.native.normalize, text, digits)
            for style in (False, True):
                with self.subTest(function="suggestions", style=style, text=text):
                    self.compare(self.python.suggestions, self.native.suggestions, text, style)
            fixed = self.python.normalize(text, digits="keep")[0]
            with self.subTest(function="line_changes", text=text):
                self.compare(self.python.line_changes, self.native.line_changes, text, fixed)


class SubcoreNativeParityTests(NativeParityCase):
    @classmethod
    def setUpClass(cls):
        cls.native = native_module("subcore_rs")

    def test_all_exported_text_transforms(self):
        cases = ("", "Plain dialogue.", "سلام، اسم من Rin است.", "می رود و خانه ها",
                 "<i>Meaningful words</i>", "<b>سلام</b><br>دنیا!", r"{\i1}سلام!{\i0}",
                 r"line\Ntwo", "line\nline", "\u2067سلام\nدنیا\u2069",
                 r"{\p1}m 0 0 l 1 1{\p0}سلام", "one two three four five six seven eight",
                 "<font color=#123456>x<font color=#AABBCC>y</font>z</font>",
                 "Open https://example.test to read the manual.", "https://example.test",
                 "Subtitles by Someone", "I found it on OpenSubtitles yesterday.",
                 "before\x00after", "- first speaker\n- second speaker", "\U00010d50!",
                 "مي رود", "\U00010d50 \U0001e4d0?", "مي روم\U0001e4d0",
                 "\U0001e4d0می روم", "خانه ها\U0001e4d0", "خانه ام\U0001e4d0",
                 "هم چنین\U0001e4d0", "Subtitles by \U0001e4d0", "Subtitles by \u1c89",
                 "Subtitles by amateurs are unreliable.", "«SUBTITLES BY HUMANS FAIL!»",
                 "Captioned by humans looks better.", "Subtitles by amateurs<br>are unreliable.",
                 "Subtitles by amateurs\nare unreliable.", "Subtitles by John O'Connor",
                 "زیرنویس از دو دوست", "Downloaded from the web", "Subtitles by @actual_team",
                 "Downloaded from https://example.test")
        for text in cases:
            for name in ("normalize_persian_zwnj", "watermark_reason", "is_watermark_cue",
                         "convert_srt_markup_to_ass"):
                with self.subTest(function=name, text=text):
                    self.compare(getattr(text_pipeline, name), getattr(self.native, name), text)
            for width, limit in ((42, 2), (10, 2), (10, 3), (0, 2), (42, 0)):
                with self.subTest(function="wrap_subtitle_lines", width=width, limit=limit, text=text):
                    self.compare(text_pipeline.wrap_subtitle_lines, self.native.wrap_subtitle_lines,
                                 text, width, limit)
            for kind in ("ass", "srt"):
                for direction in ("rtl", "ltr"):
                    with self.subTest(function="anchor_rtl_line", kind=kind, direction=direction, text=text):
                        self.compare(text_pipeline.anchor_rtl_line, self.native.anchor_rtl_line,
                                     text, kind, direction)
                    with self.subTest(function="process_cue_text", kind=kind, direction=direction, text=text):
                        self.compare(text_pipeline.process_cue_text, self.native.process_cue_text,
                                     text, kind, direction, 18, True, True)


if __name__ == "__main__":
    unittest.main()
