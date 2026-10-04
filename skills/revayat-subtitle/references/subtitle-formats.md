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
Compared values retain Unicode whitespace literally; only equivalent ASCII
space/tab padding is removed. Numeric PlayRes, LayoutRes and WrapStyle values
with leading ASCII padding before an `&H` or `0x` hexadecimal prefix require
explicit repair: libass chooses the numeric base before skipping that padding.
Unpadded hexadecimal values, trailing ASCII padding and padded decimal values
remain supported. An empty donor imposes no unused track-setting constraint.

Supported SRT `br`/`br/` tags, including closing forms such as `</br>`, become
display breaks, including ASCII-space attributes and `<br/ >`, when the tag body
fits FFmpeg's 127 UTF-8-byte scan limit (excluding `<`, an optional closing `/`,
and `>`). Longer tags remain literal text, including multibyte attributes.
A tab or nonbreaking space attached to the tag name is
literal text, following FFmpeg rather than browser HTML rules. ASS parenthesized
scalar values take precedence over ignored prefixes: `\rIgnored(Default)` refers
to `Default`, while literal reset-looking prose remains untouched. The helper
preserves delimiters/ignored source arguments when remapping the actual style value.
Malformed or unsupported controls still require deliberate source-preserving repair.
Before SRT-to-ASS conversion, renderer-consumed but unsupported markup such as
`<small>`, `<foo>`, `<>` or `</>` is refused rather than exposed as visible ASS
dialogue. Completed hidden comments are removed before checking the remaining
source for ASS controls; unterminated comments are retained and may require repair.
Adapt the worksheet explicitly after checking the source; do not blindly remove
all angle-bracket content. Literal comparisons, entities and guillemets remain
literal. Repeated simple bold/italic/underline/strike tags use binary toggles,
not browser-style nested formatting stacks.

Inputs must be regular files; a ZIP-looking pipe or device is not an archive.
Source copies and emitted subtitles are byte-verified before publishing their
workspace or edition. Failed conversion leaves the donor and base objects intact
so that a corrected retry cannot inherit partly remapped styles.

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
state. Generation recipe remains 10; render sampler 11 invalidates earlier
review receipts without changing valid edition identities.
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

Prune unused styles using renderer-effective names, retaining exact `\rStyle` references.
A bare empty `Style:` row can instantiate renderer defaults and requires deliberate
repair; an empty Name field in an otherwise complete style resolves to `Default`.
Style changes and vector modifications require `structure_note` and visual review.
Promotion removal follows [translation-policy.md](translation-policy.md).

## ASS track grammar and presentation

ASS row labels must use canonical `Format:`, `Style:`, `Dialogue:` and `Comment:`
spelling. Unlike section/field names, they are not case-insensitive. Do not activate
renderer-ignored rows by rewriting their labels. Syntax case folding and indentation
use ASCII rules, not Unicode text normalization. Styles must precede Events.
Renderer-active suffixes on section names require deliberate repair.

Unknown/editor sections do not necessarily reset libass parser state. Active event,
style, format or track-setting rows inside them are refused rather than copied or
removed outside cue review. This also includes the exact legacy FFmpeg generator
signature: it can remain effective under an unknown header while Script Info is
active. Harmless metadata and opaque font payloads stay intact.

Declared style names and event references ignore leading stars; events also map
ASCII case variants of `Default` to `Default`. Reset arguments have exact, separate
name semantics. Trailing ASCII spaces and Unicode spaces in names are meaningful.
Ambiguous effective-name collisions or undefined exact references need deliberate
repair, not fallback guessing. Original rows/identifiers remain source evidence.

When the supported original track matches libass's legacy FFmpeg signature heuristic,
materialize `ScaledBorderAndShadow: yes` before removing the signature or pruning/
merging styles. Preserve explicit settings and do not infer legacy behavior from
an arbitrary comment. The original source file itself remains unchanged.

## Grammar and immutable metadata

SRT formatting names are ASCII syntax even when the dialogue is Unicode. Names
such as `<ſ>`, `<İ>` and `<ı>` are literal text, not strike/italic aliases. Only
ASCII space separates a supported formatting name from its attributes; a tab,
NBSP or other Unicode whitespace attached to a tag name remains literal. Do not
apply Unicode case folding or general HTML whitespace rules to this grammar.

Directional normalization joins adjacent prose fragments exposed by removed
comments before choosing its boundary controls. It never joins across an actual
override, style, drawing payload or retained vector-object boundary. Reapplying
the normalizer must preserve both logical content and already-normalized bytes.

Immutable manifests and saved glossaries must retain their decoded JSON types:
`true`, `1` and `1.0` are not interchangeable metadata. JSON key order and
formatting are not an identity change. Do not transfer image approvals after
output or recipe changes; build and inspect the new edition.

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
