"""Regression evidence for format-aware conversion and final cue accounting."""

from __future__ import annotations

import importlib.util
import json
import logging
import os
from pathlib import Path
import sys
import unittest

import pytest

from check import SCRIPTS
import markup
from process_control import ToolFailure
import runtime
import subtitle_formats as formats
import text_pipeline as pipeline
import workflow
from test_ass_track_contracts import TrackWorkspace, source
from test_event_order_contracts import authored, overlapping


@pytest.fixture(autouse=True)
def pipeline_log(request):
    logging.debug("Pipeline audit regression=%s", request.node.name)


@pytest.mark.parametrize("text", [
    "Type <i> here.",
    "Use <br> literally.",
    '<font color="#FF0000">red</font> is the displayed example.',
    r"{\i1}Read <b> literally.{\i0}",
    r"{\fnKeep <i> literal}Display <i> in the subtitle.",
])
def test_ass_conversion_preserves_literal_html_and_existing_controls(text):
    original = formats.parse(source(text=text), "ass")
    expected = original.cues[0].text
    converted, removed = formats.ass_document_to_ass(
        original, direction="ltr", fix_zwnj=False, wrap_lines=False,
        strip_watermarks=False, style_line=",".join(original.styles["Default"]),
    )
    assert not removed
    assert converted.cues[0].text == expected
    assert original.cues[0].text == expected
    assert formats.parse(formats.serialize(converted, converted.cues), "ass").cues[0].text == expected


def test_ass_line_width_counts_literal_html_as_visible_prose():
    text = "<font color=red>one two three</font> four five"
    wrapped = pipeline.wrap_subtitle_lines(text, 24)
    assert wrapped != text
    assert wrapped.count(r"\N") == 1
    assert formats.visible(wrapped, "ass").replace("\n", " ") == text
    assert len(formats.visible(wrapped, "ass").split("\n")[0]) <= 24


def test_ass_rtl_anchor_sees_letters_inside_literal_html():
    text = '<b title="سلام">.</b>'
    anchored = pipeline.anchor_rtl_line(text, "ass")
    assert anchored.startswith(markup.RLE)
    assert formats.visible(anchored, "ass") == text
    assert pipeline.anchor_rtl_line(anchored, "ass") == anchored
    markup.validate_bidi(anchored, "ass")


def test_unterminated_comment_scan_completes_within_owned_process_budget():
    # Each function previously rescanned every unmatched opener to EOF.
    code = (
        "import sys;sys.path.insert(0,sys.argv[1]);"
        "from text_pipeline import watermark_reason,normalize_persian_zwnj,convert_srt_markup_to_ass;"
        "text='<!--'*100000;"
        "assert watermark_reason(text)=='';"
        "assert normalize_persian_zwnj(text)==text;"
        "assert convert_srt_markup_to_ass(text)==text;"
        "assert normalize_persian_zwnj(text+'مي روم كتاب ها خانه ام هم چنین')"
        "==text+'می‌روم کتاب‌ها خانه‌ام همچنین'"
    )
    try:
        runtime.run([sys.executable, "-X", "utf8", "-c", code, str(SCRIPTS)], timeout=10, idle_timeout=10)
    except ToolFailure as error:
        # This child processes only authored literals, so its traceback is safe evidence.
        pytest.fail(error.stderr.decode("utf-8", errors="replace"), pytrace=False)


def test_complete_comments_remain_protected_in_text_transforms():
    text = "می روم <!-- مي روم <i>hidden</i> --> می روم"
    assert pipeline.normalize_persian_zwnj(text) == "می‌روم <!-- مي روم <i>hidden</i> --> می‌روم"
    assert pipeline.convert_srt_markup_to_ass("A<!--<i>hidden</i>-->B") == "A<!--<i>hidden</i>-->B"
    assert pipeline.watermark_reason("Subtitles by Alice<!--not dialogue-->") == "fansub-credit"
    control = r"{\fnمي <!--x--> <i> literal}word"
    assert pipeline.normalize_persian_zwnj(control) == control
    assert pipeline.convert_srt_markup_to_ass(control) == control
    assert pipeline.convert_srt_markup_to_ass("A<!--{-->B<i>C</i>") == r"A<!--{-->B{\i1}C{\i0}"


CREDIT_PROSE = (
    "Downloaded from the device, the files contain evidence.",
    "Subtitles by amateurs can contain mistakes.",
    "Subtitles by amateurs are unreliable.",
    "«SUBTITLES BY AMATEURS ARE UNRELIABLE!»",
    "Subtitles by humans fail.",
    "Captioned by humans looks better.",
    "Encoded by machines looks better.",
    "Subtitles by amateurs\nare unreliable.",
    "Subtitles by amateurs<br>are unreliable.",
    "Subtitles by John O'Connor",
    "Downloaded from the web",
    "زیرنویس از دو دوست",
)


