---
description: Check a Revayat Subtitle build and report missing review or stale delivery evidence.
argument-hint: <build directory>
---

Check the `revayat-subtitle` build at **$ARGUMENTS**:

```text
python "SKILL_DIR/scripts/revayat-subtitle.py" qa --build "BUILD"
```

Report source or output changes first, then missing cue/image reviews, incorrect
episode coverage, and other errors with the affected path or cue. Use the repair
table in `references/troubleshooting.md`. On success, report episodes, reviewed
cues, reviewed frames, embedded-font policy and the background used for rendering.

Distinguish checked file integrity from editorial accuracy. The command cannot
prove that an agent understood every line or actually looked at a frame. If asked
to assess language or visual quality, read the relevant source/target and images.
`sync_verified: false` means no audiovisual synchronization certification.

This command reports findings. Do not edit the subtitles, sign reviews or create
a replacement ZIP unless the user also asks for repairs.
