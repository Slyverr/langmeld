{ pkgs }:

{
  default = pkgs.mkShell {
    packages = [
      pkgs.rustup
      pkgs.bun
      pkgs.wasm-bindgen-cli
    ];
  };
}
