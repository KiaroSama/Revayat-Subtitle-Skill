# Persian typography and rendering

Use this for every Persian editing pass and whenever an image shows direction,
punctuation, joining, font or layout problems.

## Logical order before display order

Store ordinary Unicode Persian in reading order. Do not reverse strings, sort
words by script, paste Arabic presentation forms, or apply a visual reshaper to
stored subtitle text. libass performs glyph shaping and visual ordering.

```text
Broken:   رو دیدم. OVA من دیروز
Correct:  من دیروز OVA رو دیدم.
```

Fix the sentence itself when the source was stored in visual order. A direction
mark cannot repair word order. The same applies to Latin names, acronyms, URLs,
dates, measurements and numbers; preserve a meaningful Latin literal.

The builder inserts actual U+200F RIGHT-TO-LEFT MARK at Persian display-line
boundaries, outside ASS commands and drawing payloads. The six literal characters
`\u200f` are not a mark. Keep meaningful U+200C ZWNJ in Persian words.

For mixed Persian/Latin or Persian/number lines it also uses balanced U+202B RLE
and U+202C PDF. RLM alone left word groups in the wrong order in actual libass
renders. This embeds direction without reversing the logical string. Preserve
valid isolates, including scopes crossing display lines; those lines receive marks
without an extra embedding wrapper. Repair accidental overrides only in editable prose.

When a Persian display line starts with a Latin or English word, an acronym, or a
number-plus-Latin token, put Right-to-Left Isolate (`RLI`, `U+2067`) before the
line's readable text and Pop Directional Isolate (`PDI`, `U+2069`) after the
last character, including any trailing punctuation. If the renderer infers
paragraph direction and needs an RTL base, put Right-to-Left Mark (`RLM`,
`U+200F`) immediately before `RLI`, outside the isolate.
Use the actual Unicode characters outside markup. Check word order and trailing
punctuation in the target renderer (libass, VLC, mpv, or the web player). The
punctuation should stay with the sentence; the left edge can be correct for RTL.

For a difficult Latin phrase within a line, test `U+2066` LRI with `U+2069` PDI
in the real renderer. Never use `U+202E` RLO to fix logical word order.

## Punctuation and quotation scope

Read the raw logical string and the image. A period shown on the left of RTL
text may already be correctly stored at the end; move punctuation only when its
logical placement is wrong. Preserve meaningful ellipses, interruptions and
dialogue dashes.

Identify exactly which name or phrase each quote encloses before repairing
reversed, unmatched or duplicated quotes. `«نام»` and `«جمله»` suit Persian prose;
ASCII quotes can remain in code, literal citations or English. Do not replace
inch marks or quotes in style syntax. Check parentheses and brackets with bidi
mirroring in mind.

Normalize Arabic ي/ك to Persian ی/ک in Persian prose when appropriate; use ZWNJ
consistently without changing names or valid colloquial forms. Never run a global
alphabet/punctuation replacement over commands, timestamps, vector paths or fonts.

Before a typography pass, identify protected spans: locked glossary spellings,
Latin names, URLs, version numbers, quantities, ASS overrides and drawing payloads.
Change only the readable Persian prose. Compare polarity, numbers, units and speaker
register against the original after the pass; a typographic fix must preserve them.
Reapplying the same correction must leave the text unchanged. A second changing
pass signals an unstable rule that needs review before building.

This adapts the protected-token and idempotence checks in the MIT-licensed
[Persian skill](https://github.com/Mojtaba-Alehosseini/persian-skill/blob/4264f6ef03f7568bc15b14287a3db09bc815fc31/SKILL.md)
to subtitle structure; its linter and automatic numeral/unit conversions are not
part of this workflow. Source evidence and the approved glossary decide meaning.

## Bold and italic emphasis

Keep intentional source emphasis on the corresponding Persian words. In ASS,
use complete overrides such as `{\i1}...{\i0}` and `{\b1}...{\b0}`. Close every
override block and restore the previous style at the end of the emphasized span.
In SRT, keep `<i>...</i>` and `<b>...</b>` balanced and correctly nested. Flag any
emphasis the target format or player cannot retain; never strip it silently.

## ASS styles and fonts

Preserve positioning, animation, shadows and drawing data. Translate the readable
text around those controls. If a Persian glyph is missing or its fallback is
wrong, select a suitable font for that readable text and record `structure_note`.
Leave a decorative symbol font alone. Embedded fonts are retained by default.

Inherited nonzero `\fsp` tracking can break Arabic joining and visual order even
with a suitable font and complex shaping. When the image demonstrates this, test
`\fsp0` on the affected readable text and record the structural edit.

If mixed text still reorders whole groups across inline style changes, test
libass's `\fe-1` whole-text layout for that cue. VSFilter does not support
this extension: disclose that limitation or adapt the typesetting
for the requested player. Do not apply it to every sign without inspection.

## Physical inspection

Render every output episode with FFmpeg/libass. Open every generated PNG and
inspect joined letters, logical reading order, English spans, numbers, quotes,
punctuation, missing glyphs, clipping, overlap, effects and font fallback. Use more
samples or an actual clip when motion or contrast remains unclear.

Neutral-background images establish presentation only. Matching-video images
also show scene contrast, but neither alone certifies audiovisual synchronization.
Generating images is not the same as looking at them. A missing viewer or renderer
leaves visual QA incomplete. [workflow.md](workflow.md) defines the review records.

## Default ASS font, outline, line wrapping and punctuation anchoring

The only bundled subtitle font is `B Koodak` (`assets/fonts/BKOODB.TTF`).
FFmpeg/libass loads it through `fontsdir`. The default size is `110`, the black
outline and shadow are both `2`, and the left, right, and vertical margins are
all `10`. The Default style is non-bold unless the source ASS Default is bold
or the user overrides it. Preserve inline emphasis.

To keep terminal punctuation on its own line in libass and players such as VLC,
mpv and PotPlayer, the text pipeline balances lines over 42 visible characters
with explicit `\N` breaks. It does not split override blocks or ZWNJ compounds.
It wraps each physical RTL line in `U+202B` and `U+202C` and puts `U+200F` after
terminal neutral punctuation, before `U+202C`. Tag-aware ZWNJ enforcement
normalizes `می‌`/`نمی‌`, `‌ها`/`‌های`, `‌تر`/`‌ترین`, and heh-final enclitics without
altering tags or Latin spans.
