"""Plain-text subtitle transformations with an optional native backend."""

from __future__ import annotations

import os
import re
from collections.abc import Callable
from typing import Any
import unicodedata

from markup import BIDI, PDF, RLE, RLM, SRT_BREAK, drawing_mode, srt_tag_fits, validate_bidi

try:
    import subcore_rs as _subcore_rs
except ImportError:
    _subcore_rs = None

ZWNJ = "‌"
TERMINAL = ".،؟!…:؛»)]\"'-"
_INVISIBLE = str.maketrans("", "", BIDI + ZWNJ)
_LETTERS = str.maketrans("يكى", "یکی")
_FA = r"[ء-غف-يٮ-ۓە]"
_WORD = rf"{_FA}(?:{_FA}|{ZWNJ})*"
_ASS = r"(?<!\\)\{[^{}]*\}"
_HTML = r"<!--[\s\S]*?-->|</?[A-Za-z][^<>]*>"
_LINE = re.compile(r"(\\N|\r\n|\n|\r)")
_URL = r"(?:(?:[a-z][a-z0-9+.-]*://|www\.|t\.me/)[^\s<>{}\\]+|\b(?:[a-z0-9-]+\.)+[a-z]{2,}/[^\s<>{}\\]*)"
_PROTECTED = re.compile(rf"({_ASS}|{_HTML}|\\[Nnh]|{_URL}|[\w.+-]+@[\w.-]+\.[a-z]{{2,}})", re.I | re.ASCII)
_TAGS = re.compile(rf"({_ASS}|{_HTML})")
SRT_ALIGNMENT = re.compile(r"(?<!\\)\{\\?an[1-9]\}", re.I | re.ASCII)
SRT_COLOR = re.compile(r'''<font +color *= *(?:"#([0-9a-f]{6})"|'#([0-9a-f]{6})'|#([0-9a-f]{6})) *>''', re.I | re.ASCII)
_SIMPLE_MARKUP = re.compile(r"</?[bisu]>|</font>", re.I | re.ASCII)

# shortcut: common verb stems distinguish verbal prefixes from prose; use a lexicon for rarer verbs.
_STEMS = (
    "رو روند آی آو آور بر بد باش شو ش کن تون توان دان دون گو گ گیر نگیر بین شن شنو خند گری گریه خور نوش "
    "خواه خواهم خوای خورد خواب خوان خون کش فروش خر گذار ذار ساز سوز مان مون مر میر دار ده د دونیم "
    "زن زد افت ایست ایس نشین شین پر ریز پرس فهم فهمید ترس رس رون ران تاب باز دهش رقص جنگ چرخ پیچ "
    "بخش پذیر فرست جوی کوش جوش سپار شمار شمارش لرز یاب بند چسب گشای پوش مال بوس لیس تراش کار "
    "رفت آمد شد کرد گفت دید زد داد برد گرفت داشت گذاشت نوشت نویس نشد گشت بود خورد شنید آورد پاش پاشید"
).split()
_VERB = re.compile(rf"(?<![\w{ZWNJ}])(ن?می) +((?:{'|'.join(_STEMS)})(?:یم|ید|ند|یم|ین|ن|م|ی|د|ه)?)(?![\w{ZWNJ}])")
_SUFFIX = re.compile(rf"(?<![\w{ZWNJ}])({_WORD}) +((?:ها(?:ی(?:مان|تان|شان|م|ت|ش|ی)?)?|ترین|تر))(?![\w{ZWNJ}])")
_ENCLITIC = re.compile(rf"(?<![\w{ZWNJ}])({_WORD}ه) +(ام|ات|اش|ایم|اید|اند|ای)(?![\w{ZWNJ}])")
_COMPOUNDS = {"هم چنین": "همچنین", "بی شک": "بی‌شک", "هیچ وقت": "هیچ‌وقت", "یک دیگر": "یکدیگر", "به جای": "به‌جای"}
_COMPOUND = re.compile(rf"(?<![\w{ZWNJ}])({'|'.join(_COMPOUNDS)})(?![\w{ZWNJ}])")
_FUNCTION_WORDS = frozenset("و یا که تا از در بر به با را هم نه اما ولی من تو او ما شما ایشان آنها اینها خود".split())


def _native(name: str) -> Callable[..., Any] | None:
    return None if os.environ.get("REVAYAT_DISABLE_RUST") == "1" else getattr(_subcore_rs, name, None)


