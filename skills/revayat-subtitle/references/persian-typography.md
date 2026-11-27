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

The builder wraps each Persian display line in `RLM` (`U+200F`) followed by
`RLI` (`U+2067`), with `PDI` (`U+2069`) after the readable text and punctuation.
This applies regardless of the opening script, digit, or dialogue dash. Keep
ASS overrides, drawing data, and SRT tags outside the generated wrapper.
Use actual Unicode characters and check the result in the target renderer.

Conversion removes only unmatched directional controls from imported prose,
matching scopes across the whole cue before handling display lines. It keeps
intentional balanced inner and cross-line scopes, including scopes across style
tags, and leaves controls in tags and drawing data untouched. A `PDI` closes its
matching isolate and any embeddings opened inside it; a stray terminator must
not discard another balanced scope. Legacy embedding wrappers are removed only
when their endpoints enclose one complete scope. Before adding an RTL isolate
or LTR embedding, check the [UAX #9](https://www.unicode.org/reports/tr9/tr9-52.html#Explicit_Levels_and_Directions)
effective explicit level of the readable text. Each opener advances to the next
odd or even level, by one or two. `FSI` takes its direction from the first strong
character in its own isolate, skipping nested isolates; it defaults to LTR.
If adding a wrapper would exceed level 125, preserve the authored controls.
Reviewed markup uses boundary `RLM` or `LRM` marks in that case; conversion keeps
the existing line. Inside Persian words, conversion replaces stray `RLM` between
letters with `ZWNJ` (`U+200C`).
Reversed leading punctuation moves to the logical end only when it directly precedes a Persian
letter and the line has no terminal punctuation. Keep legitimate continuation
ellipses and dialogue dashes.

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
Wraps keep detached terminal punctuation with the preceding text. Tag-aware ZWNJ enforcement
normalizes `می‌`/`نمی‌`, `‌ها`/`‌های`, `‌تر`/`‌ترین`, and heh-final enclitics without
altering tags or Latin spans.
