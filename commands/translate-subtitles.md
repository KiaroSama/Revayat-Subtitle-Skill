---
description: Translate or edit anime subtitles. Persian is the default; specify another target in the request.
argument-hint: <ASS, SRT, ZIP or folder> [series and season]
---

Use `revayat-subtitle` for **$ARGUMENTS**. Resolve the installed skill's `SKILL.md`,
follow its intake checklist and run the [six stages with the supplied prompts](../skills/revayat-subtitle/references/multi-agent-workflow.md)
in order: Initial Translation, Typesetting & Layout, Editorial Review, Translation
Applier & Edition Builder, File Re-Check & Issue Fixer, Final Review & Delivery.
Persian is the default target; choose the voice guide by output language. Translate
directly from the original when understood. Read every cue of every
candidate, research the anime, reuse the series glossary, keep Japanese honorifics,
ask about retaining/removing embedded fonts when present unless already decided,
and inspect the FFmpeg images. Keep the required agent activity log beside the
translated files throughout the task.
The six roles run in sequence. Offer parallel cue assignments within a stage only
with user consent and host support; they do not replace independent review.
Return the verified `Sub.zip` and the separate continuity glossary.
