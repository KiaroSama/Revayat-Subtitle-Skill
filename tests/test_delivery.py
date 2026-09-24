"""Edition upgrades and delivery evidence preserve reviewed source work."""

from pathlib import Path
import shutil
import os
import struct
import unittest
from unittest.mock import patch
import zlib

from check import FIXTURES, WorkspaceCase, completed
from runtime import digest, read_json, write_json, run
from render import package, requested_fonts, sample_plan, sample_times, write_render_evidence
from subtitle_formats import Cue, parse
from workflow import build
import workflow
from publication import publish_bytes
from render import qa, SAMPLER_VERSION
from png_validation import decode_png


class DeliveryTests(WorkspaceCase):
    def test_middle_srt_font_is_sampled_and_identified(self):
        from subtitle_formats import Document
        doc = Document("srt", [Cue("a", 1000, 2000, "First"),
                               Cue("b", 3000, 4000, '<font face="Example Sans">Middle</font>'),
                               Cue("c", 5000, 6000, "Last")])
        self.assertIn(3.5, sample_times(doc, False))
        self.assertEqual(requested_fonts(doc), ["Example Sans"])

    @unittest.skipUnless(os.name == "nt", "Windows inherited publication permissions")
    def test_published_file_inherits_destination_permissions(self):
        output = self.root / "published.bin"
        publish_bytes(output, b"complete artifact")
        script = self.root / "permissions.ps1"
        script.write_text("param([string]$Path)\n$ErrorActionPreference='Stop'\n"
                          "if ((Get-Acl -LiteralPath $Path).AreAccessRulesProtected) { throw 'Publication has a private DACL' }\n"
                          "[IO.File]::ReadAllText($Path,[Text.Encoding]::UTF8)\n", encoding="utf-8")
        result = run([shutil.which("pwsh") or "powershell.exe", "-NoProfile", "-File", str(script), str(output)], timeout=10)
        self.assertIn(b"complete artifact", result)

    @unittest.skipIf(__import__("os").name == "nt", "POSIX publication retains a staging hardlink")
    def test_postcommit_cleanup_failure_does_not_report_failed_delivery(self):
        destination = self.root / "complete.bin"
        real = Path.unlink
        def fail_stage(path, *args, **kwargs):
            if path.name.startswith(".publish-"):
                raise PermissionError("Injected staging cleanup")
            return real(path, *args, **kwargs)
        with patch.object(Path, "unlink", fail_stage), self.assertLogs(level="WARNING"):
            publish_bytes(destination, b"complete artifact")
        self.assertEqual(destination.read_bytes(), b"complete artifact")

    def test_recipe_change_creates_distinct_identity_without_changing_inputs(self):
        self.import_work()
        completed(self.work)
        first = build(self.work)
        previous = (Path(first["build"]) / "manifest.json").read_bytes()
        with patch.dict(workflow.GENERATION_RECIPE, {"version": workflow.GENERATION_RECIPE["version"] + 1}):
            second = build(self.work)
        self.assertNotEqual(first["identity"], second["identity"])
        self.assertEqual(first["input_identity"], second["input_identity"])
        self.assertEqual((Path(first["build"]) / "manifest.json").read_bytes(), previous)
        self.assertFalse((Path(second["build"]) / "renders").exists())

    def test_distinct_identical_frames_are_valid_and_archives_are_repeatable(self):
        edition = self.reviewed_build()
        first, second = self.root / "first.zip", self.root / "second.zip"
        package(edition, first)
        package(edition, second)
        self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_mutable_coverage_flag_does_not_override_receipt(self):
        edition = self.reviewed_build()
        path = edition / "renders/S01E01.json"
        review = read_json(path)
        review["all_cues"] = False
        write_json(path, review)
        with self.assertRaisesRegex(ValueError, "association changed"):
            qa(edition)

    def test_hardlinked_frames_are_rejected(self):
        import os
        edition = self.reviewed_build()
        review = read_json(edition / "renders/S01E01.json")
        first, second = [edition / item["file"] for item in review["frames"][:2]]
        second.unlink()
        os.link(first, second)
        with self.assertRaisesRegex(ValueError, "aliased"):
            qa(edition)

    def test_review_boolean_and_frame_association_are_strict(self):
        edition = self.reviewed_build()
        path = edition / "renders/S01E01.json"
        original = read_json(path)
        import copy
        for field, value in (("reviewed", 1), ("time_seconds", True), ("cue_indices", [True]), ("width", False)):
            changed = copy.deepcopy(original)
            changed["frames"][0][field] = value
            write_json(path, changed)
            with self.assertRaisesRegex(ValueError, field):
                qa(edition)
        write_json(path, original)
        original["frames"][1]["file"] = original["frames"][0]["file"]
        write_json(path, original)
        with self.assertRaisesRegex(ValueError, "metadata"):
            qa(edition)

    def test_png_truncation_crc_and_dimension_budgets(self):
        def chunk(kind, body):
            return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
        valid = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
                 + chunk(b"IDAT", zlib.compress(b"\0\0\0\0")) + chunk(b"IEND", b""))
        self.assertEqual(decode_png(valid), (1, 1))
        corrupt = bytearray(valid)
        corrupt[29] ^= 1
        huge = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 100000, 100000, 8, 2, 0, 0, 0))
        for data in (valid[:8], valid[:-3], bytes(corrupt), huge):
            with self.assertRaisesRegex(ValueError, "PNG"):
                decode_png(data)

    def test_publication_flush_and_close_failures_are_retryable(self):
        real_open = Path.open
        for phase in ("flush", "close"):
            destination = self.root / (phase + ".bin")
            class BrokenStream:
                def __init__(self, stream):
                    self.stream = stream
                def __getattr__(self, name):
                    return getattr(self.stream, name)
                def __enter__(self):
                    return self
                def flush(self):
                    if phase == "flush":
                        raise OSError("Injected flush")
                    return self.stream.flush()
                def __exit__(self, *args):
                    self.stream.close()
                    if phase == "close":
                        raise OSError("Injected close")
            def open_fault(path, mode="r", *args, **kwargs):
                stream = real_open(path, mode, *args, **kwargs)
                return BrokenStream(stream) if mode == "xb" else stream
            with patch.object(Path, "open", open_fault):
                with self.assertRaisesRegex(OSError, "Injected"):
                    publish_bytes(destination, b"complete artifact")
            self.assertFalse(destination.exists())
            publish_bytes(destination, b"complete artifact")
            self.assertEqual(destination.read_bytes(), b"complete artifact")

    def test_publication_preserves_a_competing_destination(self):
        import os
        destination = self.root / "winner.bin"
        if os.name == "nt":
            real = Path.rename
            def race(source, target):
                Path(target).write_bytes(b"winner")
                return real(source, target)
            seam = patch.object(Path, "rename", race)
        else:
            real = os.link
            def race(source, target, **kwargs):
                Path(target).write_bytes(b"winner")
                return real(source, target, **kwargs)
            seam = patch("publication.os.link", race)
        with seam:
            with self.assertRaises(FileExistsError):
                publish_bytes(destination, b"loser")
        self.assertEqual(destination.read_bytes(), b"winner")

    def test_default_sampling_covers_middle_style_and_font_overrides(self):
        doc = parse((FIXTURES / "episode.ass").read_text(encoding="utf-8"), "ass")
        for override in (r"\rResetOnly", r"\fnTahoma", r"\fsp0", r"\fe-1"):
            with self.subTest(override=override):
                doc.cues = [Cue("a", 1000, 2000, "یک", {"style": "Default"}),
                            Cue("b", 3000, 4000, "{" + override + "}دو", {"style": "Default"}),
                            Cue("c", 5000, 6000, "سه", {"style": "Default"})]
                self.assertIn(3.5, sample_times(doc, False))

    def test_hashed_nonimage_cannot_satisfy_visual_evidence(self):
        edition = self.reviewed_build()
        review_path = edition / "renders/S01E01.json"
        review = read_json(review_path)
        frame = review["frames"][0]
        raw = b"This is ordinary text, not a rendered image."
        (edition / frame["file"]).write_bytes(raw)
        frame["sha256"] = digest(raw)
        receipt_path = edition / review["receipt_file"]
        receipt = read_json(receipt_path)
        receipt["frames"][0]["sha256"] = digest(raw)
        write_json(receipt_path, receipt)
        review["receipt_sha256"] = digest(receipt_path.read_bytes())
        write_json(review_path, review)
        with self.assertRaisesRegex(ValueError, "PNG"):
            package(edition, self.root / "invalid.zip")
        self.assertFalse((self.root / "invalid.zip").exists())

    def reviewed_build(self):
        self.import_work()
        completed(self.work)
        result = build(self.work)
        edition = Path(result["build"])
        episode = result["episodes"][0]
        doc = parse((edition / episode["file"]).read_text(encoding="utf-8"), "ass")
        def chunk(kind, body):
            return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
        png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
               + chunk(b"IDAT", zlib.compress(b"\0\0\0\0")) + chunk(b"IEND", b""))
        folder = edition / "renders" / "S01E01-fixture"
        folder.mkdir(parents=True)
        frames = []
        for index, sample in enumerate(sample_plan(doc, True), 1):
            image = folder / f"frame-{index:04}.png"
            image.write_bytes(png)
            frames.append({"file": image.relative_to(edition).as_posix(), **sample,
                           "sha256": digest(png), "width": 1, "height": 1})
        recipe = {"sampler": SAMPLER_VERSION, "profile": "rgb24-v1", "fonts": [], "requested_fonts": requested_fonts(doc), "video": None,
                  "renderer": {"name": "authored-fixture", "version": "synthetic; not executed",
                               "bytes": 0, "sha256": digest(b"")}}
        record = write_render_evidence(edition, result, episode, doc, frames, True, recipe)
        review_path = Path(record["review"])
        review = read_json(review_path)
        for frame in review["frames"]:
            frame.update(reviewed=True, note="Synthetic mechanical gate fixture; no visual approval claimed.")
        write_json(review_path, review)
        return edition

    def test_failed_publication_leaves_no_partial_output_and_retry_works(self):
        edition = self.reviewed_build()
        output = self.root / "Sub.zip"
        real_open = Path.open
        class BrokenWriter:
            def __init__(self, stream):
                self.stream = stream
            def __enter__(self):
                return self
            def __getattr__(self, name):
                return getattr(self.stream, name)
            def write(self, data):
                self.stream.write(data[:13])
                raise OSError("Injected publication write failure")
            def __exit__(self, *args):
                self.stream.close()
        def open_with_failure(path, mode="r", *args, **kwargs):
            stream = real_open(path, mode, *args, **kwargs)
            return BrokenWriter(stream) if mode == "xb" and path.parent == output.parent else stream
        with patch.object(Path, "open", open_with_failure):
            with self.assertRaisesRegex(OSError, "Injected"):
                package(edition, output)
        self.assertFalse(output.exists(), "Failure must not publish a partial final filename")
        package(edition, output)
        self.assertTrue(output.is_file())

    def test_legacy_workspace_gets_new_edition_without_rewriting_history(self):
        shutil.copytree(FIXTURES / "legacy-work", self.work)
        before = {p.relative_to(self.work): p.read_bytes() for p in self.work.rglob("*") if p.is_file()}
        legacy = next((self.work / "builds").iterdir())
        result = build(self.work)
        self.assertEqual(result["version"], 2)
        self.assertNotEqual(Path(result["build"]), legacy)
        self.assertEqual({p: (self.work / p).read_bytes() for p in before}, before)
        self.assertEqual(build(self.work)["identity"], result["identity"])
        self.assertFalse((Path(result["build"]) / "renders").exists())


if __name__ == "__main__":
    unittest.main()
