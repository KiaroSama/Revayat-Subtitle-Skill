# Render snapshots and track-value audit — 2026-10-04

**First obey every applicable owner Rules file, global/project instruction, hook and the configured Spec Kit chain.** This public audit does not replace private review gates. The audit author only proposes code and opens a PR; integration, history correction and PR disposition remain with the independently authorized reviewer.

Audited revision: `da7bd571bfa0776f8e9e1a70ca01f41f62c04516`, after PR #10 was merged. The accepted ASS grammar/style-binding fixes are retained. This pass addresses three concrete boundary defects, not a claim that every possible subtitle, platform or external renderer has been exhaustively certified.

## R01 — A render receipt can describe bytes that were not rendered

**Severity: high for evidence integrity.** `render.render` validates the build and later stages its subtitle through an unchecked `shutil.copyfile`. It fingerprints the canonical build again at the end, but never checks the staged subtitle. A failed/corrupted copy, or a transient source replacement between those operations, can render different captions while the receipt records the canonical subtitle hash. Restoring the canonical file makes the final `load_build` succeed. Copying the resulting PNG also lacks readback verification: the routine can return success with a frame whose bytes do not match the recorded digest, although subsequent QA correctly catches that second problem.

**Deterministic reproduction:** render an authored `VISIBLE AVAVAV` ASS normally. Intercept only the old staging copy and replace `VISIBLE` with `ALTERED` in `input.ass`; keep all canonical build/workspace bytes unchanged. The old implementation returns a valid-looking receipt with different pixels. Separately intercept the old final PNG copy and append bytes: the old routine returns an evidence record whose frame digest does not match the delivered frame. These are fault-injection reproductions, not assertions that an attacker can defeat an arbitrary trusted filesystem.

**Implemented correction:** `subtitle_snapshot` performs a bounded regular-file read through `local_path` and `read_limited`, verifies the bytes against the episode's expected SHA-256 and returns those exact bytes. The 64 MiB encoded ceiling covers the parser's 16 Mi-character limit even with four-byte UTF-8. Render staging and PNG publication use the existing exclusive, short-write/readback-checked `publish_bytes` primitive. Font staging also reads a bounded snapshot, checks its length/hash against the previously captured fingerprint and publishes those bytes; existing font fingerprint verification remains. Packaging uses the same subtitle snapshot immediately before `writestr`, preventing a post-QA source change from being compressed or unboundedly read before verification. Final workspace, video and renderer checks remain enabled.

This closes the unchecked-copy boundary; it does not introduce cryptographic authentication against an actor who can modify the process, its memory or every local artifact. A valid hash is not a human visual approval. External font bytes and private media are not added to the repository.

**Acceptance:** the staged-caption counterexample must either be refused or produce the canonical caption; every successfully returned PNG must match its receipt; short writes and readback faults must stop before receipt publication; changes after initial validation must not reach the renderer/ZIP writer; old approvals and source bytes must survive failures; corrected retries must work. Test the positive ASS/SRT snapshots and real full delivery, not only exceptions.

## R02 — Render scratch cleanup can replace the real operation outcome

**Severity: medium.** Import/build already use the identity-checked, outcome-preserving `runtime.staging_directory`. Rendering still uses `TemporaryDirectory.__exit__`. A failing removal can turn successful FFmpeg work into an exception, or replace an actual renderer error/cancellation with an unrelated cleanup `OSError` before evidence handling runs. The earlier shared-helper fix was not applied at this call site.

**Implemented correction:** use `staging_directory(build, ".render-")` for the render input directory too. Keep ownership/reparse/replacement checks from the shared helper. On cleanup failure, retain a clearly logged recovery directory; do not delete an unrelated replacement, swallow the primary error, bypass the image-review gate or overwrite prior reviewed images.

**Acceptance:** simulate cleanup failure after successful actual rendering, after a renderer `ValueError` and during `KeyboardInterrupt`; the first must still produce unreviewed evidence and the latter two must preserve the exact exception object. Ordinary cleanup and all existing import/build/package/installer failure tests remain enabled. A recovery directory is diagnostic evidence, not an approved output or an unfinished code fix.

## R03 — Unicode whitespace can bypass donor track-setting reconciliation

