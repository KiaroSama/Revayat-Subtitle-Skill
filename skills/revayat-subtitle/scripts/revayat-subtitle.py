"""Revayat Subtitle: prepare -> build -> render -> qa -> package; translation belongs to the agent."""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path
import subprocess
import sys
import zipfile

from runtime import operational_log, LogConfigurationError, read_limited


def _convert(args) -> dict:
    from publication import publish_bytes
    from subtitle_formats import (ass_document_to_ass, build_ass_style, build_translator_ass,
                                  parse, serialize, srt_document_to_ass)
    from workflow import MAX_FILE

    def load(path):
        if path.is_symlink() or path.suffix.lower() not in {".srt", ".ass"}:
            raise ValueError("Conversion requires a regular .srt or .ass input file")
        return parse(read_limited(path, MAX_FILE).decode("utf-8-sig"), path.suffix[1:].lower())

    if args.out.suffix.lower() != ".ass":
        raise ValueError("Conversion output must use the .ass extension")
    options = {"style_line": build_ass_style(fontname=args.font, outline=args.outline, shadow=args.shadow),
               "max_chars": args.max_chars, "strip_watermarks": not args.keep_watermarks,
               "fix_zwnj": not args.no_zwnj, "direction": args.direction}
    primary = load(args.input)
    if args.translator_mode:
        donors = [(path.stem, load(path)) for path in args.donor]
        doc, stats = build_translator_ass(primary, donors, include_source_comments=args.source_comments, **options)
    else:
        if args.donor or args.source_comments:
            raise ValueError("Donors and source comments require translator mode")
        convert = srt_document_to_ass if primary.kind == "srt" else ass_document_to_ass
        doc, removed = convert(primary, **options)
        stats = {"watermarks_removed": len(removed)}
    payload = serialize(doc, doc.cues)
    if len(parse(payload, "ass").cues) != len(doc.cues):
        raise ValueError("Output cue count changed during serialization")
    publish_bytes(args.out, payload.encode("utf-8"))
    return {"output": str(args.out), "cues": len(doc.cues), **stats}


