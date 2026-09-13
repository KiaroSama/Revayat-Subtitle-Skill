"""Source inventory, complete cue review, episode merging and immutable builds."""

from __future__ import annotations

import copy
import json
import logging
from pathlib import Path, PurePosixPath
import re
import tempfile
from urllib.parse import urlparse
import zipfile

from runtime import digest, local_path, read_json, write_json
from subtitle_formats import (Cue, DEFAULT_STYLE, STYLE_FIELDS, has_drawing, parse,
                              rtl, serialize, srt_to_ass, structure, uncomment, visible)

MAX_FILE = 16 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
EPISODE = re.compile(r"S\d{2}(?:E|OVA)\d{2,3}")
SOURCE = re.compile(r"s\d{4}")


def inputs(paths: list[Path]):
    total, count = 0, 0
    for path in paths:
        if path.is_symlink():
            raise ValueError("Symbolic-link subtitle inputs are not accepted")
        candidates = sorted(path.rglob("*")) if path.is_dir() else [path]
        for candidate in candidates:
            if candidate.is_symlink():
                continue
            suffix = candidate.suffix.lower()
            if suffix == ".zip":
                with zipfile.ZipFile(candidate) as archive:
                    if len(archive.infolist()) > 10000:
                        raise ValueError("Too many ZIP members")
                    for entry in archive.infolist():
                        name = PurePosixPath(entry.filename.replace("\\", "/"))
                        if name.is_absolute() or ".." in name.parts or any(":" in part for part in name.parts):
                            raise ValueError("Unsafe ZIP member path")
                        if name.suffix.lower() not in {".ass", ".srt"} or entry.is_dir():
                            continue
                        if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                            raise ValueError("ZIP symbolic links are not accepted")
                        if entry.flag_bits & 1:
                            raise ValueError("Encrypted ZIP: ask for the password and extract separately")
                        total += entry.file_size
                        count += 1
                        if entry.file_size > MAX_FILE or total > MAX_TOTAL or count > 1000:
                            raise ValueError("Subtitle archive exceeds bounded import limits")
                        yield f"{candidate.name}/{name.as_posix()}", name.suffix[1:].lower(), archive.read(entry)
            elif suffix in {".ass", ".srt"} and candidate.is_file():
                size = candidate.stat().st_size
                total += size
                count += 1
                if size > MAX_FILE or total > MAX_TOTAL or count > 1000:
                    raise ValueError("Subtitle input exceeds bounded import limits")
                yield candidate.name, suffix[1:], candidate.read_bytes()
            elif not path.is_dir():
                raise ValueError("Expected ASS, SRT, ZIP, or a directory containing subtitles")


def prepare(paths: list[Path], work: Path, series: str, season: int, encoding: str,
            glossary: Path | None, target_language: str) -> dict:
    if not series.strip() or not 1 <= season <= 99:
        raise ValueError("A series title and season from 1 to 99 are required")
    if work.exists():
        raise ValueError("Work directory already exists; resume it or choose a new directory")
    if glossary:
        terms = read_json(glossary)
        if terms.get("series") != series:
            raise ValueError("The previous glossary belongs to a different series identity")
    else:
        terms = {"series": series, "research": [], "terms": [], "terms_reviewed": False}
    # Validate every input before publishing a workspace; originals are never rewritten.
    imported = [(name, kind, raw, parse(raw.decode(encoding), kind)) for name, kind, raw in inputs(paths)]
    if not imported:
        raise ValueError("No ASS/SRT subtitles found")
    work.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".subtitle-import-", dir=work.parent) as temporary:
        stage = Path(temporary) / "work"
        (stage / "sources").mkdir(parents=True)
        (stage / "worksheets").mkdir()
        project = {"version": 1, "series": series, "season": season, "target_language": target_language,
                   "font_policy": "keep", "sources": [], "episodes": []}
        for i, (name, kind, raw, doc) in enumerate(imported, 1):
            source_id = f"s{i:04}"
            relative = f"sources/{source_id}.{kind}"
            (stage / relative).write_bytes(raw)
            project["sources"].append({"id": source_id, "name": name, "file": relative, "kind": kind,
                                       "sha256": digest(raw), "encoding": encoding, "cues": len(doc.cues),
                                       "comments_removed": doc.comments})
            write_json(stage / "worksheets" / f"{source_id}.json", [
                {"id": cue.id, "source_text": cue.text, "start_ms": cue.start, "end_ms": cue.end,
                 "action": "edit", "text": None, "reviewed": False, "links": [], "note": "",
                 "timing_note": "", "structure_note": ""} for cue in doc.cues])
        write_json(stage / "project.json", project)
        write_json(stage / "glossary.json", terms)
        stage.rename(work)
    logging.info("Imported sources=%d cues=%d", len(imported), sum(len(doc.cues) for *_, doc in imported))
    return {"work": str(work), "sources": project["sources"], "next": "Read and fill every worksheet; map episodes in project.json"}