**Severity: high for presentation preservation.** `_merge_donor.script_info` uses unrestricted `value.strip()`. Python removes NBSP and other Unicode whitespace; libass's header grammar does not use that equivalence. Consequently `Kerning: yes` and `Kerning: \u00a0yes` compare equal in the helper despite different effective renderer values. The donor then inherits the base's setting, silently changing its typography. This is a remaining value-comparison gap adjacent to the previous canonical-label fix, not a reason to undo that fix.

**Native evidence:** actual FFmpeg 7.1.5/libass rendering of authored `AVAVAV To To` captions distinguishes the ordinary-space and NBSP kerning inputs. The relevant upstream `parse_bool` skips the renderer's ASCII space/tab and checks a `yes` prefix or a positive integer. Broad Unicode trimming is not a safe canonicalizer.

**Implemented correction:** trim only ASCII space/tab in compared values, matching the conservative grammar boundary already applied to header labels. Do not normalize Unicode inside `Language`, matrix names or other values. Reject differing values before committing any donor style/font/cue mutation. Preserve identical Unicode values literally, allow equivalent ASCII padding and maintain last-effective-header behavior. A source whose retained donor differs must be deliberately reconciled; an entirely excluded alternate does not impose unused presentation settings.

**Acceptance:** cover 11 tracked fields × 7 non-ASCII whitespace characters × both merge directions (154 deterministic combinations), identical-value and ASCII-padding controls, native kerning pixels, exception atomicity and complete build refusal/retry without changing imported sources. The 154 combinations are not 154 independent bugs; not every header/character pair is claimed to yield a distinct screenshot.

## R04 — ASCII padding can change a hexadecimal track value

Independent integration review found a native counterexample to treating every
ASCII-padded value as equivalent. libass `parse_int_header` chooses hexadecimal
base only when `&H`/`0x` starts at the original value's first byte, before skipping
ASCII spaces/tabs. `PlayResX:&H140` resolves to 320; `PlayResX: &H140` resolves to
zero, then the 180-height fallback gives a 240-wide canvas. Actual nonblank native
rasters prove unpadded hex equals decimal 320, padded hex equals decimal 240 and
the two differ. The proposed comparator nevertheless allowed their donor merge.
The same minimized regression failed on missing refusal, then passed after repair;
its four raster controls and atomicity assertions were not weakened.

The shared comparator now refuses leading ASCII padding before hexadecimal
prefixes in PlayResX/Y, LayoutResX/Y and WrapStyle. It does not implement a second
native integer parser. Unpadded hexadecimal values, hexadecimal suffix padding
and padded decimal values remain supported. Empty donors impose no unused
constraint; refusal preserves both source objects and a corrected retry works.
Three permanent methods extend the original 31 unchanged methods to 34, including
10 equipped-renderer methods. The portable 60-case prefix/field/padding matrix is
one defect class, not 60 independent bugs.

## Compatibility and verification

Project/build schemas and generation/normalization recipe stay unchanged at 2 and 10. The render sampler/receipt recipe advances to 11 so existing receipts created before the snapshot boundary must be regenerated; valid build identities, sources, worksheets, glossary and previous edition bytes remain unchanged. Re-render the existing valid edition, inspect every fresh image, then record genuine observations. Never transfer old approval flags to new screenshots.

The new `tests/test_render_snapshot_contracts.py` is picked up by the existing `test_*.py` discovery. Its native methods run in equipped Linux and Windows renderer lanes; other methods run in the existing portable/minimum-Python/macOS lanes. The associated PR records the actually measured baseline failures, candidate counts, native CI outcomes, exact tested SHAs and skips. A failed preliminary run is not a passing result, a skipped test is not an executed test, and repeated matrix runs are not independent test counts.

Run from the repository root:

```powershell
python -X utf8 tests/check.py
python -X utf8 tests/check.py --render
```

Use the owner's existing logged PowerShell 7 runner and check every native exit code. Install only the existing test prerequisites; do not replace pinned dependencies merely to obtain green checks. Preserve all current native installation/rollback, owned-process, archive/PNG, source integrity, immutable-metadata, rendering and language-evaluation contracts.

## CI and optional improvements

The current CI already runs Python 3.10/3.14, Linux/Windows/macOS, native Linux/Windows FFmpeg, CodeQL, blocking workflow lint and Dependency Review. Dependabot covers Actions and both pip locations. Extend these gates with the new discovered regressions rather than adding redundant jobs or permanent write permissions.

