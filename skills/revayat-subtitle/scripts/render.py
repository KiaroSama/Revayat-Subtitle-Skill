"""FFmpeg/libass render evidence and hash-checked ZIP delivery."""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import re
import shutil
import tempfile
import time
import zipfile
import validation

from runtime import digest, file_fingerprint, local_path, output_directory, read_json, read_limited, run, write_json
from subtitle_formats import ARABIC, has_drawing, parse, visible, pieces, style_references
from markup import has_ltr, has_rtl, overrides, srt_font_names
from publication import publish_bytes
from png_validation import decode_png, MAX_PNG_BYTES


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
    manifest = validation.obj(read_json(local_path(build, "manifest.json")), "manifest.json")
    if type(manifest.get("version")) is not int:
        raise ValueError("manifest.json.version: expected an integer schema version")
    if manifest.get("version") == 1:
        raise ValueError("Legacy build: run build --work on its workspace, then render and review the new edition; old artifacts remain unchanged")
    if type(manifest.get("version")) is not int or manifest["version"] != 2:
        raise ValueError("manifest.json.version: unsupported build schema")
    from workflow import assemble
    work = build.parent.parent
    expected, files, glossary = assemble(work)
    current = expected["identity"]
    if manifest != expected:
        raise ValueError("Build manifest or workspace modified; rebuild and review the new edition")
    if build.name != current or build.parent.name != "builds":
        raise ValueError("Build must remain at its original workspace path")
    if read_json(local_path(build, "glossary.json")) != glossary:
        raise ValueError("Build glossary differs from the reviewed continuity glossary")
    docs = {}
    for episode in manifest["episodes"]:
        path = local_path(build, episode["file"])
        raw = read_limited(path, len(files[path.name]))
        if raw != files[path.name] or digest(raw) != episode["sha256"]:
            raise ValueError("Built subtitle changed; rebuild from worksheets before rendering or delivery")
        docs[episode["id"]] = parse(raw.decode("utf-8"), path.suffix[1:])
    return manifest, docs


SAMPLER_VERSION = 4
ANIMATED_TAGS = frozenset({"t", "k", "K", "kf", "ko", "kt", "move", "fad", "fade"})
MAX_RENDER_FRAMES = 5000


def animated_event(cue, commands) -> bool:
    # Legacy ASS Effect animations are event-local, even without override tags.
    effect = cue.fields.get("effect", "")
    return effect.startswith(("Banner;", "Scroll up;", "Scroll down;")) or any(
        name in ANIMATED_TAGS for name, _ in commands)


def sample_plan(doc, all_cues: bool, changed_indices=()) -> list[dict]:
    """Bind each required sample time to the emitted cues whose risks it covers."""
    validation.boolean(all_cues, "render.all_cues")
    chosen = {0, len(doc.cues) - 1, *(index - 1 for index in changed_indices)}
    styles, signatures = set(), set()
    tokens = {}
    for index, cue in enumerate(doc.cues):
        commands = [(name, argument) for type_, value in pieces(cue.text, doc.kind)
                    if type_ == "tag" and doc.kind == "ass"
                    for name, argument, _, _ in overrides(value)]
        if doc.kind == "srt":
            commands = [("srt-tag", value.casefold()) for type_, value in pieces(cue.text, doc.kind) if type_ == "tag"]
        tokens[index] = commands
        refs = style_references(cue) if doc.kind == "ass" else {"Default"}
        signature = (cue.fields.get("style", "Default"), tuple(commands),
                     tuple(cue.fields.get(field, "") for field in ("layer", "marginl", "marginr", "marginv", "effect")))
        prose = visible(cue.text, doc.kind)
        risky = has_rtl(prose) and (has_ltr(prose) or re.search(r"[\"'«»“”()<>\[\]]", prose))
        animated = animated_event(cue, commands)
        if (all_cues or animated or risky or has_drawing(cue.text, doc.kind) or refs - styles
                or signature not in signatures or "\n" in prose):
            chosen.add(index)
        styles.update(refs)
        signatures.add(signature)
    times = {}
    for index in sorted(chosen):
        if not 0 <= index < len(doc.cues):
            raise ValueError("Changed-structure cue index is outside the episode")
        cue = doc.cues[index]
        animated = animated_event(cue, tokens[index])
        for fraction in ((0.1, 0.5, 0.9) if animated else (0.5,)):
            seconds = round((cue.start + (cue.end - cue.start) * fraction) / 1000, 4)
            times.setdefault(seconds, set()).add(index + 1)
            if len(times) > MAX_RENDER_FRAMES:
                raise ValueError("Render sampling exceeds the 5000-frame episode budget")
    return [{"time_seconds": seconds, "cue_indices": sorted(indices)}
            for seconds, indices in sorted(times.items())]


