# Agent Instructions

Read [README.md](README.md) for the current repository contents. Follow
[CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow, verification,
and commit conventions.

## Project Rules

- The project direction is a general-purpose language framework that combines
  Langmeld definitions with a Rust host. Keep this as a design goal until the
  relevant capability exists in code and is verified.
- Make changes in small, coherent, buildable steps. Prefer clear contracts and
  focused implementations over broad, speculative changes.
- Keep modules focused and APIs explicit. Add abstractions and dependencies only
  when a concrete requirement justifies them.
- Do not claim a capability until code and relevant verification demonstrate it.
- Keep README limited to the project's current, verified contents. Update it
  and contributor guidance when those contents or workflows change.

## Local Skills

Use the project-local skills when relevant:

- Rust implementation and review: `.agents/skills/rust-best-practices/SKILL.md`
- Rust tests and benchmarks: `.agents/skills/rust-testing/SKILL.md`

Skill versions and integrity hashes are recorded in `skills-lock.json`.
Review updates before replacing the copied skills, and apply their advice
selectively to the project's actual needs.
