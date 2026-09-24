# Subtitle structure and preservation

Use this for ASS/SRT import, cleanup, font removal and structural edits.

## Format boundaries

The helper accepts ASS, SRT, directories and ZIPs. Text decoding is strict UTF-8;
confirm a legacy encoding before passing `--encoding`. SSA, VTT, bitmap subtitles,
encrypted archives and malformed files need deliberate conversion or repair of a
separate copy. Originals remain unchanged.

ASS `Format` rows define fields, with `Text` last in Events. Commas inside text
are content. A physical line break would corrupt an ASS row; use literal `\N`.
SRT block numbers are regenerated after a stable sort by start time. Equal-start
events retain their order; overlap is not automatically an error.

SRT-to-ASS conversion floors each timestamp to a centisecond and records original,
reviewed and emitted times. Durations that become zero are refused. Literal angle
comparisons such as `x < 5` remain visible; unsupported HTML needs an explicit edit.
Real ASS style resets are parsed inside override blocks, including transforms;
reset-looking prose outside those blocks is not rewritten.

## Readable text versus effects

Override blocks, vector paths, clips, masks, positioning, animation, colors and
timing controls are not prose. A line with `\p1` may later switch back to readable
text with `\p0`. Translate that text while retaining its effect. A vector event
with no words is not an empty cue. Karaoke lyrics remain readable content; adapt
their text with timing and syllable/tag alignment checked in actual images.

## Cleanup

Remove non-displayed ASS Comment events, semicolon comments outside attachment
sections, standalone `{inline comments}`, SRT HTML comments and genuinely empty
cues. Remove empty display lines within a retained cue without losing its content.
For `{note\i1}`, separate the comment from the real tag manually and document the
structural change. A hidden override block may still control visible output.

Meaningful interior direction controls and ZWNJ are preserved; unbalanced direction
scopes are refused. Output language selects paragraph direction independently of
embedded names. An explicit worksheet `direction` (`ltr`/`rtl`) needs `direction_note`.
Intentional blank layout uses `preserve_empty_lines: true` plus `structure_note`;
soft breaks, drawings and karaoke are not blindly stripped.

Prune empty and unused styles, retaining those used only by `\rStyle` resets.
Style changes and vector modifications require `structure_note` and visual review.
Promotion removal follows [translation-policy.md](translation-policy.md).

## Embedded fonts are optional to remove

The safe stored default is `font_policy: keep`. When actual embedded fonts are
present, every using agent must ask whether to remove or retain them unless the
user has already explicitly answered for this batch. Record the answer, use
`remove` only for an explicit removal choice, and resolve the question before
delivery. No embedded fonts means no question. This choice is independent of
unused-style cleanup. Preserve font payloads as opaque
data: encoded lines can resemble comments or section headers.

After removal, inspect the actual replacement font and every affected symbol.
Fonts with the same name but different payloads require reconciliation when merging
ASS donors. External fonts supplied through `--fonts-dir` are rendering inputs,
not permission to redistribute them. See [persian-typography.md](persian-typography.md).
