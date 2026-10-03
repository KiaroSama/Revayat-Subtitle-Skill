# Round-trip and publication re-audit — 2026-10-03

Baseline: `0270cf943216629296f84daa385fef3976faab6a`.
This report distinguishes reproduced behavior, injected-fault hardening and
native CI verification. Earlier preservation CI is baseline evidence, not evidence
for this change. Original subtitle bytes and previous immutable editions remain
unchanged; project/build schemas remain 2.

## Findings and repairs

| ID | Reproduced boundary | Repair and persistent evidence |
| --- | --- | --- |
| C01 | FFmpeg consumes unsupported SRT tags such as `<small>` and `<foo>`, whereas permissive donor conversion exposed them as ASS prose. Closing `</br>` was not a shared display break. | Preflight refuses unsupported decoder-consumed tags with an explicit worksheet-adaptation error. Opening/closing breaks share the scanner. Binary bold/italic/underline/strike behavior and literal comparisons remain controls. |
| C02 | A ZIP-looking special file could reach archive opening before regular-file validation. | `lstat`/`S_ISREG` precedes opening; persistent mocked device and POSIX FIFO tests verify refusal without opening the archive. Existing symlink/reparse protection remains. This is not a race-free filesystem sandbox. |
| C03 | Injected incomplete writes could publish truncated staged source or subtitle bytes through the two direct-write sites. | Both sites reuse the existing exclusive byte publisher with count, flush/fsync and readback checks. Tests require no partial final workspace/edition, preserved originals and successful corrected retries. This does not claim normal buffered CPython writes routinely return silent short counts. |
| C04 | A late donor font/style/reset failure could leave caller-owned presentation or cues partly remapped. | Merge works on copied styles, sections and selected cues, committing presentation only after success. Tests compare caller objects after failure and exercise retry. The builder already copied its input, so this is helper exception safety, not demonstrated ordinary on-disk corruption. |
| C05 | Overlapping whitespace quantifiers made a 50,000-space malformed break exceed the bounded child deadline. | A nonoverlapping, scan-bounded break pattern rejects the same input promptly. ASCII-space attributes and literal tab/NBSP controls remain covered. |

Independent source review identified three additional C01 boundaries: FFmpeg's
127 UTF-8-byte tag-body ceiling, consumed empty tags (`<>`, `</>`) and ASS-control
characters inside completed hidden comments. Persistent tests failed first and
passed after correction. Long tags remain literal; consumed unsupported empty
tags require adaptation; completed comments are removed before checking the
remaining source for ASS control syntax. Unterminated comments are not silently
removed.

The native decoder corpus includes ASCII and multibyte byte-limit boundaries and
empty tags. The former 1,000-space *accepted-tag* assertion was incompatible with
the native scan ceiling: accepted attributes now stay within that ceiling, with
separate oversized-tag literal assertions. No source-integrity assertion was
removed or made an expected failure.

## Verification scope

`tests/test_reaudit_roundtrip.py` contains 40 methods. CI discovery in
`tests/check.py` includes it automatically: the Linux/Windows rendering lanes run
native FFmpeg conversion, closing-break pixel equality and mixed-build
render/QA/ZIP checks; minimum Python and macOS lanes run portable checks with
explicit prerequisite skips. Native commands have wall/idle/output bounds and
owned process cleanup. Fault fixtures use project-contained temporary directories
and restore patches/environment on cleanup; no live service is required.

Local light-loop evidence:

- C01, C02, both C03 staging seams and C04 each failed before their repair and
  passed after it.
- C05's bounded malformed-input child timed out at 8 seconds before repair; the
  same corrected check completed in approximately 0.453 seconds. This is a
  regression observation, not a statistically established benchmark claim.
- The three independent C01 regression methods passed together in 0.005 seconds
  after their observed failures.

The full integrated suite belongs to CI on the final commit, not a duplicate
local run. Consult this change's actual pull-request/commit checks for its result;
this report does not represent prior or reconstructed Linux results as current
multi-platform CI. CodeQL, workflow checks and PR dependency review retain their
existing workflows.

A local-only real-media excerpt rebuild used recipe 8, preserving 120 pre-existing
files and all original sources. Ten fresh images were actually inspected; QA
covered 40 reviewed cues and ten images on the video background, with fonts kept.
The one-member subtitle ZIP was checked against the emitted manifest. This is
bounded excerpt evidence, not full-episode meaning, audio synchronization
(`sync_verified` remains false), or arbitrary-player certification. A final-source
refresh rebuilt the same immutable edition, verified the exact previously viewed
image hashes through QA, and checked 25 installed skill files and 34 unique plugin
members against source bytes and ZIP CRC. Its owned guarded run exited 0 in
25.4 seconds with no termination or leaked child processes.
No private media, worksheet, image or package is included in the public change.

Generation/normalization and sampler versions advance to 8 so older receipts do
not certify the changed conversion and sample plan. Previous builds and receipts
are retained rather than rewritten or transferred.

## Sources and decisions

- [FFmpeg HTML subtitle decoder, inspected commit](https://github.com/FFmpeg/FFmpeg/blob/78bd91056f844d479ef9013c5f6ec5aa891d9ada/libavcodec/htmlsubtitles.c):
  scan ceiling, ASCII-space attributes, binary formatting and unknown-tag behavior.
- [Python `stat`](https://docs.python.org/3/library/stat.html): regular-file mode checks.
- [Python `io`](https://docs.python.org/3/library/io.html): raw versus buffered write contracts.
- [pysubs2 SubRip implementation](https://github.com/tkarabela/pysubs2/blob/master/pysubs2/formats/subrip.py):
  maintained reference inspected; tolerant parsing/blanket stripping is not adopted.

No new runtime dependency, alternate player, translation provider or generalized
HTML parser was added. The repair reuses the existing tokenizer, publisher,
owned subprocess runner and source-preserving worksheet workflow.