def _letters(text: str) -> str:
    return re.sub(rf"(?<={_FA})ة|ة(?={_FA})", "ه", text.translate(_LETTERS))


def normalize_persian_zwnj(text: str) -> str:
    """Normalize Persian prose without rewriting tags, hard breaks, or links."""
    if native := _native("normalize_persian_zwnj"):
        return native(text)

    def attach_verb(match: re.Match[str]) -> str:
        if match[2] == "روی" and re.match(rf" +{_FA}", match.string[match.end():]):
            return match[0]
        return match[1] + ZWNJ + match[2]

    def attach_suffix(match: re.Match[str]) -> str:
        word, suffix = match.groups()
        if word in _FUNCTION_WORDS or (suffix.startswith("تر") and len(word) < 2):
            return match[0]
        return word + ZWNJ + suffix

    parts = _PROTECTED.split(text)
    for i in range(0, len(parts), 2):
        prose = _VERB.sub(attach_verb, _letters(parts[i]))
        prose = _SUFFIX.sub(attach_suffix, prose)
        prose = _ENCLITIC.sub(r"\1" + ZWNJ + r"\2", prose)
        parts[i] = _COMPOUND.sub(lambda match: _COMPOUNDS[match[0]], prose)
    return "".join(parts)


_CREDIT = re.compile(
    r"(?<!\w)(?:زیرنویس از|ترجمه و تنظیم|تنظیم و ترجمه|مترجم\s*:|تقدیم به نگاه|"
    r"sync(?:ed)? and corrected by|subtitles by|captioned by|encoded by|downloaded from|"
    r"support us and become vip|advertise your product)(?!\w)"
)
_STUDIO = re.compile(
    r"(?<!\w)(?:deluxe media|sous[- ]titres?\s*:|sous[- ]titrage\s*:|supervision créative|"
    r"vertaling\s*:|creatief supervisor)(?!\w)|日本語字幕"
)
_SITE = re.compile(
    r"(?<!\w)(?:addic7ed|opensubtitles|subscene|yify|yts|shine021|farsisubtitle|worldsubtitle|subf2m|"
    r"کانال تلگرام|ساب ?تایتل|فارس ?ساب|موویکو|دیجی ?موویز|زارفیل|سافت ?ساب)(?!\w)"
)
_LINK = re.compile(r"(?<![\w@])(?:https?://|www\.|t\.me/)[^\s]+")


def watermark_reason(text: str) -> str:
    """Return a reason only when a cue contains a credit or ad marker."""
    if native := _native("watermark_reason"):
        return native(text)
    plain = _TAGS.sub(lambda m: " " if SRT_BREAK.fullmatch(m[0]) else "", text)
    plain = re.sub(r"\\[Nnh]", " ", plain).translate(str.maketrans("", "", BIDI)).replace(ZWNJ, " ")
    plain = " ".join(_letters(plain).casefold().split())
    if _STUDIO.search(plain):
        return "studio-credit"
    if _CREDIT.search(plain):
        return "fansub-credit"
    if _SITE.search(plain) or plain.strip(" .!«»[]():-•") in {"اپلیکیشن", "وب سایت", "وبسایت"}:
        return "site-watermark"
    if re.fullmatch(r"[\[({]?\s*@[a-z0-9_]+\s*[\])}]?", plain) or re.search(r"(?:https?://)?\bt\.me/[^\s]+", plain):
        return "telegram-handle"
    return "url-ad" if _LINK.search(plain) else ""


def is_watermark_cue(text: str) -> bool:
    if native := _native("is_watermark_cue"):
        return native(text)
    return bool(watermark_reason(text))


def convert_srt_markup_to_ass(text: str) -> str:
    """Map supported SRT effects into inline ASS overrides."""
    if native := _native("convert_srt_markup_to_ass"):
        return native(text)
    parts = _TAGS.split(text.replace("\r\n", "\n").replace("\r", "\n"))
    colors: list[str] = []
    for i, part in enumerate(parts):
        if i % 2 == 0:
            parts[i] = part.replace("\n", r"\N")
        elif SRT_ALIGNMENT.fullmatch(part):
            parts[i] = "{\\an" + part[-2] + "}"
        elif not part.startswith("<") or not srt_tag_fits(part):
            continue
        elif SRT_BREAK.fullmatch(part):
            parts[i] = r"\N"
        elif color := SRT_COLOR.fullmatch(part):
            rgb = next(value for value in color.groups() if value).upper()
            colors.append("&H" + rgb[4:] + rgb[2:4] + rgb[:2] + "&")
            parts[i] = r"{\c" + colors[-1] + "}"
        elif _SIMPLE_MARKUP.fullmatch(part):
            if part.lower() == "</font>":
                if colors:
                    colors.pop()
                parts[i] = r"{\c" + (colors[-1] if colors else "") + "}"
            else:
                closing = part.startswith("</")
                parts[i] = "{\\" + part[-2].lower() + ("0" if closing else "1") + "}"
    return "".join(parts)


