# Language evaluation

These short examples are authored for the project, including the mixed-order
sentence from the requested skill rules. They exercise spoken Persian, logical
order, honorifics, quotation scope, profanity intensity and promotion classification.
Japanese, Chinese, French and Spanish cases also probe participant roles, aspect,
negation/restriction and forms of address. They are calibration examples, not a
representative translation-quality benchmark. Do not show the accepted answers
to an agent whose unaided translation you are evaluating.

Some cases supply a `context` field to establish an omitted referent or singular
polite address. Read it with the source. Adversarial cases include interruption,
literal math delimiters, negative requests, not-yet/no-longer, and false friends.

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
consistency and rendered subtitles. Use the [categorical rubric](rubric.md) to
record participant/referent, negation/aspect/restriction, name/honorific/intensity,
spoken-register and logical mixed-script/quotation observations. Categories are
`faithful`, `needs_context`, `meaning_changed` and `register_issue`, not a numeric
quality score. The [five authored pilot proposals](pilot.json) disclose author and
reviewer roles, evidence and uncertainty; an absent independent review is pending,
never manufactured. AI observations are not independent human certification.

These language cases do not replace the structural/FFmpeg checks in
`tests/check.py` or the per-cue and per-image workflow.
