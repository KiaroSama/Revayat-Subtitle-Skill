"""Bounded read-only workspace status; mechanical readiness never certifies meaning."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re

import validation
import workflow
from runtime import local_path, read_json

MAX_REPORT_BYTES = 1024 * 1024


def workspace_state(work: Path):
    project, docs, sheets = workflow.load(work)
    glossary = validation.glossary(read_json(local_path(work, "glossary.json")))
    if glossary["series"] != project["series"]:
        raise ValueError("Glossary belongs to a different series identity")
    for key, doc in docs.items():
        if [row["id"] for row in sheets[key]] != [cue.id for cue in doc.cues]:
            raise ValueError(f"Worksheet {key}: original cue coverage/order differs")
        if any(row["source_text"] != cue.text for row, cue in zip(sheets[key], doc.cues)):
            raise ValueError(f"Worksheet {key}: source text differs")
    assigned, names = [], set()
    for episode in project["episodes"]:
        match = workflow.EPISODE.fullmatch(episode["id"])
        numeric = (int(match[1]), match[2], int(match[3])) if match else None
        if (not match or numeric[0] != project["season"] or numeric[2] < 1
                or numeric in names or not episode["comparison"].strip()):
            raise ValueError("Episode assignment identity or comparison is invalid")
        names.add(numeric)
        sources = [episode["base"], *episode.get("alternates", [])]
        if any(key not in docs for key in sources):
            raise ValueError("Episode assignment references an unknown source")
        assigned.extend(sources)
    if len(assigned) != len(set(assigned)):
        raise ValueError("A source belongs to more than one episode assignment")
    return project, docs, sheets, glossary, sorted(set(docs) - set(assigned))


def bounded_report(report):
    if len(json.dumps(report, ensure_ascii=False, allow_nan=False).encode("utf-8")) > MAX_REPORT_BYTES:
        raise ValueError("Read-only report exceeds its 1 MiB byte limit")
    return report


def status(work: Path) -> dict:
    project, docs, sheets, glossary, unassigned = workspace_state(work)
    pending = [(key, row["id"]) for key in docs for row in sheets[key]
               if not row["reviewed"] or (row["action"] == "edit" and row.get("text") is None)]
    glossary_ready = glossary.get("terms_reviewed") is True
    if glossary_ready:
        workflow.verify_glossary(glossary, project["series"])
    complete_input = not pending and glossary_ready and not unassigned and bool(project["episodes"])
    expected = workflow.assemble(work)[0] if complete_input else None
    editions = []
    root = local_path(work, "builds")
    current = None
    if root.exists():
        if not root.is_dir():
            raise ValueError("Edition inventory must be a directory")
        with os.scandir(root) as entries:
            for entry in entries:
                if len(editions) >= 1000 or not entry.is_dir(follow_symlinks=False):
                    raise ValueError("Edition inventory exceeds its limit or contains a non-directory")
                path = local_path(work, "builds/" + entry.name)
                if not re.fullmatch(r"[0-9a-f]{64}", entry.name):
                    raise ValueError("Edition inventory contains an invalid identity")
                manifest = validation.obj(read_json(local_path(path, "manifest.json")), "edition manifest")
                if manifest.get("identity") != entry.name:
                    raise ValueError("Edition manifest identity differs from its directory")
                if type(manifest.get("version")) is not int or manifest["version"] not in {1, 2}:
                    raise ValueError("Edition manifest schema is invalid")
                validation.matched(manifest["identity"], validation.HASH, "edition.identity")
                validation.string(manifest.get("series"), "edition.series")
                for episode in validation.array(manifest.get("episodes"), "edition.episodes", 1000):
                    validation.obj(episode, "edition.episode")
                    validation.string(episode.get("id"), "edition.episode.id")
                    validation.string(episode.get("file"), "edition.episode.file")
                    validation.matched(episode.get("sha256"), validation.HASH, "edition.episode.sha256")
                item = {"identity": entry.name, "state": "stale", "episodes": 0, "visual_pending": 0}
                if expected and entry.name == expected["identity"]:
                    import render
                    valid, _ = render.load_build(path)
                    item["episodes"] = len(valid["episodes"])
                    existing, missing, visual_pending = [], 0, 0
                    for episode in valid["episodes"]:
                        record = local_path(path, f"renders/{episode['id']}.json")
                        if not record.exists():
                            missing += 1
                            continue
                        evidence = validation.render_record(read_json(record), "render review")
                        existing.append(episode["id"])
                        visual_pending += sum(not frame["reviewed"] or not frame["note"].strip()
                                              for frame in evidence["frames"])
                    if existing:
                        render.check_delivery(path, require_review=False, episode_ids=set(existing))
                    if not visual_pending and not missing:
                        render.check_delivery(path)  # Full cross-episode alias/approval gate remains authoritative.
                    item["visual_pending"] = visual_pending + missing
                    item["state"] = "pending" if visual_pending or missing else "complete"
                    current = item
                editions.append(item)
    ready = complete_input and current is not None and current["state"] == "complete"
    return bounded_report({"version": 1, "ready": bool(ready), "state": "complete" if ready else "pending",
                           "sources": len(docs), "cues": sum(len(doc.cues) for doc in docs.values()),
                           "pending_cues": len(pending),
                           "first_pending": {"source": pending[0][0], "cue": pending[0][1]} if pending else None,
                           "glossary": "complete" if glossary_ready else "pending",
                           "unassigned_sources": unassigned, "editions": sorted(editions, key=lambda item: item["identity"]),
                           "linguistic_quality_certified": False})
