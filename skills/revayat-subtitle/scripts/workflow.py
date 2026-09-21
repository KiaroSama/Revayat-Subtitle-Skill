"""Source inventory, complete cue review, episode merging and immutable builds."""

from __future__ import annotations

import copy
import codecs
import json
import logging
import os
from pathlib import Path, PurePosixPath
import re
import zipfile
import validation

from runtime import digest, local_path, read_json, read_limited, staging_directory, write_json
from markup import clean_empty_lines, paragraph_direction, remap_resets
from subtitle_formats import (Cue, DEFAULT_STYLE, STYLE_FIELDS, has_drawing, parse,
                              rtl, serialize, srt_to_ass, structure, uncomment, visible, effective_times)

MAX_FILE = 16 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
MAX_ENTRIES = 100000
MAX_TOTAL_CUES = 250000
MAX_WORKSPACE = 512 * 1024 * 1024
EPISODE = re.compile(r"S([0-9]{2})(E|OVA)([0-9]{2}|[1-9][0-9]{2})")
SOURCE = re.compile(r"s[0-9]{4}")
PROJECT_SCHEMA = 2
GENERATION_RECIPE = {"version": 2, "normalization": 2, "timing": "floor-centisecond"}


def input_candidates(path: Path):
    if not path.is_dir():
        return [path]
    pending, found, visited = [path], [], 0
    while pending:
        with os.scandir(pending.pop()) as entries:
            for entry in entries:
                visited += 1
                if visited > MAX_ENTRIES:
                    raise ValueError("Input traversal exceeds 100000 directory entries")
                if entry.is_symlink() or getattr(entry.stat(follow_symlinks=False), "st_file_attributes", 0) & 1024:
                    continue
                if entry.is_dir(follow_symlinks=False):
                    pending.append(Path(entry.path))
                elif entry.is_file(follow_symlinks=False) and Path(entry.name).suffix.lower() in {".zip", ".ass", ".srt"}:
                    found.append(Path(entry.path))
                    if len(found) > 1000:
                        raise ValueError("Too many candidate subtitle files or archives")
    return sorted(found)


def inputs(paths: list[Path]):
    total, count = 0, 0
    if len(paths) > 1000:
        raise ValueError("Too many input roots")
    for root_index, path in enumerate(paths, 1):
        if path.is_symlink() or getattr(path.lstat(), "st_file_attributes", 0) & 1024:
            raise ValueError("Symbolic-link subtitle inputs are not accepted")
        candidates = input_candidates(path)
        for candidate in candidates:
            if candidate.is_symlink():
                continue
            relative = candidate.relative_to(path).as_posix() if path.is_dir() else candidate.name
            origin = {"root_id": f"input{root_index:03}", "root_label": path.name or "filesystem-root",
                      "relative_path": relative, "member": None, "member_index": None}
            suffix = candidate.suffix.lower()
            if suffix == ".zip":
                if candidate.stat().st_size > MAX_TOTAL:
                    raise ValueError("ZIP input exceeds the 256 MiB archive limit")
                with zipfile.ZipFile(candidate) as archive:
                    if len(archive.infolist()) > 10000:
                        raise ValueError("Too many ZIP members")
                    for member_index, entry in enumerate(archive.infolist(), 1):
                        name = PurePosixPath(entry.filename.replace("\\", "/"))
                        if name.is_absolute() or ".." in name.parts or any(":" in part for part in name.parts):
                            raise ValueError("Unsafe ZIP member path")
                        if name.suffix.lower() not in {".ass", ".srt"} or entry.is_dir():
                            continue
                        if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                            raise ValueError("ZIP symbolic links are not accepted")
                        if entry.flag_bits & 1:
                            raise ValueError("Encrypted ZIP: ask for the password and extract separately")
                        if entry.compress_type not in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2, zipfile.ZIP_LZMA}:
                            raise ValueError("Unsupported ZIP compression method")
                        count += 1
                        if entry.file_size > MAX_FILE or total + entry.file_size > MAX_TOTAL or count > 1000:
                            raise ValueError("Subtitle archive exceeds bounded import limits")
                        try:
                            with archive.open(entry) as stream:
                                raw = stream.read(MAX_FILE + 1)
                        except (NotImplementedError, RuntimeError) as error:
                            raise ValueError("Unsupported or encrypted ZIP member") from error
                        total += len(raw)
                        if len(raw) > MAX_FILE or total > MAX_TOTAL:
                            raise ValueError("Expanded subtitle data exceeds import limits")
                        member_origin = {**origin, "member": entry.filename, "member_index": member_index}
                        yield f"{origin['root_id']}/{relative}!/{name.as_posix()}", name.suffix[1:].lower(), raw, member_origin
            elif suffix in {".ass", ".srt"} and candidate.is_file():
                size = candidate.stat().st_size
                count += 1
                if size > MAX_FILE or total + size > MAX_TOTAL or count > 1000:
                    raise ValueError("Subtitle input exceeds bounded import limits")
                with candidate.open("rb") as stream:
                    raw = stream.read(MAX_FILE + 1)
                total += len(raw)
                if len(raw) > MAX_FILE or total > MAX_TOTAL:
                    raise ValueError("Subtitle grew beyond import limits while reading")
                yield f"{origin['root_id']}/{relative}", suffix[1:], raw, origin
            elif not path.is_dir():
                raise ValueError("Expected ASS, SRT, ZIP, or a directory containing subtitles")


