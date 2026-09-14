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
valid isolates and repair accidental overrides only in editable prose. For a
difficult Latin phrase, test U+2066 LRI with U+2069 PDI in the real renderer.
Never use U+202E RLO as a substitute for correct logical text.

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