def _lines(text: str, kind: str) -> tuple[list[list[tuple[str, bool]]], list[str]]:
    lines: list[list[tuple[str, bool]]] = [[]]
    breaks: list[str] = []
    drawing = False
    for i, part in enumerate(_TAGS.split(text)):
        if i % 2:
            if kind == "ass" and part.startswith("{"):
                drawing = drawing_mode(part, drawing)
            if kind == "srt" and SRT_BREAK.fullmatch(part) and srt_tag_fits(part):
                breaks.append(part)
                lines.append([])
                continue
            # In SRT, braces are only nonprinting when they are alignment tags.
            lines[-1].append((part, kind == "srt" and part.startswith("{") and not SRT_ALIGNMENT.fullmatch(part)))
            continue
        for j, piece in enumerate(_LINE.split(part) if kind == "ass" else re.split(r"(\r\n|\n|\r)", part)):
            if j % 2:
                breaks.append(piece)
                lines.append([])
            elif piece:
                lines[-1].append((piece, not drawing))
    return lines, breaks


def _visible_line(line: list[tuple[str, bool]], kind: str) -> str:
    text = "".join(part for part, visible in line if visible).translate(_INVISIBLE)
    return text.replace(r"\n", " ").replace(r"\h", " ").replace(r"\{", "{").replace(r"\}", "}") if kind == "ass" else text