def sample_times(doc, all_cues: bool) -> list[float]:
    return [item["time_seconds"] for item in sample_plan(doc, all_cues)]


FRAME_FIELDS = ("file", "time_seconds", "cue_indices", "sha256", "width", "height")


def changed_indices(episode: dict) -> list[int]:
    return [item["emitted_index"] for item in episode["provenance"] if item.get("structure_changed")]


def requested_fonts(doc) -> list[str]:
    names = set()
    if "fontname" in doc.style_fields:
        index = doc.style_fields.index("fontname")
        names.update(row[index].strip() for row in doc.styles.values() if row[index].strip())
    for cue in doc.cues:
        for type_, value in pieces(cue.text, doc.kind):
            if type_ == "tag" and doc.kind == "ass":
                names.update(arg.strip() for name, arg, _, _ in overrides(value) if name == "fn" and arg.strip())
            elif type_ == "tag" and doc.kind == "srt":
                names.update(srt_font_names(value))
    return sorted(names)


def write_render_evidence(build: Path, manifest: dict, episode: dict, doc,
                          frames: list[dict], all_cues: bool, recipe: dict) -> dict:
    path = local_path(build, f"renders/{episode['id']}.json")
    if not frames:
        raise ValueError("A render must produce at least one frame")
    immutable = [{key: frame[key] for key in FRAME_FIELDS} for frame in frames]
    samples = sample_plan(doc, all_cues, changed_indices(episode))
    if [{key: frame[key] for key in ("time_seconds", "cue_indices")} for frame in frames] != samples:
        raise ValueError("Rendered frames do not match their required cue samples")
    background = "video" if recipe.get("video") is not None else "neutral"
    receipt = {"version": 2, "identity": manifest["identity"], "episode": episode["id"],
               "subtitle_sha256": episode["sha256"], "all_cues": all_cues, "background": background,
               "recipe": recipe, "samples": samples, "frames": immutable}
    receipt_relative = (Path(frames[0]["file"]).parent / "receipt.json").as_posix()
    receipt_path = local_path(build, receipt_relative)
    write_json(receipt_path, receipt)
    evidence = {key: receipt[key] for key in ("version", "identity", "episode", "subtitle_sha256", "all_cues", "background")}
    evidence.update(receipt_file=receipt_path.relative_to(build).as_posix(),
                    receipt_sha256=digest(receipt_path.read_bytes()),
                    frames=[{**frame, "reviewed": False, "note": ""} for frame in immutable])
    write_json(path, evidence)
    return {"review": str(path), "frames": evidence["frames"],
            "next": "Inspect every image; set reviewed=true and write an observation for each frame"}


