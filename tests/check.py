"""Bounded stdlib checks of the actual CLI, subtitle data and portable installation."""

from __future__ import annotations

import argparse
import ast
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "skills" / "revayat-subtitle" / "scripts"
sys.path.insert(0, str(SCRIPTS))
from runtime import digest, operational_log, read_json, run, write_json
from subtitle_formats import PDF, RLE, RLM, parse, rtl, serialize, visible
from workflow import build, prepare, reviewed_cues

CLI = SCRIPTS / "revayat-subtitle.py"
FIXTURES = ROOT / "tests" / "fixtures"


def completed(work: Path):
    project = read_json(work / "project.json")
    project["episodes"] = [{"id": "S01E01", "base": "s0001", "alternates": ["s0002"],
                            "comparison": "ASS presentation; SRT supplies a missing sign and comparison wording."}]
    write_json(work / "project.json", project)
    write_json(work / "glossary.json", {"series": "Fixture Series", "terms_reviewed": True,
               "research": [{"url": "https://example.org/fixture",
                             "note": "Structural test reference only; this authored fixture is not a real anime."}],
               "terms": [{"source": "Rin", "target": "رین", "locked": True}]})
    sheet = read_json(work / "worksheets" / "s0001.json")
    translations = ["من دیروز OVA رو دیدم.", "رین-سان، برگرد خونه!", r"{\rResetOnly}اسمش رو گذاشته بود «امید».",
                    None, None, None, "لعنتی! اون مترجم بود.", r"خط اول.\Nخط دوم."]
    for index, row in enumerate(sheet):
        row["reviewed"] = True
        row["text"] = translations[index]
        if index == 3:
            row.update(action="credit", note="Explicit fansub promotion, separate from story dialogue.")
        elif index == 4:
            row["action"] = "empty"
        elif index == 5:
            row["action"] = "preserve"
    write_json(work / "worksheets" / "s0001.json", sheet)
    donor = read_json(work / "worksheets" / "s0002.json")
    for row, link in zip(donor[:2], ["s0001:c000002", "s0001:c000001"]):
        row.update(reviewed=True, action="alternate", links=[link], note="Content retained in the linked ASS cue.")
    donor[2].update(reviewed=True, text="<i>دروازهٔ شمالی</i>")
    write_json(work / "worksheets" / "s0002.json", donor)


