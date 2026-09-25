"""Strict ASS/SRT parsing and presentation-preserving subtitle transformations."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from markup import SRT_BREAK, ARABIC, BIDI, PDF, RLE, RLM, overrides, pieces, rtl, uncomment
from validation import MAX_TIME_MS

ASS_FIELDS = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
STYLE_FIELDS = ("Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, "
                "BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, "
                "Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding")
DEFAULT_STYLE = ("Default,Arial,42,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,"
                 "0,0,0,0,100,100,0,0,1,2,1,2,30,30,35,1")


@dataclass
class Cue:
    id: str
    start: int
    end: int
    text: str
    fields: dict[str, str] = field(default_factory=dict)


@dataclass
class Document:
    kind: str
    cues: list[Cue]
    sections: list[tuple[str, list[str]]] = field(default_factory=list)
    event_fields: list[str] = field(default_factory=list)
    style_fields: list[str] = field(default_factory=list)
    styles: dict[str, list[str]] = field(default_factory=dict)
    comments: int = 0


def timestamp(value: str, kind: str) -> int:
    if not isinstance(value, str) or kind not in {"ass", "srt"}:
        raise ValueError("Invalid subtitle timestamp or format")
    pattern = (r"([0-9]{1,12}):([0-9]{2}):([0-9]{2})\.([0-9]{2})" if kind == "ass"
               else r"([0-9]{1,12}):([0-9]{2}):([0-9]{2}),([0-9]{3})")
    match = re.fullmatch(pattern, value.strip())
    if not match:
        raise ValueError("Invalid subtitle timestamp")
    hours, minutes, seconds, fraction = map(int, match.groups())
    if minutes > 59 or seconds > 59:
        raise ValueError("Subtitle minutes and seconds must be below 60")
    result = ((hours * 60 + minutes) * 60 + seconds) * 1000 + fraction * (10 if kind == "ass" else 1)
    if result > MAX_TIME_MS:
        raise ValueError("Subtitle timestamp exceeds the supported millisecond limit")
    return result


def timecode(ms: int, kind: str) -> str:
    if type(ms) is not int or not 0 <= ms <= MAX_TIME_MS or kind not in {"ass", "srt"}:
        raise ValueError("Timestamp serialization requires nonnegative integer milliseconds and ASS/SRT")
    hours, ms = divmod(ms, 3600000)
    minutes, ms = divmod(ms, 60000)
    seconds, fraction = divmod(ms, 1000)
    if kind == "ass":
        return f"{hours}:{minutes:02}:{seconds:02}.{fraction // 10:02}"
    return f"{hours:02}:{minutes:02}:{seconds:02},{fraction:03}"


def effective_times(start: int, end: int, kind: str) -> tuple[int, int]:
    if type(start) is not int or type(end) is not int or not 0 <= start < end <= MAX_TIME_MS:
        raise ValueError("Cue timing requires nonnegative integers and end after start")
    result = (start // 10 * 10, end // 10 * 10) if kind == "ass" else (start, end)
    if result[1] <= result[0]:
        raise ValueError("Centisecond quantization collapses this cue; review its timing explicitly")
    return result


def parse(data: str, kind: str) -> Document:
    if not isinstance(data, str) or len(data) > 16 * 1024 * 1024:
        raise ValueError("Subtitle text exceeds the supported parse budget")
    data = data.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    if "\x00" in data or "\ufffd" in data:
        raise ValueError("NUL or replacement characters in subtitles; check the input encoding")
    doc = Document(kind, [])
    if kind == "srt":
        def blocks():
            source, cursor = data.strip(), 0
            for match in re.finditer(r"\n[ \t]*\n", source):
                yield source[cursor:match.start()]
                cursor = match.end()
            yield source[cursor:]
        for block in blocks():
            if not block.strip():
                continue
            lines = block.split("\n")
            if len(lines) < 2 or not lines[0].strip().isdigit():
                raise ValueError("Malformed SRT cue; expected a number and timestamp line")
            times = lines[1].split(" --> ")
            if len(times) != 2:
                raise ValueError("Malformed SRT timestamp line")
            start, end = (timestamp(t, kind) for t in times)
            if len(doc.cues) >= 100000:
                raise ValueError("Subtitle exceeds 100000 cues")
            doc.cues.append(Cue(f"c{len(doc.cues) + 1:06}", start, end, "\n".join(lines[2:])))
    elif kind == "ass":
        section, lines = "", []
        seen_sections = set()
        known_sections = {"[script info]", "[v4+ styles]", "[v4 styles]", "[events]", "[fonts]",
                          "[graphics]", "[aegisub project garbage]", "[aegisub extradata]"}
        for line in data.split("\n"):
            stripped = line.strip()
            header = stripped.startswith("[") and stripped.endswith("]")
            if section.casefold() in {"[fonts]", "[graphics]"}:
                header = header and stripped.casefold() in known_sections
            if header:
                section = stripped
                if section.casefold() in seen_sections:
                    raise ValueError("Repeated ASS sections are not supported")
                seen_sections.add(section.casefold())
                lines = []
                doc.sections.append((section, lines))
            elif section:
                lines.append(line)
            elif stripped and not stripped.startswith(";"):
                raise ValueError("ASS content before the first section")
        for section, lines in doc.sections:
            key = section.casefold()
            if key not in {"[events]", "[v4+ styles]"}:
                if key == "[v4 styles]":
                    raise ValueError("Legacy SSA requires explicit conversion to ASS first")
                continue
            fields = []
            for line in lines:
                if not line.strip() or line.lstrip().startswith(";"):
                    continue
                label, separator, value = line.partition(":")
                label = label.strip().casefold()
                if not separator:
                    raise ValueError("Malformed ASS section row")
                if label == "format":
                    if fields:
                        raise ValueError("Multiple Format rows in an ASS section")
                    fields = [s.strip().casefold() for s in value.split(",")]
                    if len(fields) != len(set(fields)):
                        raise ValueError("Duplicate ASS Format fields")
                    if key == "[events]":
                        if not {"start", "end", "text", "style"} <= set(fields) or fields[-1] != "text":
                            raise ValueError("ASS Events requires Start, End, Style and final Text")
                        doc.event_fields = fields
                    else:
                        if "name" not in fields:
                            raise ValueError("ASS Styles requires a Name field")
                        doc.style_fields = fields
                elif label == "comment" and key == "[events]":
                    doc.comments += 1
                elif label == ("dialogue" if key == "[events]" else "style"):
                    if not value.strip() and label == "style":
                        continue
                    if not fields:
                        raise ValueError("ASS rows require a preceding Format")
                    parts = value.lstrip().split(",", len(fields) - 1)
                    if len(parts) != len(fields):
                        raise ValueError("ASS row does not match its Format")
                    row = dict(zip(fields, parts))
                    if label == "style":
                        name = row["name"].strip()
                        if not name:
                            continue
                        if name in doc.styles:
                            raise ValueError("Duplicate ASS style name")
                        doc.styles[name] = parts
                    else:
                        if len(doc.cues) >= 100000:
                            raise ValueError("Subtitle exceeds 100000 cues")
                        doc.cues.append(Cue(f"c{len(doc.cues) + 1:06}", timestamp(row["start"], kind),
                                            timestamp(row["end"], kind), row["text"], row))
                else:
                    raise ValueError("Unsupported ASS row in Events or Styles; inspect before conversion")
        if not doc.event_fields or not doc.style_fields:
            raise ValueError("Missing ASS Events or V4+ Styles Format")
    else:
        raise ValueError("Supported formats are .ass and .srt")
    if not doc.cues:
        raise ValueError("No subtitle cues found")
    for cue in doc.cues:
        if cue.end <= cue.start:
            raise ValueError(f"Cue {cue.id} ends before or at its start")
    return doc


def visible(text: str, kind: str) -> str:
    result = "".join(value for type_, value in pieces(text, kind) if type_ == "text")
    if kind == "ass":
        result = result.replace(r"\N", "\n").replace(r"\n", " ").replace(r"\h", " ")
    return result.translate(str.maketrans("", "", BIDI)).strip()


def structure(text: str, kind: str) -> list[tuple[str, str]]:
    return [(type_, value) for type_, value in pieces(text, kind) if type_ != "text"]


def has_drawing(text: str, kind: str) -> bool:
    return any(type_ == "drawing" and value.strip() for type_, value in pieces(text, kind))


def style_references(cue: Cue) -> set[str]:
    refs = {cue.fields.get("style", "Default").strip()}
    for type_, value in pieces(cue.text, "ass"):
        if type_ == "tag":
            refs.update(argument.strip() for name, argument, _, _ in overrides(value)
                        if name == "r" and argument.strip())
    return refs


def serialize(doc: Document, cues: list[Cue], remove_fonts: bool = False) -> str:
    cues = sorted(cues, key=lambda cue: cue.start)  # Ties retain their source/layer order.
    if not cues:
        raise ValueError("Refusing an episode with no retained cues")
    for cue in cues:
        effective_times(cue.start, cue.end, doc.kind)
    if doc.kind == "srt":
        return "\n\n".join(f"{i}\n{timecode(c.start, 'srt')} --> {timecode(c.end, 'srt')}\n{c.text}"
                           for i, c in enumerate(cues, 1)) + "\n"
    used = set().union(*(style_references(c) for c in cues))
    if used - doc.styles.keys():
        raise ValueError("Subtitle references an undefined ASS style")
    output = []
    for name, lines in doc.sections:
        key = name.casefold()
        if key == "[aegisub project garbage]" or (remove_fonts and key == "[fonts]"):
            continue
        output.append(name)
        if key == "[events]":
            output.append("Format: " + ", ".join(doc.event_fields))
            for cue in cues:
                row = {**cue.fields, "start": timecode(cue.start, "ass"), "end": timecode(cue.end, "ass"),
                       "text": cue.text}
                if "\n" in cue.text or "\r" in cue.text:
                    raise ValueError("ASS cue text must use literal \\N instead of a physical newline")
                output.append("Dialogue: " + ",".join(row.get(field_, "") for field_ in doc.event_fields))
        elif key == "[v4+ styles]":
            output.append("Format: " + ", ".join(doc.style_fields))
            output.extend("Style: " + ",".join(values) for style, values in doc.styles.items() if style in used)
        else:
            output.extend(line for line in lines if line.strip() and
                          (key in {"[fonts]", "[graphics]"} or not line.lstrip().startswith(";")))
        output.append("")
    return "\n".join(output)


def srt_to_ass(cue: Cue) -> Cue:
    text = SRT_BREAK.sub("\n", cue.text.replace("\r\n", "\n").replace("\r", "\n"))
    for tag in ("i", "b", "u", "s"):
        ass_tag = "s" if tag == "s" else tag
        text = re.sub(f"<{tag}>", lambda _: "{\\" + ass_tag + "1}", text, flags=re.I)
        text = re.sub(f"</{tag}>", lambda _: "{\\" + ass_tag + "0}", text, flags=re.I)
    if any(type_ == "tag" for type_, _ in pieces(text, "srt")):
        raise ValueError("SRT donor uses markup requiring explicit ASS adaptation")
    if "{" in cue.text or "}" in cue.text or "\\" in cue.text:
        raise ValueError("SRT donor contains ASS control syntax; adapt it explicitly")
    fields = dict(zip([f.strip().lower() for f in ASS_FIELDS.split(",")],
                      ["0", "", "", "Default", "", "0", "0", "0", "", ""]))
    start, end = effective_times(cue.start, cue.end, "ass")
    return Cue(cue.id, start, end, text.replace("\n", r"\N"), fields)
