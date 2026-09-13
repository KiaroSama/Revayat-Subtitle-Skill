"""FFmpeg/libass render evidence and hash-checked ZIP delivery."""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import re
import shutil
import tempfile
import zipfile

from runtime import digest, local_path, read_json, run, write_json
from subtitle_formats import ARABIC, has_drawing, parse, visible


def ffmpeg_path(override: str | None = None) -> str:
    executable = override or os.environ.get("REVAYAT_FFMPEG") or shutil.which("ffmpeg")
    if not executable or not Path(executable).is_file():
        raise ValueError("FFmpeg not found; install a libass-enabled build or set REVAYAT_FFMPEG")
    return str(Path(executable).resolve())


def doctor(override: str | None = None) -> dict:
    try:
        executable = ffmpeg_path(override)
        filters = run([executable, "-hide_banner", "-filters"], timeout=15).decode("utf-8")
        ready = all(re.search(r"\s" + name + r"\s", filters) for name in ("ass", "subtitles", "color"))
        return {"ready": ready, "ffmpeg": executable, "libass": ready,
                "font_check": "A successful render plus visual inspection is required to verify Persian glyphs"}
    except (OSError, ValueError) as error:
        return {"ready": False, "error": str(error)}


def load_build(build: Path):
    manifest = read_json(build / "manifest.json")
    if manifest.get("version") != 1 or not manifest.get("episodes"):
        raise ValueError("Invalid build manifest")
    from workflow import load
    work = build.parent.parent
    project, _, sheets = load(work)
    glossary = read_json(work / "glossary.json")
    current = digest(json.dumps([project, sheets, glossary], ensure_ascii=False, sort_keys=True).encode("utf-8"))
    if current != manifest.get("identity"):
        raise ValueError("Workspace changed after this build; rebuild and review the new edition")
    docs = {}
    for episode in manifest["episodes"]:
        path = local_path(build, episode["file"])
        if digest(path.read_bytes()) != episode["sha256"]:
            raise ValueError("Built subtitle changed; rebuild from worksheets before rendering or delivery")
        docs[episode["id"]] = parse(path.read_text(encoding="utf-8"), path.suffix[1:])
    return manifest, docs


def sample_times(doc, all_cues: bool) -> list[float]:
    """Cover all risky cues, first/last, every style, and each animation phase."""
    chosen = {0, len(doc.cues) - 1}
    styles = set()
    for index, cue in enumerate(doc.cues):
        prose = visible(cue.text, doc.kind)
        style = cue.fields.get("style", "Default")
        risky = (ARABIC.search(prose) and re.search(r"[A-Za-z0-9\"'«»“”()\[\]]", prose))
        if (all_cues or risky or has_drawing(cue.text, doc.kind) or style not in styles
                or "\n" in prose or re.search(r"\\(?:t\(|k|K|move\(|fad|pos\()", cue.text)):
            chosen.add(index)
        styles.add(style)
    times = set()
    for index in sorted(chosen):
        cue = doc.cues[index]
        phases = (0.1, 0.5, 0.9) if re.search(r"\\(?:t\(|k|K|move\(|fad)", cue.text) else (0.5,)
        for fraction in phases:
            times.add(round((cue.start + (cue.end - cue.start) * fraction) / 1000, 4))
    return sorted(times)


