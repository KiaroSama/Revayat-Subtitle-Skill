"""Offline smoke tests for the subtitle sync bridge and its entry points."""

import importlib.util
import json
import math
import os
import shutil
import struct
import sys
import threading
import time
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


def test_locate_subsync_prefers_root_bin(bridge, tmp_path, monkeypatch):
    monkeypatch.setattr(bridge, "ROOT", tmp_path)
    name = "subsync.exe" if os.name == "nt" else "subsync"
    local = tmp_path / "bin" / name
    target = tmp_path / "rust" / "subsync-rs" / "target"
    for candidate in (local, target / "release" / name, target / "debug" / name):
        candidate.parent.mkdir(parents=True)
        candidate.touch()
    monkeypatch.setattr(bridge.shutil, "which", lambda _: str(tmp_path / "path" / name))

    assert bridge.locate_subsync() == local


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


@pytest.mark.parametrize("newline", ["\r", "\r\n", "\n"])
def test_sync_preserves_every_cue_and_original_newline_style(bridge, tmp_path, newline):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    output = tmp_path / "aligned.srt"
    document = "\n\n".join(
        f"{i}\n{srt_time(begin + 1300)} --> {srt_time(end + 1300)}\nText {i}"
        for i, (begin, end) in enumerate(INTERVALS, 1)
    ) + "\n"
    original.write_bytes(document.replace("\n", newline).encode("utf-8"))
    report = bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, fps_search=False)
    result = output.read_bytes()
    assert report["cues_total"] == len(INTERVALS)
    assert result.count(newline.encode("ascii")) == original.read_bytes().count(newline.encode("ascii"))
    for i, (begin, end) in enumerate(INTERVALS, 1):
        assert f"Text {i}".encode("ascii") in result
        assert srt_time(begin + 1300).encode("ascii") not in result


@pytest.mark.parametrize("numbered", [True, False])
def test_sync_refuses_ambiguous_srt_without_publishing_a_partial_result(bridge, tmp_path, numbered):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    output = tmp_path / "aligned.srt"
    report = tmp_path / "report.json"
    source = b"1\n00:00:01,000 --> 00:00:02,000\nFirst\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n"
    if not numbered:
        source = source[2:].replace(b"\n2\n", b"\n")
    original.write_bytes(source)
    with pytest.raises(bridge.SyncBridgeError, match="separator"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=report)
    assert original.read_bytes() == source
    assert not output.exists()
    assert not report.exists()


@pytest.mark.parametrize("options", [
    {"max_offset_sec": -1}, {"max_split_offset_ms": -1},
    {"split_penalty": float("nan")}, {"split_penalty": -1}, {"snap_ms": 5001},
])
def test_sync_invalid_configuration_never_publishes_output(bridge, tmp_path, options):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_text("1\n00:00:01,000 --> 00:00:02,000\nText\n", encoding="utf-8")
    output = tmp_path / "aligned.srt"
    report = tmp_path / "report.json"
    with pytest.raises(bridge.SyncBridgeError, match="configuration"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=report, **options)
    assert not output.exists()
    assert not report.exists()


def test_sync_silent_audio_and_existing_outputs_are_safe_failures(bridge, tmp_path):
    audio = tmp_path / "silent.wav"
    with wave.open(str(audio), "wb") as writer:
        writer.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
        writer.writeframes(bytes(32000))
    original = tmp_path / "original.srt"
    source = b"1\n00:00:00,100 --> 00:00:00,900\nText\n"
    original.write_bytes(source)
    output = tmp_path / "aligned.srt"
    with pytest.raises(bridge.SyncBridgeError, match="No usable speech"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio)
    assert not output.exists()
    output.write_bytes(b"existing deliverable")
    with pytest.raises(bridge.SyncBridgeError, match="existing files"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio)
    assert output.read_bytes() == b"existing deliverable"
    assert original.read_bytes() == source


