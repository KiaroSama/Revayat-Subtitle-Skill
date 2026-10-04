"""Shared subtitle token boundaries, ASS overrides and logical bidi normalization."""

from __future__ import annotations

import re
import logging
import unicodedata
from html.parser import HTMLParser

RLM, LRM = "\u200f", "\u200e"
RLE, LRE, PDF = "\u202b", "\u202a", "\u202c"
BIDI = "\u061c\u200e\u200f\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069"
ARABIC = re.compile(r"[\u0620-\u063f\u0641-\u064a\u0660-\u0669\u066e-\u06d3\u06f0-\u06fc]")
SRT_BREAK = re.compile(r"</?(?=[^<>]{0,127}>)br/?(?: [^<>]*)?>", re.I | re.ASCII)
ASS_BLOCK = re.compile(r"\{[^}]*\}")
SRT_BLOCK = re.compile(SRT_BREAK.pattern + r"|<!--|</?(?:i|b|u|s|font)(?: [^<>]*)?>|\{\\an[1-9]\}", re.I | re.S | re.ASCII)
TAG_NAME = re.compile(r"(?:fscx|fscy|fsc|iclip|alpha|xbord|ybord|xshad|yshad|border|blur|bord|shad|move|fade|clip|frx|fry|frz|fr|be|fax|fay|pbo|pos|org|fad|fsp|fn|fs|fe|kf|ko|kt|an|[1-4][ac]|[biuskKqrptac])")


def srt_tag_fits(tag: str) -> bool:
    """FFmpeg scans at most 127 UTF-8 bytes after the optional closing slash."""
    return len(tag[2 if tag.startswith("</") else 1:-1].encode("utf-8")) <= 127


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
            name_start = slash + 1
            while name_start < end and block[name_start] in " \t":
                name_start += 1
            match = TAG_NAME.match(block, name_start, end)
            if not match:
                cursor = slash + 1
                continue
            name, begin = match[0], match.end()
            next_slash = block.find("\\", begin, end)
            argument_limit = end if next_slash < 0 else next_slash
            opening = block.find("(", begin, argument_limit)
            complex_argument = name in {"t", "clip", "iclip", "pos", "org", "move", "fad", "fade"}
            if opening >= 0:
                # Parenthesized values precede an ignored scalar prefix in libass.
                begin = opening
                level, finish = 1, begin + 1
                while finish < end and level:
                    level += (complex_argument and block[finish] == "(") - (block[finish] == ")")
                    finish += 1
                if level:
                    raise ValueError("Unbalanced ASS override parentheses")
                if complex_argument:
                    yield name, block[begin:finish], begin, finish
                else:
                    # libass accepts a parenthesized single scalar argument. Its
                    # delimiters are syntax, not part of a style/font name.
                    argument_start, argument_end = begin + 1, finish - 1
                    # Scalar consumers use the first nonempty argument, as the
                    # renderer does; leave ignored arguments in the source.
                    while argument_start < finish - 1:
                        comma = block.find(",", argument_start, finish - 1)
                        argument_end = finish - 1 if comma < 0 else comma
                        if block[argument_start:argument_end].strip(" \t") or comma < 0:
                            break
                        argument_start = comma + 1
                    if not block[argument_start:argument_end].strip(" \t"):
                        argument_start, argument_end = match.end(), opening
                    else:
                        # Parenthesized scalar arguments skip leading ASCII space.
                        while argument_start < argument_end and block[argument_start] in " \t":
                            argument_start += 1
                    while argument_end > argument_start and block[argument_end - 1] in " \t":
                        argument_end -= 1
                    yield name, block[argument_start:argument_end], argument_start, argument_end
                if name == "t" and nested:
                    nested_start = block.find("\\", begin + 1, finish - 1)
                    # libass ignores transforms with over three nonempty prefix
                    # arguments; commas after the first backslash belong to tags.
                    prefix = block[begin + 1:nested_start] if nested_start >= 0 else ""
                    if nested_start >= 0 and sum(bool(arg.strip(" \t")) for arg in prefix.split(",")[:-1]) <= 3:
                        yield from scan(nested_start, finish - 1, depth + 1)
            else:
                finish = block.find("\\", begin, end)
                if finish < 0:
                    finish = end
                argument_end = finish
                while argument_end > begin and block[argument_end - 1] in " \t":
                    argument_end -= 1
                yield name, block[begin:argument_end], begin, argument_end
            count += 1
            if count > 4096:
                raise ValueError("ASS override block exceeds token budget")
            cursor = finish
    yield from scan(1, len(block) - 1, 0)


