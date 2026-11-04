
## Browser/WASM adapter

The optional `wasm` feature exposes `LanguageHost` for JavaScript. It accepts
explicit in-memory `.langmeld` root/module sources or canonical `LanguageSpec` JSON.
`LanguageHost.fromProject()` uses this project's compiled model and Rust
callbacks. Parsing returns diagnostics, editor classifications, and
parser-derived completion context; `analyzeWorkspace` accepts explicitly
supplied source files and returns cross-file generic analysis. The adapter
performs no filesystem access; callers resolve imports and provide every
module. Returned spans are UTF-8 byte offsets. Opaque Rust host values are not
serialized. Expression values are recursively tagged JSON objects (`number`,
`boolean`, `name`, `custom`, `prefix`, `infix`, `group`, `call`, `member`, or
`index`) with byte spans; operators and member names include their own spans.
Dynamically supplied models use generic analysis only and cannot bind host
callbacks; `fromSpec` rejects models requiring a custom parser.

This runs Langmeld inside WebAssembly; it does not compile this language to
WebAssembly. Add the wasm target and `wasm-bindgen` CLI matching the pinned
crate, then build with:

```sh
rustup target add wasm32-unknown-unknown
cargo build --lib --features wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir target/wasm-bindgen target/wasm32-unknown-unknown/debug/@CRATE@.wasm
```
