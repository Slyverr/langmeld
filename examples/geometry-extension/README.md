# Geometry definition extension

This crate models a separately distributed language extension. It exports
`.langmeld` source text and project-relative keys without depending on the
consumer's parser, runtime, or editor stack. A language host chooses the
crate dependency and registers the exported sources with its resolver.

The [`embedded-definitions` example](../embedded-definitions) consumes this
crate and parses a program using its drawing construct.