class WorkspaceCase(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / ".scratch" / "checks"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="subtitle check فارسی ", dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.addCleanup(self.retain_failure_evidence)
        self.work = self.root / "work with spaces"
        self.old_log = os.environ.get("REVAYAT_LOG_DIR")
        os.environ["REVAYAT_LOG_DIR"] = str(self.root / "logs")
        self.addCleanup(self.restore_log)

    def restore_log(self):
        if self.old_log is None:
            os.environ.pop("REVAYAT_LOG_DIR", None)
        else:
            os.environ["REVAYAT_LOG_DIR"] = self.old_log

    def import_work(self):
        prepare([FIXTURES / "episode.ass", FIXTURES / "alternative.srt"], self.work,
                "Fixture Series", 1, "utf-8-sig", None, "fa")

    def retain_failure_evidence(self):
        result = getattr(self._outcome, "result", None)
        if result is None or not any(test is self for test, _ in result.failures + result.errors):
            return
        destination = ROOT / ".scratch/ci-artifacts" / self.id()
        remaining, count = 10 * 1024 * 1024, 0
        for source in self.root.rglob("*"):
            if source.is_symlink() or not source.is_file() or source.suffix not in {".png", ".log"}:
                continue
            size = source.stat().st_size
            if size > remaining or count >= 20:
                continue
            target = destination / source.relative_to(self.root)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            remaining -= size
            count += 1


class SubtitleChecks(WorkspaceCase):
    def test_multilingual_sources_survive_import(self):
        cases = read_json(ROOT / "evaluation" / "cases.json")
        for language in ("ja", "zh", "fr", "es"):
            with self.subTest(language=language):
                source_text = next(case["source"] for case in cases if case["source_language"] == language)
                source = self.root / f"{language}.srt"
                raw = f"1\n00:00:01,000 --> 00:00:03,000\n{source_text}\n".encode("utf-8")
                source.write_bytes(raw)
                work = self.root / language
                prepare([source], work, "Fixture Series", 1, "utf-8-sig", None, "fa")
                row = read_json(work / "worksheets" / "s0001.json")[0]
                self.assertEqual(row["source_text"], source_text)
                self.assertFalse(row["reviewed"])
                self.assertEqual(source.read_bytes(), raw)

    def test_cli_roundtrip_and_refusal(self):
        original = (FIXTURES / "episode.ass").read_bytes()
        result = json.loads(run([sys.executable, str(CLI), "prepare", str(FIXTURES / "episode.ass"),
                                str(FIXTURES / "alternative.srt"), "--work", str(self.work),
                                "--series", "Fixture Series", "--season", "1"], cwd=self.root, timeout=15))
        self.assertEqual(len(result["sources"]), 2)
        with self.assertRaisesRegex(ValueError, "glossary"):
            run([sys.executable, str(CLI), "build", "--work", str(self.work)], timeout=15)
        completed(self.work)
        result = json.loads(run([sys.executable, str(CLI), "build", "--work", str(self.work)], timeout=15))
        output = Path(result["build"]) / "Sub" / "S01E01.ass"
        raw = output.read_text(encoding="utf-8")
        doc = parse(raw, "ass")
        self.assertEqual(len(doc.cues), 7)
        self.assertEqual([cue.start for cue in doc.cues], sorted(cue.start for cue in doc.cues))
        self.assertEqual(set(doc.styles), {"Default", "ResetOnly"})
        self.assertNotIn("Translated by", raw)
        self.assertNotIn("hidden comment", raw)
        self.assertNotIn("Comment:", raw)
        self.assertIn("اون مترجم بود", raw)
        self.assertIn("لعنتی", raw)
        self.assertIn("m 0 0 l 100 0 100 100 0 100", raw)
        self.assertIn(RLM + "من دیروز OVA رو دیدم." + RLM, raw)
        self.assertIn(r"{\i1}" + RLM + "دروازهٔ شمالی" + RLM + r"{\i0}", raw)
        self.assertEqual((FIXTURES / "episode.ass").read_bytes(), original)
        logs = list((self.root / "logs").glob("*.log"))
        self.assertEqual(len(logs), 3)
        entries = "".join(path.read_text(encoding="utf-8") for path in logs)
        self.assertIn("exit=2", entries)
        self.assertNotIn("اون مترجم بود", entries)
        # A changed output cannot be silently reused or delivered.
        output.write_text(raw + "; changed\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "modified"):
            build(self.work)

    def test_all_rows_and_sources_are_accounted(self):
        self.import_work()
        completed(self.work)
        sheet_path = self.work / "worksheets" / "s0002.json"
        sheet = read_json(sheet_path)
        sheet[0]["reviewed"] = False
        write_json(sheet_path, sheet)
        with self.assertRaisesRegex(ValueError, "reviewed"):
            build(self.work)
        sheet[0]["reviewed"] = True
        removed = sheet.pop()
        write_json(sheet_path, sheet)
        with self.assertRaisesRegex(ValueError, "every original cue"):
            build(self.work)
        sheet.append(removed)
        write_json(sheet_path, sheet)
        completed(self.work)
        project = read_json(self.work / "project.json")
        project["episodes"][0]["alternates"] = []
        write_json(self.work / "project.json", project)
        with self.assertRaisesRegex(ValueError, "Assign each source"):
            build(self.work)

    def test_font_policy_and_style_resets(self):
        raw = (FIXTURES / "episode.ass").read_text(encoding="utf-8")
        font_data = "[Fonts]\nfontname: synthetic_0.ttf\n!!!!\n[AAAA]\n;OPAQUE\n"
        doc = parse(raw + font_data, "ass")
        self.assertIn(font_data.strip(), serialize(doc, doc.cues))
        self.assertNotIn("[Fonts]", serialize(doc, doc.cues, remove_fonts=True))
        self.assertNotIn("OPAQUE", serialize(doc, doc.cues, remove_fonts=True))
        self.assertIn("ResetOnly", serialize(doc, doc.cues))
        self.assertNotIn("Unused", serialize(doc, doc.cues))

    def test_inventory_and_manifest_cannot_drop_content(self):
        from render import load_build
        self.import_work()
        completed(self.work)
        result = build(self.work)
        output = Path(result["build"])
        manifest_path = output / "manifest.json"
        manifest = read_json(manifest_path)
        manifest["episodes"][0]["cues"] = 1
        write_json(manifest_path, manifest)
        with self.assertRaisesRegex(ValueError, "modified"):
            load_build(output)
        write_json(manifest_path, {key: value for key, value in result.items() if key != "build"})
        write_json(output / "glossary.json", {"series": "Wrong series"})
        with self.assertRaisesRegex(ValueError, "glossary differs"):
            load_build(output)
        project = read_json(self.work / "project.json")
        project["sources"].pop()
        project["episodes"][0]["alternates"] = []
        write_json(self.work / "project.json", project)
        with self.assertRaisesRegex(ValueError, "inventory differs"):
            build(self.work)

    def test_rtl_markup_drawings_and_idempotency(self):
        value = r"{\p1}m 0 0 l 20 20{\p0}{\i1}من دیروز OVA رو دیدم.{\i0}\N«رین-سان» برگشت."
        marked = rtl(value, "ass")
        self.assertEqual(rtl(marked, "ass"), marked)
        self.assertTrue(marked.startswith(r"{\p1}m 0 0 l 20 20{\p0}{\i1}" + RLE + RLM))
        self.assertEqual(marked.count(RLM), 4)
        self.assertEqual(visible(marked, "ass"), "من دیروز OVA رو دیدم.\n«رین-سان» برگشت.")

    def test_empty_drawing_timing_and_structure_guards(self):
        self.import_work()
        doc = parse((FIXTURES / "episode.ass").read_text(encoding="utf-8"), "ass")
        completed(self.work)
        rows = read_json(self.work / "worksheets" / "s0001.json")
        rows[5]["action"] = "empty"
        with self.assertRaisesRegex(ValueError, "nonempty"):
            reviewed_cues(doc, rows)
        rows[5]["action"] = "preserve"
        rows[0]["start_ms"] += 100
        with self.assertRaisesRegex(ValueError, "Timing changes"):
            reviewed_cues(doc, rows)
        rows[0]["start_ms"] -= 100
        rows[2]["text"] = "متن بدون استایل"
        with self.assertRaisesRegex(ValueError, "Changed tags"):
            reviewed_cues(doc, rows)

    def test_zip_safety_and_encoding_fail_before_writes(self):
        archive = self.root / "unsafe.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("../outside.srt", (FIXTURES / "alternative.srt").read_bytes())
        with self.assertRaisesRegex(ValueError, "Unsafe ZIP"):
            prepare([archive], self.work, "Fixture Series", 1, "utf-8", None, "fa")
        self.assertFalse(self.work.exists())
        bad = self.root / "bad.srt"
        bad.write_bytes(b"\xff\xff")
        with self.assertRaises(UnicodeDecodeError):
            prepare([bad], self.work, "Fixture Series", 1, "utf-8", None, "fa")
        self.assertFalse(self.work.exists())

    def test_srt_ova_and_season_glossary(self):
        source = self.root / "ova.srt"
        source.write_text("8\n00:00:02,000 --> 00:00:04,000\n<i>OVA برگشت.</i>\n\n"
                          "2\n00:00:00,000 --> 00:00:01,000\nسلام.\n", encoding="utf-8")
        glossary = self.root / "previous.json"
        terms = {"series": "Fixture Series", "research": [{"url": "https://example.org/fixture", "note": "Fixture only"}],
                 "terms": [{"source": "Rin", "target": "رین", "locked": True}], "terms_reviewed": True}
        write_json(glossary, terms)
        prepare([source], self.work, "Fixture Series", 2, "utf-8", glossary, "fa")
        self.assertEqual(read_json(self.work / "glossary.json"), terms)
        sheet = read_json(self.work / "worksheets" / "s0001.json")
        for row in sheet:
            row.update(reviewed=True, text=row["source_text"])
        write_json(self.work / "worksheets" / "s0001.json", sheet)
        project = read_json(self.work / "project.json")
        project["episodes"] = [{"id": "S02OVA02", "base": "s0001", "comparison": "Only candidate"}]
        write_json(self.work / "project.json", project)
        result = build(self.work)
        raw = (Path(result["build"]) / "Sub" / "S02OVA02.srt").read_text(encoding="utf-8")
        self.assertTrue(raw.startswith("1\n00:00:00,000"))
        self.assertIn("<i>" + RLE + RLM + "OVA برگشت." + RLM + PDF + "</i>", raw)

    def test_installers_and_manifest_bundle(self):
        project = self.root / "consumer"
        project.mkdir()
        command = [sys.executable, str(ROOT / "install" / "install.py"), "--agent", "all", "--scope", "project", "--path", str(project)]
        run(command, cwd=self.root, timeout=15)
        for folder in (".agents", ".claude", ".cursor", ".kiro", ".cline", ".hermes", ".opencode"):
            self.assertTrue((project / folder / "skills" / "revayat-subtitle" / "SKILL.md").is_file())
        installed = project / ".agents" / "skills" / "revayat-subtitle"
        self.assertFalse(list(installed.rglob("*.log")))
        self.assertFalse((project / "AGENTS.md").exists())
        with self.assertRaisesRegex(ValueError, "already exists"):
            run(command, timeout=15)
        plugin = self.root / "plugin copy"
        run([sys.executable, str(ROOT / "install" / "install.py"), "--plugin", "--destination", str(plugin)], timeout=15)
        self.assertEqual(read_json(plugin / "plugin.json")["name"], "revayat-subtitle")
        self.assertTrue((plugin / ".codex-plugin" / "plugin.json").exists())
        self.assertTrue((plugin / "commands" / "revayat-subtitle-resume.md").is_file())
        self.assertTrue((plugin / "commands" / "revayat-subtitle-qa.md").is_file())
        self.assertTrue((plugin / "skills" / "revayat-subtitle" / "requirements.txt").is_file())
        self.assertFalse((plugin / "tests").exists())
        self.assertFalse((plugin / ".git").exists())
        self.assertFalse(list(plugin.rglob("*.log")))
        self.assertEqual((plugin / "LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes())
        spec = importlib.util.spec_from_file_location("subtitle_installer", ROOT / "install" / "install.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        # Match the installer's constructor: macOS /var symlinks and Windows
        # short temp paths must be canonical before assigning source roots.
        module.REPO = plugin.resolve()
        module.SKILL = module.REPO / "skills" / "revayat-subtitle"
        with self.assertRaisesRegex(ValueError, "overlaps"):
            module.install(plugin / "skills", plugin=False, force=True)
        self.assertTrue((plugin / "skills" / "revayat-subtitle" / "SKILL.md").exists())
        # Exercise the OS launcher itself from an unrelated directory with spaces.
        target = self.root / "launcher copy"
        if os.name == "nt":
            launcher = [shutil.which("pwsh") or "powershell", "-NoProfile", "-File", str(ROOT / "install" / "install.ps1")]
        else:
            launcher = ["bash", str(ROOT / "install" / "install.sh")]
        run(launcher + ["--agent", "codex", "--destination", str(target)], cwd=self.root, timeout=20)
        self.assertEqual((target / "SKILL.md").read_bytes(), (installed / "SKILL.md").read_bytes())
        run([sys.executable, str(ROOT / "install" / "install.py"), "--destination", str(target), "--force"], timeout=15)
        self.assertEqual(len(list(self.root.glob("launcher copy.backup-*"))), 1)

    def test_language_evaluation_requires_review_for_unknown_wording(self):
        cases = read_json(ROOT / "evaluation" / "cases.json")
        answers = {case["id"]: case["accepted"][0] for case in cases}
        path = self.root / "answers.json"
        write_json(path, answers)
        command = [sys.executable, str(ROOT / "evaluation" / "score.py"), "--answers", str(path)]
        result = json.loads(run(command, cwd=self.root, timeout=15))
        self.assertEqual(result["counts"]["known_wording"], len(cases))
        self.assertFalse(result["linguistic_quality_certified"])
        answers[cases[0]["id"]] = "A new wording needing editorial review"
        write_json(path, answers)
        with self.assertRaisesRegex(ValueError, "needs_review"):
            run(command, cwd=self.root, timeout=15)

    def test_distribution_syntax_and_links(self):
        paths = [path for folder in ("skills", "install", "evaluation", "tests") for path in (ROOT / folder).rglob("*.py")]
        for path in paths:
            if any(part.startswith(".") or part in {"work", "out"} for part in path.relative_to(ROOT).parts):
                continue
            source = path.read_text(encoding="utf-8")
            ast.parse(source, filename=str(path))
            self.assertLessEqual(len(source.splitlines()), 800, str(path))
        for relative in ("plugin.json", ".codex-plugin/plugin.json", ".cursor-plugin/plugin.json", ".claude-plugin/plugin.json"):
            manifest = read_json(ROOT / relative)
            self.assertEqual(manifest["name"], "revayat-subtitle")
            self.assertEqual(manifest["version"], "1.2.0")
        documents = [ROOT / "README.md", ROOT / "README.fa.md", *ROOT.glob("docs/**/*.md"),
                     *ROOT.glob("skills/**/*.md")]
        for document in documents:
            for target in re.findall(r"\]\(([^)#]+)(?:#[^)]*)?\)", document.read_text(encoding="utf-8")):
                if "://" not in target:
                    self.assertTrue((document.parent / target).is_file(), f"Broken link in {document.name}: {target}")


class RenderCheck(WorkspaceCase):
    def test_ffmpeg_review_gate_and_zip_bytes(self):
        from render import package, render
        self.import_work()
        completed(self.work)
        sheet_path = self.work / "worksheets" / "s0001.json"
        sheet = read_json(sheet_path)
        sheet[1].update(start_ms=200, end_ms=800, timing_note="Fractional-time render regression fixture")
        write_json(sheet_path, sheet)
        result = build(self.work)
        output = Path(result["build"])
        evidence = render(output, "S01E01", None, None, None, True)
        review = read_json(Path(evidence["review"]))
        self.assertEqual(len(review["frames"]), 7)
        from render import ffmpeg_path
        first_png = output / review["frames"][0]["file"]
        pixels = run([ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-i", str(first_png),
                      "-vf", "crop=1280:120:0:600", "-frames:v", "1", "-pix_fmt", "gray",
                      "-f", "rawvideo", "pipe:1"], timeout=15)
        self.assertGreater(len(set(pixels)), 10, "Fractional sample must show caption pixels, not a blank frame")
        video = self.root / "ten-bit.mkv"
        run([ffmpeg_path(), "-hide_banner", "-loglevel", "error", "-nostdin", "-f", "lavfi",
             "-i", "color=c=navy:s=320x180:r=10:d=20", "-filter_threads", "1", "-c:v", "ffv1",
             "-pix_fmt", "yuv420p10le", "-threads", "1", str(video)], timeout=20)
        evidence = render(output, "S01E01", None, video, None, True)
        review = read_json(Path(evidence["review"]))
        for frame in review["frames"]:
            self.assertEqual((output / frame["file"]).read_bytes()[24], 8, "Previews must be viewable 8-bit PNGs")
        with self.assertRaisesRegex(ValueError, "Inspect every"):
            package(output, self.root / "Sub.zip")
        for frame in review["frames"]:
            frame["reviewed"] = True
            frame["note"] = "Automated gate-contract fixture; not a claim of human visual approval."
        write_json(Path(evidence["review"]), review)
        before = {p.relative_to(self.work).as_posix(): digest(p.read_bytes())
                  for p in self.work.rglob("*") if p.is_file()}
        checked = json.loads(run([sys.executable, str(CLI), "qa", "--build", str(output)], timeout=15))
        self.assertTrue(checked["ok"])
        self.assertEqual(checked["reviewed_frames"], 7)
        after = {p.relative_to(self.work).as_posix(): digest(p.read_bytes())
                 for p in self.work.rglob("*") if p.is_file()}
        self.assertEqual(before, after, "QA must not change translation or approval files")
        delivered = package(output, self.root / "Sub.zip")
        with zipfile.ZipFile(delivered["zip"]) as archive:
            self.assertEqual(archive.namelist(), ["Sub/S01E01.ass"])
            self.assertEqual(digest(archive.read("Sub/S01E01.ass")), result["episodes"][0]["sha256"])
        png = output / review["frames"][0]["file"]
        png.write_bytes(png.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "image changed"):
            package(output, self.root / "stale.zip")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if args.worker:
        from importlib.metadata import version
        print(f"Runtime: Python {sys.version.split()[0]} on {sys.platform}; "
              f"psutil {version('psutil')}; Hypothesis {version('hypothesis')}", flush=True)
        if args.render:
            from render import ffmpeg_path
            print(run([ffmpeg_path(), "-version"], timeout=15).decode("utf-8").splitlines()[0], flush=True)
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(SubtitleChecks)
        suite.addTests(unittest.defaultTestLoader.discover(str(ROOT / "tests"), pattern="test_*.py"))
        if args.render:
            suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(RenderCheck))
            suite.addTests(unittest.defaultTestLoader.discover(str(ROOT / "tests"), pattern="render_checks.py"))
        return 0 if unittest.TextTestRunner(stream=sys.stdout, verbosity=2).run(suite).wasSuccessful() else 1
    with operational_log("check"):
        command = [sys.executable, "-X", "utf8", str(Path(__file__).resolve()), "--worker"]
        if args.render:
            command.append("--render")
        try:
            print(run(command, timeout=180 if args.render else 90, idle_timeout=30).decode("utf-8"), end="")
            return 0
        except (ValueError, subprocess.SubprocessError) as error:
            # Only this owned authored-test worker's diagnostics are safe to display.
            for field in ("stdout", "stderr"):
                captured = getattr(error, field, None)
                if isinstance(captured, bytes):
                    print(captured.decode("utf-8", errors="replace"), file=sys.stderr)
                    destination = ROOT / ".scratch/ci-artifacts"
                    destination.mkdir(parents=True, exist_ok=True)
                    (destination / ("worker-" + field + ".txt")).write_text(captured.decode("utf-8", errors="replace"), encoding="utf-8")
            print(str(error), file=sys.stderr)
            return 1


if __name__ == "__main__":
    sys.exit(main())
