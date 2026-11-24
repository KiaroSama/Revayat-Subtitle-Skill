"""Run the Rust subtitle sync engine from Python or the command line."""

from __future__ import annotations

import argparse
import json
import logging
import math
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any
from process_control import ToolFailure, run as run_owned
from runtime import LogConfigurationError, operational_log

ROOT = Path(__file__).resolve().parents[3]
BUILD_TIMEOUT = 600.0
SYNC_TIMEOUT = 300.0
REQUIRED_REPORT_FIELDS = {
    "audio_duration_ms",
    "cues_snapped",
    "cues_total",
    "fps_ratio",
    "global_offset_ms",
    "max_cue_delta_ms",
    "median_cue_delta_ms",
    "overlap_score_after",
    "overlap_score_before",
    "speech_segments_count",
    "split_segments",
}


class SyncBridgeError(RuntimeError):
    """The sync binary could not be located, built, or run successfully."""


def _run(command: list[str], timeout: float, operation: str) -> str:
    logging.info("Start external step: %s", operation)
    try:
        return run_owned(command, timeout=timeout).decode("utf-8-sig")
    except ToolFailure as exc:
        detail = exc.stderr.decode("utf-8", errors="replace").strip()
        raise SyncBridgeError(f"Could not {operation} (exit {exc.returncode}): {detail or 'no error output'}") from exc
    except subprocess.TimeoutExpired as exc:
        raise SyncBridgeError(f"Could not {operation}: timed out after {timeout:g} seconds") from exc
    except UnicodeDecodeError as exc:
        raise SyncBridgeError(f"Could not {operation}: output was not valid UTF-8") from exc
    except (OSError, ValueError) as exc:
        raise SyncBridgeError(f"Could not {operation}: {exc}") from exc


def locate_subsync(*, build_timeout: float = BUILD_TIMEOUT) -> Path:
    """Find a local or PATH binary, or build the Rust crate if available."""
    crate = ROOT / "rust" / "subsync-rs"
    target = crate / "target"
    name = "subsync.exe" if os.name == "nt" else "subsync"
    for profile in ("release", "debug"):
        binary = target / profile / name
        if binary.is_file():
            return binary
    if path := shutil.which("subsync"):
        return Path(path)
    manifest = crate / "Cargo.toml"
    cargo = shutil.which("cargo")
    if not manifest.is_file() or cargo is None:
        raise SyncBridgeError("subsync was not found; install it on PATH or run from a checkout with Cargo available")
    _run(
        [cargo, "build", "--locked", "--release", "--manifest-path", str(manifest), "--target-dir", str(target)],
        build_timeout,
        "build subsync",
    )
    binary = target / "release" / name
    if not binary.is_file():
        raise SyncBridgeError("Cargo did not produce the subsync release binary")
    return binary


def sync_subtitle_to_video(
    video_path: str | os.PathLike[str] | None,
    subtitle_in: str | os.PathLike[str],
    subtitle_out: str | os.PathLike[str],
    *,
    wav_path: str | os.PathLike[str] | None = None,
    ffmpeg: str | os.PathLike[str] = "ffmpeg",
    max_offset_sec: float = 60,
    max_split_offset_ms: int = 15000,
    split_penalty: float = 7.0,
    snap_ms: int = 350,
    fps_search: bool = True,
    json_report: str | os.PathLike[str] | None = None,
    timeout: float = SYNC_TIMEOUT,
) -> dict[str, Any]:
    """Align cues to video audio or a mono PCM WAV and return the JSON report."""
    if (video_path is None) == (wav_path is None):
        raise SyncBridgeError("Choose exactly one of video_path or wav_path")
    if not math.isfinite(timeout) or timeout <= 0:
        raise SyncBridgeError("The sync timeout must be a positive finite number")
    if video_path is not None:
        source = ["--video", str(Path(video_path).absolute())]
    else:
        assert wav_path is not None
        source = ["--wav", str(Path(wav_path).absolute())]
    command = [
        str(locate_subsync()), *source,
        "--in", str(Path(subtitle_in).absolute()),
        "--out", str(Path(subtitle_out).absolute()),
        "--ffmpeg", os.fspath(ffmpeg),
        "--max-offset-sec", str(max_offset_sec),
        "--max-split-offset-ms", str(max_split_offset_ms),
        "--split-penalty", str(split_penalty),
        "--snap-ms", str(snap_ms),
    ]
    if not fps_search:
        command.append("--no-fps-search")
    if json_report is not None:
        command.extend(["--json-report", str(Path(json_report).absolute())])
    try:
        report = json.loads(_run(command, timeout, "sync subtitles"))
    except (json.JSONDecodeError, RecursionError) as exc:
        raise SyncBridgeError("subsync did not return valid JSON") from exc
    if not isinstance(report, dict) or not REQUIRED_REPORT_FIELDS.issubset(report):
        raise SyncBridgeError("subsync returned an incomplete sync report")
    return report


def main(argv: list[str] | None = None) -> int:
    """Parse sync options and print a report to stdout."""
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--video", type=Path)
    source.add_argument("--wav", type=Path)
    parser.add_argument("--in", dest="subtitle_in", type=Path, required=True)
    parser.add_argument("--out", dest="subtitle_out", type=Path, required=True)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--max-offset-sec", type=float, default=60)
    parser.add_argument("--max-split-offset-ms", type=int, default=15000)
    parser.add_argument("--split-penalty", type=float, default=7.0)
    parser.add_argument("--snap-ms", type=int, default=350)
    parser.add_argument("--no-fps-search", action="store_false", dest="fps_search")
    parser.add_argument("--json-report", type=Path)
    parser.add_argument("--timeout", type=float, default=SYNC_TIMEOUT)
    options = vars(parser.parse_args(argv))
    options["video_path"] = options.pop("video")
    options["wav_path"] = options.pop("wav")
    try:
        with operational_log("sync_bridge"):
            report = sync_subtitle_to_video(**options)
            logging.info("Sync completed cue_count=%d", report["cues_total"])
    except (SyncBridgeError, LogConfigurationError) as exc:
        parser.exit(1, f"sync_bridge: {exc}\n")
    # ASCII escapes keep Unicode paths round-trippable even when stdout is a legacy console.
    print(json.dumps(report, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
