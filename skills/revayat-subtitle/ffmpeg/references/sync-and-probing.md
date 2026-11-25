# Sync and probing with FFmpeg

## Find the dialogue and subtitle streams

```text
ffprobe -v error -show_entries "format=start_time,duration:stream=index,codec_type,codec_name,start_time,duration,time_base,r_frame_rate,avg_frame_rate,channels,sample_rate:stream_tags=language,title:stream_disposition=default,forced" -of json "Episode 01.mkv"
```

Check `codec_type`, `index`, language, title, and disposition. A default track can still be a dub. A title can be wrong or missing, so listen to the track before relying on it. `0:2` means absolute stream index 2 of the first input; `0:a:1` means its second audio stream. Use the absolute index from the probe when mapping a particular track, and confirm that it is audio. Revayat's `--video` route uses FFmpeg's automatic audio selection.

Subtitle streams may carry text (such as ASS or SubRip) or images (such as PGS). The sync CLI takes a sidecar SRT or ASS. A picture-based stream needs OCR and review before it can supply text cues. A stream tagged `forced` may contain signs or short translations and omit most dialogue.

## Extract the chosen track

If you confirmed that stream index `2` is the dialogue track, use:

```text
ffmpeg -nostdin -n -i "Episode 01.mkv" -map 0:2 -vn -sn -dn -c:a pcm_s16le -ar 16000 -ac 1 "Episode 01 dialogue.wav"
ffprobe -v error -select_streams a:0 -show_entries stream=codec_name,sample_rate,channels,duration -of json "Episode 01 dialogue.wav"
```

Use a fresh output path: `-n` refuses to overwrite. Expect `pcm_s16le`, a `sample_rate` of `16000`, one channel, and a nonzero duration that covers the dialogue track. A small difference from the video container duration need not be an error. If the length is missing or suspect, count decoded samples:

```text
ffmpeg -nostdin -hide_banner -nostats -i "Episode 01 dialogue.wav" -map 0:a:0 -af "astats=measure_perchannel=none:measure_overall=Number_of_samples" -f null -
```

Divide `Number of samples` by 16000 for the duration in seconds. Then use `--wav` with `revayat-subtitle sync` or `subsync`. Keep the full track; trimming silence, shifting timestamps, or changing tempo can move dialogue relative to the picture. Ordinary resampling to 16 kHz changes the sample rate, not playback speed.

## Compare timestamps before correcting drift

Compare the times of the same spoken lines at the start, middle, and end. Use distinctive dialogue rather than music or repeated effects.

| What you observe | What to check |
| --- | --- |
| Similar lead or lag throughout | A fixed offset, such as an intro or a different start time. Check the stream and container starts before choosing a correction. |
| Offset increases steadily | Possible speed mismatch, such as 24 versus 25 fps, or clock drift. Confirm against widely separated dialogue points; reported FPS alone does not prove a speed change. |
| Offset jumps after a scene break | Look for inserted or missing footage and compare both sides of the cut. Do not stretch the whole subtitle to compensate for an edit. |
| Offsets vary and do not match known edits | Check the audio track and release again. Sparse dialogue, music, and effects can make a numerical match misleading. |

Use elapsed seconds on the playback timeline for comparisons. FFmpeg accepts seconds or `HH:MM:SS.mmm` for `-ss`. SRT uses milliseconds (`00:01:02,500`) and ASS uses centiseconds (`0:01:02.50`). Convert frame-based timecode using its source frame rate and drop-frame convention before comparing it with a subtitle timestamp.

Disagreement between `r_frame_rate` and `avg_frame_rate` is a reason to inspect frame timestamps, not proof of variable frame rate. Read a bounded set of decoded frame timestamps where the issue occurs:

```text
ffprobe -v error -select_streams V:0 -read_intervals "600%+#100" -show_frames -show_entries frame=best_effort_timestamp_time -of json "Episode 01.mkv"
```

`-read_intervals` seeks approximately, so check the timestamps it actually returns. Compare successive presentation times rather than treating packet decode times as playback times. If an audio stream begins late or has timestamp gaps, a WAV cannot carry that stream's original timeline by itself. Compare the first audible dialogue against the original video before accepting a WAV-based result. Check `fps_ratio` and `split_segments` in the Rust report against these observations before accepting the retime.

## Quote paths and check the render

- In PowerShell 7 and Linux shells, pass each path with spaces as one quoted argument. Prefer local paths. Prefix a relative filename beginning with `-` or containing a protocol-like prefix such as `a:b` with `./`.
- Filter paths need escaping beyond shell quoting. The simplest option on either OS is to run from a directory with a short relative subtitle filename, such as `-vf "subtitles=filename='aligned.ass'"`. Within the filter's quoted filename, use forward slashes for Windows paths and escape the drive colon: `-vf "subtitles=filename='C\:/Media Work/aligned.ass'"`. Commas, semicolons, and brackets can be parsed as filtergraph syntax when unquoted. If a filename contains an apostrophe or a literal backslash on Linux, use a staging copy with a plain filename rather than guessing the escaping.
- Check filter availability with `ffmpeg -hide_banner -h filter=subtitles`. A build without the `subtitles` filter cannot do the libass preview, but can still extract WAV audio.

For a frame near a cue at 1:02.5, put `-ss` after `-i` so the filter can see the original timeline up to that point. This may decode from the start of the file. Input-side `-ss` is useful for a quick seek but can reset timestamps before the subtitle filter.

```text
ffmpeg -nostdin -n -i "Episode 01.mkv" -vf "subtitles=filename='aligned.ass'" -ss 00:01:02.500 -frames:v 1 -an -update 1 "sync-preview.png"
```

## Verify the result

- Require successful exit, a nonempty subtitle, and a readable report. Inspect `global_offset_ms`, `fps_ratio`, `split_segments`, and `overlap_score_before` versus `overlap_score_after`. Even a higher score can match the wrong speaker or scene.
- Check the new SRT cue order or ASS timing and rendering. Look at the preview if you made one, then watch the same dialogue track at widely separated points and around each split. Verify that ASS signs, layers, and effects remain usable. A still frame cannot confirm dialogue sync.
- Report the chosen stream, what you ran, and which points you checked. If the tools or media were unavailable, distinguish a file or report check from actual playback.

References: [FFmpeg stream selection](https://ffmpeg.org/ffmpeg.html#Stream-selection), [FFprobe options](https://ffmpeg.org/ffprobe.html#Main-options), and [FFmpeg filtergraph escaping](https://ffmpeg.org/ffmpeg-filters.html#Notes-on-filtergraph-escaping). Adapted from [FFmpeg Skill v4.7.1](https://github.com/kajisho5/ffmpeg-skill) under the [MIT license](../LICENSE).
