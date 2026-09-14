# Subtitle translation research

Primary-source review: 2026-09-14. Repositories were inspected as reference data;
their scripts, services and models were not installed or executed. The guidance
below is independently written; no upstream prompts, corpora, fonts or code are
vendored. Software licenses do not automatically license associated datasets or
model weights. Links pin the inspected revision where available.

## What changed in this skill

The [source-language guide](source-languages.md) applies direct original→Persian
translation, read-only neighboring cues, a small speaker/referent record, two
distinct fidelity/fluency passes, and specific Japanese/Chinese/French/Spanish
checks. [Translation policy](translation-policy.md) retains every meaningful cue,
character register and source intensity. The existing immutable-source worksheets,
glossary and rendering gates carry these decisions; no translation API dependency
or second orchestration framework was added.

The agent must research EACH actual source language, not assume a repository's
language selector establishes quality. Dedicated French→Persian and Spanish→Persian
subtitle skills with measured quality were not verified in this search. That is
a limitation of the inspected evidence, not proof that no such project exists.

## Persian-target and subtitle workflows

| Inspected source | Useful evidence adopted | Boundary or rejected behavior |
| --- | --- | --- |
| [pyVideoTrans Persian prompt](https://github.com/jianchang512/pyvideotrans/blob/3eb3b2f3ab11013632d5d35329620c861ce8df7c/videotrans/prompts/language_prompts/fa.txt), [routing](https://github.com/jianchang512/pyvideotrans/blob/3eb3b2f3ab11013632d5d35329620c861ce8df7c/videotrans/translator/_base.py), GPL-3.0 | A real Persian-target prompt is selected by target language; conversational Persian is an explicit concern. | Reject unconditional subject-pronoun deletion. Keep emphatic/contrastive pronouns and character-specific respect. A routed prompt does not establish measured quality. |
| [pyVideoTrans SRT prompt](https://github.com/jianchang512/pyvideotrans/blob/3eb3b2f3ab11013632d5d35329620c861ce8df7c/videotrans/prompts/srt/chatgpt.txt) | Preserve cue identity and short reactions; inspect the final batch as well. | Reject aggressive TTS compression, compulsory ellipses and block-isolated interpretation. Its base implementation can pad missing positions with empty strings; Revayat requires every nonempty linguistic cue to be reviewed. |
| [LLM-Subtrans instructions](https://github.com/machinewrapped/llm-subtrans/blob/89c3276d70ae6bbec0a612aeb5940e8fbdf19bfb/instructions/instructions.txt), [prompt construction](https://github.com/machinewrapped/llm-subtrans/blob/89c3276d70ae6bbec0a612aeb5940e8fbdf19bfb/PySubtrans/TranslationPrompt.py), MIT with third-party notices | Stable IDs, terminology, speaker context, profanity equivalence and retries informed by actual errors. | Reuse our worksheet contract. No provider integration is needed; scene summaries are context, never replacement source. |
| [OOMOL subtitle skill](https://github.com/oomol-lab/video-subtitle-translator/blob/3d6c4a280ace7389e49ed7fe08192bb70dabfbe7/SKILL.md), MIT | Read-only surrounding cues, emotion/subtext, smaller retries and source-matched checkpoints. | Reject inferred target language, foreign output defaults and runtime requirements. Persian is the explicit default here; no automatic ASR or CJK splitting. |
| [Cinemacc translation/QA](https://github.com/HaiyiMei/cinemacc-subtitle-skill/blob/080fd0baa946a44c9b8cd6ecaa7eb4d939b678dc/skills/cinemacc-subtitle/references/translation-and-qa.md), MIT | Neighboring cues remain context; uncertainties stay outside viewing text; check semantic drift at chunk boundaries. | Its Chinese targets and reading thresholds are not Persian standards. Keep our ASS-aware workflow and naming rather than importing its SRT-only job format. |
| [Subtitle Edit AI assistant](https://github.com/SubtitleEdit/subtitleedit/blob/5c7366957d1ed54f41b118be7a0f172348cf6ebe/docs/features/ai-assistant.md), [prompt substitution](https://github.com/SubtitleEdit/subtitleedit/blob/5c7366957d1ed54f41b118be7a0f172348cf6ebe/src/libuilogic/AutoTranslate/LlmTranslatePrompt.cs), MIT | Neighboring context and keeping source text distinct from prompt-template syntax. | Subtitle braces and dialogue instructions remain data. No C# port or universal reading-speed claim. |
| [TermSub translator](https://github.com/seaweedbeehive/TermSub/blob/0cbda8acb27d64b7dc1a9a55c06190b3886b415a/app/agents/translator.py), MIT | Glossary concepts remain consistent while grammar and inflection stay natural. | Reject blanket prohibitions on numbers/punctuation and allowance of empty translations. |

## Japanese and Chinese investigation

Two Persian-focused repositories were traced beyond their README claims:

- [SubTrans-Ollama prompts](https://github.com/mshojaei77/SubTrans-Ollama/blob/5c7b196c86502a01e51344d9975a66d1e31caf78/src/translation/prompts.py)
  and [API](https://github.com/mshojaei77/SubTrans-Ollama/blob/5c7b196c86502a01e51344d9975a66d1e31caf78/src/api/main.py)
  (MIT) provide exact-ID output and preceding/following context. In this inspected
  API path, source/target language values do not reach the batch prompt, and no
  semantic judge is constructed despite a quality-mode setting. Its explicit
  colloquial-Persian prompt is in a standalone demo. Adaptation: every translation
  assignment explicitly states the source language and Persian target; every
  correction re-enters structural and meaning review. Do not infer quality from
  a filename ending in `.fa` or from an enabled UI option.
- [Kthree's webpage source](https://github.com/Kthree-K3/Anime-Subtitle-Translator-To-Persian-Webpage/blob/ba683bce262e8cc4693a93abb64e7e3bfe82a583/docs/script.js)
  has explicit Persian voice/intensity guidance, but assumes English input. It
  also skips some events, uses a default frame rate during conversion, reverses
  translated segments and replaces/embeds fonts. Its romaji-song exception does
  not fit our complete-translation requirement. Preserve original timing and
  logical order, read every lyric, ask about embedded fonts and inspect images.
  No license was established from the inspected repository; no code, prompt or
  font was copied. An [original-font request](https://github.com/Kthree-K3/Anime-Subtitle-Translator-To-Persian-Webpage/issues/3)
  is user feedback, not a reproduced test of our implementation.

These observations apply to the pinned source paths; the upstream programs were
not executed. They do not measure current model accuracy.

| Inspected source | Useful evidence adopted | Boundary or rejected behavior |
| --- | --- | --- |
| [GalTransl prompts](https://github.com/GalTransl/GalTransl/blob/98ee841aeb753f28200d65e3fe3070cbc58d4b6c/GalTransl/Backend/Prompts.py), [parser](https://github.com/GalTransl/GalTransl/blob/98ee841aeb753f28200d65e3fe3070cbc58d4b6c/GalTransl/Backend/ForGalJsonTranslate.py), [problem checks](https://github.com/GalTransl/GalTransl/blob/98ee841aeb753f28200d65e3fe3070cbc58d4b6c/GalTransl/Problem.py), GPL-3.0 | Separate original, history and glossary; resolve omitted participants and causative/passive roles; verify IDs, strings and missing output. | Chinese-oriented ratios, GBK checks and pronoun heuristics do not evaluate Persian. Earlier translations cannot override source evidence. |
| [SakuraLLM templates](https://github.com/SakuraLLM/SakuraLLM/blob/0ff69116222ca66c7a15dcff00fd4f9f86b18d5c/utils/consts.py), [subtitle path](https://github.com/SakuraLLM/SakuraLLM/blob/0ff69116222ca66c7a15dcff00fd4f9f86b18d5c/translate_sub.py), GPL-3.0 | Japanese participant-role analysis is relevant. | The templates target Chinese. Equal-time/adjacent-text merging, event removal, ASS flattening and CJK font replacement would lose valid subtitle content/presentation here. |
| [VideoLingo prompts](https://github.com/Huanshere/VideoLingo/blob/c5f8fe71c9d7092b9073f98f7e811fc1d2a1c9fa/core/prompts.py), Apache-2.0 | Prior/next source context; faithful draft followed by controlled fluency review. | Reject trim/alignment permissions to omit or freely rewrite. Keep the source visible during both passes. |
| [NiuTrans LMT](https://github.com/NiuTrans/LMT/tree/d87c68a65377814827427a36645905b0bd227431), [model card](https://huggingface.co/NiuTrans/LMT-60-8B) | Chinese/Persian coverage is a possible direct-translation research lead. | Repository license was not established; the model card has separate terms. Coverage is not subtitle evaluation. No weights or code imported. |
| [Natural Instructions tasks](https://github.com/allenai/natural-instructions/tree/55a365637381ce7f3748fa2eac7aef1a113bbb82/tasks), Apache-2.0 repository | A Japanese→Persian TED task is discoverable; direct-pair resources need not pivot through English. | Task metadata is not anime subtitle evidence, and instance/data rights must be checked separately. No dataset examples copied. |

The source-language guide cites Japan Foundation and Universal Dependencies for
the actual Japanese/Chinese distinctions; adjacent model prompts alone are not
grammar authorities. Japanese honorifics follow the user's rule regardless of an
upstream project's Chinese localization convention.

## French and Spanish investigation

Searches combined French/français and Spanish/español with Persian/Farsi/فارسی,
subtitle translation and GitHub. The actual Persian-target pyVideoTrans prompt,
LLM-Subtrans, OOMOL, TermSub and Subtitle Edit supplied applicable workflow ideas.
pyVideoTrans `fr`, `es` and `es-419` prompts describe target-language styles;
their presence does not verify French/Spanish→Persian translation quality.

The language guide therefore uses source-language authorities: OQLF and the
Government of Canada for French restriction/expletive negation, FundéuRAE for
Spanish negative concord and RAE for voseo. Those distinctions were translated
into original short Persian calibration cases rather than copied benchmarks.
Regional slang and ambiguous spoken forms require scene-specific research.

## Acceptance limits

The [evaluation catalogue](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tree/main/evaluation) contains original short
cases, not a representative multilingual benchmark. Exact known wording is only
a regression aid; new faithful wording requires editorial review. It cannot
certify an agent's language skills, a complete season or subtitle synchronization.

Research improves instructions, not proof of unseen outputs. Every real batch
still needs full cue review, stable continuity, the user's embedded-font choice,
an agent-written activity log beside translated files and actual FFmpeg images.
No upstream claim of perfect RTL, unchanged cue counts or model confidence
replaces those checks.
