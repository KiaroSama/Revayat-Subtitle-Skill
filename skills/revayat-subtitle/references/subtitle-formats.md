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

ASS block scanning follows the selected libass renderer: the first closing brace
ends a block, escaped opening braces remain literal prose, and an unclosed opening
is not silently removed. Do not repeatedly remove innermost brace pairs or apply
an odd/even-backslash escape rule. Escaped literal braces are a libass extension,
not a promise of identical behavior in every other player.

Before adding retained ASS donor cues, reconcile differing track-global settings
as well as the canvas: LayoutRes, Kerning, YCbCr Matrix, Language, Collisions and
Timer can belong to the source presentation contract. The helper refuses differing
values rather than silently applying the base header to a donor's presentation.
Tracked headers must use canonical spelling, such as `Kerning:`; noncanonical
case or whitespace before the colon cannot shadow a renderer-effective setting.

Supported SRT `br`/`br/` tags become display breaks, including ASCII-space
attributes and `<br/ >`. A tab or nonbreaking space attached to the tag name is
literal text, following FFmpeg rather than browser HTML rules. ASS parenthesized
scalar values take precedence over ignored prefixes: `\rIgnored(Default)` refers
to `Default`, while literal reset-looking prose remains untouched. The helper
preserves delimiters/ignored source arguments when remapping the actual style value.
Malformed or unsupported controls still require deliberate source-preserving repair.

## Readable text versus effects

Override blocks, vector paths, clips, masks, positioning, animation, colors and
timing controls are not prose. A line with `\p1` may later switch back to readable
text with `\p0`. Translate that text while retaining its effect. A vector event
with no words is not an empty cue. Karaoke lyrics remain readable content; adapt
their text with timing and syllable/tag alignment checked in actual images.
Style reset `\r` does not leave drawing mode. Drawing assignments inside supported
`\t` controls also affect the following payload. Such vector changes require the
same structural explanation and visual review as ordinary `\p` drawing changes.
Transforms with more than three nonempty comma-terminated arguments before the
backslash argument are ignored by libass; their nested tags do not change drawing
state. Generation and sampler recipe 7 invalidate earlier review receipts.
Drawing-mode brace scanning uses raw vector boundaries, not prose escape rules.
Removing a hidden comment between vector objects retains an empty `{}` boundary;
space/tab after a tag backslash and signed-positive drawing scales stay supported.

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
