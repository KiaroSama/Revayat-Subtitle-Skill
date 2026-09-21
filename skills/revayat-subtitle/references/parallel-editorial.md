# Optional parallel translation and editing

Use this only when the host actually supports subagents and the user has agreed
to parallel work for this batch. It applies to translation from any language and
to correction of existing Persian subtitles. The same fidelity and review rules apply.

## Ask before dispatch

Before editorial work, ask once: **“Would you like multiple subagents to translate
or edit separate parts in parallel? This can reduce elapsed time on large batches,
but may use more tokens and needs a final consistency review.”**

In Persian: **«می‌خواهی ترجمه یا اصلاح این مجموعه را چند ساب‌ایجنت به‌صورت موازی
انجام بدهند؟ برای کارهای بزرگ می‌تواند سریع‌تر باشد، ولی ممکن است توکن بیشتری
مصرف کند و در پایان به یکدست‌سازی نیاز دارد.»**

Do not ask again if the user already decided for this batch or supplied an explicit
standing preference. Record consent/decline and any worker/cost limit in the activity
log. No answer is not consent: continue authorized sequential work without dispatch.
If the host has no subagents, explain that limitation and use the sequential workflow.
Do not pretend separate prompts, fictional workers or parallel API calls are subagents.
Never install a provider, upload subtitles or change host settings to enable this mode.

## Divide the work without shared writes

The coordinator completes inventory, series research, glossary and release/episode
assignment first. Prefer assigning a whole episode with all its candidates to one
worker. For a long single episode, use disjoint cue ranges with explicit source IDs;
include donor cues in the assignment so candidate content is neither lost nor counted
twice. Neighboring source cues are read-only context, not additional assigned output.

Give each worker:

- Exact assigned source/cue IDs, immutable source text/hashes and output language.
- The same locked glossary and its revision/hash, character voice and language profile.
- Read-only surrounding dialogue and resolved speaker/referent notes.
- A unique result file and a unique activity-log filename; the required review schema.
- The complete preservation, Japanese-honorific, no-censorship, markup and uncertainty rules.

Choose a bounded worker count from actual host slots, the user's limit and context
cost. Two workers are a reasonable starting point for a large batch. Workers must
not spawn further workers or overwrite each other's results. Small batches may be
faster sequentially; do not promise a measured speedup without measurement.

Only the coordinator writes shared worksheets, project configuration and glossary.
Workers return proposed row decisions in their own files. They never edit originals,
published subtitles, shared approvals or another worker's range. Font choices remain
the user's decision; workers report a font issue to the coordinator instead of asking
the user independently or silently removing/replacing fonts.

## Worker handoff contract

Every assignment names the series, episode, source language, target, source identity,
glossary revision, assigned IDs, context-only IDs, result path and log path. Require
exactly one result for each assigned cue and no results for context-only cues.
Keep original IDs/text and all worksheet fields intact. Use the existing actions and
reasons; an unresolved meaning stays pending instead of receiving invented approval.

Each worker records its real progress, reviewed ID ranges, counts, uncertainties and
errors in its own UTF-8 UTC activity log. Keep logs beside the translated subtitle
files once that directory exists; temporary work-folder logs are moved there before
delivery. Do not concurrently append to one shared file. Do not log full subtitle
bodies, private reasoning or unperformed checks.

## Integrate and review

Before importing a worker result, the coordinator checks source identity and glossary
revision, exact ID coverage, unchanged source text, valid decisions/fields, and absence
of overlapping or extra rows. Reject stale, malformed, incomplete or conflicting
results. The normal build validation remains mandatory after integration.

Resolve new terminology centrally. If a glossary decision changes, notify affected
workers and re-review affected returned cues against the new revision; do not silently
mix revisions. Review chunk boundaries, speaker references, negation, honorifics,
names, tone, signs/lyrics and donor links across the complete edition.

Read every returned cue against its source before accepting it and perform the final
whole-episode fluency/continuity pass. Worker success messages are not quality evidence.
The coordinator owns final building, physical rendering/image inspection, QA and
packaging, and keeps the master activity log with the final outcome and worker coverage.
The subtitle ZIP still contains only one subtitle per episode unless the user asks
for logs or other extra files.

## Failure and resume

If a worker fails, keep completed validated ranges, record the missing IDs and retry
only that assignment or finish it sequentially. Stop/reconcile duplicate or abandoned
workers before reassignment. On resume, compare actual source/glossary revisions and
saved results; never assume an interrupted worker finished. Resolve all assigned ranges
and stop owned workers before final delivery. Partial work is reported as partial.
