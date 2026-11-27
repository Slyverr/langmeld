{
  mkShell,
  rust-bin,
  cargo-fuzz,
  cargo-insta,
  cargo-nextest,
  nodejs,
  wasm-pack,
}:

mkShell {
  packages = [
    (rust-bin.fromRustupToolchainFile ../rust-toolchain.toml)
    cargo-fuzz
    cargo-insta
    cargo-nextest
    nodejs
    wasm-pack
  ];
}
