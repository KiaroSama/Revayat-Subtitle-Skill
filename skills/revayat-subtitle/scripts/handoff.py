"""Validate revision-bound disjoint editorial proposals without importing or approval."""
from __future__ import annotations

from pathlib import Path
import re

import validation
import workflow
from runtime import digest, local_path, read_json, read_limited
from status import bounded_report, workspace_state

MAX_FILE = 16 * 1024 * 1024
ASSIGNMENT_FIELDS = {"version", "id", "source", "episode", "source_sha256", "glossary_sha256", "assigned_ids", "context_ids"}
RESULT_FIELDS = {"version", "assignment_id", "source_sha256", "glossary_sha256", "rows", "status", "activity_log"}
ROW_FIELDS = {"id", "source_text", "start_ms", "end_ms", "action", "text", "reviewed", "links", "note",
              "timing_note", "structure_note", "direction", "direction_note", "preserve_empty_lines"}


def envelope(path: Path, fields: set[str], required: set[str]):
    path = path.absolute()
    for item in (path, *path.parents):
        if item.is_symlink() or (item.exists() and getattr(item.lstat(), "st_file_attributes", 0) & 1024):
            raise ValueError("Linked handoff artifacts are not accepted")
    value = validation.obj(read_json(path, MAX_FILE), "handoff envelope")
    if not required <= set(value) or set(value) - fields:
        raise ValueError("Handoff envelope has missing or unsupported fields")
    if type(value.get("version")) is not int or value["version"] != 1:
        raise ValueError("Unsupported handoff envelope version")
    return value


def cue_ids(value, where):
    result = validation.array(value, where)
    for key in result:
        validation.matched(key, validation.CUE_ID, where)
    if len(result) != len(set(result)):
        raise ValueError("Handoff cue IDs must be unique")
    return set(result)


def handoff(work: Path, assignments: list[Path], results: list[Path]) -> dict:
    if not assignments or len(assignments) > 1000 or len(results) > 1000:
        raise ValueError("Handoff needs assignments and at most 1000 files of each kind")
    project, docs, sheets, _, _ = workspace_state(work)
    glossary_hash = digest(read_limited(local_path(work, "glossary.json"), MAX_FILE))
    source_hashes = {source["id"]: source["sha256"] for source in project["sources"]}
    assignment_map, covered, total = {}, set(), 0
    for path in assignments:
        assignment = envelope(path, ASSIGNMENT_FIELDS, ASSIGNMENT_FIELDS - {"episode"})
        identifier = validation.matched(assignment["id"], re.compile(r"[A-Za-z0-9_-]{1,80}"), "assignment.id")
        source = validation.matched(assignment["source"], validation.SOURCE_ID, "assignment.source")
        if identifier in assignment_map or source not in docs:
            raise ValueError("Duplicate assignment ID or unknown assignment source")
        for field, expected in (("source_sha256", source_hashes[source]), ("glossary_sha256", glossary_hash)):
            if validation.matched(assignment[field], validation.HASH, "assignment." + field) != expected:
                raise ValueError("Assignment source or glossary revision is stale")
        if "episode" in assignment:
            actual = [episode["id"] for episode in project["episodes"]
                      if source in [episode["base"], *episode.get("alternates", [])]]
            if actual != [assignment["episode"]]:
                raise ValueError("Assignment episode differs from current workspace mapping")
        assigned = cue_ids(assignment["assigned_ids"], "assigned_ids")
        context = cue_ids(assignment["context_ids"], "context_ids")
        known = {cue.id for cue in docs[source].cues}
        if not assigned or assigned & context or not assigned | context <= known:
            raise ValueError("Assignment coverage/context IDs are invalid")
        keys = {(source, cue) for cue in assigned}
        if covered & keys:
            raise ValueError("Assignments overlap")
        covered.update(keys)
        total += len(assigned)
        if total > 250000:
            raise ValueError("Handoff exceeds its total row limit")
        assignment_map[identifier] = assignment
    reports, received, proposals, row_sources = [], set(), {}, {}
    pending_total = 0
    effective = {f"{key}:{row['id']}": row for key, rows in sheets.items() for row in rows}
    for path in results:
        result = envelope(path, RESULT_FIELDS, RESULT_FIELDS - {"activity_log"})
        identifier = validation.matched(result["assignment_id"], re.compile(r"[A-Za-z0-9_-]{1,80}"), "result.assignment_id")
        if identifier not in assignment_map or identifier in received:
            raise ValueError("Result assignment is unknown or duplicated")
        received.add(identifier)
        assignment = assignment_map[identifier]
        for field in ("source_sha256", "glossary_sha256"):
            if type(result[field]) is not str or result[field] != assignment[field]:
                raise ValueError("Result source or glossary revision is stale")
        if type(result["status"]) is not str or result["status"] not in {"complete", "pending"}:
            raise ValueError("Result status must be complete or pending")
        if "activity_log" in result:
            local_path(work, result["activity_log"])  # Data-only reference; never read or executed.
        rows = validation.worksheet(result["rows"], "result.rows")
        if any(set(row) - ROW_FIELDS for row in rows):
            raise ValueError("Result row has an unsupported field")
        expected_ids = set(assignment["assigned_ids"])
        if len(rows) != len(expected_ids) or {row["id"] for row in rows} != expected_ids:
            raise ValueError("Result needs exactly one row per assigned cue, without context-only rows")
        source = assignment["source"]
        originals = {cue.id: cue for cue in docs[source].cues}
        pending = 0
        episode_sources = next(([episode["base"], *episode.get("alternates", [])] for episode in project["episodes"]
                                if source in [episode["base"], *episode.get("alternates", [])]), [source])
        known_links = {f"{key}:{cue.id}" for key in episode_sources for cue in docs[key].cues}
        for row in rows:
            link_key = f"{source}:{row['id']}"
            effective[link_key] = row
            proposals[link_key] = row
            row_sources[link_key] = set(episode_sources)
            cue = originals[row["id"]]
            if any(link not in known_links for link in row.get("links", [])):
                raise ValueError("Result links must identify original cues in the same episode")
            if row["source_text"] != cue.text:
                raise ValueError("Result source text changed")
            if not row["reviewed"] or (row["action"] == "edit" and row.get("text") is None):
                pending += 1
            else:
                # Reuse the real decision/timing/structure seam for complete proposals.
                doc = type(docs[source])(docs[source].kind, [cue])
                workflow.reviewed_cues(doc, [row], project["target_language"])
        if (result["status"] == "complete") != (pending == 0):
            raise ValueError("Result declared status differs from its pending rows")
        pending_total += pending
        reports.append({"assignment_id": identifier, "rows": len(rows), "pending_rows": pending})
    if received != set(assignment_map):
        raise ValueError("A result is required for every assignment")
    unresolved_links = set()
    for key, row in proposals.items():
        if not row["reviewed"]:
            continue
        for link in row.get("links", []):
            target = effective.get(link)
            if link == key or target is None or link.split(":", 1)[0] not in row_sources[key]:
                raise ValueError("Result link cannot target itself or a foreign cue")
            if not target["reviewed"] or (target["action"] == "edit" and target.get("text") is None):
                unresolved_links.add(key)
            elif target["action"] not in {"edit", "preserve"}:
                raise ValueError("Result links must identify effective retained cues, not excluded decisions")
    pending_total += len(unresolved_links)
    return bounded_report({"version": 1, "ready": pending_total == 0,
                           "state": "pending" if pending_total else "complete", "assignments": sorted(reports, key=lambda item: item["assignment_id"]),
                           "pending_rows": pending_total, "coordinator_review_required": True,
                           "linguistic_quality_certified": False})