def prepare(paths: list[Path], work: Path, series: str, season: int, encoding: str,
            glossary: Path | None, target_language: str) -> dict:
    validation.string(series, "series")
    validation.integer(season, "season", 1, 99)
    validation.string(encoding, "encoding")
    try:
        codecs.lookup(encoding)
    except LookupError:
        raise ValueError("encoding: unknown codec") from None
    if not series.strip() or not 1 <= season <= 99:
        raise ValueError("A series title and season from 1 to 99 are required")
    if work.exists():
        raise ValueError("Work directory already exists; resume it or choose a new directory")
    if glossary:
        terms = validation.glossary(read_json(glossary))
        if terms.get("series") != series:
            raise ValueError("The previous glossary belongs to a different series identity")
    else:
        terms = {"series": series, "research": [], "terms": [], "terms_reviewed": False}
    validation.matched(target_language, validation.LANGUAGE, "target_language")
    # Validate while staging, keeping only one parsed input in memory at a time.
    work.parent.mkdir(parents=True, exist_ok=True)
    with staging_directory(work.parent, ".subtitle-import-") as temporary:
        stage = Path(temporary) / "work"
        (stage / "sources").mkdir(parents=True)
        (stage / "worksheets").mkdir()
        project = {"version": PROJECT_SCHEMA, "series": series, "season": season, "target_language": target_language,
                   "font_policy": "keep", "sources": [], "episodes": []}
        total_cues = expanded = 0
        for i, (name, kind, raw, origin) in enumerate(inputs(paths), 1):
            doc = parse(raw.decode(encoding), kind)
            total_cues += len(doc.cues)
            if total_cues > MAX_TOTAL_CUES:
                raise ValueError("Workspace exceeds 250000 imported cues")
            source_id = f"s{i:04}"
            relative = f"sources/{source_id}.{kind}"
            (stage / relative).write_bytes(raw)
            project["sources"].append({"id": source_id, "name": name, "file": relative, "kind": kind,
                                       "sha256": digest(raw), "encoding": encoding, "cues": len(doc.cues),
                                       "comments_removed": doc.comments, "origin": origin})
            write_json(stage / "worksheets" / f"{source_id}.json", [
                {"id": cue.id, "source_text": cue.text, "start_ms": cue.start, "end_ms": cue.end,
                 "action": "edit", "text": None, "reviewed": False, "links": [], "note": "",
                 "timing_note": "", "structure_note": ""} for cue in doc.cues])
            expanded += len(raw) + (stage / "worksheets" / f"{source_id}.json").stat().st_size
            if expanded > MAX_WORKSPACE:
                raise ValueError("Expanded workspace exceeds the 512 MiB limit")
        if not project["sources"]:
            raise ValueError("No ASS/SRT subtitles found")
        write_json(stage / "project.json", project)
        write_json(stage / "glossary.json", terms)
        stage.rename(work)
    logging.info("Imported sources=%d cues=%d", len(project["sources"]), total_cues)
    return {"work": str(work), "sources": project["sources"], "next": "Read and fill every worksheet; map episodes in project.json"}