@pytest.mark.parametrize("text", CREDIT_PROSE)
def test_credit_prefix_with_a_spoken_clause_is_preserved(text):
    assert pipeline.watermark_reason(text) == ""
    original = formats.Document("srt", [formats.Cue("c000001", 1000, 3000, text)])
    converted, removed = formats.srt_document_to_ass(original, direction="ltr", wrap_lines=False)
    assert not removed
    assert formats.visible(converted.cues[0].text, "ass") == formats.visible(text, "srt")


def test_public_srt_conversion_preserves_free_form_credit_prose(tmp_path):
    credits = ("Subtitles by Alice", "Subtitles by @actual_team", "Downloaded from https://example.test")
    path = tmp_path / "credit-prose.srt"
    original = "\n\n".join(
        f"{index}\n00:00:{2 * index:02d},000 --> 00:00:{2 * index + 1:02d},000\n{text}"
        for index, text in enumerate((*CREDIT_PROSE, *credits), 1)
    ).encode("utf-8")
    path.write_bytes(original)
    output = tmp_path / "credit-prose.ass"
    report = json.loads(runtime.run([
        sys.executable, "-X", "utf8", str(SCRIPTS / "revayat-subtitle.py"), "convert", str(path),
        "--out", str(output), "--direction", "ltr", "--no-zwnj", "--max-chars", "500",
    ]))
    converted = formats.parse(output.read_text(encoding="utf-8"), "ass")
    assert [formats.visible(cue.text, "ass") for cue in converted.cues] == [
        formats.visible(text, "srt") for text in CREDIT_PROSE
    ]
    assert report["watermarks_removed"] == len(credits)
    assert report["cues"] == len(CREDIT_PROSE)
    assert path.read_bytes() == original


@pytest.mark.parametrize("text", [
    "Open https://example.test to read the manual.",
    "They used OpenSubtitles for the experiment.",
    "Send that address, t.me/channel, to your friend.",
    "Deluxe Media is mentioned in the documentary.",
    "I asked who the subtitles by Alice were for.",
    "من این کانال تلگرام را نمی‌شناسم.",
    "Save the evidence.\nhttps://example.test",
    "Save the evidence.<br>https://example.test",
    "Save the evidence.\\N@channel",
])
def test_watermark_detection_never_discards_ordinary_or_mixed_dialogue(text):
    assert pipeline.watermark_reason(text) == ""
    original = formats.Document("srt", [formats.Cue("c000001", 1000, 3000, text.replace(r"\N", "\n"))])
    converted, removed = formats.srt_document_to_ass(original, direction="ltr", wrap_lines=False)
    assert not removed
    assert len(converted.cues) == 1
    assert formats.visible(converted.cues[0].text, "ass") == formats.visible(original.cues[0].text, "srt")


@pytest.mark.parametrize("text, reason", [
    ("https://example.test", "url-ad"),
    ("[www.example.test]", "url-ad"),
    ("@channel", "telegram-handle"),
    ("https://t.me/channel", "telegram-handle"),
    ("OpenSubtitles", "site-watermark"),
    ("OpenSubtitles.org", "site-watermark"),
    ("Subtitles by Alice", "fansub-credit"),
    ("Subtitles by O'Connor", "fansub-credit"),
    ("Subtitles by Alice-Smith", "fansub-credit"),
    ("Subtitles by @actual_team", "fansub-credit"),
    ("Subtitles by https://example.test", "fansub-credit"),
    ("Downloaded from https://example.test", "fansub-credit"),
    ("Sous-titres : Alice", "studio-credit"),
    ("<i>Subtitles by Alice</i><br>https://example.test", "fansub-credit"),
])
def test_unambiguous_standalone_watermarks_still_have_a_reason(text, reason):
    assert pipeline.watermark_reason(text) == reason


def _review_workspace(tmp_path: Path, texts: list[str]):
    paths = []
    for index, text in enumerate(texts, 1):
        path = tmp_path / f"source-{index}.srt"
        path.write_text(text, encoding="utf-8")
        paths.append(path)
    work = tmp_path / "work"
    workflow.prepare(paths, work, "Authored audit fixture", 1, "utf-8", None, "en")
    project = runtime.read_json(work / "project.json")
    project["episodes"] = [{"id": "S01E01", "base": "s0001", "alternates": [
        item["id"] for item in project["sources"][1:]], "comparison": "Authored accounting fixture"}]
    runtime.write_json(work / "project.json", project)
    runtime.write_json(work / "glossary.json", {
        "series": project["series"], "terms_reviewed": True, "terms": [],
        "research": [{"url": "https://example.org/fixture", "note": "Authored structural fixture"}],
    })
    for item in project["sources"]:
        path = work / "worksheets" / (item["id"] + ".json")
        rows = runtime.read_json(path)
        for row in rows:
            row.update(action="preserve", reviewed=True)
        runtime.write_json(path, rows)
    return work


