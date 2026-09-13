# Editorial and Persian display rules

Read this on every invocation, before editing any cue.

## Meaning, completeness and voice

- Read every line of every candidate and every episode. Compare dialogue with
  neighboring cues; a natural sentence can still have the wrong subject,
  negation, tense, speaker, referent, joke or relationship.
- Write fluent **spoken Persian**, not literary prose. For ordinary dialogue,
  prefer `می‌خوام برم خونه` to `می‌خواهم به منزل بروم`. Keep deliberate in-story
  formality when it belongs to the character; do not make every character identical.
- Preserve all meaning and the original intensity of profanity, sexual language
  and insults. No moral filtering, euphemistic replacement, omitted hard lines,
  summary, added explanation, or invented details.
- Translate readable signs, songs, titles and narrative text. Keep decorative
  shapes and symbols unchanged. For karaoke, preserve timing and styling while
  adapting lyric text; changed syllable/tag alignment needs actual visual review.
- Keep names, locations, organizations, recurring terms and character voice
  consistent across episodes, OVAs and sequels. User-approved spellings override
  new preferences; record a deliberate correction instead of silently renaming.
- Keep Japanese honorifics: `-san` -> `-سان`, `-kun` -> `-کون`, `-sama` -> `-ساما`,
  `-chan` -> `-چان`, `onii-chan` -> `اونی-چان`. Follow the series glossary for
  separators. Do not confuse a suffix with a different word having the same letters.

## Logical order before display order

Store normal Unicode Persian in reading order. Do not reverse strings, sort words
by script, paste Arabic presentation forms, or run a visual bidi/reshaping library
over the stored subtitle. libass handles glyph shaping and visual ordering.

Repair the sentence itself when words were stored in visual order:

```text
Broken:   رو دیدم. OVA من دیروز
Correct:  من دیروز OVA رو دیدم.
```

This requires understanding the sentence. Adding a direction mark alone cannot
repair its word order. The same principle applies to URLs, names, English phrases,
dates, measurements and numbers. Retain Latin acronyms/names where appropriate.

The builder inserts **actual U+200F RIGHT-TO-LEFT MARK** at the boundaries of
Persian display lines, outside ASS commands and vector payloads. The literal six
characters `\u200f` are not an RTL mark. Keep meaningful U+200C ZWNJ in Persian.
Remove or repair accidental bidi overrides only in editable prose; preserve valid
isolates when needed. For a difficult embedded Latin phrase, test U+2066 LRI with
U+2069 PDI in the real renderer before accepting it. Never use U+202E RLO to reverse
the text. A renderer-specific result is not a guarantee for every subtitle player.

## Punctuation and quotes

Examine the raw logical string and the rendered image. A period that *looks* on
the left of RTL text may already be correctly stored at the end. Move punctuation
only when its logical placement is wrong; do not blindly move every leading or
trailing punctuation mark. Keep ellipses, interrupted speech and dialogue dashes
when they express the intended delivery.

Find exactly which name or phrase each pair of quotes encloses. Repair reversed,
unmatched or duplicated quotes only after resolving their intended scope. In
Persian prose `«نام»` or `«جمله»` is suitable, but ASCII quotes can be meaningful in
code, a literal citation or English dialogue. Do not turn inch marks into quotes,
or replace quotes in ASS/style syntax. Check parentheses and brackets in the same
way; bidi mirroring affects their visual appearance.

Normalize Arabic ي/ك to Persian ی/ک in Persian prose when appropriate, use ZWNJ
consistently, and fix spelling without damaging character names or colloquial
forms. Do not apply a global alphabet or punctuation substitution to tags,
timestamps, vector paths, Latin literals or embedded font data.

## What may be removed

Verified fansub promotions such as `ترجمه از ...`, `کاری از ...`, channel handles,
release-group advertisements and unrelated URLs are removable. Search terms only
produce candidates for inspection: dialogue such as `اون مترجم بود` stays.
Keep credits that are part of the actual story. Remove only promotional fragments
from a cue containing both promotion and story content.

Remove ASS Comment events, semicolon comment lines outside attachment sections,
standalone `{inline comments}`, SRT HTML comments and truly empty cues. Remove empty
display lines inside a retained cue as well, without removing its meaningful lines. For mixed
ASS blocks such as `{note\i1}`, separate the hidden comment from the actual tag
manually and document the structural change. Do not delete an override block just
because it is not directly visible; it may control the visible drawing/text.

Prune unused and empty styles, retaining a style used only by an inline `\rName`
reset. A drawing with no prose is not an empty subtitle. The presence of `\p1`
does not make the whole cue nonlinguistic: text following `\p0` is readable again.
Embedded fonts stay unless the user selected removal. Retain all required style
and font references independently of that choice.

## Research references

The display rules follow [Unicode UAX #9](https://www.unicode.org/reports/tr9/).
ASS structure follows [Aegisub's tag reference](https://aegisub.org/docs/latest/ass_tags/).
Rendering uses the [FFmpeg ass/subtitles filters](https://ffmpeg.org/ffmpeg-filters.html#ass).
Look up the actual anime separately; these technical references do not establish
character names or translation accuracy.