def render(build: Path, episode_id: str, override: str | None, video: Path | None,
           fonts: Path | None, all_cues: bool) -> dict:
    executable = ffmpeg_path(override)
    manifest, docs = load_build(build)
    if episode_id not in docs:
        raise ValueError("Episode ID is not in this build")
    episode = next(item for item in manifest["episodes"] if item["id"] == episode_id)
    doc = docs[episode_id]
    times = sample_times(doc, all_cues)
    root = build / "renders"
    root.mkdir(exist_ok=True)
    # Every render is a new directory: old reviewed images are never overwritten.
    destination = Path(tempfile.mkdtemp(prefix=episode_id + "-", dir=root))
    frames = []
    try:
        with tempfile.TemporaryDirectory(prefix=".render-", dir=build) as temporary:
            stage = Path(temporary)
            input_name = "input." + doc.kind
            shutil.copyfile(local_path(build, episode["file"]), stage / input_name)
            font_option = ""
            if fonts:
                if not fonts.is_dir():
                    raise ValueError("fonts-dir must be a directory containing TTF/OTF files")
                (stage / "fonts").mkdir()
                for font in fonts.iterdir():
                    if font.is_file() and not font.is_symlink() and font.suffix.lower() in {".ttf", ".otf", ".ttc"}:
                        shutil.copyfile(font, stage / "fonts" / font.name)
                font_option = ":fontsdir=fonts"
            filter_ = (f"ass={input_name}:shaping=complex" if doc.kind == "ass" else f"subtitles={input_name}") + font_option
            for index, seconds in enumerate(times, 1):
                filename = f"frame-{index:04}.png"
                command = [executable, "-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-threads", "1"]
                if video:
                    command += ["-ss", str(seconds), "-i", str(video.resolve())]
                else:
                    command += ["-f", "lavfi", "-i", "color=c=0x202020:s=1280x720:r=1:d=1"]
                command += ["-filter_threads", "1", "-vf", f"setpts=PTS+{seconds}/TB,{filter_}",
                            "-frames:v", "1", "-update", "1", "-threads", "1", filename]
                run(command, cwd=stage, timeout=45)
                frame = stage / filename
                if not frame.is_file() or frame.stat().st_size < 100:
                    raise ValueError("FFmpeg produced no image at the requested time")
                raw = frame.read_bytes()
                if raw[:8] != b"\x89PNG\r\n\x1a\n":
                    raise ValueError("FFmpeg output is not a PNG")
                shutil.copyfile(frame, destination / filename)
                frames.append({"file": (destination / filename).relative_to(build).as_posix(),
                               "time_seconds": seconds, "sha256": digest(raw), "reviewed": False, "note": ""})
                logging.info("Rendered episode=%s frame=%d total=%d", episode_id, index, len(times))
        evidence = {"identity": manifest["identity"], "episode": episode_id,
                    "subtitle_sha256": episode["sha256"], "background": "video" if video else "neutral",
                    "all_cues": all_cues, "frames": frames}
        write_json(build / "renders" / f"{episode_id}.json", evidence)
        return {"review": str(build / "renders" / f"{episode_id}.json"), "frames": frames,
                "next": "Inspect every image; set reviewed=true and write an observation for each frame"}
    except BaseException:
        logging.error("Render failed; previous render evidence, if any, remains unchanged")
        raise


def package(build: Path, output: Path) -> dict:
    manifest, docs = load_build(build)
    names = [item["file"] for item in manifest["episodes"]]
    if len(set(names)) != len(names):
        raise ValueError("Duplicate episode filenames")
    backgrounds = set()
    for episode in manifest["episodes"]:
        evidence = read_json(build / "renders" / f"{episode['id']}.json")
        if (evidence.get("identity") != manifest["identity"] or evidence.get("episode") != episode["id"]
                or evidence.get("subtitle_sha256") != episode["sha256"]):
            raise ValueError("Render evidence is stale")
        expected = set(sample_times(docs[episode["id"]], evidence.get("all_cues") is True))
        frames = evidence.get("frames", [])
        if {frame["time_seconds"] for frame in frames} != expected or len(frames) != len(expected):
            raise ValueError("Render coverage is incomplete")
        for frame in frames:
            path = local_path(build, frame["file"])
            if frame.get("reviewed") is not True or not frame.get("note", "").strip():
                raise ValueError("Inspect every rendered frame before packaging")
            if digest(path.read_bytes()) != frame["sha256"]:
                raise ValueError("Rendered image changed after review")
        backgrounds.add(evidence.get("background"))
    if output.exists():
        raise ValueError("Output already exists; use a new ZIP filename")
    if output.suffix.lower() != ".zip":
        raise ValueError("Final output must have a .zip extension")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output.parent) as temporary:
        archive_path = Path(temporary) / "edition.zip"
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for episode in manifest["episodes"]:
                archive.write(local_path(build, episode["file"]), episode["file"])
        with zipfile.ZipFile(archive_path) as archive:
            if archive.namelist() != names or archive.testzip() is not None:
                raise ValueError("ZIP content validation failed")
            for episode in manifest["episodes"]:
                if digest(archive.read(episode["file"])) != episode["sha256"]:
                    raise ValueError("Packaged subtitle bytes differ from reviewed build")
        # Exclusive creation avoids overwriting a destination created during validation.
        with output.open("xb") as handle:
            handle.write(archive_path.read_bytes())
    logging.info("Packaged episodes=%d", len(names))
    return {"zip": str(output), "sha256": digest(output.read_bytes()), "episodes": len(names),
            "glossary": str(build / "glossary.json"), "backgrounds": sorted(backgrounds),
            "sync_verified": False}