def render(build: Path, episode_id: str, override: str | None, video: Path | None,
           fonts: Path | None, all_cues: bool) -> dict:
    # Reject linked output roots before invoking tools or publishing any artifact.
    root = local_path(build, "renders")
    executable = ffmpeg_path(override)
    manifest, docs = load_build(build)
    if episode_id not in docs:
        raise ValueError("Episode ID is not in this build")
    local_path(build, f"renders/{episode_id}.json")
    episode = next(item for item in manifest["episodes"] if item["id"] == episode_id)
    doc = docs[episode_id]
    samples = sample_plan(doc, all_cues, changed_indices(episode))
    deadline = time.monotonic() + 900
    renderer = file_fingerprint(Path(executable), 256 * 1024 * 1024)
    renderer["version"] = run([executable, "-hide_banner", "-version"], timeout=15).decode("utf-8").splitlines()[0][:500]
    recipe = {"sampler": SAMPLER_VERSION, "profile": "rgb24-v1", "renderer": renderer,
              "fonts": [], "requested_fonts": requested_fonts(doc), "font_resolution": "system fallback requires visual inspection; selected system font files are not attested",
              "video": file_fingerprint(video) if video else None}
    root.mkdir(exist_ok=True)
    # Every render is a new directory: old reviewed images are never overwritten.
    destination = output_directory(root, episode_id + "-")
    frames = []
    rendered_bytes = 0
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
                font_total = 0
                font_entries = []
                with os.scandir(fonts) as entries:
                    for entry_index, entry in enumerate(entries, 1):
                        if entry_index > 10000:
                            raise ValueError("Font directory exceeds the entry budget")
                        font_entries.append(Path(entry.path))
                for font in sorted(font_entries):
                    if font.is_file() and not font.is_symlink() and font.suffix.lower() in {".ttf", ".otf", ".ttc"}:
                        info = file_fingerprint(font, 32 * 1024 * 1024)
                        font_total += info["bytes"]
                        if font_total > 64 * 1024 * 1024 or len(recipe["fonts"]) >= 128:
                            raise ValueError("Selected fonts exceed the 64 MiB/128-file budget")
                        shutil.copyfile(font, stage / "fonts" / font.name)
                        if file_fingerprint(stage / "fonts" / font.name, 32 * 1024 * 1024) != info:
                            raise ValueError("Font changed during render staging")
                        recipe["fonts"].append(info)
                if not recipe["fonts"]:
                    raise ValueError("fonts-dir contains no supported font files")
                font_option = ":fontsdir=fonts"
            filter_ = (f"ass={input_name}:shaping=complex" if doc.kind == "ass" else f"subtitles={input_name}") + font_option
            for index, sample in enumerate(samples, 1):
                seconds = sample["time_seconds"]
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ValueError("Render exceeded its 900-second episode budget; existing evidence remains intact")
                filename = f"frame-{index:04}.png"
                command = [executable, "-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-threads", "1"]
                if video:
                    command += ["-ss", str(seconds), "-i", str(video.resolve())]
                else:
                    command += ["-f", "lavfi", "-i", "color=c=0x202020:s=1280x720:r=1:d=1"]
                command += ["-filter_threads", "1", "-vf", f"settb=AVTB,setpts=PTS-STARTPTS+{seconds}/TB,{filter_}",
                            "-frames:v", "1", "-update", "1", "-pix_fmt", "rgb24", "-threads", "1", filename]
                run(command, cwd=stage, timeout=min(45, remaining))
                frame = stage / filename
                if not frame.is_file() or frame.stat().st_size < 100:
                    raise ValueError("FFmpeg produced no image at the requested time")
                with frame.open("rb") as handle:
                    raw = handle.read(MAX_PNG_BYTES + 1)
                width, height = decode_png(raw)
                rendered_bytes += len(raw)
                if rendered_bytes > 2 * 1024**3:
                    raise ValueError("Render exceeded its 2 GiB episode artifact budget")
                shutil.copyfile(frame, destination / filename)
                frames.append({"file": (destination / filename).relative_to(build).as_posix(),
                               **sample, "sha256": digest(raw), "width": width, "height": height})
                logging.info("Rendered episode=%s frame=%d total=%d", episode_id, index, len(samples))
        if video and file_fingerprint(video) != recipe["video"]:
            raise ValueError("Video changed during rendering")
        if file_fingerprint(Path(executable), 256 * 1024 * 1024) != {key: renderer[key] for key in ("name", "bytes", "sha256")}:
            raise ValueError("Renderer changed during rendering")
        if time.monotonic() > deadline:
            raise ValueError("Render exceeded its 900-second episode budget")
        load_build(build)
        return write_render_evidence(build, manifest, episode, doc, frames, all_cues, recipe)
    except BaseException:
        logging.error("Render failed; previous render evidence, if any, remains unchanged")
        raise


