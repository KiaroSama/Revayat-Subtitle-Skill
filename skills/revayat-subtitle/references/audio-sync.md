# Subtitle audio sync

Run `revayat-subtitle sync` for in-process alignment, add `--timeout 300` for a supervised whole-operation deadline, or use the standalone Rust `subsync` binary directly.

```text
revayat-subtitle sync --video "episode.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json" --timeout 300
subsync --wav "episode-dialogue.wav" --in "original.ass" --out "aligned-from-wav.ass" --json-report "wav-report.json"
```

These are alternatives. Choose the one that fits the available media. See [FFmpeg sync and probing](../ffmpeg/references/sync-and-probing.md) for choosing a dialogue stream, checking timestamps and extracting WAV audio.

Video input needs FFmpeg. The binary asks FFmpeg for 16 kHz mono, signed 16-bit little-endian PCM from the first audio stream (`0:a:0`). It extracts Front Center (`FC`) automatically from documented multichannel layouts, including `5.1`, `5.1(side)`, `7.1`, and `7.1(wide-side)`, falling back to a mono downmix when the channel is absent, cannot be checked, or is silent. `TFC` is a different channel and is not treated as `FC`. Probing, Center extraction, and any mono fallback share one extraction deadline; a fallback does not restart the clock. To sync to a different track, supply it as a mono 16-bit PCM WAV with `--wav` instead of `--video`. Choose fresh output paths in existing directories; the tool never overwrites source or destination files.

To select the Front Center (`FC`) channel manually when music or effects dominate a 5.1/7.1 downmix, use this command. In this example, `0:a:0` is the first audio track; change it to the matching dialogue track if needed.

```text
ffmpeg -n -i "episode.mkv" -map 0:a:0 -af "pan=mono|c0=FC,aresample=async=1:first_pts=0" -ar 16000 -c:a pcm_s16le "episode-dialogue.wav"
```

Pass `--wav "episode-dialogue.wav"` and verify by ear that the selected channel contains the expected speech.

Extraction uses `aresample=async=1:first_pts=0` to pad an initially delayed audio track and fill or trim material timestamp discontinuities. FFmpeg's normal container-start normalization stays enabled; a negative absolute container timestamp must not add a false delay. Raw PCM without this timestamp compensation would move delayed or gapped dialogue earlier than the picture. See the [FFmpeg resampler options](https://www.ffmpeg.org/ffmpeg-resampler.html) and [aresample filter](https://www.ffmpeg.org/ffmpeg-filters.html#aresample).

Subtitle and WAV inputs must resolve to regular files. Subtitle input is limited to 32 MiB and WAV input to 1,342,181,376 bytes; reads also enforce the limit if a file grows after opening. Symlinks deliberately selected by the user may resolve to regular inputs. Directories, devices, and named pipes are refused before those reads. The engine still validates WAV structure, supported sample rates, positive cue durations, and bounded alignment timelines.

The engine stages and flushes the complete subtitle and optional JSON report in their destination directories before publishing either path. A staging or write failure leaves those final names available for retry. Publication is exclusive for each individual file. If another writer claims the report name after staging, the subtitle may already have been committed; the error explicitly identifies that complete subtitle and preserves the other writer's report. Two different output paths are not an atomic transaction, so inspect that specific error before choosing a fresh report path.

Ctrl+C requests cooperative cancellation. The executables stop and reap owned FFmpeg or worker processes before returning exit code `130`; detection and alignment also check cancellation between bounded work units. No new result is published after cancellation is observed. A cancellation between the two individual publications has the same explicit committed-subtitle boundary described above. Embedding the Rust library does not install signal handlers; a host may call `subsync_rs::request_cancel()` to request sticky, process-wide cancellation.

## How it estimates timing

- Speech activity: 10 ms hops and a 20 ms Hann window measure RMS relative to the background floor, zero crossings, and energy in the 300 to 3400 Hz band. The detector bridges gaps of at most 200 ms, extends speech by up to 100 ms, and filters bursts shorter than 60 ms.

1. **Global offset and FPS drift:** Normalized FFT cross-correlation compares speech activity with rasterized cues at the original speed and four ratios between 23.976 or 24 fps and 25 fps. FPS scoring includes the full subtitle energy, even for cues beyond the correlation window. The engine applies the chosen ratio before adding the signed offset.
2. **Cuts and edits:** Split-penalty dynamic programming searches local offsets and correlation peaks. It compares speech overlap against silence and charges a penalty for changing offset.
3. **Per-dialogue cue-by-cue alignment:** Every dialogue cue must be aligned with its own spoken audio. Refine each drifted cue's start and end individually, within neighbor-safe bounds and the local offset limit set by `--max-split-offset-ms`. The report records local alignment in `split_segments` for replay, using inclusive, one-based cue positions in time order. Keep back-to-back dialogue cues with original gaps of 150 ms or less together, without introducing artificial gaps or dropping cues. A uniform global shift alone does not meet the requirement.
4. **Bounded speech-onset snapping:** The engine targets 50 ms before a nearby speech onset by default. For confirmed speech segments, it can look back for a quiet, low-pitched attack using energy from 125 to 3400 Hz, upper-band corroboration, and a local noise floor. It screens out isolated bass or drones and flat, continuous music. A qualifying run already active before the lookback or previous-segment boundary does not create a new onset at that boundary. Candidate search includes recovered onsets and ASS centisecond rounding throughout the permitted snap window; the final formatted target must still satisfy the exact movement and neighbor bounds. Before ranking candidates, it compares speech overlap from the actual onset, excluding the intentional lead-in. A snap must leave at least 500 ms before the cue end as it stood before end adjustment. The engine and worker replay both reject changed starts at zero and forward moves of 20 ms or less. Snaps respect neighbors and skip intentionally overlapping ASS groups.
5. **Bounded cue-end adjustment:** For continuing speech, use the cue's own unpadded speech segment when available; a padded or merged interval alone does not justify crossing into the next phrase. Aim for about 250 ms of trail-out within the movement limit, the next cue, and the audio duration, even if speech outlasts the budget. If the ceiling is at or before the current end, leave the end unchanged in this branch. Dead-silence trimming uses a separate rule. Leave about 900 ms for reading when the timing bounds allow it.

