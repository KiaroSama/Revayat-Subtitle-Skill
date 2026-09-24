# Source languages into Persian

Read the common procedure and the relevant language profile before translating.
Persian remains the default target for every source language. A language profile
is an editorial aid, not a claim that every agent knows that language accurately.

## Identify and research the source

Record each track's language and relevant variety in the activity log. Check real
dialogue, metadata and original credits together: script alone is insufficient.
Separate mixed-language spans; Japanese kanji, Chinese characters and romanized
names do not establish a single shared language or pronunciation.

Translate directly from the original when the agent can understand it. An English
release can help compare omissions or timing, but is not a compulsory intermediate
translation. If only a pivot translation is available, identify it and disclose
that fidelity to the unavailable original is unverified. If the agent cannot read
the source reliably, request a suitable source or qualified review; do not pretend
an unchanged source line is a finished Persian translation.

State the actual source language and the spoken-Persian target in each translation
assignment. A language dropdown, `.fa` filename or previous batch is insufficient.
Every correction or polish pass must repeat structural and source-meaning checks.

For EACH source language encountered, search current primary repositories and
language references using its name, native name, Persian/Farsi/فارسی and subtitle
translation. Inspect actual prompts or code, not just a language dropdown. Record
the relevant source URLs, what was adopted and what remains uncertain. Existing
research in [research.md](research.md) is a starting point, not a substitute for
checking a new language, regional usage, proper name or ambiguous expression.
Prefer original creators' documentation and authoritative dictionaries/grammar.
Do not install a model, upload the user's subtitles or copy a corpus just because
a research page suggests it. Repository, dataset, font and model licenses differ.

## Context and review shared by all languages

Read preceding and following SOURCE cues as read-only context. Translate only the
current cue IDs; do not emit the neighbors twice or move a revelation ahead of its
spoken time. Adjust batch size to scene and complexity, not English word counts.
Do not split a sentence mechanically at punctuation or count CJK characters as
English words. A cue boundary alone does not justify adding an ellipsis.
If one source sentence spans adjacent cues, read the complete sentence across
their IDs before translating it. Distribute fluent Persian back across those
same cue IDs and original display times; check that each part appears when its
meaning is available, with no repeated or missing phrase. Do not combine or
drop cues to make the sentence easier. If its meaning cannot be assigned safely
within those boundaries, keep the affected IDs pending and document the timing
or source uncertainty for review rather than inventing a complete answer.

Previously approved Persian neighbor cues can also inform voice and the flow of
a split sentence. Keep them read-only, identify their cue IDs, and check them
against the source if they conflict. Stale translations and unapproved donors
are not evidence of meaning.

Keep brief scene notes with speaker/addressee, confirmed referents, name readings,
relationship/register and unresolved cue IDs. Use the existing glossary for stable
decisions and the activity log for transient questions; no extra database is needed.
Source evidence overrides an earlier mistaken translation or generated summary.
Glossary consistency fixes meaning and name spelling, not the inflected form of
every common word. Recheck affected cues when a term or referent changes.

First draft faithful Persian with the source visible; then make it speakable and
check it against that same source again. Preserve agency, negation, tense/aspect,
certainty, numbers/units, sarcasm, reactions, threats and sexual/profane intensity.
Keep emphatic or disambiguating pronouns. Do not erase every subject pronoun or
give every character the same Tehran influencer voice. An intentionally formal
character can speak naturally with respect, without turning all dialogue literary.
Avoid invented Iranian accents, memes or explanations for a foreign dialect/joke.

On retry, address the failing IDs and actual errors. Reuse a checkpoint only when
source IDs/text, selected release and applicable glossary decisions still match.
Verify every nonempty linguistic cue, including the final partial batch. Length
ratios, absence of source script and model confidence do not certify completeness.
Reading-speed/line-length warnings require editorial and video review; never
delete meaning or retime blindly to meet a foreign-language numerical threshold.

## Japanese → Persian

- Read the predicate through its final negation/modality, including continuation
  cues. Recover omitted subjects from context; do not invent gender or relationships.
- Distinguish doing, making someone do, being made to do, and passive action.
  Honorific verb forms can resemble passives; resolve who did what to whom.
- Verify original name readings with official credits; kanji alone is not enough.
  Preserve the approved surname/given-name order, nicknames and Japanese suffixes.
  Retain سان، کون، ساما، چان، اونی-چان without adding absent suffixes.
- Family words used as address do not automatically prove literal kinship. Track
  who addresses whom and whether a politeness shift changes the relationship.