def test_sync_real_ffmpeg_media_extraction(bridge, tmp_path):
    if shutil.which("ffmpeg") is None:
        if os.environ.get("REVAYAT_REQUIRE_FFMPEG") == "1":
            pytest.fail("Required FFmpeg media extraction coverage is unavailable")
        pytest.skip("FFmpeg is required for the real media extraction test")
    audio = tmp_path / "authored speech.wav"
    make_wav(audio)
    media = tmp_path / "authored media.mkv"
    bridge.run_owned([
        shutil.which("ffmpeg"), "-nostdin", "-v", "error", "-i", str(audio),
        "-c:a", "pcm_s16le", str(media),
    ], timeout=30)
    original = tmp_path / "original.srt"
    output = tmp_path / "aligned.srt"
    original.write_text("\n\n".join(
        f"{i}\n{srt_time(begin + 1300)} --> {srt_time(end + 1300)}\nText {i}"
        for i, (begin, end) in enumerate(INTERVALS, 1)
    ) + "\n", encoding="utf-8")
    report = bridge.sync_subtitle_to_video(media, original, output, fps_search=False)
    assert report["cues_total"] == len(INTERVALS)
    assert abs(report["global_offset_ms"] + 1300) <= 100
    assert report["overlap_score_after"] >= 0.9


@pytest.mark.parametrize("collision", ["input", "audio", "report-input", "report-audio", "output-report"])
def test_sync_rejects_path_collisions_without_altering_source_files(bridge, tmp_path, collision):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    output = tmp_path / "aligned.srt"
    report = tmp_path / "report.json"
    if collision == "input":
        output = original
    elif collision == "audio":
        output = audio
    elif collision == "report-input":
        report = original
    elif collision == "report-audio":
        report = audio
    else:
        report = output
    before = (original.read_bytes(), audio.read_bytes())
    with pytest.raises(bridge.SyncBridgeError):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=report)
    assert (original.read_bytes(), audio.read_bytes()) == before
    if output not in (original, audio):
        assert not output.exists()


@pytest.mark.parametrize("destination", ["output", "report"])
def test_sync_rejects_hardlinked_destinations(bridge, tmp_path, destination):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    source = b"1\n00:00:01,000 --> 00:00:02,000\nText\n"
    original.write_bytes(source)
    output = tmp_path / "aligned.srt"
    report = tmp_path / "report.json"
    alias = output if destination == "output" else report
    os.link(original, alias)
    with pytest.raises(bridge.SyncBridgeError):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=report)
    assert original.read_bytes() == source
    assert alias.read_bytes() == source
    if destination == "report":
        assert not output.exists()


@pytest.mark.skipif(os.name == "nt", reason="Symlink creation requires an optional Windows privilege")
def test_sync_rejects_symlinked_parent_alias_of_output(bridge, tmp_path):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    directory = tmp_path / "outputs"
    directory.mkdir()
    alias = tmp_path / "alias"
    alias.symlink_to(directory, target_is_directory=True)
    output = directory / "aligned.srt"
    with pytest.raises(bridge.SyncBridgeError, match="different files"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=alias / output.name)
    assert not output.exists()


@pytest.mark.skipif(sys.platform != "linux", reason="Uses Linux RLIMIT_FSIZE to cause a real partial write")
def test_sync_write_failure_leaves_output_name_retryable(bridge, tmp_path):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_text("1\n00:00:01,000 --> 00:00:02,000\n" + "large subtitle " * 1000, encoding="utf-8")
    output = tmp_path / "aligned.srt"
    before = set(tmp_path.iterdir())
    limiter = (
        "import os,resource,signal,sys; "
        "signal.signal(signal.SIGXFSZ,signal.SIG_IGN); "
        "resource.setrlimit(resource.RLIMIT_FSIZE,(128,128)); "
        "os.execv(sys.argv[1],sys.argv[1:])"
    )
    result = bridge.run_owned([
        sys.executable, "-c", limiter, str(bridge.locate_subsync()),
        "--wav", str(audio), "--in", str(original), "--out", str(output),
        "--no-fps-search",
    ], timeout=15, check=False)
    assert result.returncode != 0
    assert not output.exists(), "A failed write published a truncated subtitle at its final path"
    assert set(tmp_path.iterdir()) == before
    report = bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, fps_search=False)
    assert report["cues_total"] == 1
    assert "large subtitle " * 1000 in output.read_text(encoding="utf-8")


