---
description: Resume a partially reviewed Revayat Subtitle season without repeating completed editorial work.
argument-hint: <work directory, default work/>
---

Resume `revayat-subtitle` at **$ARGUMENTS** (default `work/`).

Load the skill and its translation policy again. Read `project.json`,
`glossary.json` and the worksheets. Identify the first incomplete source/cue and
the latest build matching the current edits; report that state before continuing.
Use the original series identity, season numbering, locked names and font policy.
Read the previous activity log and create a new run log beside the translated
files, identifying the resumed batch and recording subsequent actions as they occur.

For parallel work, read `references/parallel-editorial.md`: retain an explicit batch
choice, or ask before dispatch. Reconcile saved worker IDs, source/glossary revisions
and disjoint ranges before retrying missing work; stop abandoned workers first.

Continue from the corresponding numbered step in `SKILL.md`. Keep completed
decisions and original cue IDs. `prepare` creates a new workspace and must not
be run over this one. A sequel loads the previous glossary before any translation.

For a completed build, check which delivery work remains:

```text
python "SKILL_DIR/scripts/revayat-subtitle.py" qa --build "BUILD"
```

Fix an incomplete worksheet in the worksheet, then build again. A changed build
needs fresh render evidence. Inspect every pending image and finish through the
normal packaging gate; an old ZIP or a completed-looking folder is not proof.