- Preserve dialect familiarity/roughness without inventing a Persian regional
  identity. Translate short reactions, readable lyrics and signs too; romaji is
  not automatically a decorative effect or permission to omit opening/ending songs.

Primary grammar: [Japan Foundation grammar collection](https://www.irodori.jpf.go.jp/assets/data/Grammar_all.pdf)
and [causative/passive lesson](https://www.irodori.jpf.go.jp/assets/data/pre-intermediate/pdf/ZZ_L16.pdf).
Adjacent repository: GalTransl's participant/context checks; SakuraLLM targets
Chinese and its event-merging/font defaults are unsuitable here. See research.

## Chinese → Persian

- Identify spoken variety separately from simplified/traditional script. Traditional
  text does not prove Cantonese. Keep confirmed readings; do not give Japanese
  names Mandarin pronunciations merely because characters overlap.
- Resolve omitted participants, 被 passive roles, 不/没 negation and 了/过/着
  aspect in context. Do not turn every aspect marker into the same Persian tense.
- Translate classifiers by the noun/reference they express. Preserve numbers and
  magnitudes such as 万/亿; verify arithmetic and units instead of copying grouping.
- Check idioms and kinship/address terms against the scene; avoid literal character
  substitution or invented family ties. Preserve short acknowledgments and reactions.

Primary references: [UD Chinese auxiliaries](https://universaldependencies.org/zh/pos/AUX_.html),
[passives](https://universaldependencies.org/zh/dep/aux-pass.html), and
[classifiers](https://universaldependencies.org/zh/dep/clf.html).
NiuTrans LMT's Chinese/Persian language coverage is a research lead, not a subtitle
quality certificate. Chinese-target prompts in VideoLingo/pyVideoTrans are not
Persian-target instructions.

## French → Persian

- Distinguish restriction from negation: `Il n'a que deux clés.` means
  `فقط دو تا کلید داره.` An expletive ne does not negate the subordinate action:
  `Je crains qu'il ne parte.` can mean `می‌ترسم بره.`
- Spoken French can omit ne; plus and other short forms require sentence/audio
  context. Do not recover polarity by a single keyword replacement.
- Resolve on, tu/vous and object pronouns from speaker/addressee context. Keep a
  meaningful change in familiarity/respect without automatically making it literary.
- Preserve conditional uncertainty, idioms and regional profanity. Verify the
  name's actual pronunciation rather than mechanically transliterating spelling.

Primary references: [OQLF restriction](https://vitrinelinguistique.oqlf.gouv.qc.ca/22466/la-syntaxe/la-negation-et-la-restriction/constructions-avec-ne-que)
and [Government of Canada expletive ne](https://nos-langues.canada.ca/fr/cles-de-la-redaction/ne-expletif).
LLM-Subtrans and Subtitle Edit offer useful contextual workflows; no dedicated
French→Persian subtitle quality benchmark was verified in the inspected sources.

## Spanish → Persian

- Negative concord remains negative: `No vino nadie.` → `هیچ‌کس نیومد.`
  Do not cancel two negative words as if they were English double negation.
- Identify the region when relevant. Tú/vos/usted/ustedes encode relationships
  differently across varieties; map the actual relationship into natural Persian.
- Resolve omitted subjects and clitic references from the scene. Preserve the
  difference between a fact, a wish, doubt and a conditional possibility.
- Check region-dependent slang, idioms and vulgarity before choosing intensity.
  Do not replace a regional meaning with the first dictionary sense or add accent.

Primary references: [FundéuRAE negative concord](https://www.fundeu.es/consulta/no-vino-nadie-15832/)
and [RAE voseo](https://www.rae.es/dpd/voseo).
pyVideoTrans es/es-419 profiles describe Spanish OUTPUT, not proof of Spanish→Persian
quality. Use its Persian-target lessons only within the common fidelity rules.

## English, Persian and all other languages

English: check phrasal verbs, idioms, negation, gender/referent ambiguity, modal
certainty and sarcasm. Retain Japanese honorifics carried by an English release.
Persian: perform complete correction against available source/context, retaining
the user's colloquial target and the typography policy.

For Arabic, Korean, Turkish, German, Russian, Portuguese, Hindi, Urdu or any other
source, run the per-language research procedure above before editing. Investigate
its actual negation, participant roles, modality, register, idioms, number system
and name readings. Do not apply another language's profile solely because scripts
or words overlap. Log missing evidence and resolve meaning-changing uncertainty
before marking the affected cues reviewed.
