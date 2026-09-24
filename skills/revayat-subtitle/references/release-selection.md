# Release selection and episode identity

Use this when assigning episode numbers or comparing multiple subtitle releases.

## One output per episode

Group only candidates for the same episode and video edition. Confirm the season,
episode and OVA numbers. Do not extract a number from a resolution, year or CRC.
Before assigning a candidate, compare its series/episode metadata, language,
encoding, cue timeline and the beginning, middle and ending against the actual
episode when video is available. A different cut or an implausible timeline stays
unassigned for investigation. A release-group label or resolution is not proof of
a match or mismatch. If no matching video is available, record that cut matching
remains unverified and use the candidates' content and timing as limited evidence.
Use `S01E01`, `S01OVA01`, `S02E01` and `S02OVA01`. Movies, specials, episode zero
and ambiguous numbering need the user's naming decision.

Each source is assigned exactly once in `project.json`. A source omitted from that
configuration still exists in the inventory and blocks the build.

## Choose the base

Prefer a usable ASS base. Within the same format, compare meaning, spoken Persian,
missing dialogue, timing, signs, songs, honorifics and presentation. File size,
cue count, release label and extension do not measure editorial quality.

Read every candidate, including those not selected. Record the comparison and the
reason for the base choice. Keep its timing and presentation; put a better donor
translation into the corresponding base cue, preserving markup, and note its source.
Mark a redundant donor cue `alternate`, linking it to retained cue(s) that cover
its meaning. Keep donor-only content when it adds a real sign or dialogue line.

## Align content before merging

Match by meaning and time, not row index. Preserve intentional overlaps and
foreground/shadow layers; identical-looking text may form one visual effect.
Do not concatenate complete releases or stack duplicate translations.

Different cuts and frame rates need actual-video alignment before retiming.
Document every change with `timing_note`. ASS donors require compatible canvas and
wrapping settings; conflicting font sets or complex cross-format markup need
explicit reconciliation. These refusals protect presentation rather than choosing
one release arbitrarily. The decision fields are in [workflow.md](workflow.md).