def check_delivery(build: Path) -> tuple[dict, list[str], set[str], int]:
    manifest, docs = load_build(build)
    names = [item["file"] for item in manifest["episodes"]]
    if len(set(names)) != len(names):
        raise ValueError("Duplicate episode filenames")
    backgrounds, paths, physical = set(), set(), set()
    frame_count = 0
    deadline = time.monotonic() + 900
    for episode in manifest["episodes"]:
        where = f"renders/{episode['id']}.json"
        evidence = validation.render_record(read_json(local_path(build, where)), where)
        if (evidence["identity"] != manifest["identity"] or evidence["episode"] != episode["id"]
                or evidence["subtitle_sha256"] != episode["sha256"]):
            raise ValueError("Render evidence is stale")
        receipt_path = local_path(build, evidence["receipt_file"])
        directory = receipt_path.parent
        if (receipt_path.name != "receipt.json" or directory.parent != (build / "renders").resolve()
                or not directory.name.startswith(episode["id"] + "-")):
            raise ValueError("Render receipt is outside this episode's artifacts")
        receipt_bytes = read_limited(receipt_path, 16 * 1024 * 1024)
        if digest(receipt_bytes) != evidence["receipt_sha256"]:
            raise ValueError("Render receipt changed; rerender rather than transferring approval")
        receipt = validation.obj(read_json(receipt_path), "render receipt")
        for field in ("version", "identity", "episode", "subtitle_sha256", "background", "all_cues"):
            if type(receipt.get(field)) is not type(evidence[field]) or receipt[field] != evidence[field]:
                raise ValueError(f"Render receipt.{field}: association changed")
        recipe = validation.obj(receipt.get("recipe"), "render receipt.recipe")
        if type(recipe.get("sampler")) is not int or recipe["sampler"] != SAMPLER_VERSION or recipe.get("profile") != "rgb24-v1":
            raise ValueError("Render sampling/profile recipe changed; rerender the edition")
        renderer = validation.fingerprint(recipe.get("renderer"), "render receipt.renderer", 256 * 1024 * 1024)
        validation.string(renderer.get("version"), "render receipt.renderer.version")
        for index, font in enumerate(validation.array(recipe.get("fonts"), "render receipt.fonts", 128)):
            validation.fingerprint(font, f"render receipt.fonts[{index}]", 32 * 1024 * 1024)
        for font in validation.array(recipe.get("requested_fonts"), "render receipt.requested_fonts", 10000):
            validation.string(font, "render receipt.requested_fonts")
        if recipe["requested_fonts"] != requested_fonts(docs[episode["id"]]):
            raise ValueError("Render font requests differ from the subtitle")
        video = recipe.get("video")
        if video is not None:
            validation.fingerprint(video, "render receipt.video")
        if ("video" if video is not None else "neutral") != evidence["background"]:
            raise ValueError("Render background differs from its video provenance")
        expected = sample_plan(docs[episode["id"]], evidence["all_cues"], changed_indices(episode))
        samples = validation.array(receipt.get("samples"), "render receipt.samples", MAX_RENDER_FRAMES)
        for sample in samples:
            validation.obj(sample, "render receipt.sample")
            validation.number(sample.get("time_seconds"), "render receipt.sample.time_seconds")
            for index in validation.array(sample.get("cue_indices"), "render receipt.sample.cue_indices"):
                validation.integer(index, "render receipt.sample.cue_indices", 1, 100000)
        immutable = validation.array(receipt.get("frames"), "render receipt.frames", MAX_RENDER_FRAMES)
        frames = evidence["frames"]
        if samples != expected or len(frames) != len(expected) or len(immutable) != len(expected):
            raise ValueError("Render coverage is incomplete")
        for index, (frame, original, sample) in enumerate(zip(frames, immutable, expected), 1):
            if time.monotonic() > deadline:
                raise ValueError("Delivery QA exceeded its 900-second image budget")
            validation.frame(original, f"render receipt.frames[{index - 1}]", review=False)
            if {key: frame[key] for key in FRAME_FIELDS} != original:
                raise ValueError("Reviewed frame metadata differs from its immutable render receipt")
            if {key: frame[key] for key in ("time_seconds", "cue_indices")} != sample:
                raise ValueError("Frame is not associated with the required sample")
            path = local_path(build, frame["file"])
            if path.parent != directory or path.name != f"frame-{index:04}.png":
                raise ValueError("Frame path is not its assigned episode/sample file")
            key = os.path.normcase(str(path))
            info = path.stat()
            inode = (info.st_dev, info.st_ino)
            if key in paths or (info.st_ino and inode in physical):
                raise ValueError("Duplicate or aliased render frame path")
            paths.add(key)
            physical.add(inode)
            if frame["reviewed"] is not True or not frame["note"].strip():
                raise ValueError("Inspect every rendered frame before packaging")
            raw = read_limited(path, MAX_PNG_BYTES)
            if digest(raw) != frame["sha256"]:
                raise ValueError("Rendered image changed after review")
            if decode_png(raw) != (frame["width"], frame["height"]):
                raise ValueError("PNG dimensions differ from the render receipt")
        backgrounds.add(evidence["background"])
        frame_count += len(frames)
    return manifest, names, backgrounds, frame_count


