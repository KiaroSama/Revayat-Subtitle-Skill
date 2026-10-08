# Editorial calibration rubric

Use this rubric after reading the source, supplied context and series glossary.
It structures a coordinator's review; it never accepts a translation automatically.
An unseen wording is not a failure merely because `score.py` does not list it.

## Five dimensions

| Dimension | Evidence to record | Authored control |
|---|---|---|
| Participant and referent | Who acts, who receives the action, and which established person a pronoun denotes | “The teacher criticized Rin.” must not become “Rin criticized the teacher.” |
| Negation, aspect and restriction | Preserve negative requests, not-yet/no-longer, experience and only; retain interruptions without completing them | “Don't touch it.” must not become permission to touch it. |
| Names, honorifics and intensity | Check locked spelling, meaningful address and source intensity without censorship or escalation | “Thank you, Rin-san.” retains the researched name and `-san`. |
| Spoken register | Check natural spoken Persian and appropriate politeness without inventing relationships | “I want to go home.” can be expressed by a faithful unseen colloquial variant. |
| Logical mixed-script and quotation scope | Check logical word order, math, embedded Latin tokens and which words are quoted; appearance is assessed separately | `x < 5` remains a comparison; quotes around “Hope” do not extend to the whole sentence. |

## Categorical outcomes

- `faithful`: the supplied source/context support the proposal across the relevant dimensions.
- `needs_context`: a decisive referent, register or glossary fact is unavailable. Name the missing fact; do not invent it.
- `meaning_changed`: identify the changed participant, polarity, aspect, restriction or omitted content.
- `register_issue`: meaning is retained but speech/register/address needs coordinator review.

Record one overall outcome and a short observation for each dimension. A dimension
without decisive context remains uncertain even when another dimension proves a
meaning change. These categories are not numeric scores, severity ratings or
statistically calibrated estimates.

## Review record

Record case ID, authored source, supplied context (including explicitly missing
context), proposed answer, reviewer role/model or genuine human qualification,
UTC date, category, concise evidence and uncertainty. Distinguish author,
reviewer and coordinator roles; one AI serving multiple roles is not independent
human review. Never invent reviewer participation or qualifications.

The finite [pilot](pilot.json) discloses its actual AI roles and limitations.
Its five observations are illustrative calibration, not a representative benchmark,
blind evaluation, full-language certification or an automatic approval policy.
Full source/episode meaning review, glossary consistency, audio alignment, rendered
image inspection and active-player selection remain separate work.
