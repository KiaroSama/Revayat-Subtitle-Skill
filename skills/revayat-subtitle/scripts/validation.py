"""Typed validation for editable records, with content-free field diagnostics."""

from __future__ import annotations

import math
import codecs
import re
import ipaddress
from urllib.parse import urlsplit

MAX_TIME_MS = 10**12

HASH = re.compile(r"[0-9a-f]{64}")
SOURCE_ID = re.compile(r"s[0-9]{4}")
CUE_ID = re.compile(r"c[0-9]{6}")
LANGUAGE = re.compile(r"[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*")


def fail(where: str, expected: str):
    raise ValueError(f"{where}: {expected}")


def obj(value, where: str) -> dict:
    if not isinstance(value, dict):
        fail(where, "expected an object")
    return value


def array(value, where: str, maximum: int = 100000) -> list:
    if not isinstance(value, list) or len(value) > maximum:
        fail(where, f"expected an array with at most {maximum} entries")
    return value


def string(value, where: str, *, empty: bool = False) -> str:
    if (not isinstance(value, str) or (not empty and not value.strip()) or "\x00" in value
            or any(0xD800 <= ord(char) <= 0xDFFF for char in value)):
        fail(where, "expected a valid string" + ("" if empty else " that is not empty"))
    return value


def integer(value, where: str, minimum: int = 0, maximum: int = 10**12) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        fail(where, f"expected an integer in {minimum}..{maximum}")
    return value


def boolean(value, where: str) -> bool:
    if type(value) is not bool:
        fail(where, "expected a boolean")
    return value


def number(value, where: str) -> float:
    if type(value) not in (int, float) or not 0 <= value <= 10**12 or not math.isfinite(value):
        fail(where, "expected a finite nonnegative number")
    return value


def matched(value, pattern: re.Pattern, where: str) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        fail(where, "invalid identifier or digest")
    return value


def glossary(value, where: str = "glossary.json") -> dict:
    value = obj(value, where)
    string(value.get("series"), where + ".series")
    boolean(value.get("terms_reviewed", False), where + ".terms_reviewed")
    for index, item in enumerate(array(value.get("research", []), where + ".research", 10000)):
        loc = f"{where}.research[{index}]"
        obj(item, loc)
        address = string(item.get("url"), loc + ".url")
        try:
            parsed = urlsplit(address)
            valid = (parsed.scheme in {"http", "https"} and parsed.hostname
                     and parsed.username is None and parsed.password is None
                     and not any(c.isspace() or ord(c) < 32 for c in address))
            if valid:
                host = parsed.hostname.encode("idna").decode("ascii")
                if ":" in host:
                    ipaddress.IPv6Address(host)
                else:
                    valid = len(host) <= 253 and all(re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?", part)
                                                    for part in host.rstrip(".").split("."))
            parsed.port
        except (ValueError, UnicodeError):
            valid = False
        if not valid:
            fail(loc + ".url", "expected an HTTP(S) URL with a hostname and no credentials")
        string(item.get("note"), loc + ".note")
    for index, term in enumerate(array(value.get("terms", []), where + ".terms")):
        loc = f"{where}.terms[{index}]"
        obj(term, loc)
        for field in ("source", "target"):
            string(term.get(field), loc + "." + field)
        boolean(term.get("locked"), loc + ".locked")
    return value


def project(value) -> dict:
    value = obj(value, "project.json")
    if type(value.get("version")) is not int or value["version"] not in {1, 2}:
        fail("project.json.version", "unsupported project schema")
    string(value.get("series"), "project.json.series")
    integer(value.get("season"), "project.json.season", 1, 99)
    matched(value.get("target_language"), LANGUAGE, "project.json.target_language")
    if value.get("font_policy") not in ("keep", "remove"):
        fail("project.json.font_policy", "expected keep or remove")
    for index, source in enumerate(array(value.get("sources"), "project.json.sources", 1000)):
        loc = f"project.json.sources[{index}]"
        obj(source, loc)
        matched(source.get("id"), SOURCE_ID, loc + ".id")
        matched(source.get("sha256"), HASH, loc + ".sha256")
        if source.get("kind") not in ("ass", "srt"):
            fail(loc + ".kind", "expected ass or srt")
        for field in ("name", "file", "encoding"):
            string(source.get(field), loc + "." + field)
        try:
            codecs.lookup(source["encoding"])
        except LookupError:
            fail(loc + ".encoding", "unknown codec")
        integer(source.get("cues"), loc + ".cues", 1, 100000)
        if "origin" in source or value["version"] == 2:
            origin = obj(source.get("origin"), loc + ".origin")
            for field in ("root_id", "root_label", "relative_path"):
                string(origin.get(field), loc + ".origin." + field)
            if origin.get("member") is not None:
                string(origin["member"], loc + ".origin.member")
                integer(origin.get("member_index"), loc + ".origin.member_index", 1, 10000)
    for index, episode in enumerate(array(value.get("episodes"), "project.json.episodes", 1000)):
        loc = f"project.json.episodes[{index}]"
        obj(episode, loc)
        for field in ("id", "base", "comparison"):
            string(episode.get(field), loc + "." + field)
        for offset, source in enumerate(array(episode.get("alternates", []), loc + ".alternates", 1000)):
            matched(source, SOURCE_ID, f"{loc}.alternates[{offset}]")
    return value


