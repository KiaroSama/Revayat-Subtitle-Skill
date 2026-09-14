# Language evaluation

These short examples are authored for the project, including the mixed-order
sentence from the requested skill rules. They exercise spoken Persian, logical
order, honorifics, quotation scope, profanity intensity and promotion classification.
Japanese, Chinese, French and Spanish cases also probe participant roles, aspect,
negation/restriction and forms of address. They are calibration examples, not a
representative translation-quality benchmark. Do not show the accepted answers
to an agent whose unaided translation you are evaluating.

Ask the agent to translate or correct each `source` using the skill, then save a
UTF-8 JSON object mapping case IDs to its answers:

```json
{"spoken_persian":"می‌خوام برم خونه.","mixed_word_order":"من دیروز OVA رو دیدم."}
```

The example contains only two entries; a complete run answers every case in the catalogue.

```text
python evaluation/score.py --answers my-answers.json
```

The command recognizes a small set of known acceptable wordings. An unseen answer
is `needs_review`, not automatically wrong. Missing answers remain `missing`.
Exit 0 means every answer matched this limited catalogue; exit 1 requires review;
exit 2 means malformed input. No overall translation-quality score is fabricated.

A human/agent must still review new faithful variants, context, full-season
consistency and rendered subtitles. These language cases do not replace the
structural/FFmpeg checks in `tests/check.py` or the per-cue and per-image workflow.
