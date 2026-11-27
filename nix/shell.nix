{
  mkShell,
  rust-bin,
  cargo-fuzz,
  cargo-insta,
  cargo-mutants,
  cargo-nextest,
  nodejs,
  wasm-bindgen-cli,
  wasm-pack,
  rustToolchain ? rust-bin.fromRustupToolchainFile ../rust-toolchain.toml,
}:

mkShell {
  packages = [
    rustToolchain
    cargo-fuzz
    cargo-insta
    cargo-mutants
    cargo-nextest
    nodejs
    wasm-bindgen-cli
    wasm-pack
  ];
}
