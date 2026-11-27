---
name: revayat-subtitle-sync
description: Use when timing existing SRT or ASS subtitles against matching video or audio to correct an offset, FPS drift, or a mid-episode edit.
---

# Retiming subtitles

If the Revayat Subtitle skill is installed beside this one, follow its [audio sync workflow](../revayat-subtitle/sync/SKILL.md). Otherwise install `subsync` on `PATH` and use it directly. Video input requires FFmpeg on `PATH`; `--wav` accepts local mono 16-bit PCM WAV without FFmpeg.

```text
subsync --video "episode.mkv" --in "original.srt" --out "aligned.srt" --json-report "sync-report.json"
```

Use a new output path with the same subtitle format. For 5.1/7.1 video, the engine selects the Front Center (`FC`) dialogue channel automatically and falls back to mono if needed. It aligns cues individually, keeps back-to-back dialogue together, and targets a 50 ms lead-in before speech for eligible starts (`--lead-in-ms 50`). End refinement defaults to `--end-adjust-ms 600` and targets about 250 ms after speech, with about 900 ms for reading when the gap allows. It does not translate or regenerate subtitles.

Read the report for `fps_ratio`, changes in `split_segments`, and speech overlap before and after. Play the opening, middle, ending, and both sides of each split; verify dialogue by ear and check that ASS styling and layered signs still render as intended. Keep the original and report uncertain timing for manual review.
