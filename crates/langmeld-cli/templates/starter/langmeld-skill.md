---
name: langmeld-language-project
description: Work on this Rust language project using its modular Langmeld definitions and host APIs.
---

# Langmeld project workflow

- Read the project `README.md` and `AGENTS.md` first.
- Put the language header in `language/language.lm` and constructs under
  `language/constructs/`.
- Use Rust code under `src/` for behavior that does not fit declarative syntax,
  including custom lexers, parser callbacks, analysis, and output backends.
- Keep runnable source fixtures in `examples/` and cover behavior in `tests/`.
- Treat `.langmeld/generated/` as derived output; update the source model, then
  regenerate through Langmeld's API.
- Run formatting, tests, and strict Clippy before finishing a change.