def load(work: Path):
    project = read_json(work / "project.json")
    if project.get("version") != 1 or not isinstance(project.get("sources"), list):
        raise ValueError("Unsupported project schema")
    docs, sheets = {}, {}
    for source in project["sources"]:
        key = source["id"]
        if not SOURCE.fullmatch(key) or key in docs:
            raise ValueError("Invalid or duplicate source ID")
        raw = local_path(work, source["file"]).read_bytes()
        if digest(raw) != source["sha256"]:
            raise ValueError("Source changed since import; create a fresh workspace")
        docs[key] = parse(raw.decode(source["encoding"]), source["kind"])
        sheets[key] = read_json(work / "worksheets" / f"{key}.json")
    return project, docs, sheets


def verify_glossary(terms: dict, series: str) -> None:
    if terms.get("series") != series or terms.get("terms_reviewed") is not True:
        raise ValueError("Review the series glossary before building")
    research = terms.get("research", [])
    if not research or any(urlparse(item.get("url", "")).scheme not in {"http", "https"}
                           or not item.get("note", "").strip() for item in research):
        raise ValueError("Record actual anime research URLs and what they establish")
    seen = set()
    for term in terms.get("terms", []):
        if not term.get("source") or not term.get("target") or term.get("locked") is not True:
            raise ValueError("Each glossary term needs source, target, and locked=true")
        if term["source"] in seen:
            raise ValueError("Duplicate glossary source term")
        seen.add(term["source"])


def reviewed_cues(doc, sheet: list) -> tuple[list[Cue], list[dict]]:
    if not isinstance(sheet, list) or [row.get("id") for row in sheet] != [cue.id for cue in doc.cues]:
        raise ValueError("Worksheet must contain every original cue ID exactly once, in original order")
    kept, decisions = [], []
    for cue, row in zip(doc.cues, sheet):
        if row.get("source_text") != cue.text or row.get("reviewed") is not True:
            raise ValueError(f"Cue {cue.id} has changed source text or has not been reviewed")
        action = row.get("action")
        if action not in {"edit", "preserve", "credit", "empty", "alternate"}:
            raise ValueError(f"Cue {cue.id} needs a valid review action")
        if action in {"credit", "alternate"} and not row.get("note", "").strip():
            raise ValueError(f"Cue {cue.id} needs a reason for exclusion")
        if action == "empty" and (visible(cue.text, doc.kind) or has_drawing(cue.text, doc.kind)):
            raise ValueError("A nonempty cue cannot be discarded as empty")
        if action == "alternate" and not row.get("links"):
            raise ValueError("An alternate cue must link to the retained cue covering its content")
        decisions.append({"id": cue.id, "action": action, "links": row.get("links", []), "note": row.get("note", "")})
        if action in {"credit", "empty", "alternate"}:
            continue
        text = cue.text if action == "preserve" else row.get("text")
        if not isinstance(text, str) or (not visible(text, doc.kind) and not has_drawing(text, doc.kind)):
            raise ValueError(f"Cue {cue.id} requires nonempty reviewed text")
        if structure(text, doc.kind) != structure(cue.text, doc.kind) and not row.get("structure_note", "").strip():
            raise ValueError("Changed tags/drawings require a reason and visual review")
        start, end = row.get("start_ms"), row.get("end_ms")
        if type(start) is not int or type(end) is not int or not 0 <= start < end:
            raise ValueError("Cue timing must be nonnegative integer milliseconds with end after start")
        if (start, end) != (cue.start, cue.end) and not row.get("timing_note", "").strip():
            raise ValueError("Timing changes require an alignment reason")
        text = uncomment(text, doc.kind)
        if doc.kind == "srt":
            text = "\n".join(line for line in text.splitlines() if line.strip())
        kept.append(Cue(cue.id, start, end, rtl(text, doc.kind), dict(cue.fields)))
    return kept, decisions


def merge_donor(base, donor, cues: list[Cue], source_id: str, keep_fonts: bool) -> list[Cue]:
    if not cues:
        return []
    if base.kind == "srt":
        if donor.kind != "srt":
            raise ValueError("An ASS candidate must be the base to preserve its presentation")
        return cues
    if donor.kind == "srt":
        if "Default" not in base.styles:
            values = dict(zip([f.strip().lower() for f in STYLE_FIELDS.split(",")], DEFAULT_STYLE.split(",")))
            base.styles["Default"] = [values.get(key, "") for key in base.style_fields]
        return [srt_to_ass(cue) for cue in cues]
    if base.style_fields != donor.style_fields or base.event_fields != donor.event_fields:
        raise ValueError("ASS candidate formats differ; adapt them explicitly before merging")
    def script_info(doc):
        return {line.partition(":")[0].strip().casefold(): line.partition(":")[2].strip()
                for name, lines in doc.sections if name.casefold() == "[script info]" for line in lines if ":" in line}
    a, b = script_info(base), script_info(donor)
    if any(a.get(key) != b.get(key) for key in ("playresx", "playresy", "wrapstyle", "scaledborderandshadow")):
        raise ValueError("ASS candidates use different canvas/wrapping settings; align before merging")
    mapping = {}
    for name, values in donor.styles.items():
        mapped = f"{source_id}__{name}"
        if mapped in base.styles:
            raise ValueError("Donor style namespace collision")
        mapping[name] = mapped
        values = values.copy()
        values[base.style_fields.index("name")] = mapped
        base.styles[mapped] = values
    if keep_fonts:
        donor_fonts = [lines for name, lines in donor.sections if name.casefold() == "[fonts]"]
        base_fonts = [lines for name, lines in base.sections if name.casefold() == "[fonts]"]
        if donor_fonts and base_fonts and donor_fonts != base_fonts:
            raise ValueError("Different embedded font sets: reconcile fonts before merging ASS donor events")
        if donor_fonts and not base_fonts:
            base.sections.append(("[Fonts]", donor_fonts[0].copy()))
    for cue in cues:
        old = cue.fields["style"].strip()
        if old not in mapping:
            raise ValueError("Donor references an undefined style")
        cue.fields["style"] = mapping[old]
        cue.text = re.sub(r"\\r([^\\}]+)", lambda m: "\\r" + mapping.get(m[1].strip(), m[1]), cue.text)
    return cues