@pytest.mark.skipif(os.name == "nt", reason="Uses POSIX directory write permissions")
def test_sync_report_staging_failure_does_not_publish_subtitle(bridge, tmp_path):
    if os.geteuid() == 0:
        pytest.skip("The superuser bypasses the read-only directory fixture")
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    output = tmp_path / "aligned.srt"
    report_dir = tmp_path / "read only reports"
    report_dir.mkdir()
    receipt = report_dir / "report.json"
    report_dir.chmod(0o500)
    try:
        with pytest.raises(bridge.SyncBridgeError):
            bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=receipt)
        assert not output.exists(), "Subtitle was published before report writability was established"
        assert not receipt.exists()
    finally:
        report_dir.chmod(0o700)
    report = bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=receipt)
    assert report == json.loads(receipt.read_text(encoding="utf-8"))


def test_sync_continuous_audio_does_not_publish_a_spurious_fps_change(bridge, tmp_path):
    audio = tmp_path / "continuous tone.wav"
    with wave.open(str(audio), "wb") as writer:
        writer.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
        writer.writeframes(b"".join(struct.pack("<h", int(9000 * math.sin(math.tau * 400 * i / 16000)))
                                  for i in range(160000)))
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    output = tmp_path / "aligned.srt"
    receipt = tmp_path / "report.json"
    with pytest.raises(bridge.SyncBridgeError, match="continuous"):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=receipt)
    assert not output.exists()
    assert not receipt.exists()


@pytest.mark.skipif(os.name == "nt", reason="Symlink creation requires an optional Windows privilege")
@pytest.mark.parametrize("destination", ["output", "report"])
def test_sync_preserves_dangling_destination_symlinks(bridge, tmp_path, destination):
    audio = tmp_path / "audio.wav"
    make_wav(audio)
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    output = tmp_path / "aligned.srt"
    receipt = tmp_path / "report.json"
    alias = output if destination == "output" else receipt
    missing = tmp_path / "missing.srt"
    alias.symlink_to(missing)
    with pytest.raises(bridge.SyncBridgeError):
        bridge.sync_subtitle_to_video(None, original, output, wav_path=audio, json_report=receipt)
    assert alias.is_symlink()
    assert alias.readlink() == missing
    assert not missing.exists()
    if destination == "report":
        assert not output.exists()


@pytest.mark.skipif(os.name == "nt", reason="Uses a POSIX FIFO to block a real FFmpeg read")
def test_sync_timeout_terminates_owned_rust_and_ffmpeg(bridge, tmp_path):
    import psutil

    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        if os.environ.get("REVAYAT_REQUIRE_FFMPEG") == "1":
            pytest.fail("Required FFmpeg media extraction coverage is unavailable")
        pytest.skip("FFmpeg is required for the owned pipeline timeout test")
    fifo = tmp_path / "blocked media.fifo"
    os.mkfifo(fifo)
    original = tmp_path / "original.srt"
    original.write_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nText\n")
    output = tmp_path / "aligned.srt"
    observed = []
    stop = threading.Event()

    def observe_owned_ffmpeg():
        while not stop.wait(0.02):
            for process in psutil.Process().children(recursive=True):
                try:
                    if str(fifo) in process.cmdline() and Path(process.name()).stem == "ffmpeg":
                        observed.append(process)
                        return
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    pass

    def active(process):
        try:
            return process.is_running() and process.status() not in (psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD)
        except psutil.NoSuchProcess:
            return False

    observer = threading.Thread(target=observe_owned_ffmpeg)
    observer.start()
    began = time.monotonic()
    try:
        with pytest.raises(bridge.SyncBridgeError, match="timed out"):
            bridge.sync_subtitle_to_video(fifo, original, output, ffmpeg=ffmpeg, timeout=2)
        stop.set()
        observer.join(timeout=3)
        assert not observer.is_alive()
        assert observed, "The real FFmpeg child must start before cancellation is tested"
        assert time.monotonic() - began < 10
        # POSIX delivers SIGKILL to the entire group, but orphan status changes
        # may be observed just after the supervised leader has been reaped.
        deadline = time.monotonic() + 3
        for process in observed:
            while active(process) and time.monotonic() < deadline:
                threading.Event().wait(0.02)
            assert not active(process)
        assert not output.exists()
    finally:
        stop.set()
        observer.join(timeout=3)
        for process in observed:
            try:
                if active(process):
                    process.kill()
                    process.wait(timeout=3)
            except psutil.NoSuchProcess:
                pass