def drawing_mode(block: str, drawing: bool) -> bool:
    for name, argument, _, _ in overrides(block):
        if name == "p":
            if not re.fullmatch(r"\+?[0-9]+", argument.strip(" \t")):
                raise ValueError("ASS drawing mode must be a nonnegative integer")
            drawing = bool(argument.strip(" \t").lstrip("+0"))
    return drawing


def block_spans(text: str, kind: str):
    """Yield disjoint markup spans without rescanning unmatched opening delimiters."""
    cursor, missing_comment_end, drawing = 0, False, False
    while cursor < len(text):
        if kind == "ass":
            start = text.find("{", cursor)
            if start < 0:
                break
            if not drawing and start and text[start - 1] == "\\":
                # libass consumes escaped braces as text, including after another
                # literal backslash. An odd/even escape-parity rule is incorrect.
                cursor = start + 1
                continue
            end = text.find("}", start + 1)
            if end < 0:
                break
            end += 1  # The first closing brace ends an ASS override/comment.
            drawing = drawing_mode(text[start:end], drawing)
        else:
            match = SRT_BLOCK.search(text, cursor)
            if match is None:
                break
            start, end = match.span()
            if match[0] == "<!--":
                closing = -1 if missing_comment_end else text.find("-->", end)
                if closing < 0:
                    missing_comment_end = True
                    cursor = end
                    continue  # Keep unterminated comment text, as before.
                end = closing + 3
            elif not srt_tag_fits(match[0]):
                cursor = end
                continue
        yield start, end
        cursor = end


def uncomment(text: str, kind: str) -> str:
    output, cursor, removed, drawing = [], 0, 0, False
    for start, end in block_spans(text, kind):
        block = text[start:end]
        output.append(text[cursor:start])
        hidden = block.startswith("<!--") if kind == "srt" else "\\" not in block
        if hidden:
            removed += 1
            if kind == "ass" and drawing:
                output.append("{}")  # Keep the renderer's separate drawing-object boundary.
        else:
            output.append(block)
        if kind == "ass":
            drawing = drawing_mode(block, drawing)
        cursor = end
    output.append(text[cursor:])
    if removed:
        logging.getLogger(__name__).debug("Removed hidden comments format=%s count=%d", kind, removed)
    return "".join(output)


def pieces(text: str, kind: str):
    drawing, cursor = False, 0
    for start, end in block_spans(text, kind):
        if start > cursor:
            yield "drawing" if drawing else "text", text[cursor:start]
        block = text[start:end]
        cursor = end
        if (kind == "ass" and "\\" not in block) or (kind == "srt" and block.startswith("<!--")):
            if kind == "ass" and drawing:
                yield "tag", "{}"
            continue
        if kind == "ass":
            drawing = drawing_mode(block, drawing)
        if kind == "srt" and SRT_BREAK.fullmatch(block):
            yield "text", "\n"
        else:
            yield "tag", block
    if cursor < len(text):
        yield "drawing" if drawing else "text", text[cursor:]


def remap_resets(text: str, mapping: dict[str, str]) -> str:
    output, cursor = [], 0
    for begin, finish in block_spans(text, "ass"):
        block = text[begin:finish]
        replacements = []
        for name, argument, start, end in overrides(block):
            if name != "r" or not argument:
                continue
            if argument not in mapping:
                raise ValueError("Donor reset references an undefined style")
            replacements.append((start, end, mapping[argument]))
        for start, end, value in reversed(replacements):
            block = block[:start] + value + block[end:]
        output.extend((text[cursor:begin], block))
        cursor = finish
    output.append(text[cursor:])
    return "".join(output)


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


def _text_runs(tokens):
    """Coalesce adjacent prose without reparsing or joining across real controls."""
    pending = []
    for type_, value in tokens:
        if type_ == "text":
            pending.append(value)
            continue
        if pending:
            yield "text", "".join(pending)
            pending.clear()
        yield type_, value
    if pending:
        yield "text", "".join(pending)


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
                # A wrapper cannot end inside an isolate opened by the source
                # on another display line. Preserve crossing source scopes and
                # use direction marks alone on those lines.
                self_contained = True
                try:
                    validate_bidi("".join(value for _, value in line), kind)
                except ValueError:
                    self_contained = False
                embedding = (RLE if direction == "rtl" else LRE) if mixed and self_contained else ""
                line[first] = ("text", embedding + mark + line[first][1])
                line[last] = ("text", line[last][1] + mark + (PDF if embedding else ""))
        output.append("".join(value for _, value in line))
        line.clear()
    for type_, value in _text_runs(pieces(text, kind)):
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
