# AGENTS.md

Entry point for AI agents. This file holds no project facts of its own; it tells you
where they are. Each fact lives in exactly one file: read the one that owns it, and never
copy it elsewhere.

## Read

| File | Owns | Read when |
|------|------|-----------|
| [README.md](README.md) | What the project is, install, usage | First, for context |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Setup, commands, structure, workflow, rules, conventions, commits, PRs, releases | Before changing anything |
| [CHANGELOG.md](CHANGELOG.md) | Notable changes per version | Before finishing a change |
| [docs/README.md](docs/README.md) | In-depth docs: architecture, guides, reference | Changing how something works, or needing depth |

Files that don't exist yet are skipped. Nested `AGENTS.md` files in subdirectories add to
this one; the one nearest the files you change wins on conflict.

## Do

1. Follow [CONTRIBUTING.md](CONTRIBUTING.md) exactly. It applies to you as to any
   contributor: workflow, rules, conventions, commits, changelog, docs.
2. Before finishing, run the format, lint, test and WASM check commands it lists; they must
   pass.
3. Where CONTRIBUTING.md says "discuss first", state the change and its reason in the pull
   request description, so the reviewer can weigh it there.
4. Push your branch and open a pull request as CONTRIBUTING.md describes, and merge it with
   Rebase and merge once every check passes. Never tag, release or publish: that stays with
   the maintainer.
5. If anything here or in CONTRIBUTING.md is missing, wrong or out of date, say so and
   propose the fix in the file that owns it, not here.