Evaluate optional improvements against actual demand: a read-only effective-track-value diagnostic with escaped code points; a bounded, version-pinned libass differential corpus; structured failure summaries that link source/cue IDs without copying dialogue or credentials. Accept or decline each with a reason in the owner's required records. None is a prerequisite to hide an unresolved R01–R03 defect, and adopting one must include its tests in the same session.

The three optional enhancements were evaluated and not selected for this repair:
existing source/cue-linked exceptions suffice without a new diagnostic command;
the authored native positive/negative controls cover the confirmed discrepancy
without a new corpus maintenance dependency; existing structured operational logs
already preserve failure category and recovery context without copying dialogue.
These decisions do not defer any confirmed R01–R04 correction.

## Independent delivery and identity evidence

A separately copied retained edition was rendered with sampler 11. All ten fresh
images were actually viewed and received new hash-bound visual observations;
no old approval flag was transferred. QA verified 40 reviewed cues and ten frames.
The single-ASS delivery ZIP passed member, CRC and subtitle SHA-256 checks.
All 166 original workspace files stayed byte-identical; copied previous files and
the prior sampler-10 review were preserved. The generation-10 build identity did
not change. The installed 25-file skill and 34-file plugin archive match current
source, and the previous installation is backed up. This is bounded excerpt
visual/mechanical evidence, not audio synchronization, full-episode translation
or active-player track certification. No private media or font was published.

The two unsigned, GitHub-owner-linked proposal commits were corrected locally
with a verified private bundle and metadata-only partial rewrite:
`124f9831e6af6241043212ab31e18e3540e22208` →
`5f17562059e4db75e10c1824a0a263c3b2725161`, and
`fcf3f629642701aaa392904b16c7cabff9067b5a` →
`6dbd275a31cdcf301312c905448c4d37397ec927`.
Raw author and committer emails use the approved public identity. Content trees,
parent topology, contributor names, dates and messages remain unchanged; no
signature was invalidated and genuine bot/third-party ancestors remain intact.
Remote publication requires an exact affected-ref lease against the observed
original head. Historical green CI on the original proposal is not evidence for
the final correction; current PR and integrated-main checks must verify their
exact SHAs before closure. Hosting-provider test objects and cached originals
are different surfaces; this rewrite does not promise their global erasure.

## Primary references and related implementations

- [libass header parser, pinned source](https://github.com/libass/libass/blob/f61db567e6593df3470e91594bcd4ad2d0473aff/libass/ass.c): `parse_bool`, header handling and renderer state, not generic HTML/Unicode whitespace assumptions.
- [libass utility grammar](https://github.com/libass/libass/blob/f61db567e6593df3470e91594bcd4ad2d0473aff/libass/ass_utils.h), [native test corpus](https://github.com/libass/libass-tests), [FFmpeg filters](https://ffmpeg.org/ffmpeg-filters.html): exact input-byte and same-renderer before/after tests.
- [Python string trimming](https://docs.python.org/3/library/stdtypes.html#str.strip), [temporary directory cleanup](https://docs.python.org/3/library/tempfile.html#tempfile.TemporaryDirectory), [hashlib](https://docs.python.org/3/library/hashlib.html): distinguish Unicode trimming, cleanup failures and byte identity.
- [Aegisub](https://github.com/TypesettingTools/Aegisub), [pysubs2](https://github.com/tkarabela/pysubs2), [Hypothesis](https://github.com/HypothesisWorks/hypothesis): reference implementations and testing patterns, not newly added production dependencies.
- [Spec Kit current process](https://github.github.com/spec-kit/), [agentic command reference](https://github.com/github/spec-kit/blob/main/docs/reference/agentic-sdd.md): follow the actual installed owner-governed chain; do not reinitialize ignored private specifications or report merely written checklists as executed skills.
- [git-filter-repo](https://github.com/newren/git-filter-repo), [Git push leases](https://git-scm.com/docs/git-push): independently authorized historical identity correction needs verified private backups, an old-to-new mapping and protection against concurrent changes.

Retain the [existing broader research](../skills/revayat-subtitle/references/research.md). No third-party implementation or font binary was copied. This public document contains bounded technical evidence, not private Rules/Spec Kit files.