def load(work: Path):
    project = validation.project(read_json(work / "project.json"))
    def inventory(folder):
        result = set()
        with os.scandir(local_path(work, folder)) as entries:
            for entry in entries:
                if len(result) >= 1000 or not entry.is_file(follow_symlinks=False):
                    raise ValueError("Workspace inventory exceeds its file limit or contains a non-file")
                result.add(entry.name)
        return result
    source_files = inventory("sources")
    worksheet_files = inventory("worksheets")
    expected_sources = {f"{item['id']}.{item['kind']}" for item in project["sources"]}
    expected_sheets = {f"{item['id']}.json" for item in project["sources"]}
    if source_files != expected_sources or worksheet_files != expected_sheets:
        raise ValueError("Source inventory differs from imported files or worksheets")
    docs, sheets = {}, {}
    source_bytes = expanded = cues = 0
    for source in project["sources"]:
        key = source["id"]
        if not SOURCE.fullmatch(key) or key in docs:
            raise ValueError("Invalid or duplicate source ID")
        if source["file"] != f"sources/{key}.{source['kind']}":
            raise ValueError("Source path differs from its imported identity")
        raw = read_limited(local_path(work, source["file"]), MAX_FILE)
        source_bytes += len(raw)
        cues += source["cues"]
        if source_bytes > MAX_TOTAL or cues > MAX_TOTAL_CUES:
            raise ValueError("Workspace exceeds imported byte/cue limits")
        if digest(raw) != source["sha256"]:
            raise ValueError("Source changed since import; create a fresh workspace")
        docs[key] = parse(raw.decode(source["encoding"]), source["kind"])
        if len(docs[key].cues) != source["cues"]:
            raise ValueError(f"project.json.sources.{key}.cues: source cue count differs")
        sheet_path = local_path(work, f"worksheets/{key}.json")
        expanded += len(raw)
        sheets[key] = validation.worksheet(read_json(sheet_path, MAX_WORKSPACE - expanded), f"worksheets/{key}.json")
        expanded += sheet_path.stat().st_size
        if expanded > MAX_WORKSPACE:
            raise ValueError("Expanded workspace exceeds the 512 MiB limit")
    return project, docs, sheets


def verify_glossary(terms: dict, series: str) -> None:
    validation.glossary(terms)
    if terms.get("series") != series or terms.get("terms_reviewed") is not True:
        raise ValueError("Review the series glossary before building")
    research = terms.get("research", [])
    if not research:
        raise ValueError("Record actual anime research URLs and what they establish")
    seen = set()
    for term in terms.get("terms", []):
        if not term.get("source") or not term.get("target") or term.get("locked") is not True:
            raise ValueError("Each glossary term needs source, target, and locked=true")
        if term["source"] in seen:
            raise ValueError("Duplicate glossary source term")
        seen.add(term["source"])


def reviewed_cues(doc, sheet: list, target_language: str = "fa") -> tuple[list[Cue], list[dict]]:
    validation.worksheet(sheet)
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
        if doc.kind == "ass" and ("\n" in text or "\r" in text):
            raise ValueError(f"Cue {cue.id}: ASS text must use literal \\N, not physical newlines")
        if structure(text, doc.kind) != structure(cue.text, doc.kind) and not row.get("structure_note", "").strip():
            raise ValueError("Changed tags/drawings require a reason and visual review")
        start, end = row.get("start_ms"), row.get("end_ms")
        if type(start) is not int or type(end) is not int or not 0 <= start < end:
            raise ValueError("Cue timing must be nonnegative integer milliseconds with end after start")
        if (start, end) != (cue.start, cue.end) and not row.get("timing_note", "").strip():
            raise ValueError("Timing changes require an alignment reason")
        text = uncomment(text, doc.kind)
        preserve_lines = row.get("preserve_empty_lines", False)
        if preserve_lines and not row.get("structure_note", "").strip():
            raise ValueError(f"Cue {cue.id}: preserving empty display lines needs a structure_note")
        text = clean_empty_lines(text, doc.kind, preserve_lines)
        direction = row.get("direction", "auto")
        if direction != "auto" and not row.get("direction_note", "").strip():
            raise ValueError(f"Cue {cue.id}: direction override requires direction_note")
        direction = paragraph_direction(target_language) if direction == "auto" else direction
        kept.append(Cue(cue.id, start, end, rtl(text, doc.kind, direction, force=row.get("direction", "auto") != "auto"), dict(cue.fields)))
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
        cue.text = remap_resets(cue.text, mapping)
    return cues