def build(work: Path) -> dict:
    project, docs, sheets = load(work)
    glossary = read_json(work / "glossary.json")
    verify_glossary(glossary, project["series"])
    if project.get("font_policy") not in {"keep", "remove"}:
        raise ValueError("font_policy must be keep or remove")
    if not project.get("episodes"):
        raise ValueError("Map every source to an episode in project.json")
    reviewed = {key: reviewed_cues(doc, sheets[key]) for key, doc in docs.items()}
    assigned, names, files, report = [], set(), {}, []
    for episode in project["episodes"]:
        name, base_id = episode["id"], episode["base"]
        if not EPISODE.fullmatch(name) or name in names or not name.startswith(f"S{project['season']:02}"):
            raise ValueError("Episode names must be unique SxxEyy or SxxOVAyy for this season")
        if int(re.search(r"(?:E|OVA)(\d+)$", name)[1]) < 1:
            raise ValueError("Episode and OVA numbers start at 1")
        names.add(name)
        sources = [base_id] + episode.get("alternates", [])
        if any(key not in docs for key in sources) or not episode.get("comparison", "").strip():
            raise ValueError("Every episode needs valid sources and a comparison/selection reason")
        if any(docs[key].kind == "ass" for key in sources) and docs[base_id].kind != "ass":
            raise ValueError("Choose the ASS candidate as the base; content can come from other candidates")
        assigned.extend(sources)
        base = copy.deepcopy(docs[base_id])
        retained = {f"{key}:{cue.id}" for key in sources for cue in reviewed[key][0]}
        merged, provenance = [], []
        for key in sources:
            cues, decisions = copy.deepcopy(reviewed[key])
            for decision in decisions:
                if any(link not in retained for link in decision["links"]):
                    raise ValueError("Review links must identify retained cues in this episode")
            original_ids = [cue.id for cue in cues]
            if key != base_id:
                cues = merge_donor(base, docs[key], cues, key, project["font_policy"] == "keep")
            merged.extend(cues)
            provenance.extend({"source": key, "cue": cue_id} for cue_id in original_ids)
        payload = serialize(base, merged, remove_fonts=project["font_policy"] == "remove").encode("utf-8")
        check = parse(payload.decode("utf-8"), base.kind)
        if len(check.cues) != len(merged):
            raise ValueError("Output cue count changed during serialization")
        filename = name + "." + base.kind
        files[filename] = payload
        report.append({"id": name, "file": f"Sub/{filename}", "sha256": digest(payload),
                       "cues": len(check.cues), "sources": sources, "comparison": episode["comparison"],
                       "provenance": provenance})
    if sorted(assigned) != sorted(docs):
        raise ValueError("Assign each source to exactly one episode")
    identity = digest(json.dumps([project, sheets, glossary], ensure_ascii=False, sort_keys=True).encode("utf-8"))
    destination = work / "builds" / identity
    manifest = {"version": 1, "identity": identity, "series": project["series"],
                "target_language": project["target_language"], "font_policy": project["font_policy"],
                "episodes": report, "reviewed_cues": sum(len(doc.cues) for doc in docs.values())}
    if destination.exists():
        old = read_json(destination / "manifest.json")
        if old != manifest or any((destination / "Sub" / name).read_bytes() != data for name, data in files.items()):
            raise ValueError("Existing build has been modified; restore it or create a fresh workspace")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".build-", dir=destination.parent) as temporary:
            stage = Path(temporary) / "edition"
            (stage / "Sub").mkdir(parents=True)
            for name, data in files.items():
                (stage / "Sub" / name).write_bytes(data)
            write_json(stage / "manifest.json", manifest)
            write_json(stage / "glossary.json", glossary)
            stage.rename(destination)
    logging.info("Built episodes=%d reviewed_cues=%d", len(report), manifest["reviewed_cues"])
    return {"build": str(destination), **manifest}
