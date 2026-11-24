"""Offline smoke tests for the subtitle sync bridge and its entry points."""

import importlib.util
import json
import math
import struct
import wave
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / "skills" / "revayat-subtitle" / "scripts" / "sync_bridge.py"
INTERVALS = [(350, 970), (1590, 2630), (3710, 4480), (5110, 6640)]


@pytest.fixture(scope="module")
def bridge():
    import sys

    sys.path.insert(0, str(BRIDGE.parent))
    try:
        yield load_bridge()
    finally:
        sys.path.remove(str(BRIDGE.parent))


def load_bridge():
    assert BRIDGE.is_file(), "The sync bridge is not installed"
    spec = importlib.util.spec_from_file_location("sync_bridge", BRIDGE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def make_wav(path):
    frames = bytearray()
    for i in range(8 * 16000):
        if any(begin * 16 <= i < end * 16 for begin, end in INTERVALS):
            value = sum(3500 * math.sin(math.tau * freq * i / 16000) for freq in (400, 1200, 2400))
        else:
            value = (i * 4093 % 131) - 65
        frames.extend(struct.pack("<h", int(value)))
    with wave.open(str(path), "wb") as writer:
        writer.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
        writer.writeframes(frames)


def srt_time(ms):
    return f"00:00:{ms // 1000:02},{ms % 1000:03}"


def ass_time(ms):
    return f"0:00:{ms // 1000:02}.{ms % 1000 // 10:02}"


@pytest.mark.parametrize("extension,shift", [("srt", 1300), ("ass", -250)])
def test_sync_wav_preserves_unicode_and_inline_tags(bridge, tmp_path, extension, shift):
    audio = tmp_path / "sample audio.wav"
    make_wav(audio)
    original = tmp_path / f"original.{extension}"
    output = tmp_path / f"aligned.{extension}"
    receipt = tmp_path / "sync report.json"
    if extension == "srt":
        text = r"<i>سلام</i> {\an8}"
        document = "\n\n".join(
            f"{i}\n{srt_time(begin + shift)} --> {srt_time(end + shift)}\n{text}\n<b>دنیا</b>"
            for i, (begin, end) in enumerate(INTERVALS, 1)
        ) + "\n"
    else:
        text = r"{\i1}{\an8}سلام، دنیا"
        document = "[Script Info]\nTitle: Sample\n[V4+ Styles]\n[Events]\n"
        document += "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
        document += "Comment: 0,0:00:00.00,0:00:00.10,Default,,0,0,0,,Editorial note\n"
        document += "".join(
            f"Dialogue: 1,{ass_time(begin + shift)},{ass_time(end + shift)},Default,A,0,0,0,,{text}\n"
            for begin, end in INTERVALS
        )
    original.write_bytes(("﻿" + document.replace("\n", "\r\n")).encode("utf-8"))
    report = bridge.sync_subtitle_to_video(
        None, original, output, wav_path=audio, json_report=receipt, fps_search=False
    )
    assert report == json.loads(receipt.read_text(encoding="utf-8"))
    assert report["cues_total"] == len(INTERVALS)
    assert abs(report["global_offset_ms"] + shift) <= 100
    assert report["fps_ratio"] == 1.0
    assert report["overlap_score_after"] > report["overlap_score_before"]
    assert report["overlap_score_after"] >= 0.9
    assert report["split_segments"][0]["start_cue"] == 1
    assert report["split_segments"][-1]["end_cue"] == len(INTERVALS)
    result = output.read_bytes()
    assert not result.startswith(b"\xef\xbb\xbf")
    assert b"\r\n" in result
    assert result.decode("utf-8").count(text) == len(INTERVALS)
    assert original.read_bytes().startswith(b"\xef\xbb\xbf")
    if extension == "ass":
        assert b"Comment: 0,0:00:00.00,0:00:00.10" in result
    else:
        assert result.decode("utf-8").count("<b>دنیا</b>") == len(INTERVALS)


@pytest.mark.parametrize("skill", ["revayat-subtitle/sync", "revayat-subtitle-sync"])
def test_skill_frontmatter(skill):
    path = ROOT / "skills" / skill / "SKILL.md"
    assert path.is_file(), f"Missing skill entry: {skill}"
    marker, head, body = path.read_text(encoding="utf-8").split("---\n", 2)
    assert marker == ""
    fields = dict(line.split(": ", 1) for line in head.splitlines())
    assert fields["name"] == "revayat-subtitle-sync"
    assert 0 < len(fields["description"]) <= 1024
    assert "sync" in body.casefold()