def _wrap_group(lines: list[list[tuple[str, bool]]], width: int, limit: int, kind: str) -> str:
    merged: list[tuple[str, bool]] = []
    for line in lines:
        if merged and line:
            merged.append((" ", True))
        merged.extend(line)
    raw = "".join(part for part, _ in merged)
    width_total = len(_visible_line(merged, kind))
    if width_total <= width or limit <= 1:
        return raw

    visible_before, offset = 0, 0
    candidates: list[tuple[int, int, int, bool]] = []
    boundary = re.compile(r"(?:،|\b(?:و|که|اما|ولی))$")
    tail = ""
    for part, visible in merged:
        if visible:
            cursor = 0
            for space in re.finditer(r" +", part):
                chunk = _visible_line([(part[cursor:space.start()], True)], kind)
                visible_before += len(chunk)
                tail = (tail + chunk)[-5:]
                if 0 < visible_before < width_total - len(space[0]):
                    candidates.append((offset + space.start(), offset + space.end(), visible_before, bool(boundary.search(tail))))
                visible_before += len(space[0])
                tail = " "
                cursor = space.end()
            chunk = _visible_line([(part[cursor:], True)], kind)
            visible_before += len(chunk)
            tail = (tail + chunk)[-5:]
        offset += len(part)

    count = min(limit, max(2, (width_total + 1 + width) // (width + 1)))
    result = []
    start = consumed = 0
    while candidates and count > 1:
        remaining = width_total - consumed
        target = (remaining - 1) / count

        def score(candidate: tuple[int, int, int, bool]) -> tuple[int, float, int]:
            left, right, at, pause = candidate
            size = at - consumed
            overflow = max(0, size - width, remaining - size - (right - left) - width * (count - 1) - (count - 2))
            return overflow, abs(size - target) - (2 if pause else 0), left

        left, right, at, _ = min(candidates, key=score)
        result.append(raw[start:left])
        start, consumed = right, at + right - left
        candidates = [item for item in candidates if item[0] > right]
        count -= 1
    delimiter = r"\N" if kind == "ass" else "\n"
    return delimiter.join([*result, raw[start:]]) if result else raw


def _wrap(text: str, max_chars: int, max_lines: int, kind: str) -> str:
    if max_chars < 1 or max_lines < 1:
        raise ValueError("Line wrapping requires positive max_chars and max_lines")
    if kind == "ass" and any(drawing_mode(block, False) for block in re.findall(_ASS, text)):
        return text
    lines, breaks = _lines(text, kind)
    visible = [_visible_line(line, kind).strip() for line in lines]
    if all(len(line) <= max_chars for line in visible):
        return text
    # A new speaker is a hard boundary even when an adjacent line wraps.
    starts = [0, *(i for i, line in enumerate(visible[1:], 1) if line.startswith(("- ", "– "))), len(lines)]
    output = []
    for begin, end in zip(starts, starts[1:]):
        if begin:
            output.append(breaks[begin - 1])
        if all(len(line) <= max_chars for line in visible[begin:end]):
            output.append("".join("".join(p for p, _ in lines[i]) + (breaks[i] if i + 1 < end else "")
                                  for i in range(begin, end)))
        else:
            output.append(_wrap_group(lines[begin:end], max_chars, max(1, max_lines - len(starts) + 2), kind))
    return "".join(output)


def wrap_subtitle_lines(text: str, max_chars: int = 42, max_lines: int = 2) -> str:
    """Rebalance only overlong cues, preserving ASS overrides and speakers."""
    if native := _native("wrap_subtitle_lines"):
        return native(text, max_chars, max_lines)
    return _wrap(text, max_chars, max_lines, "ass")


def _balanced(text: str, kind: str) -> bool:
    try:
        validate_bidi(text, kind)
    except ValueError:
        return False
    return True


def anchor_rtl_line(text: str, kind: str = "ass", direction: str = "rtl") -> str:
    """Anchor neutral terminal punctuation inside each RTL display line."""
    if native := _native("anchor_rtl_line"):
        return native(text, kind, direction)
    if direction != "rtl":
        return text
    lines, breaks = _lines(text, kind)
    whole_balanced = _balanced(text, kind)
    output: list[str] = []
    for idx, line in enumerate(lines):
        original = "".join(part for part, _ in line)
        prose = _visible_line(line, kind)
        letters = any(char.isalpha() and unicodedata.bidirectional(char) in {"R", "AL"} for char in prose)
        line_balanced = _balanced(original, kind) if letters else True
        if letters and (line_balanced or not whole_balanced):
            indices = [i for i, (part, vis) in enumerate(line) if vis and part.strip()]
            first, last = indices[0], indices[-1]
            parts = [part for part, _ in line]
            # Preserve author-supplied interior scopes; remove only boundary noise or our previous wrapper.
            opening = parts[first].startswith(RLE)
            closing = parts[last].rstrip().endswith(PDF)
            if opening and closing and _balanced(original, kind):
                a, b = parts[first], parts[last]
                parts[first] = a[1:]
                stripped = parts[last].rstrip()
                parts[last] = stripped[:-1] + parts[last][len(stripped):]
                if not _balanced("".join(parts), kind):
                    parts[first], parts[last] = a, b
            noise = BIDI if not line_balanced else RLM + "‎؜"
            parts[first] = parts[first].lstrip(noise)
            final = parts[last].rstrip()
            parts[last] = final.rstrip(noise) + parts[last][len(final):]
            if parts[first].lstrip().startswith("- "):
                at = len(parts[first]) - len(parts[first].lstrip())
                parts[first] = parts[first][:at] + RLM + parts[first][at:]
            final = parts[last].rstrip()
            if final and final[-1] in TERMINAL:
                parts[last] = final + RLM + parts[last][len(final):]
            parts[first] = RLE + parts[first]
            parts[last] += PDF
            candidate = "".join(parts)
            if _balanced(candidate, kind):
                original = candidate
        output.append(original)
        if idx < len(breaks):
            output.append(breaks[idx])
    return "".join(output)


def process_cue_text(text: str, kind: str = "ass", direction: str = "rtl", max_chars: int = 42,
                     fix_zwnj: bool = True, wrap_lines: bool = True) -> str:
    """Apply conversion, Persian normalization, wrapping, and anchoring in order."""
    if native := _native("process_cue_text"):
        return native(text, kind, direction, max_chars, fix_zwnj, wrap_lines)
    if kind == "ass":
        text = convert_srt_markup_to_ass(text)
    if direction == "rtl" and fix_zwnj:
        text = normalize_persian_zwnj(text)
    if wrap_lines:
        text = wrap_subtitle_lines(text, max_chars) if kind == "ass" else _wrap(text, max_chars, 2, kind)
    return anchor_rtl_line(text, kind, direction)