The global search follows the [FFsubsync approach](https://github.com/smacke/ffsubsync#how-does-it-work). The split penalty follows the principle described by [ALASS](https://github.com/kaegi/alass). The engine implements both in Rust and does not run either external tool.

## Options

| Option | Default | Use |
|---|---:|---|
| `--ffmpeg PATH` | `ffmpeg` | Select an FFmpeg executable. No effect with `--wav`. |
| `--max-offset-sec N` | `60` | Search within N seconds of zero for the global offset, from 0 to 600. |
| `--max-split-offset-ms N` | `15000` | Search this far from the global offset for local shifts, from 0 to 60000. |
| `--split-penalty N` | `7.0` | Raise it to discourage cuts; lower it if a known edit is missed. |
| `--snap-ms N` | `350` | Limit changes to cue starts to this distance from the aligned position. Use `0` to disable. |
| `--lead-in-ms N` | `50` | Start eligible cues this many milliseconds before detected speech, from 0 to 500. |
| `--end-adjust-ms N` | `600` | Maximum cue-end movement in milliseconds. Use `0` to disable. |
| `--no-fps-search` | off | Keep the original subtitle speed. |
| `--json-report PATH` | stdout only | Also save the JSON report to a new file. |
| `--timeout SECONDS` | unset | Select supervision and bound the whole operation, including extraction, alignment and staging. Use with `revayat-subtitle sync`; allowed range is greater than 0 and at most 86400. Without this flag, in-process video extraction has a 300-second deadline. |
| `--subsync PATH` / `--sibling-subsync` | in process | Select a compatible external executable. The supervised whole-operation deadline defaults to 300 seconds unless `--timeout` is supplied. |
| `--ffmpeg-timeout SECONDS` | `300` | Standalone `subsync` only: one shared budget for probing, Center extraction and any mono fallback, not WAV processing or CPU alignment; greater than 0 and at most 86400. Owned-process cleanup remains bounded separately. |

## What stays intact

The parser accepts UTF-8 with or without an initial BOM and keeps the input's line endings. SRT timing uses millisecond precision; the engine retains cue numbers, multiline text, tags, and timing-line position settings. ASS/SSA timing uses centiseconds, with a minimum retimed span of 10 ms. The ASS reader preserves event order, comments, style fields, override tags, and unmodified sections such as `[Fonts]` and `[Graphics]`. ASS dialogue intervals that originally overlap can continue to overlap; the engine does not introduce overlap between independent groups. Cue-local effect and karaoke parameters are unchanged, so check their rendering after an FPS correction.

CR, LF, CRLF and mixed line separators are preserved. A numbered or unnumbered SRT timing block embedded in the preceding cue without a blank separator is ambiguous and is refused with a line-specific error; repair that separator before retrying. The engine must not silently retime only the first part of such a file.

## Check the result

The report gives cue and speech segment counts, the FPS ratio, offset and split estimates, snapped and end-adjusted cue counts, and median and maximum start-time changes. An end-adjustment count records moved endpoints; a bounded extension may still end before the spoken segment finishes. `overlap_score_before` and `overlap_score_after` measure the fraction of cue time the detector marks as speech. They do not measure whether a line belongs to the person speaking.

The supervised bridge validates the worker's JSON types and numeric bounds, contiguous split coverage, requested FPS and offset search limits, and agreement between input/output cue counts and the report's actual timing deltas. Duration cannot exceed the engine's 2^22-frame timeline, and speech segment count cannot exceed its number of 10 ms frames. Returned subtitles must parse as UTF-8 in the input format, preserve every byte outside timestamp fields after the parser's existing initial-BOM normalization, and keep SRT cues ordered without new overlap. These protocol failures are refused before either destination is published; both final paths remain available for a corrected retry. Compatible older helpers may omit optional path metadata, and additional report fields are retained. Executable overrides remain trusted programs with normal user permissions; this validation is a protocol check, not an execution sandbox or a certification of alignment quality.

Endpoint validation replays FPS rounding, format precision, split shifts, ASS group clamping, and SRT ordering repair. It checks that changes to eligible starts and ends stay within their separate configured limits and agree with the reported `cues_snapped` and `cues_end_adjusted` counts. A snapped start must leave at least 500 ms before the replayed end that existed before end adjustment, and the final adjusted cue must also keep that minimum duration. A changed start plus its configured lead-in must be before the reported audio ends. Zero search, snap, or end-adjustment limits still permit the documented format repairs. The report does not replace listening to the result.

Silence and continuous detected activity both lack the timing anchors needed for alignment and are refused before output publication. A constant tone must not produce a successful FPS correction merely because every cue overlaps detected activity.

If the report shows a split you do not expect, listen on both sides before adjusting `--split-penalty`. If the estimated FPS ratio is wrong, try `--no-fps-search`. If audio is silent, music masks most speech, or the selected audio track differs from the subtitle release, use the matching dialogue track or retime manually. Verify the beginning, middle, and ending by ear and revisit every reported split. Use playback to check signs, songs, and layers that have little or no spoken audio.
