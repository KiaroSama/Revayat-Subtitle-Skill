"""Revayat Subtitle: prepare -> build -> render -> qa -> package; translation belongs to the agent."""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path
import subprocess
import sys
import zipfile

from runtime import operational_log


def main(argv=None) -> int:
    with operational_log("revayat-subtitle"):
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
        build = commands.add_parser("build", help="Validate every cue decision and write a clean episode edition")
        build.add_argument("--work", required=True, type=Path)
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
        args = parser.parse_args(argv)
        try:
            import render as render_module
            import workflow
            logging.info("Command=%s", args.command)
            if args.command == "doctor":
                result = render_module.doctor(args.ffmpeg)
            elif args.command == "prepare":
                result = workflow.prepare(args.inputs, args.work.resolve(), args.series, args.season,
                                          args.encoding, args.glossary, args.target_language)
            elif args.command == "build":
                result = workflow.build(args.work.resolve())
            elif args.command == "render":
                result = render_module.render(args.build.resolve(), args.episode, args.ffmpeg,
                                              args.video, args.fonts_dir, args.all_cues)
            elif args.command == "qa":
                result = render_module.qa(args.build.resolve())
            else:
                result = render_module.package(args.build.resolve(), args.out.resolve())
            code = 1 if result.get("ready") is False else 0
            print(json.dumps(result, ensure_ascii=False, indent=2))
            logging.info("Command completed exit=%d", code)
            return code
        except (OSError, ValueError, LookupError, TypeError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
            logging.error("Command failed error_type=%s exit=2", type(error).__name__)
            print(f"ERROR: {error}", file=sys.stderr)
            return 2


if __name__ == "__main__":
    sys.exit(main())