def execute(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    doctor = commands.add_parser("doctor", help="Check FFmpeg with libass")
    doctor.add_argument("--ffmpeg")
    prepare = commands.add_parser("prepare", help="Import ASS/SRT/ZIP and create complete review worksheets")
    prepare.add_argument("inputs", nargs="+", type=Path)
    prepare.add_argument("--work", required=True, type=Path)
    prepare.add_argument("--series", required=True)
    prepare.add_argument("--season", type=int, required=True)
    prepare.add_argument("--encoding", default="utf-8-sig", help="Explicit input encoding; defaults to strict UTF-8")
    prepare.add_argument("--glossary", type=Path, help="Previous season's glossary for the same series")
    prepare.add_argument("--target-language", default="fa")
    prepare.add_argument("--output-format", choices=("ass", "srt"), default="ass")
    build = commands.add_parser("build", help="Validate every cue decision and write a clean episode edition")
    build.add_argument("--work", required=True, type=Path)
    build.add_argument("--output-format", choices=("ass", "srt"), help="Override the prepared output format")
    from subtitle_formats import DEFAULT_FONT_NAME, DEFAULT_OUTLINE, DEFAULT_SHADOW
    for name, help_text in (("convert", "Convert SRT or ASS to Persian-ready ASS"),
                            ("translator-ass", "Create an ASS worksheet with missing donor slots")):
        convert = commands.add_parser(name, help=help_text)
        convert.set_defaults(translator_mode=name == "translator-ass")
        convert.add_argument("input", type=Path)
        convert.add_argument("--out", required=True, type=Path)
        convert.add_argument("--font", default=DEFAULT_FONT_NAME)
        convert.add_argument("--outline", type=float, default=DEFAULT_OUTLINE)
        convert.add_argument("--shadow", type=float, default=DEFAULT_SHADOW)
        convert.add_argument("--max-chars", type=int, default=42)
        convert.add_argument("--keep-watermarks", action="store_true")
        convert.add_argument("--no-zwnj", action="store_true")
        convert.add_argument("--direction", choices=("rtl", "ltr"), default="rtl")
        if name == "convert":
            convert.add_argument("--translator-mode", action="store_true")
        convert.add_argument("--donor", type=Path, action="append", default=[])
        convert.add_argument("--source-comments", action="store_true")
    commands.add_parser("sync", help="Synchronize subtitles against audio", add_help=False)
    render = commands.add_parser("render", help="Render required visual samples with FFmpeg/libass")
    render.add_argument("--build", required=True, type=Path)
    render.add_argument("--episode", required=True)
    render.add_argument("--ffmpeg")
    render.add_argument("--video", type=Path)
    render.add_argument("--fonts-dir", type=Path)
    render.add_argument("--all-cues", action="store_true")
    qa = commands.add_parser("qa", help="Read-only delivery checks for a reviewed subtitle build")
    qa.add_argument("--build", required=True, type=Path)
    package = commands.add_parser("package", help="Deliver only reviewed episode subtitles inside Sub/ in a ZIP")
    package.add_argument("--build", required=True, type=Path)
    package.add_argument("--out", required=True, type=Path)
    status = commands.add_parser("status", help="Read-only pending/current workspace overview")
    status.add_argument("--work", required=True, type=Path)
    handoff = commands.add_parser("handoff", help="Read-only revision-bound editorial proposal validation")
    handoff.add_argument("--work", required=True, type=Path)
    handoff.add_argument("--assignment", required=True, action="append", type=Path)
    handoff.add_argument("--result", required=True, action="append", type=Path)
    argv = sys.argv[1:] if argv is None else argv
    try:
        if sys.version_info < (3, 11):
            raise ValueError("Python 3.11+ is required; use an upstream-maintained interpreter")
        if argv[:1] == ["sync"]:
            from sync_bridge import main as sync_main
            logging.info("Command=sync")
            code = sync_main(argv[1:])
            logging.log(logging.INFO if code == 0 else logging.ERROR, "Command completed exit=%d", code)
            return code
        args = parser.parse_args(argv)
        import render as render_module
        import workflow
        logging.info("Command=%s", args.command)
        if args.command == "doctor":
            result = render_module.doctor(args.ffmpeg)
        elif args.command == "prepare":
            result = workflow.prepare(args.inputs, args.work.resolve(), args.series, args.season,
                                      args.encoding, args.glossary, args.target_language, output_format=args.output_format)
        elif args.command == "build":
            result = workflow.build(args.work.resolve(), output_format=args.output_format)
        elif args.command in {"convert", "translator-ass"}:
            result = _convert(args)
        elif args.command == "render":
            result = render_module.render(args.build.resolve(), args.episode, args.ffmpeg,
                                          args.video, args.fonts_dir, args.all_cues)
        elif args.command == "qa":
            result = render_module.qa(args.build.resolve())
        elif args.command == "status":
            from status import status
            result = status(args.work.resolve())
        elif args.command == "handoff":
            from handoff import handoff
            result = handoff(args.work.resolve(), args.assignment, args.result)
        else:
            # Preserve an occupied leaf name for the exclusive publication check.
            result = render_module.package(args.build.resolve(), args.out.absolute())
        code = 1 if result.get("ready") is False else 0
        print(json.dumps(result, ensure_ascii=False, indent=2))
        logging.log(logging.INFO if code == 0 else logging.ERROR, "Command completed exit=%d", code)
        return code
    except (OSError, ValueError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        logging.error("Command failed error_type=%s exit=2", type(error).__name__)
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


def main(argv=None) -> int:
    try:
        with operational_log("revayat-subtitle"):
            return execute(argv)
    except LogConfigurationError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
