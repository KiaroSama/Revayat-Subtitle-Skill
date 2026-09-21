# Revayat Subtitle

A subtitle edition for one anime series, using consistent Persian across seasons.

**Series**: The anime identity shared by its seasons and OVAs. A sequel retains its
established names; a different anime gets a separate glossary.

**Episode**: One regular episode or OVA, identified by season and number.

**Candidate**: One source subtitle release for an episode. Candidates may differ
in translation, timing, signs, effects, completeness, and format.

**Base**: The candidate whose timing and presentation anchor the episode's output.
ASS is preferred when available and usable.

**Cue**: One timed subtitle event, which may contain speech, readable signs,
lyrics, decorative effects, or credits.

**Glossary**: Researched names, places, honorifics, and recurring terms with
locked target spellings and source links.

**Review**: A decision about every cue, including the candidates not selected.

**Edition**: Exactly one subtitle file per episode, delivered inside `Sub/` in a ZIP.

**Reviewed input**: Source identities, editorial decisions and the locked glossary.
Its identity is distinct from the generation recipe used to build an edition.

**Cue provenance**: Source/cue origin, emitted index and original, reviewed and
effective timestamps. ASS conversion floors to 10 ms and refuses collapsed duration.

**Render evidence**: A versioned receipt binding the edition, sampler, renderer,
provided fonts, background and unique images to their sample times and cue IDs.
Editable inspection notes do not redefine that receipt.

**Parallel editorial assignment**: A user-consented, disjoint episode or cue range
with immutable source context, glossary revision and separate result/activity log.
The coordinator integrates every cue and owns the final consistency and visual review.
