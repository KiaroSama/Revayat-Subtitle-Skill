"""Strict ASS/SRT parsing and presentation-preserving subtitle transformations."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

RLM = "\u200f"
RLE, PDF = "\u202b", "\u202c"
BIDI = "\u061c\u200e\u200f\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069"
ARABIC = re.compile(r"[\u0620-\u063f\u0641-\u064a\u066e-\u06d3\u06fa-\u06fc]")
BLOCK = re.compile(r"(\{[^{}]*\}|<[^>]*>)")
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
    pattern = r"(\d+):(\d{2}):(\d{2})\.(\d{2})" if kind == "ass" else r"(\d+):(\d{2}):(\d{2}),(\d{3})"
    match = re.fullmatch(pattern, value.strip())
    if not match:
        raise ValueError("Invalid subtitle timestamp")
    hours, minutes, seconds, fraction = map(int, match.groups())
    if minutes > 59 or seconds > 59:
        raise ValueError("Subtitle minutes and seconds must be below 60")
    return ((hours * 60 + minutes) * 60 + seconds) * 1000 + fraction * (10 if kind == "ass" else 1)


def timecode(ms: int, kind: str) -> str:
    hours, ms = divmod(ms, 3600000)
    minutes, ms = divmod(ms, 60000)
    seconds, fraction = divmod(ms, 1000)
    if kind == "ass":
        return f"{hours}:{minutes:02}:{seconds:02}.{fraction // 10:02}"
    return f"{hours:02}:{minutes:02}:{seconds:02},{fraction:03}"


def parse(data: str, kind: str) -> Document:
    data = data.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    if "\x00" in data or "\ufffd" in data:
        raise ValueError("NUL or replacement characters in subtitles; check the input encoding")
    doc = Document(kind, [])
    if kind == "srt":
        for block in re.split(r"\n[ \t]*\n", data.strip()):
            if not block.strip():
                continue
            lines = block.split("\n")
            if len(lines) < 2 or not lines[0].strip().isdigit():
                raise ValueError("Malformed SRT cue; expected a number and timestamp line")
            times = lines[1].split(" --> ")
            if len(times) != 2:
                raise ValueError("Malformed SRT timestamp line")
            start, end = (timestamp(t, kind) for t in times)
            doc.cues.append(Cue(f"c{len(doc.cues) + 1:06}", start, end, "\n".join(lines[2:])))
    elif kind == "ass":
        section, lines = "", []
        known_sections = {"[script info]", "[v4+ styles]", "[v4 styles]", "[events]", "[fonts]",
                          "[graphics]", "[aegisub project garbage]", "[aegisub extradata]"}
        for line in data.split("\n"):
            stripped = line.strip()
            header = stripped.startswith("[") and stripped.endswith("]")
            if section.casefold() in {"[fonts]", "[graphics]"}:
                header = header and stripped.casefold() in known_sections
            if header:
                section = stripped
                if any(name.casefold() == section.casefold() for name, _ in doc.sections):
                    raise ValueError("Repeated ASS sections are not supported")
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


def uncomment(text: str, kind: str) -> str:
    if kind == "srt":
        return re.sub(r"<!--.*?-->", "", text, flags=re.S)
    return re.sub(r"\{[^{}]*\}", lambda m: m[0] if "\\" in m[0] else "", text)


def pieces(text: str, kind: str):
    """Classify prose separately from tags and vector drawing payloads."""
    drawing = False
    pattern = BLOCK if kind == "srt" else re.compile(r"(\{[^{}]*\})")
    for piece in pattern.split(uncomment(text, kind)):
        if not piece:
            continue
        tag = piece.startswith("{") if kind == "ass" else bool(BLOCK.fullmatch(piece))
        if tag:
            if kind == "ass":
                for match in re.finditer(r"\\p(\d+)(?!\d)|\\r(?:[^\\}]*)", piece):
                    drawing = bool(int(match[1])) if match[1] is not None else False
            yield "tag", piece
        else:
            yield "drawing" if drawing else "text", piece


def visible(text: str, kind: str) -> str:
    result = "".join(value for type_, value in pieces(text, kind) if type_ == "text")
    if kind == "ass":
        result = result.replace(r"\N", "\n").replace(r"\n", " ").replace(r"\h", " ")
    return result.translate(str.maketrans("", "", BIDI)).strip()


def structure(text: str, kind: str) -> list[tuple[str, str]]:
    return [(type_, value) for type_, value in pieces(text, kind) if type_ != "text"]


def has_drawing(text: str, kind: str) -> bool:
    return any(type_ == "drawing" and value.strip() for type_, value in pieces(text, kind))


def rtl(text: str, kind: str) -> str:
    """Add real RLM characters to logical Persian lines, never to drawing payloads."""
    output, line = [], []

    def flush():
        prose = [i for i, (type_, value) in enumerate(line) if type_ == "text" and value.strip()]
        if prose and any(ARABIC.search(line[i][1]) for i in prose):
            first, last = prose[0], prose[-1]
            mixed = any(re.search(r"[A-Za-z0-9]", line[i][1]) for i in prose)
            if mixed and line[first][1].startswith(RLE) and line[last][1].endswith(PDF):
                line[first] = ("text", line[first][1][1:])
                line[last] = ("text", line[last][1][:-1])
            # Legacy ASS layout otherwise splits mixed prose into wrongly ordered
            # runs even with RLM. RLE preserves Latin order without reversing text.
            line[first] = ("text", (RLE if mixed else "") + RLM + line[first][1])
            line[last] = ("text", line[last][1] + RLM + (PDF if mixed else ""))
        output.append("".join(value for _, value in line))
        line.clear()

    for type_, value in pieces(text, kind):
        if type_ != "text":
            line.append((type_, value))
            continue
        # Keep isolates supplied after visual review; replace only boundary RLMs.
        value = value.replace(RLM, "")
        for part in re.split(r"(\\N|\\n)" if kind == "ass" else r"(\n)", value):
            if part in {r"\N", r"\n", "\n"}:
                flush()
                output.append(part)
            else:
                line.append(("text", part))
    flush()
    return "".join(output)


def style_references(cue: Cue) -> set[str]:
    refs = {cue.fields.get("style", "Default").strip()}
    for type_, value in pieces(cue.text, "ass"):
        if type_ == "tag":
            refs.update(match.strip() for match in re.findall(r"\\r([^\\}]+)", value))
    return refs


def serialize(doc: Document, cues: list[Cue], remove_fonts: bool = False) -> str:
    cues = sorted(cues, key=lambda cue: cue.start)  # Ties retain their source/layer order.
    if not cues:
        raise ValueError("Refusing an episode with no retained cues")
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
    text = cue.text
    for tag in ("i", "b", "u", "s"):
        ass_tag = "s" if tag == "s" else tag
        text = re.sub(f"<{tag}>", lambda _: "{\\" + ass_tag + "1}", text, flags=re.I)
        text = re.sub(f"</{tag}>", lambda _: "{\\" + ass_tag + "0}", text, flags=re.I)
    if re.search(r"<[^>]*>", text):
        raise ValueError("SRT donor uses markup requiring explicit ASS adaptation")
    if "{" in cue.text or "}" in cue.text or "\\" in cue.text:
        raise ValueError("SRT donor contains ASS control syntax; adapt it explicitly")
    fields = dict(zip([f.strip().lower() for f in ASS_FIELDS.split(",")],
                      ["0", "", "", "Default", "", "0", "0", "0", "", ""]))
    return Cue(cue.id, cue.start, cue.end, text.replace("\n", r"\N"), fields)