def qa(build: Path) -> dict:
    manifest, names, backgrounds, frame_count = check_delivery(build)
    return {"ok": True, "identity": manifest["identity"], "episodes": len(names),
            "reviewed_cues": manifest["reviewed_cues"], "reviewed_frames": frame_count,
            "font_policy": manifest["font_policy"], "backgrounds": sorted(backgrounds),
            "sync_verified": False}


def package(build: Path, output: Path) -> dict:
    manifest, names, backgrounds, _ = check_delivery(build)
    if output.exists():
        raise ValueError("Output already exists; use a new ZIP filename")
    if output.suffix.lower() != ".zip":
        raise ValueError("Final output must have a .zip extension")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = tempfile.TemporaryDirectory(prefix=".package-", dir=output.parent)
    try:
        archive_path = Path(temporary.name) / "edition.zip"
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for episode in manifest["episodes"]:
                entry = zipfile.ZipInfo(episode["file"], date_time=(1980, 1, 1, 0, 0, 0))
                entry.compress_type = zipfile.ZIP_DEFLATED
                entry.create_system = 3
                entry.external_attr = 0o100644 << 16
                archive.writestr(entry, local_path(build, episode["file"]).read_bytes())
        with zipfile.ZipFile(archive_path) as archive:
            if archive.namelist() != names or archive.testzip() is not None:
                raise ValueError("ZIP content validation failed")
            for episode in manifest["episodes"]:
                if digest(archive.read(episode["file"])) != episode["sha256"]:
                    raise ValueError("Packaged subtitle bytes differ from reviewed build")
        publish_bytes(output, archive_path.read_bytes())
    finally:
        try:
            temporary.cleanup()
        except OSError:
            logging.warning("Package staging cleanup failed; preserve recovery directory: %s", temporary.name)
    logging.info("Packaged episodes=%d", len(names))
    return {"zip": str(output), "sha256": digest(output.read_bytes()), "episodes": len(names),
            "glossary": str(build / "glossary.json"), "backgrounds": sorted(backgrounds),
            "sync_verified": False}