def assemble(work: Path) -> tuple[dict, dict[str, bytes], dict]:
    """Reconstruct the expected edition without publishing files or changing reviews."""
    project, docs, sheets = load(work)
    glossary = read_json(work / "glossary.json")
    verify_glossary(glossary, project["series"])
    if project.get("font_policy") not in {"keep", "remove"}:
        raise ValueError("font_policy must be keep or remove")
    if not project.get("episodes"):
        raise ValueError("Map every source to an episode in project.json")
    reviewed = {key: reviewed_cues(doc, sheets[key], project["target_language"]) for key, doc in docs.items()}
    assigned, names, files, report = [], set(), {}, []
    for episode in project["episodes"]:
        name, base_id = episode["id"], episode["base"]
        match = EPISODE.fullmatch(name)
        if not match:
            raise ValueError("Episode names require canonical ASCII SxxEyy or SxxOVAyy; use E01, not E001")
        numeric_identity = (int(match[1]), match[2], int(match[3]))
        if numeric_identity in names or numeric_identity[0] != project["season"]:
            raise ValueError("Episode identities must be unique and match the project season")
        if numeric_identity[2] < 1:
            raise ValueError("Episode and OVA numbers start at 1")
        names.add(numeric_identity)
        sources = [base_id] + episode.get("alternates", [])
        if any(key not in docs for key in sources) or not episode.get("comparison", "").strip():
            raise ValueError("Every episode needs valid sources and a comparison/selection reason")
        if any(docs[key].kind == "ass" for key in sources) and docs[base_id].kind != "ass":
            raise ValueError("Choose the ASS candidate as the base; content can come from other candidates")
        assigned.extend(sources)
        base = copy.deepcopy(docs[base_id])
        retained = {f"{key}:{cue.id}" for key in sources for cue in reviewed[key][0]}
        paired = []
        for key in sources:
            cues, decisions = copy.deepcopy(reviewed[key])
            for decision in decisions:
                if any(link not in retained for link in decision["links"]):
                    raise ValueError("Review links must identify retained cues in this episode")
            original = {cue.id: [cue.start, cue.end] for cue in docs[key].cues}
            reviewed_times = {cue.id: [cue.start, cue.end] for cue in cues}
            changed_structures = {row["id"] for row in sheets[key] if row.get("structure_note", "").strip()}
            if key != base_id:
                cues = merge_donor(base, docs[key], cues, key, project["font_policy"] == "keep")
            for cue in cues:
                cue.start, cue.end = effective_times(cue.start, cue.end, base.kind)
                emitted = [cue.start, cue.end]
                paired.append((cue, {"source": key, "cue": cue.id,
                                     "structure_changed": cue.id in changed_structures,
                                     "timing": {"original": original[cue.id],
                                                "reviewed": reviewed_times[cue.id], "emitted": emitted,
                                                "conversion": "floor-centisecond" if emitted != reviewed_times[cue.id] else None}}))
        paired.sort(key=lambda item: (item[0].start, item[1]["timing"]["reviewed"][0]))
        merged = [cue for cue, _ in paired]
        provenance = [{"emitted_index": index, **record} for index, (_, record) in enumerate(paired, 1)]
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
    input_identity = digest(json.dumps([project, sheets, glossary], ensure_ascii=False,
                                       sort_keys=True, allow_nan=False).encode("utf-8"))
    identity = digest(json.dumps([input_identity, GENERATION_RECIPE], sort_keys=True).encode("utf-8"))
    manifest = {"version": 2, "identity": identity, "input_identity": input_identity,
                "generation_recipe": dict(GENERATION_RECIPE), "series": project["series"],
                "source_origins": {source["id"]: source.get("origin", {"legacy_name": source["name"],
                                   "status": "legacy origin unavailable"}) for source in project["sources"]},
                "target_language": project["target_language"], "font_policy": project["font_policy"],
                "episodes": report, "reviewed_cues": sum(len(doc.cues) for doc in docs.values())}
    return manifest, files, glossary


def build(work: Path) -> dict:
    manifest, files, glossary = assemble(work)
    destination = work / "builds" / manifest["identity"]
    if destination.exists():
        old = read_json(destination / "manifest.json")
        if old != manifest or any((destination / "Sub" / name).read_bytes() != data for name, data in files.items()):
            raise ValueError("Existing build has been modified; restore it or create a fresh workspace")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with staging_directory(destination.parent, ".build-") as temporary:
            stage = Path(temporary) / "edition"
            (stage / "Sub").mkdir(parents=True)
            for name, data in files.items():
                (stage / "Sub" / name).write_bytes(data)
            write_json(stage / "manifest.json", manifest)
            write_json(stage / "glossary.json", glossary)
            stage.rename(destination)
    logging.info("Built episodes=%d reviewed_cues=%d", len(manifest["episodes"]), manifest["reviewed_cues"])
    return {"build": str(destination), **manifest}
