# Compatibility and versioning

Langmeld is pre-1.0. Rust APIs, host callbacks, generated Rust code, and
generated artifact layouts can change in a minor release. Rebuild and
compile-check language hosts when upgrading. There is no stable binary plugin
ABI.

The project's version boundaries are independent:

| Boundary | Current contract |
| --- | --- |
| Rust crates and generated Rust | Crate SemVer; APIs may change in a minor release before 1.0. |
| Language model | Schema 1 retains inline brace blocks. Schema 2 lets each block declare its opener, closer, and child separators. Schema 3 adds reusable expression grammars and expression fields. Schema 4 adds lexical comment policy. Schema 5 adds per-symbol declaration-order, duplicate, and shadowing policies. `version` describes the language release and is SemVer when versioned extensions are used. |
| `langmeld.json` | Schema 1 supports declarative definitions; schema 2 supports declarative and Rust-host definitions. |
| `langmeld.lock` | Schema 1 records definition hashes (CRLF is normalized to LF), resolved extension metadata, and the effective language hash. |
| Local declarative packages | Each package declares its own version, target-language version range, and package dependencies. Resolution is local to declared paths. |
| `.langmeld/generated/` | Derived files from the installed Langmeld version; regenerate after upgrading. Layout is not a cross-version stability promise before 1.0. |

Loaders reject unsupported schema versions. Manifest writes preserve the
existing supported schema. A stale definition lock prevents project checks
until refreshed with `langmeld lock`. Langmeld does not automatically migrate
language model, manifest, or lock schemas.

The version policy and release checks are documented in
[CONTRIBUTING.md](../CONTRIBUTING.md). User-facing changes are tracked in the
[changelog](../CHANGELOG.md).
