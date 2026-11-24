---
name: revayat-subtitle-sync
description: Use when timing existing SRT or ASS subtitles against matching video or audio to correct an offset, FPS drift, or a mid-episode edit.
---

# Retiming subtitles

If the Revayat Subtitle skill is installed beside this one, follow its [audio sync workflow](../revayat-subtitle/sync/SKILL.md). Otherwise install `subsync` on `PATH` and use it directly. Video input requires FFmpeg on `PATH`; `--wav` accepts local mono 16-bit PCM WAV without FFmpeg.

```text
subsync --video "episode.mkv" --in "original.srt" --out "aligned.srt" --json-report "sync-report.json"
```

Use a new output path with the same subtitle format. The engine extracts mono PCM from video, estimates global offset and FPS drift with FFT correlation, looks for cuts with split-penalty dynamic programming, then adjusts eligible cue starts toward nearby speech onsets. It does not translate or regenerate subtitles.

Read the report for `fps_ratio`, changes in `split_segments`, and speech overlap before and after. Play the opening, middle, ending, and both sides of each split; verify dialogue by ear and check that ASS styling and layered signs still render as intended. Keep the original and report uncertain timing for manual review.
