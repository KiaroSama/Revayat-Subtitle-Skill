# Rust v3 audit: implemented repairs and reviewer completion instructions

**First read and obey the owner's applicable Rules, AGENTS instructions, hooks, and configured Spec Kit chain. Keep those controls active through research, implementation, review, integration, and publication. Never weaken a check or bypass a hook to obtain a passing result.**

This is a fresh audit of the complete Rust rewrite at `93a24253224c2682f90880a260ad8026f00f82d9`, performed on November 25, 2026. The repairs belong to the PR linked from [issue #33](https://github.com/KiaroSama/Revayat-Subtitle-Skill/issues/33), on `audit/rust-v3-correctness-20261125`. Review that implementation and its regressions together. The earlier Python/PyO3 audit is historical evidence for its named revision, not a certification of v3.

The audit author may open and update the repair PR and issue, **but may not merge or close them**. The completion instructions below are for the owner's subsequently authorized reviewer.

## 1. Scope and evidence

The review covers the three Rust crates, public CLI and compatibility APIs, subtitle parsing/conversion/normalization, editorial identities and migration, validation, rendering/PNG checks, status/handoff/package boundaries, installer ownership/recovery, subprocess supervision, audio synchronization, plugin manifests, workflows, dependency automation and relevant documentation. Existing review comments, including PR #32's overflow/minor comments, were checked against this checkout rather than accepted as current defects automatically.

The untouched baseline passed **1059 tests, 0 failed, 0 ignored**. Authored negative cases then exposed defects beyond that suite. Local practical checks use Rust `1.100.0`, Linux x64, FFmpeg `7.1.5` with libass, temporary installation homes, and authored subtitles/audio/video. Preserved baseline executables were used for before/after CLI comparisons. No private subtitles, production workspace, personal installation or signing secret was used.

The initial complete integrated post-fix run passed **1151 tests, 0 failed, 0 ignored**; formatting, warnings-denied Clippy and a release build also passed. This includes the mechanical bridge-test extraction and the timeout-cause regressions. The PR supplies the final revision and hosted CI receipt, including the subsequent R04 endpoint-validation review follow-up. Counts are test functions, not loop examples; repeated focused runs are not added to the workspace total. Local Linux results do not stand in for Windows or arm64 execution.

A separate release-binary smoke completed doctor, prepare, pending status, reviewed build, real FFmpeg/libass rendering, QA, complete status, handoff and package. Both 1280-by-720 images were inspected individually: the authored Persian/`OpenAI` line, italic text and Latin `123` remained readable with margins. The ZIP contained exactly `Sub/S01E01.ass`, passed CRC validation and matched the built bytes. Source hashes and the full workspace file snapshot remained unchanged through read-only QA/status/handoff/package. The neutral-background fixture correctly retained `sync_verified=false` and `linguistic_quality_certified=false`.

The public checkout contains `docs/agents/issue-tracker.md`, but no private owner Rules, local `AGENTS.md`, configured hooks, or `.specify` chain. This report does not claim those unavailable private controls ran. In the owner's environment, discover and follow the actual controls without replacing or publishing them.

## 2. Synchronization and process ownership

### R01 — Raw PCM extraction lost the media clock

**Reproduction/impact:** A 10-second authored Matroska starts video at zero and its 8-second PCM track at 2 seconds. Captions already match playback. Standalone, default in-process and supervised baseline sync report 8000 ms and offset -2000 ms, moving the first cue from `02.350–02.970` to `00.350–00.970`.

**Implemented repair:** `subsync-rs/src/audio.rs` adds `aresample=async=1:first_pts=0` before PCM extraction. Raw samples now preserve leading delay and material packet gaps, while FFmpeg's existing container-origin normalization remains intact. The documented selected-track WAV command uses the same filter. This fills/trims timestamps; it does not enable tempo stretching.

**Acceptance:** `sync_media_clock.rs` checks delayed audio, packet gaps, zero-start audio and negative container origins. Real CLI probes of all three routes now report 10000 ms, zero offset and the original first cue. Preserve the positive controls. FFmpeg's default hard-compensation threshold remains 0.1 seconds; do not claim sample-exact repair of every tiny timestamp jitter.

### R02 — Interruption abandoned owned FFmpeg processes

**Reproduction/impact:** Send SIGINT while actual FFmpeg waits on authored media. The baseline CLI exits by signal but FFmpeg remains alive in its independent process group. CPU alignment also lacks cooperative cancellation.

**Implemented repair:** Executables install `ctrlc` handlers with termination support. Sticky cancellation APIs in the core runner and sync library are checked before launch, during process polling, VAD, cue rasterization, FFT/DP work and before publication. Cancellation enters the existing terminate/reap path; CLI status and final operational log agree on exit `130`. Libraries expose cancellation without installing a global handler for embedding applications.

**Acceptance:** `process_cancellation.rs` and `sync_cancellation.rs` test owned descendants, released locks, sticky cancellation, SIGINT/SIGTERM and no premature output. Actual main default/supervised probes return `130` with no live FFmpeg. Existing timeout and orphan-process tests remain green. An embedding process deliberately requesting cancellation must not silently start later work through the cancelled global runner.

### R03 — Standalone subtitle and WAV reads could block or exceed bounds

**Reproduction/impact:** FIFO subtitle/WAV paths keep standalone sync blocked beyond the probe deadline. Bridge-only checks did not protect standalone or library callers.

**Implemented repair:** Shared `subsync-rs/src/input.rs` validates path and opened-handle regular-file metadata, bounds reads to the limit plus one byte, and rejects growth. Unix nonblocking open prevents a FIFO substituted between checks from blocking. Subtitle input is capped at 32 MiB; WAV at 1,342,181,376 bytes, consistent with the decoded-PCM limit and container allowance. User-selected symlinks to regular files remain supported.

**Acceptance:** `sync_input_boundary.rs` checks ordinary files, oversized sparse inputs, regular-file symlinks, directories and both FIFO cases. Rejection must occur before reading an excessive body or creating final outputs. Preserve RIFF/PCM/sample-rate/timeline validation as separate checks.

### R04 — Successful external workers could publish invalid output

**Reproduction/impact:** A helper returning zero could publish arbitrary text, invalid UTF-8, negative/invalid report values, malformed split ranges, or a subtitle whose cue count and timing disagree with its report.

**Implemented repair:** `revayat-cli/src/sync_bridge.rs` validates a typed report and actual output before either final destination is staged. Counts and timeline dimensions are bounded; FPS must be an allowed candidate; overlap and shifts must satisfy configuration; split coverage must be contiguous. Output must be a regular nonlinked file with valid UTF-8/format grammar. Compare exact source bytes outside timestamp ranges after the existing initial-BOM normalization; recompute cue counts and start-delta statistics. Preserve intentional ASS event order/overlap, attachments, comments and reversed timing columns. Optional path metadata remains optional and additional report fields survive.

**Final PR review closure:** A second red reproduction showed that an output moving one cue from 1000–2000 ms to 3600000–3601000 ms could still pass when all requested shift/snap bounds were zero, if it supplied accurate delta statistics and reported zero shifts. An end-only alteration was also accepted. The engine and bridge now share deterministic replay of FPS rounding, format quantization, stable timing order, connected ASS groups, reported total shifts, group/negative clamping and cumulative SRT repair. Require exact replayed end times. Starts may differ only for an eligible singleton's bounded snapping: sufficient pre-snap duration, at least 500 ms remaining, the previous group's end respected, a changed start before the reported media duration, and an exact changed-start count. Actual-engine 1050/1055 ms onset controls retain valid subframe SRT behavior. Unchanged cues and cue ends are not clamped to the media duration. Keep semantic audio/onset quality outside this structural check; never guess a large endpoint tolerance.

**Acceptance:** Bridge unit tests and `sync_worker_boundary.rs` cover invalid fields/content, real-engine reports, SRT ordering, ASS preservation, direct/supervised equivalence, eight malformed helper variants and successful retry on the same destinations. Specific replay controls cover exact positive/negative limits, zero-limit SRT overlap/cumulative one-millisecond repair, FPS round-then-floor/grouping, source-order-preserved ASS shifts, group clamping, and snap eligibility/distance/duration/neighbour/count limits. The current CLI was also run with the actual archived baseline `subsync`; it succeeds with byte-identical output. An executable override is still a trusted program, not a sandbox or perceptual alignment certificate.

### R05 — Compatibility timeout diagnostics hid the actual deadline

**Reproduction/impact:** An idle timeout of 2 seconds with a 5-second wall timeout reports `subprocess.TimeoutExpired: 5`; wall expiry also omits the cause. This obscures diagnosis through the public compatibility adapter.

**Implemented repair:** The adapter retains `subprocess.TimeoutExpired:` and includes the existing typed timeout error's idle/wall cause and actual elapsed time. Cleanup remains in the owned runner.

**Acceptance:** `process_control_tests.rs` has real-process idle and wall regressions. Both check the reason, descendant launch, no delayed escaped write and released OS lock. Do not reintroduce timeout formatting from the unrelated configured wall value.

## 3. Subtitle, encoding and workspace integrity

### R06 — Removing a comment could silently remove literal dialogue

**Reproduction/impact:** `A<!<!--note-->--hidden-->B` becomes a newly active hidden comment after cleanup; baseline reviewed-workspace build publishes `AB`. The rescan excluded hidden tags and missed this semantic change.

**Implemented repair:** Markup cleanup refuses a join that creates a new complete hidden comment, using the existing explicit-adaptation diagnostic before publication. Ordinary and unterminated comments retain their contracts.

**Acceptance:** `audit_pipeline_v3_formats.rs`, `audit_pipeline_v3_workspace.rs` and `audit_pipeline_commands.rs` cover direct cleanup and real build. Adapt the authored worksheet and retry successfully while preserving source bytes. Disabling comment validation or recursively deleting the newly formed comment is unacceptable.

### R07 — ASS normalization corrupted overrides and exhausted regex work

**Reproduction/impact:** In `{\fnمي روم{X}مي روم`, the real ASS override ends at the first closing brace, but normalization selected an inner block and changed font-name bytes. Separately, 100000 unmatched opening braces followed by Persian prose exhausted the regex backtracking budget.

**Implemented repair:** An ordered scanner applies ASS first-close, escape and drawing grammar, preserves HTML comment/attribute precedence, and remembers when no later closing brace exists. It does not raise regex limits or silently fall back to a different grammar.

**Acceptance:** Format and public-convert regressions assert byte-preserved override parameters, normalized visible text, comments/tags containing braces, and bounded processing of the adversarial opener sequence. Existing typography and preservation properties remain green.

### R08 — Sibling bidi isolates were treated as an outer paragraph isolate

**Reproduction/impact:** A line starting with `RLM RLI OpenAI PDI` and ending with a separate `RLI Persian PDI` is returned unchanged, although no isolate encloses the full paragraph.

**Implemented repair:** The shared predicate follows nesting and matches the initial isolate to its actual closing PDI, requiring that close to end the visible line. Siblings receive a true outer isolate; a complete outer scope with nested isolates remains idempotent.

**Acceptance:** Both public ASS/SRT anchoring paths cover sibling and nested scopes in `audit_pipeline_v3_formats.rs`. Preserve existing bidi properties; checking only the first and last character is insufficient.

### R09 — Parser, generated-style, title and timing contracts disagreed

**Reproductions:** Three/five newline characters between SRT cues fail while two/four succeed. Custom `default`/`DEFAULT` styles generate references to undefined `Default`. Leading NBSP is inconsistently removed from a generated style and Translator Mode placeholder. A requested title disappears when native ASS lacks Script Info. Effective-time APIs accept unsupported format kinds.

**Implemented repairs:** Consume complete SRT blank-line runs, including space/tab-only lines. Use one canonical generated-style resolver for definitions/events/placeholders while preserving meaningful Unicode spaces. Insert Script Info for a requested title. Require `ass` or `srt` before timing calculations. Native ASS definition semantics and source documents are not rewritten implicitly.

**Acceptance:** Format regressions cover LF/CRLF/CR, all separator runs, malformed subsequent blocks, aliases/Unicode style names, title insertion without caller mutation and invalid kinds in both APIs. Public convert checks cue retention.

### R10 — Codec preflight and decoding disagreed

**Reproduction/impact:** Validation accepts UTF-32, UTF-7 and other codecs the decoder cannot handle; `iso8859-1` maps byte `0x80` to Euro while other Latin-1 aliases map it to U+0080.

**Implemented repair:** Shared `validation/encoding.rs` serves both validation and decoding. Implement strict UTF-16/UTF-32 with explicit endian/BOM rules; reject partial units, invalid surrogates/scalars and unsupported UTF-7/escape/OEM labels before mutation. Latin-1 aliases use actual ISO-8859-1 byte mapping; explicit Windows-1252 remains distinct. Preserve original source bytes.

**Acceptance:** Workspace and CLI regressions cover accepted aliases, BOM/endian cases, malformed UTF-32, ASCII strictness and unsupported codecs before parent/staging creation. Keep browser-oriented `encoding_rs` label behavior from leaking into the explicit Latin-1 file contract.

### R11 — A non-UTF-8 work path failed after publication

**Reproduction/impact:** On Linux, import publishes a workspace at an invalid-UTF-8 path, then fails while creating its report. Retry encounters an occupied workspace.

**Implemented repair:** Validate the destination before any parent/staging/publication mutation. `path_marker` now returns `Result` and rejects lossy conversions for discovered inventory paths too; callers propagate the error.

**Acceptance:** Unix API/CLI regressions assert absent output/parent on refusal and strict inventory markers. This is a documented Rust API correction, not a claim of source compatibility for callers expecting a plain value.

### R12 — Compatibility markers broke literal JSON identity

**Reproduction/impact:** Project metadata `{"$tuple":[1]}` and `[1]` produce the same identity and reuse one edition; literal `$bytes`/`$set` objects fail. Sorted legacy `$dict` serialization also ignores its requested ordering.

**Implemented repair:** `contract_json::dump_json` treats persisted JSON literally for identities and report budgets. Keep marker interpretation in the explicit legacy `dump` adapter, with recursive requested dictionary ordering, exact cross-numeric ordering and controlled refusal of malformed/incomparable keys.

**Acceptance:** Workspace tests prove distinct marker-looking objects cannot share an identity, ordinary JSON retains its canonical form, numeric keys keep exact order, and report size limits use the actual literal serialization. Do not remove compatibility support globally to fix ordinary JSON.

### R13 — Donor compatibility API discarded required base mutations

**Reproduction/impact:** The wrapper returns cues referencing `s0002__Default` while its caller's base still contains only `Default`, because the modified clone is discarded.

**Implemented repair:** `workflow::merge_donor` accepts `&mut Value` and commits updated styles/fonts/metadata only after the complete merge succeeds. The typed donor API remains unchanged. The new core README documents this intentional Rust call-site migration.

**Acceptance:** Serialize merged cues against the updated base and verify a later invalid donor leaves it byte-equivalent to its prior value. Do not return remapped cues without committing their definitions.

### R14 — Pending review masked an invalid project output format

**Reproduction/impact:** A pending project edited to `output_format: "vtt"` is reported valid-pending with exit 1.

**Implemented repair:** Validate a present format before review completeness. Invalid values produce exit 2; omission remains readable for older workspaces.

**Acceptance:** Workspace and actual status CLI regressions distinguish valid pending, missing legacy value and invalid present value.

### Migration required by R06–R14

Generation recipe is **14**, normalization recipe **13** (previously 13/12). Changed output and identity rules must create a new edition with fresh image evidence. The explicit old-generation regression preserves project/source/worksheet/glossary bytes, historical edition bytes and old approval records, while refusing to copy those approvals into the replacement. Never hand-edit approval hashes to make an old review cover new output.

## 4. Installer, diagnostics and smaller public boundaries

### R15 — Voice lookup query/discovery paths produced wrong results

An empty selected first translation in `/ سلام` matches every query; it is now skipped. A regular local file named `N` makes query `N/A` raise NotADirectory; this impossible path spelling now becomes query text. An empty nearer automatically discovered bank hides a populated later bank; automatic discovery now requires a bank entry, while explicitly selected empty banks remain intentional. Lint filename headings now use existing control flattening/redaction so ESC/newline cannot forge diagnostics.

**Acceptance:** Core lookup tests, `audit_cli_20261125.rs` and actual preserved-baseline/rebuilt CLI probes cover unrelated-query refusal, valid first alternatives, `N/A`, later-bank discovery, explicit-empty controls and malicious filename controls. Requested subtitle text and JSON filename semantics remain intact.

### R16 — Default uninstall selected uninstalled hosts

With `.claude` and `.cursor` present but only Claude installed, default uninstall fails on the absent Cursor target and leaves both Claude skills. A lone companion cannot be removed either. Automatic all-agent uninstall now discovers main/companion presence separately with non-following metadata. Explicit target selection remains strict.

**Acceptance:** `audit_install_20261125.rs` covers mixed hosts, companion-only removal, explicit absence and foreign/dangling entries. Complete authenticated preflight must still precede live mutations; retained backups remain recoverable.

### R17 — Authenticated ownership missed executable-bit changes

Changing an installed binary from 0755 to 0644 leaves bytes unchanged, so baseline uninstall removes that modified tree. Recovery similarly accepts a changed old execute mask. New Unix manifests contain an authenticated optional `executables` map of nonzero `mode & 0111` masks, and inspection/comparison enforce it. Empty maps are recorded too.

**Acceptance:** New and legacy installed-binary changes refuse without deleting live data; restoring permissions permits retry. New-journal recovery detects old-tree mode changes, and legacy unchanged recovery remains idempotent. Validate keys, mask types and allowed bits. Legacy arbitrary-tree journals never recorded permissions, so historical masks cannot be reconstructed; known bundled binaries still require their original 0111 execute policy. This authenticates execute bits, not ACLs, ownership or every metadata field.

**Downgrade control:** The actual archived baseline installer refuses newer installed, interrupted, committed-uninstall and restored journals with exit 2 while preserving file/directory identities, bytes and modes. The compatible current installer uninstalls safely, retains the executable backup, restores the original inode/bytes/0700 mode, and supports repeated recovery. This authenticated interrupted-state fixture is not a claimed real crash. Retained terminal journals still require the compatible installer; successful uninstall does not downgrade the registry. Keep that installer outside installed trees and follow the [downgrade guidance](platforms.md#installer-downgrade-compatibility). Never strip authenticated execute evidence or prune history to make an older reader accept it.

### R18 — Distribution budgets were enforced after excess reads

Seventeen sparse 16 MiB files followed by an absent source make baseline hashing exceed its 256 MiB aggregate budget before returning an unrelated missing-file error. The entry-count analogue also reaches an excessive source. Manifest construction now validates names/uniqueness and accumulates file/directory counts, checks remaining bytes before opening each file, verifies observed size afterward, and retains final full validation.

**Acceptance:** Byte/10000-entry tests reject at the correct boundary before touching the absent excess source. No configured limit is increased.

### R19 — Windows recovery exclusions missed case aliases

Reserved `skills`/`plugins` recovery-root components were compared case-sensitively even on Windows. Windows now uses ASCII case-insensitive comparisons; Linux retains exact-name semantics.

**Acceptance:** The Windows-gated test refuses `Skills`, `SKILLS`, `Plugins` and `PLUGINS` before recovery-directory creation. This was source-confirmed locally; its runtime receipt belongs to the Windows CI lanes.

### R20 — Broken diagnostic sinks caused panics and partial lint batches

With stderr redirected to `/dev/full`, invalid arguments/runtime errors exit 101; a two-file in-place lint fixes the first file and panics before the second. The public `runtime::log_fallback` also panics. Best-effort diagnostic writes now replace panicking `eprintln!` at these boundaries, including the fallback.

**Acceptance:** `audit_cli_20261125.rs` and `logging_fallback.rs` verify controlled exit 2, complete two-file lint repair, and a successful fallback helper despite unusable stderr. Actual result-output failures must still propagate; do not suppress them as diagnostics.

### R21 — Failure logs omitted causes and supervisor delivery/status lied

Baseline runtime logs record exit 2 without its cause. They now record a bounded 2048-character ERROR cause after redaction/control flattening, then completion. Cancellation is normalized before final logging. Short child output without a newline can stay buffered and be lost on `/dev/full` while supervise returns zero; both forwarded streams are now explicitly flushed and failures propagate. Unix child SIGTERM previously becomes shell exit 241 with log status -15; the CLI boundary now maps it to 143 while retaining the library's negative-signal convention.

**Acceptance:** CLI regressions check real sink failures, normal/no-newline output, ordinary child codes, signal status, final log severity/cause and preserved timeout 124 even with unusable diagnostic stderr. Existing secret/privacy tests remain green. README/logging guidance now correctly scopes the operational logfile to `revayat-subtitle`; standalone `subsync` uses stderr and its optional JSON report.

### R22 — Strict JSON silently rounded overflow integers

`18446744073709551616` and other integer tokens beyond the integer range become floating point through Serde. The bounded token scan now requires integer spellings without decimal/exponent markers to parse exactly as i64/u64 before deserialization.

**Acceptance:** Reject root/nested overflow tokens while retaining exact `u64::MAX`/`i64::MIN`, genuine decimals/exponents and quoted/escaped numeric strings. Do not reject valid floating-point syntax to solve integer loss.

### R23 — Status rejected legitimate active/interrupted build staging

An actual live `.build-XXXXXX` staging directory makes status fail. Status now reports a separate `build_staging` count for exactly the builder's six ASCII-alphanumeric suffix form and only real, nonlinked directories. Builders explicitly request six random bytes. Stages count toward the existing 1000-entry budget; status never deletes or inspects their payload.

**Acceptance:** `status_staging.rs` covers live/retained stages, unknown names, files, symlinks and 1001 stages. Read-only snapshots include directories and symlink targets, not just file bytes. Unknown inventory must still refuse rather than disappear.

### R24 — Public render input could overflow before validation

A malformed public document with an extreme negative start reaches `end - start`, panicking in debug builds. Sampling now validates kind and timing bounds before arithmetic and uses checked span calculation.

**Acceptance:** `render_input_bounds.rs` runs the malformed public API case plus a valid control. Preserve ordinary sampling, overlap/ordering and real renderer contracts.

### R25 — PNG time budget excluded decompression

This is boundary hardening, not a claimed reproduction of a 20-second production timeout. The clock previously did not cover all inflation work. Validation now starts it before chunk/inflate processing and uses bounded 64 KiB input/output chunks, checking time and progress throughout. Exact decompressed size, stream termination, trailers and row filters remain validated without allocating the full raw pixel buffer.

**Acceptance:** A deterministic injected-clock test expires after inflation work; the existing malformed PNG corpus and real FFmpeg/libass render tests pass. Keep memory, dimension and work limits independent.

## 5. CI, release, metadata and dependency controls

### R26 — ARM lanes built binaries without exercising workspace tests

All four supported native Rust hosts now run warnings-denied Clippy and the locked workspace tests with required FFmpeg/libass: Linux x64/arm64 and Windows x64/arm64. FFmpeg installation uses the runner OS, rather than only x64 labels. Windows arm64 runs native Rust with the checksum-verified x64 FFmpeg through documented Windows 11 emulation; no native ARM FFmpeg claim is made.

Retain main-push, pull-request, merge-queue and manual triggers without path filters that hide final documentation commits. `CI required` must remain visible and successful on the latest relevant revision, alongside its component jobs. Repo hygiene keeps the 800-line Rust-file limit; the bridge tests are a separate module to satisfy it. Workflow lint, secret scanning, CodeQL and dependency review remain part of final hosted verification.

### R27 — Release tags could disagree with packaged binaries

The baseline validates tag syntax but accepts `v9.9.9` while source/binaries remain 3.0.0. Before building/uploading, the workflow now requires the tag version to match every Cargo workspace package and all five plugin manifests; both produced binaries must print that version.

The actual extracted shell guard accepts `v3.0.0` and rejects `v9.9.9`/`v3.0.0-rc.1` against current sources. A prerelease is valid only when its source/manifests match. Policy tests retain these gates. No signed release or signing-secret operation was dispatched for this audit.

### Existing controls to retain and complete under owner Rules

- Dependabot already covers Cargo and GitHub Actions with cooldowns and bounded open-PR counts. Review its PRs with all other PRs; only change policy when evidence warrants it. Keep action SHAs paired with readable version comments and review the intentional `ctrlc` lockfile update.
- Both READMEs already use the live `github/v/tag/KiaroSama/Revayat-Subtitle-Skill` badge and link their releases/tags section to this repository's tags. Preserve that dynamic data source; never hardcode the displayed tag. Release/source version checks complement the badge.
- About already accurately describes the pure-Rust skill, ASS/SRT, Persian RTL/ZWNJ, audio sync and FFmpeg/libass QA. There are already 20 topics, including `rust`. Review enrichment as requested: replace less useful tags if appropriate within GitHub's limit, retain `rust`, and avoid unsupported technology claims. Suggested About: “Pure-Rust agent skill and CLI for Persian anime subtitles: ASS/SRT editing, RTL/ZWNJ repair, audio sync, safe recovery, and FFmpeg/libass visual QA.” Metadata decisions belong in the review completion ledger.
- Every newly published human-authored and human-committed change must use **`Kiaro.Sama.Dev@gmail.com`**. Check both fields before push. Dependabot and genuine bot/platform identities are exempt as the owner specified. An existing human-author mismatch is `95ad08b1027adb70bda775e95522ff7c62f8e518`, using the old GitHub noreply address. The authorized reviewer must resolve all such human mismatches under the owner's Rules: map old/new objects, preserve a recoverable backup, account for signatures/tags/PR references, use verified expected remote heads, and revalidate the resulting history. Do not merely add `.mailmap` and claim commit objects changed. This auditor does not rewrite published main/tag history or bypass protections.

## 6. Required completion workflow for the authorized reviewer

1. Read actual Rules, AGENTS instructions and hooks. Discover the configured **Spec Kit chain** and carry the task through its prescribed specification, clarification, research, plan, tasks, analysis, implementation and verification stages. Use actual configured commands/order; do not fabricate completion of unavailable private steps. Record research and decisions at the owner-prescribed locations.
2. Fetch current main and **all open PRs**, issue #33, the repair PR, full reviews and overflow/minor comments. Reuse `audit/rust-v3-correctness-20261125` for necessary corrections. Avoid further branches unless genuinely required. Resolve moved-base conflicts before acceptance.
3. Research current primary documentation and GitHub upstreams, using the links below. Review implementation and meaningful regressions for every R01–R27 entry, including grouped subcases and associated API/recipe migration. If a concrete new defect is found during integration, fix it in this session with its regression; do not leave a known required item for another request.
4. Run formatting, warnings-denied Clippy, the complete locked workspace suite with `REVAYAT_REQUIRE_FFMPEG=1`, and release builds. Require all four native hosted lanes and current-head workflow/security/dependency checks. After any new commit, validate that new revision; an older green SHA is not acceptance.
5. Keep English code comments. Any added script must log timestamped INFO, WARNING, ERROR and DEBUG entries with clear messages and no credentials. Preserve project log redaction and CLI machine-readable output. Prefer PowerShell 7 for owner-side Windows operations; respect the owner's project/install paths and GPU preference when evaluating optional performance work.
6. Complete human commit-email remediation, About/topics (including `rust`), dynamic-tag-badge and CI visibility checks. Do not turn a discretionary enhancement into an endless prerequisite after the required ledger is satisfied.
7. Under the owner's Rules, give **every repository PR** a final disposition. If correct and mergeable, merge through the permitted method. If not directly mergeable, repair or manually integrate its needed code, test the actual result, then merge or close with the required resolution. Closing a PR while leaving its accepted bug unfixed is not completion. Do not exempt PRs merely because their subject differs from this audit.
8. Commit and push authorized corrections. If using a subsidiary branch, integrate it after successful review. Verify the final default-branch revision and its visible required checks. All required repairs and all PR dispositions must be **DONE, merged or closed according to the Rules** before the session is called complete. An actual external blocker must be reported with the failed action and evidence, never concealed as completion.

Use a finite disposition table: finding → code/test → final SHA → current CI URL → PR/issue outcome. Once every accepted defect is fixed, the specified gates pass and authorized dispositions are complete, stop. Do not start speculative refactoring or repeat unaffected tests indefinitely.

## 7. Validation commands and practical limits

From the repository root, with the pinned Rust toolchain and working FFmpeg/ffprobe/libass:

```powershell
$env:REVAYAT_REQUIRE_FFMPEG = '1'
cargo fmt --all -- --check
if ($LASTEXITCODE) { throw 'Rust formatting failed' }
cargo clippy --workspace --all-targets --locked -- -D warnings
if ($LASTEXITCODE) { throw 'Clippy failed' }
cargo test --workspace --locked
if ($LASTEXITCODE) { throw 'Workspace tests failed' }
cargo build --workspace --release --locked
if ($LASTEXITCODE) { throw 'Release build failed' }
git diff --check
if ($LASTEXITCODE) { throw 'Whitespace validation failed' }
```

These are interactive gate commands, not a new shipped script. Targeted regression files named above are committed with the fixes. The full suite also exercises installer recovery/authentication/capacity, malformed JSON, preserved ASS presentation, pending/stale review, no-clobber package/sync publication, real rendering, VAD/FPS/split alignment, process descendants, secret redaction and retry after failures.

Tests establish mechanical contracts on supported Windows/Linux x64/arm64, not every agent UI, private film, translation or GPU. Platform-gated FIFO/signal/non-UTF-8 tests execute where applicable. The current sync subtitle and JSON report publish independently: a late report collision/cancellation can leave an already complete subtitle, explicitly reported rather than removed unsafely. Legacy permission history cannot be invented. Statistical audio alignment and visible caption pixels never replace playback/editorial review. These are stated contracts and evidence limits, not deferred confirmed defects.

## 8. Optional improvements for owner/reviewer evaluation

- Benchmark opt-in GPU video decoding on actual owner hardware, while keeping libass rendering/timestamp correctness and deterministic CPU fallback. Measure end-to-end gain before adding configuration; the current audio/FFT path is not automatically a good GPU candidate.
- Add bounded scheduled fuzz/property campaigns for subtitle byte preservation, JSON-marker identities, PNG decoding and authenticated recovery states. Keep deterministic regressions in ordinary PR CI; promote minimized real failures, not random flaky gates.
- Consider caller-owned cancellation tokens for concurrent embedded jobs if that usage becomes a requirement. The current sticky global cancellation is deliberate for one-shot CLI ownership.
- Consider a shared structured operational logger for standalone `subsync`, with stdout JSON unchanged and the same redaction/error-delivery guarantees. Its documented current stderr/report behavior is intentional.
- Consider further typed public APIs replacing legacy JSON adapters, with explicit migration policy. Do not make a broad API rewrite a prerequisite for these concrete fixes.

Record accepted or rejected optional choices and their reasons. None may conceal an unfinished required repair.

## 9. Primary references

- [Repository issue/report requirements](agents/issue-tracker.md) and [verification scope](verification.md).
- [FFmpeg resampler: async, first_pts and compensation thresholds](https://www.ffmpeg.org/ffmpeg-resampler.html); [aresample filter](https://www.ffmpeg.org/ffmpeg-filters.html#aresample); [stream selection](https://ffmpeg.org/ffmpeg.html#Stream-selection).
- [ctrlc API and termination support](https://docs.rs/ctrlc/latest/ctrlc/) and [upstream implementation](https://github.com/Detegr/rust-ctrlc).
- [Unicode UAX #9 matching isolate rules](https://www.unicode.org/reports/tr9/#BD9) and [libass tag parser](https://github.com/libass/libass/blob/master/libass/ass_parse.c).
- [encoding_rs label compatibility](https://docs.rs/encoding_rs/latest/encoding_rs/) and [serde_json Number representation](https://docs.rs/serde_json/latest/serde_json/struct.Number.html).
- [Rust eprintln failure behavior](https://doc.rust-lang.org/std/macro.eprintln.html), [Write::flush](https://doc.rust-lang.org/std/io/trait.Write.html#tymethod.flush), and [flate2 incremental Decompress API](https://docs.rs/flate2/latest/flate2/struct.Decompress.html).
- [tempfile random suffix configuration](https://docs.rs/tempfile/latest/tempfile/struct.Builder.html#method.rand_bytes).
- [GitHub hosted runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [Windows ARM runner inventory](https://github.com/actions/runner-images/blob/main/images/windows/Windows11-Arm64-Readme.md), and [Windows x86/x64 application emulation](https://learn.microsoft.com/en-us/windows/arm/apps-on-arm-x86-emulation).
- [GitHub topics](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics), [Dependabot options](https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference), and [Spec Kit upstream](https://github.com/github/spec-kit). The owner's configured Spec Kit Rules determine the actual local chain.
