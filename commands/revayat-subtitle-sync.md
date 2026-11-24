---
description: Align an SRT or ASS subtitle to a matching video or mono PCM WAV.
argument-hint: --video <video> --in <subtitle> --out <new subtitle>
---

Use the [subtitle audio sync skill](../skills/revayat-subtitle/sync/SKILL.md) with `$ARGUMENTS`. Accept exactly one video or WAV input, a source subtitle, and a new output path. Ask for any missing path. Pass paths as separate arguments and preserve the original files.

Run `python skills/revayat-subtitle/scripts/sync_bridge.py` with the supplied options. Report the estimated offset, FPS ratio, splits, and change in speech overlap. Separate measured results from any playback review you did not perform.
