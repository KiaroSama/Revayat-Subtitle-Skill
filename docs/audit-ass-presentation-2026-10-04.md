# ASS track presentation and review-boundary audit — 2026-10-04

Repository: [KiaroSama/Revayat-Subtitle-Skill](https://github.com/KiaroSama/Revayat-Subtitle-Skill). Audited main: `2ab72b097e3b27cc373e5037ec9936b76520329d`. PR #9 was already merged. Its ASCII SRT grammar, bidi coalescing and typed JSON repairs, together with earlier source/publication/drawing protections, are retained. This is a new implemented audit, not a claim that those accepted fixes are still missing.

## E01 — Rewriting permissively parsed ASS can activate renderer-ignored syntax

**High: source/render fidelity.** `parse` previously stripped general Unicode whitespace and case-folded row labels. It accepted `dialogue:`, `Dialogue :`, `style:` and `format:` and serialization emitted canonical `Dialogue:`, `Style:` and `Format:`. libass's row labels are case-sensitive even though its recognized section names and field names permit ASCII case variation. An extra lowercase `dialogue:` row is invisible in the native source, but became a visible extra event after helper serialization. This was reproduced with real FFmpeg raster comparisons.

Unicode case folding also changes grammar identifiers such as `ſcaleX` into `scalex`, activating a field the native reader did not recognize. General whitespace stripping can erase meaningful NBSP or treat it as indentation. Prefix-like headers such as `[Events]junk` are another mismatch: libass recognizes a section prefix while the helper previously classified it differently.

**Implemented:** separate ASCII-only grammar folding from Unicode content; model BOM-at-line-start followed by ASCII indentation; require canonical row labels without whitespace before their colon; preserve unknown Unicode field names without folding them into ASCII; reject renderer-active section suffixes and unsupported events-before-styles ordering rather than changing when style lookup occurs. Timestamp edge whitespace is limited to ASCII. No source dialogue is normalized with these grammar helpers.

**Acceptance:** canonical mixed-case ASCII section/field controls remain accepted; noncanonical row labels, dangerous prefix headers and Unicode indentation refuse; BOM-then-ASCII indentation works; Unicode field lookalikes retain their spelling; CLI refusal has no traceback/private dialogue or partial workspace, and a corrected separate source copy imports successfully. The equipped test demonstrates that the lowercase row is natively ignored before requiring controlled rejection.

**Boundary:** this is deliberately strict source-preserving refusal, not automatic recovery of every malformed ASS dialect. It does not assert that every string rejected by this tool would crash or be rejected by libass.

## E02 — Style pruning and donor remapping use the wrong name equivalence

**High: appearance changes.** The old parser used stripped dictionary names and exact-looking event names without reproducing the renderer's two distinct lookup rules. For example, declare red `Default` and green `default`, then reference `default` from an event. libass resolves the event to `Default`; the helper pruned that actual style and changed the output's rendered colour. `Alt` and `*Alt` declarations also collide after native normalization, while the old dictionary treated them as independent. Pruning the renderer-selected last definition changed colour. Trailing spaces and Unicode spaces were silently collapsed by `.strip()` even though they are meaningful name characters.

A completely blank `Style:` row was also silently discarded even though libass allocates renderer defaults for it; the authored source/output raster differed. Empty style-name fields inside an otherwise complete row are different: the native effective name is `Default` and can be supported safely.

**Implemented:** keep raw row values but key declared styles by the renderer-effective name (leading ASCII indentation and leading stars removed; an empty resulting declaration name becomes `Default`). Reject collisions after that normalization. Event lookup separately removes leading stars and normalizes ASCII case variants of `Default`; it preserves significant suffixes/Unicode spaces. Undefined exact references are refused rather than silently assigned a fallback. Bare blank `Style:` rows now require deliberate repair.

Inline `\r` resets retain **exact** name semantics and are not processed with event alias rules. Argument spans trim only native trailing ASCII spaces; parenthesized scalars additionally skip native leading ASCII spaces. Remapping edits only the actual argument span, retaining ignored prefixes, delimiters and whitespace. Donor event lookup uses the new resolver and remains exception-atomic. The transform prefix argument count treats NBSP as a nonempty argument, matching the native grammar rather than Unicode whitespace rules.

**Acceptance:** declared/event star aliases, Default case variants, distinct lowercase reset names, empty declared names, meaningful ASCII/Unicode suffixes, collisions, exact reset spans, invalid-reference refusal and corrected donor retry. Successful donor conversion does not mutate source cues. A native colour/raster corpus verifies valid cases; a local 64-case name/reset exploration accepted 42 pixel-identical pairs and refused 22 unsupported/undefined cases, with zero accepted mismatches. These are corpus cases, not 64 independent defects.

## E03 — Renderer-active rows in unknown sections escape cue accounting

**High: complete-review boundary.** libass ignores an unknown section header without resetting its active parser state. The helper instead assigned later lines to that unknown section. A normal `[Events]` cue followed by `[Notes]` and a second canonical `Dialogue:` produced a one-cue worksheet and manifest while serialization copied the raw unknown-section row. Native rendering displayed the second, unreviewed event. Under `[Aegisub Project Garbage]`, the helper could instead remove a renderer-visible event without reviewing it. The discrepancy was reproduced by comparing source/output with an authored reviewed-only raster oracle.

**Implemented:** reject canonical renderer-active event, style, format and track-header rows in unsupported/editor-only sections. Do not silently copy them, silently discard them or guess which source episode they belong to. This includes Unicode lookalike sections, graphics/editor sections and native BOM/ASCII indentation. Genuine supported ASS sections are still parsed normally. Harmless metadata and valid opaque font payload boundaries stay preserved; existing font-removal behavior remains explicit.

**Acceptance:** `[Notes]`, `[Eventſ]`, `[Graphics]` and `[Aegisub Project Garbage]` cannot carry hidden active events; unknown metadata cannot carry active styles/formats/settings; ordinary metadata and authored opaque font lines round-trip; a failed import leaves no workspace and preserves the original file. The native oracle proves the injected row is actually visible, not merely a theoretical parser discrepancy.

**Boundary:** this closes an accounting bypass for renderer-active text rows. It is not a general hostile-media sandbox or a claim that a reviewer can ignore all unusual source formats after this change.

## E04 — Removing an old FFmpeg signature changes implicit rendering settings

**Medium: border/shadow fidelity.** libass recognizes certain legacy FFmpeg-produced ASS tracks using the exact `; Script generated by FFmpeg/Lavc...` signature, a specific set of Script Info flags, and one declared style. It then treats `ScaledBorderAndShadow` as enabled. The helper removed that apparently harmless comment, leaving the same text rendered with the opposite implicit default. At native canvas size the difference can be hidden; the authored 320×180 track rendered at 640×360 exposed it clearly. The old output matched an explicit `no` control rather than the native source/explicit `yes` control.

**Implemented:** materialize `ScaledBorderAndShadow: yes` in the parsed presentation only when the original supported track satisfies the native legacy heuristic. Preserve explicit values. Perform this before pruning or adding donor styles so neither operation can toggle the heuristic. Then remove the non-displayed signature under the existing comment policy. Retaining the signature alone would be insufficient because changing the style count also changes the heuristic. No original source bytes are edited.

**Acceptance:** positive signature/style cases, nonmatching signature case, additional renderer headers, multiple styles, explicit yes/no controls, idempotent serialization, legacy/nonlegacy donor compatibility refusal before mutation, and matching explicit-yes donor acceptance. Actual scaled-raster tests require native/output equality and inequality against the explicit-no control.

## Independent integration review

Review of the actual PR10 source, callers and native evidence confirmed E01–E04.
An additional E03/E04 interaction was reproduced: an exact legacy FFmpeg signature
under `[Notes]` leaves native Script Info state active. The original PR accepted
it but missed materializing scaling, then removed the signature. The native source
matched explicit `yes` and differed from explicit `no`; the required parse refusal
failed before the repair. The same native regression passed after adding the
signature prefix to the existing unsupported-section guard. A portable nine-case
section/indent matrix and harmless-comment control accompany that native test.
Opaque Fonts payload stays excluded; no generalized parser-state model was added.
The final module contains **40 methods, five equipped native methods**; the 38-method
proposal evidence below is historical, not verification of this correction.

The retained private workspace was rebuilt with recipe 10: all 150 previous files
remained byte-identical before and after rendering and delivery. All ten fresh
1920×1080 images were actually inspected for Persian joining, mixed Latin order,
punctuation, margins, title placement and animated sign/credit separation before
recording new hash-bound review notes. QA passed for the 40-cue excerpt; the single
ASS ZIP passed member, CRC and manifest-hash checks. The installed 25-file skill
and 34-member plugin payload match current source; the previous installation was
backed up. No private media, archive or review body is published. This is not
full-episode linguistic, audio-sync or alternate-player certification.

The existing deterministic/native corpus and minimized new regression suffice for
this bounded repair; a larger generated corpus is not selected. A user-facing
style-binding diagnostic and alternate-player lane are not selected because
neither is necessary for the requested contracts. No runtime dependency was added.
Owner-attributed raw identities use the permitted public address; legitimate
signed Dependabot/GitHub provenance is preserved, not rewritten as owner history.
The final corrected PR and integrated-main SHAs must have their own successful
native/security/workflow checks; the proposal's green run does not certify them.

## Verification and compatibility

The new automatically discovered `tests/test_ass_track_contracts.py` adds **38 test methods**, including four equipped FFmpeg methods: ignored/hidden-row oracles, a style/reset raster corpus, scaled legacy rendering and a two-episode build → render → QA → hash-verified ZIP. Other tests cover import/CLI failure-and-retry, source/caller-object preservation, alias semantics, migration and negative controls. Existing tests and assertions remain enabled. The shared positive `tests/fixtures/episode.ass` contained a bare `Style:` row that changes native defaults; remove that invalid row from the positive fixture and retain the behavior as an explicit new rejection/raster regression. This is not deletion of the empty-style boundary test.

Local isolated experiments used Python 3.13.5 and FFmpeg 7.1.5 on Linux. Only the exact markup/format Git blobs were reconstructed locally, using an import-time validation-constant stub; that stub is not shipped. The controlled publication job uses a **complete exact-base checkout**, reproduces four minimal failing groups, applies the verified patch, runs the new equipped tests and the full existing renderer suite. The normal PR native matrix supplies separate Windows, minimum-Python and macOS evidence. Consult the PR's final checked SHA and actual logs; old PR results are not substituted for new evidence. Automated review flags in tests are explicitly mechanical fixtures, not human editorial approval.

Project/build schema remains 2. Generation/normalization and sampler recipes advance **9 → 10**. Keep original sources, IDs, worksheets, glossary and previous immutable editions. Rebuild from the same workspace and inspect a new render receipt; do not transplant old image approvals. A source newly identified as ambiguous needs a deliberately corrected separate copy and re-import, not an in-place modification of an immutable source.

The current CI already covers Linux Python 3.10, Linux/Windows Python 3.14 with real FFmpeg, macOS Python 3.14, installation/rollback, owned subprocesses, package bytes, security and workflow checks. The new module joins that discovery; no duplicate workflow or runtime package is necessary. Retain pinned Actions, CodeQL, blocking workflow lint, Dependency Review and existing Actions/root-pip/skill-pip Dependabot entries. Do not manufacture unrelated dependency churn to make the audit appear larger.

## Upstream research and design decisions

- [libass `ass.c`, inspected revision](https://github.com/libass/libass/blob/f61db567e6593df3470e91594bcd4ad2d0473aff/libass/ass.c): canonical row prefixes, persistent parser state after unknown sections, declaration names, BOM handling and legacy scaling detection. This is the primary executable-format authority for E01–E04.
- [libass event-style lookup](https://github.com/libass/libass/blob/f61db567e6593df3470e91594bcd4ad2d0473aff/libass/ass_utils.c) and [strict reset lookup/argument parsing](https://github.com/libass/libass/blob/f61db567e6593df3470e91594bcd4ad2d0473aff/libass/ass_parse.c): deliberately separate event aliases from exact reset names; do not share a generic stripped-name function between them.
- [libass-tests](https://github.com/libass/libass-tests): distinguish no-crash tests from known-good raster comparisons. This repair uses independently authored source/output/negative controls rather than claiming frame existence proves fidelity.
- [pysubs2 SubStation parser](https://github.com/tkarabela/pysubs2/blob/master/pysubs2/formats/substation.py) and [name-linked style model](https://github.com/tkarabela/pysubs2/blob/master/pysubs2/ssafile.py): useful comparative model, but tolerant parsing/default dictionaries are not substituted for this project's stricter review contract.
- [Aegisub](https://github.com/TypesettingTools/Aegisub): retain editor metadata and distinguish textual cleanup from intentional typesetting changes. Editor-specific sections do not automatically make their contents renderer-inert.
- [FFmpeg](https://github.com/FFmpeg/FFmpeg): actual installed libass-enabled renderer used for differential evidence; FFmpeg/libass combinations are not two independent render engines.
- [Hypothesis](https://github.com/HypothesisWorks/hypothesis): suitable existing dev tool for expanding bounded grammar/name combinations; retain minimized deterministic failures and actual native oracles.
- [Spec Kit](https://github.com/github/spec-kit), [current process overview](https://github.github.com/spec-kit/) and [workflow resume/status](https://github.github.com/spec-kit/reference/workflows.html): use the owner's installed chain and existing run context, not a newly initialized default workflow. Inspect custom shell steps before executing them.
- [git-filter-repo](https://github.com/newren/git-filter-repo): controlled historical email repair with private backup and verified object mapping, not a display-only mailmap.
- [Existing broader upstream survey](../skills/revayat-subtitle/references/research.md): keep previously evaluated translation, parser, typography and QA projects and their licensing boundaries. This pass adds targeted implementation research; it does not claim enumeration of every GitHub project.

Implementation and fixtures are original; no upstream implementation, private media or font binary is copied into this change. Consider optional improvements once: a larger bounded generated name/section corpus, a read-only preflight report of effective style bindings, or a specifically requested alternate-player lane. Implement with evidence or record not-selected-with-reason; do not add a new parser dependency, UI, translation provider or GPU stack for these text/control-plane repairs.


The first complete-checkout run passed all 38 new methods but exposed a malformed
source in the archived schema-1 fixture: its bare `Style:` row now correctly
requires explicit repair. The migration test now first proves that refusal
preserves EVERY archived file. It then authors a separate canonical schema-1
test variant before taking the migration snapshot and keeps all existing
new-edition, preserved-byte and repeatability assertions. The committed archived
fixture remains unchanged; production code has no legacy bypass. Canonical
legacy input migrates; newly detected ambiguous input is refused without mutation.
This is an intentional input-contract boundary, not a claim that arbitrary old
malformed sources automatically become safe.
