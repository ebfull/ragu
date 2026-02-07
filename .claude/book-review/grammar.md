# Grammar Review Policy

## Scope

Review sentence-level grammar and mechanics in book chapter markdown files
(`book/src/`). This policy covers voice, sentence structure, and word choice.
For text-level concerns (terminology, transitions, coherence), see
`prose.md`. All rules in `.claude/book-review/standards.md` also apply.

## Voice and Tone

- Prefer active voice, but accept passive voice in mathematical definitions and
  protocol descriptions where the agent is irrelevant.
- Maintain a direct, confident tone. Avoid hedging ("perhaps", "it might be")
  unless genuine uncertainty is being communicated.
- Address the reader as "we" when walking through constructions together. Use
  "you" for direct instructions in the user guide.

## Sentence Structure

- Vary sentence length. A long explanatory sentence should be followed by a
  short, punchy one.
- Avoid long parenthetical asides in the middle of sentences. Use a separate
  sentence instead.
- Technical terms should be introduced before they're used. Flag forward
  references to undefined terms.

## Word Repetition

- Avoid repeating the same word within a sentence. When the same word
  appears twice, rephrase to use a synonym or restructure the sentence.
  Example: "Developed for use with the Pasta curves used in Zcash" →
  "Developed for the Pasta curves employed in Zcash".
- Within a paragraph, watch for the same word appearing too frequently.
  Vary word choice when natural alternatives exist (e.g., "verify" /
  "check" / "confirm"; "construct" / "build" / "create").
- **Exempt**: technical terms, proper nouns, acronyms, and
  domain-specific vocabulary. Terminological consistency (see
  `prose.md`) takes precedence — do not replace a defined term with a
  synonym for the sake of variety.

## Punctuation Density

- Watch for em dash overuse at the page level. Em dashes are effective for
  asides and interjections, but when a page uses them in nearly every
  paragraph the writing feels monotonous and the dashes lose their punch.
- When flagging density, do NOT suggest mechanically replacing every em dash.
  Instead, identify the least impactful usages — the ones where an em dash
  is convenient but not essential — and suggest rephrasing those sentences so
  that the dash becomes unnecessary. The goal is natural-sounding prose, not
  em dash avoidance.
- Preferred alternatives (choose whichever reads most naturally in context):
  commas, parentheticals, semicolons, subordinate clauses, footnotes, or
  restructuring the sentence to eliminate the aside entirely.
- Leave the strongest em dash usages intact. A page with 2-3 well-placed em
  dashes reads better than a page with zero.