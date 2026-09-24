"""Shared subtitle token boundaries, ASS overrides and logical bidi normalization."""

from __future__ import annotations

import re
import unicodedata
from html.parser import HTMLParser

RLM, LRM = "\u200f", "\u200e"
RLE, LRE, PDF = "\u202b", "\u202a", "\u202c"
BIDI = "\u061c\u200e\u200f\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069"
ARABIC = re.compile(r"[\u0620-\u063f\u0641-\u064a\u0660-\u0669\u066e-\u06d3\u06f0-\u06fc]")
ASS_BLOCK = re.compile(r"\{[^{}]*\}")
SRT_BLOCK = re.compile(r"<!--.*?-->|</?(?:i|b|u|s|font)(?:\s+[^<>]*)?\s*>|\{\\an[1-9]\}", re.I | re.S)
TAG_NAME = re.compile(r"(?:fscx|fscy|iclip|alpha|xbord|ybord|xshad|yshad|border|blur|bord|shad|move|fade|clip|frx|fry|frz|fax|fay|pbo|pos|org|fad|fsp|fn|fs|fe|kf|ko|kt|an|[1-4][ac]|[biuskKqrptac])")


def has_rtl(text: str) -> bool:
    return bool(ARABIC.search(text)) or any(char not in BIDI and unicodedata.bidirectional(char) in {"R", "AL", "AN"} for char in text)


def has_ltr(text: str) -> bool:
    return any(char not in BIDI and unicodedata.bidirectional(char) in {"L", "EN"} for char in text)


def srt_font_names(tag: str) -> set[str]:
    class FontTag(HTMLParser):
        def handle_starttag(self, name, attributes):
            if name == "font":
                names.update(value.strip() for key, value in attributes if key == "face" and value and value.strip())
    names = set()
    FontTag(convert_charrefs=False).feed(tag)
    return names


def overrides(block: str, *, nested: bool = True):
    """Yield (name, argument, argument-start, argument-end), including nested transforms."""
    if not ASS_BLOCK.fullmatch(block):
        return
    def scan(start, end, depth):
        if depth > 16:
            raise ValueError("ASS transform nesting exceeds 16 levels")
        cursor, count = start, 0
        while cursor < end:
            slash = block.find("\\", cursor, end)
            if slash < 0:
                break
            match = TAG_NAME.match(block, slash + 1, end)
            if not match:
                cursor = slash + 1
                continue
            name, begin = match[0], match.end()
            if begin < end and block[begin] == "(":
                level, finish = 1, begin + 1
                while finish < end and level:
                    level += (block[finish] == "(") - (block[finish] == ")")
                    finish += 1
                if level:
                    raise ValueError("Unbalanced ASS override parentheses")
                yield name, block[begin:finish], begin, finish
                if name == "t" and nested:
                    yield from scan(begin + 1, finish - 1, depth + 1)
            else:
                finish = block.find("\\", begin, end)
                if finish < 0:
                    finish = end
                yield name, block[begin:finish], begin, finish
            count += 1
            if count > 4096:
                raise ValueError("ASS override block exceeds token budget")
            cursor = finish
    yield from scan(1, len(block) - 1, 0)


def uncomment(text: str, kind: str) -> str:
    if kind == "srt":
        return re.sub(r"<!--.*?-->", "", text, flags=re.S)
    return ASS_BLOCK.sub(lambda m: m[0] if "\\" in m[0] else "", text)


def pieces(text: str, kind: str):
    drawing, cursor = False, 0
    text = uncomment(text, kind)
    pattern = ASS_BLOCK if kind == "ass" else SRT_BLOCK
    for match in pattern.finditer(text):
        if match.start() > cursor:
            yield "drawing" if drawing else "text", text[cursor:match.start()]
        block = match[0]
        if kind == "ass":
            for name, argument, _, _ in overrides(block, nested=False):
                if name == "p":
                    if not re.fullmatch(r"[0-9]+", argument.strip()):
                        raise ValueError("ASS drawing mode must be a nonnegative integer")
                    drawing = int(argument.strip()) > 0
                elif name == "r":
                    drawing = False
        yield "tag", block
        cursor = match.end()
    if cursor < len(text):
        yield "drawing" if drawing else "text", text[cursor:]


def remap_resets(text: str, mapping: dict[str, str]) -> str:
    def remap(match):
        block = match[0]
        replacements = []
        for name, argument, start, end in overrides(block):
            if name != "r" or not argument.strip():
                continue
            if argument.strip() not in mapping:
                raise ValueError("Donor reset references an undefined style")
            replacements.append((start, end, mapping[argument.strip()]))
        for start, end, value in reversed(replacements):
            block = block[:start] + value + block[end:]
        return block
    return ASS_BLOCK.sub(remap, text)