def test_build_keeps_reviewed_dialogue_containing_a_url(tmp_path):
    line = "Open https://example.test to read the manual."
    work = _review_workspace(tmp_path, [f"1\n00:00:01,000 --> 00:00:03,000\n{line}\n"])
    built = workflow.build(work)
    edition = Path(built["build"]) / built["episodes"][0]["file"]
    assert formats.visible(formats.parse(edition.read_text(encoding="utf-8"), "ass").cues[0].text, "ass").replace("\n", " ") == line
    assert built["episodes"][0]["cues"] == 1
    assert "watermarks_removed" not in built["episodes"][0]


def test_build_refuses_links_to_cues_removed_by_final_normalization(tmp_path):
    work = _review_workspace(tmp_path, [
        "1\n00:00:01,000 --> 00:00:03,000\nRetained dialogue\n\n"
        "2\n00:00:04,000 --> 00:00:06,000\nSubtitles by Alice\n",
        "1\n00:00:04,000 --> 00:00:06,000\nAlternate source text\n",
    ])
    sheet = work / "worksheets/s0002.json"
    rows = runtime.read_json(sheet)
    rows[0].update(action="alternate", links=["s0001:c000002"], note="Linked coverage must remain emitted")
    runtime.write_json(sheet, rows)
    with pytest.raises(ValueError, match="emitted|removed"):
        workflow.build(work)
    assert not (work / "builds").exists()
    rows[0]["links"] = ["s0001:c000001"]
    runtime.write_json(sheet, rows)
    assert workflow.build(work)["episodes"][0]["cues"] == 1


def test_build_refuses_self_links_consistently_with_handoff(tmp_path):
    work = _review_workspace(tmp_path, ["1\n00:00:01,000 --> 00:00:03,000\nRetained dialogue\n"])
    sheet = work / "worksheets/s0001.json"
    rows = runtime.read_json(sheet)
    rows[0]["links"] = ["s0001:c000001"]
    runtime.write_json(sheet, rows)
    with pytest.raises(ValueError, match="itself|self"):
        workflow.build(work)
    rows[0]["links"] = []
    runtime.write_json(sheet, rows)
    assert workflow.build(work)["episodes"][0]["cues"] == 1


def test_translator_mode_preserves_overlapping_primary_read_order():
    original = formats.parse(authored(overlapping()), "ass")
    converted, stats = formats.build_translator_ass(
        original, direction="ltr", fix_zwnj=False, wrap_lines=False,
        strip_watermarks=False, include_source_comments=True,
        style_line=",".join(original.styles["Default"]),
    )
    result = formats.parse(formats.serialize(converted, converted.cues), "ass")
    assert [cue.text for cue in result.cues] == [cue.text for cue in original.cues]
    assert [cue.start for cue in result.cues] == [1000, 0]
    assert stats["placeholder_slots"] == 0
    assert original.cues[0].start == 1000


def test_public_convert_cli_preserves_ass_literal_tags(tmp_path, capsys):
    path = tmp_path / "literal.ass"
    path.write_text(source(text="Print <i> and <br> literally."), encoding="utf-8")
    spec = importlib.util.spec_from_file_location("audit_revayat_cli", SCRIPTS / "revayat-subtitle.py")
    cli = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cli)
    output = tmp_path / "converted.ass"
    assert cli.execute(["convert", str(path), "--out", str(output), "--direction", "ltr", "--no-zwnj", "--max-chars", "200"]) == 0
    assert formats.parse(output.read_text(encoding="utf-8"), "ass").cues[0].text == "Print <i> and <br> literally."
    assert capsys.readouterr().err == ""


@unittest.skipUnless("--render" in sys.argv or os.environ.get("REVAYAT_TEST_RENDER") == "1", "Explicit FFmpeg rendering tier")
class TestNativeConversionPixels(TrackWorkspace):
    def test_translator_mode_preserves_source_compositing_pixels(self):
        raw = authored(overlapping())
        original = formats.parse(raw, "ass")
        edition, _ = formats.build_translator_ass(
            original, direction="ltr", fix_zwnj=False, wrap_lines=False, strip_watermarks=False,
            style_line=",".join(original.styles["Default"]),
        )
        baseline = self.pixels("source", raw)
        self.assertNotEqual(baseline, self.pixels("negative-reordered", authored(list(reversed(original.cues)))))
        self.assertEqual(baseline, self.pixels("converted", formats.serialize(edition, edition.cues)))

    def test_ass_literal_tags_keep_source_pixels(self):
        raw = source(text="A <i> B <br> C")
        original = formats.parse(raw, "ass")
        converted, _ = formats.ass_document_to_ass(
            original, direction="ltr", fix_zwnj=False, wrap_lines=False, strip_watermarks=False,
            style_line=",".join(original.styles["Default"]),
        )
        self.assertEqual(self.pixels("literal-source", raw),
                         self.pixels("literal-converted", formats.serialize(converted, converted.cues)))
