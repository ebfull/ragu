# Prose Quality Review Policy

## Scope

Review text-level writing quality in book chapter markdown files
(`book/src/`). This policy covers concerns that span paragraphs or larger
units of text — terminology, transitions, and coherence. For sentence-level
mechanics (voice, structure, word choice), see `grammar.md`. All rules in
`.claude/book-review/standards.md` also apply.

## Terminology Consistency

- Once a term is chosen for a concept, it must be used consistently throughout
  the chapter. Flag any term that appears to mean the same thing as another term
  used elsewhere in the same chapter.
- Defined terms from `book/src/appendix/terminology.md` take precedence.
- Use lowercase for technical terms that are descriptive phrases, not proper
  nouns. Write "proof-carrying data", not "Proof-Carrying Data". At the start
  of a sentence, capitalize only the first word: "Proof-carrying data".
  Proper nouns (Halo, Zcash, Pasta, Poseidon) and acronyms (SNARK, PCD, ECDLP)
  remain capitalized. Flag any descriptive technical phrase that is
  title-cased as though it were a proper noun.

## Transitions

- Each paragraph should connect to the preceding one. Flag abrupt topic shifts
  with no connecting logic.
- Section transitions should give the reader a reason to keep reading.