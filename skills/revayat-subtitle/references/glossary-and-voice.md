# Glossary and character voice

Load this before researching a new anime, continuing a season, or editing a
sequel. A fresh conversation has no guaranteed memory of the previous files.

## Establish the identity

Identify the anime and season from the user's message and attachments. Search
the anime title online before translating. Prefer official anime sites, publishers,
distributors and character credits; use reliable secondary references when a
primary source omits a name. Record actual URLs and what they establish.

Verify original-script names and confirmed readings, then choose consistent Persian spellings for
characters, places, organizations and recurring terms. User-approved spellings
take precedence over a new preference. Correct a previously mistaken spelling
explicitly and update its occurrences instead of silently changing the glossary.

## Continue a series

A new anime starts a separate glossary. The next episode, batch or season of the
same anime loads its previous glossary before its first cue. Keep locked names,
nicknames, pronouns, forms of address and character voice; add new entries as needed.
If the prior glossary is unavailable, request it or previous output and disclose
that continuity remains unverified until recovered.

Honorifics are retained: `-san` → `-سان`, `-kun` → `-کون`, `-sama` → `-ساما`,
`-chan` → `-چان`, `onii-chan` → `اونی-چان`. Keep the series' separator convention.
Distinguish a suffix from an unrelated word with the same letters.

## Working record

`glossary.json` contains `series`, `research`, `terms` and `terms_reviewed`.
Each research entry has a real `url` and explanatory `note`. Each term needs
`source`, `target` and `locked: true`; optional fields include `aliases`, `kind`,
`source_url` and `note`. Set `terms_reviewed` only after research and review.

The builder checks that this record exists and is consistent with the series.
Actually applying names throughout every cue is the agent's responsibility; a
checkbox is not a linguistic certificate. Return the updated glossary separately
from `Sub.zip` so the following batch can reuse it.