def worksheet(value, where: str = "worksheet") -> list:
    for index, row in enumerate(array(value, where)):
        loc = f"{where}[{index}]"
        obj(row, loc)
        matched(row.get("id"), CUE_ID, loc + ".id")
        string(row.get("source_text"), loc + ".source_text", empty=True)
        boolean(row.get("reviewed"), loc + ".reviewed")
        for field in ("start_ms", "end_ms"):
            integer(row.get(field), loc + "." + field, maximum=MAX_TIME_MS)
        if row.get("action") not in ("edit", "preserve", "credit", "empty", "alternate"):
            fail(loc + ".action", "expected a valid review action")
        if row.get("text") is not None:
            string(row["text"], loc + ".text", empty=True)
        for field in ("note", "timing_note", "structure_note", "direction_note"):
            if field in row:
                string(row[field], loc + "." + field, empty=True)
        if "direction" in row and row["direction"] not in ("auto", "ltr", "rtl"):
            fail(loc + ".direction", "expected auto, ltr or rtl")
        if "preserve_empty_lines" in row:
            boolean(row["preserve_empty_lines"], loc + ".preserve_empty_lines")
        for offset, link in enumerate(array(row.get("links", []), loc + ".links", 1000)):
            matched(link, re.compile(r"s[0-9]{4}:c[0-9]{6}"), f"{loc}.links[{offset}]")
    return value


def frame(value, where: str, *, review: bool) -> dict:
    value = obj(value, where)
    string(value.get("file"), where + ".file")
    matched(value.get("sha256"), HASH, where + ".sha256")
    number(value.get("time_seconds"), where + ".time_seconds")
    for field in ("width", "height"):
        integer(value.get(field), where + "." + field, 1, 8192)
    indices = array(value.get("cue_indices"), where + ".cue_indices")
    if not indices:
        fail(where + ".cue_indices", "expected associated emitted cue indices")
    for index in indices:
        integer(index, where + ".cue_indices", 1, 100000)
    if len(indices) != len(set(indices)):
        fail(where + ".cue_indices", "duplicate cue index")
    if review:
        boolean(value.get("reviewed"), where + ".reviewed")
        string(value.get("note"), where + ".note", empty=True)
    return value


def render_record(value, where: str) -> dict:
    value = obj(value, where)
    if type(value.get("version")) is not int or value["version"] != 2:
        fail(where + ".version", "legacy or unsupported render schema; rerender this edition")
    for field in ("identity", "subtitle_sha256", "receipt_sha256"):
        matched(value.get(field), HASH, where + "." + field)
    for field in ("episode", "receipt_file"):
        string(value.get(field), where + "." + field)
    boolean(value.get("all_cues"), where + ".all_cues")
    if value.get("background") not in ("video", "neutral"):
        fail(where + ".background", "expected video or neutral")
    for index, item in enumerate(array(value.get("frames"), where + ".frames", 5000)):
        frame(item, f"{where}.frames[{index}]", review=True)
    return value


def fingerprint(value, where: str, maximum: int = 16 * 1024**3) -> dict:
    value = obj(value, where)
    name = string(value.get("name"), where + ".name")
    if "/" in name or "\\" in name:
        fail(where + ".name", "expected a basename, not a private path")
    integer(value.get("bytes"), where + ".bytes", 0, maximum)
    matched(value.get("sha256"), HASH, where + ".sha256")
    return value