def validate_bidi(text: str, kind: str) -> None:
    stack = []
    for type_, value in pieces(text, kind):
        if type_ != "text":
            continue
        for char in value:
            if char in "\u202a\u202b\u202d\u202e":
                stack.append("embedding")
            elif char in "\u2066\u2067\u2068":
                stack.append("isolate")
            elif char == PDF:
                if not stack or stack[-1] != "embedding":
                    raise ValueError("Unbalanced bidi embedding terminator")
                stack.pop()
            elif char == "\u2069":
                while stack and stack[-1] == "embedding":
                    stack.pop()
                if not stack:
                    raise ValueError("Unbalanced bidi isolate terminator")
                stack.pop()
            if len(stack) > 125:
                raise ValueError("Bidi nesting exceeds the supported limit")
    if stack:
        raise ValueError("Unclosed bidi embedding or isolate")


def rtl(text: str, kind: str, direction: str = "rtl", *, force: bool = False) -> str:
    if direction not in {"ltr", "rtl"}:
        raise ValueError("Paragraph direction must be ltr or rtl")
    validate_bidi(text, kind)
    output, line = [], []
    def flush():
        indices = [i for i, (type_, value) in enumerate(line) if type_ == "text" and value.strip()]
        if indices and (force or any(has_rtl(line[i][1]) for i in indices)):
            first, last = indices[0], indices[-1]
            # Recognize only paired boundary wrappers produced by this normalizer.
            for opening, mark, closing in ((RLE, RLM, PDF), (LRE, LRM, PDF), ("", RLM, ""), ("", LRM, "")):
                prefix, suffix = opening + mark, mark + closing
                a, b = line[first][1], line[last][1]
                if a.startswith(prefix) and b.endswith(suffix) and (first != last or len(a) >= len(prefix + suffix)):
                    line[first] = ("text", a[len(prefix):])
                    line[last] = ("text", line[last][1][:-len(suffix)])
                    break
            if force or any(has_rtl(line[i][1]) for i in indices):
                mixed = any(has_ltr(line[i][1]) for i in indices)
                mark = RLM if direction == "rtl" else LRM
                embedding = (RLE if direction == "rtl" else LRE) if mixed else ""
                line[first] = ("text", embedding + mark + line[first][1])
                line[last] = ("text", line[last][1] + mark + (PDF if embedding else ""))
        output.append("".join(value for _, value in line))
        line.clear()
    for type_, value in pieces(text, kind):
        if type_ != "text":
            line.append((type_, value))
            continue
        for part in re.split(r"(\\N|\\n)" if kind == "ass" else r"(\n)", value):
            if part in {r"\N", r"\n", "\n"}:
                flush()
                output.append(part)
            else:
                line.append(("text", part))
    flush()
    result = "".join(output)
    validate_bidi(result, kind)
    return result


def clean_empty_lines(text: str, kind: str, preserve: bool = False) -> str:
    tokens = list(pieces(text, kind))
    if preserve or any(type_ == "drawing" for type_, _ in tokens):
        return text
    if kind == "ass" and any(name in {"k", "K", "kf", "ko", "kt"}
                             for type_, value in tokens if type_ == "tag"
                             for name, _, _, _ in overrides(value)):
        return text
    delimiter = r"\N" if kind == "ass" else "\n"
    lines, current = [], []
    for type_, value in tokens:
        parts = value.split(delimiter) if type_ == "text" else [value]
        for index, part in enumerate(parts):
            if index:
                lines.append(current)
                current = []
            current.append((type_, part))
    lines.append(current)
    retained, pending = [], ""
    for line in lines:
        prose = "".join(value for type_, value in line if type_ == "text")
        if kind == "ass":
            prose = prose.replace(r"\n", " ").replace(r"\h", " ")
        if prose.translate(str.maketrans("", "", BIDI)).strip():
            retained.append(pending + "".join(value for _, value in line))
            pending = ""
        else:
            pending += "".join(value if type_ == "tag" else "".join(char for char in value if char in BIDI)
                               for type_, value in line)
    return delimiter.join(retained) + pending


def paragraph_direction(language: str) -> str:
    parts = language.lower().split("-")
    if "latn" in parts:
        return "ltr"
    return "rtl" if parts[0] in {"fa", "fas", "pes", "prs", "ar", "ara", "he", "heb", "ur", "urd", "ps", "pus", "dv", "div", "yi", "yid", "ckb", "sd", "ug"} or any(p in {"arab", "hebr"} for p in parts) else "ltr"
