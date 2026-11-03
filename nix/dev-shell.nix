{ pkgs }:

{
  default = pkgs.mkShell {
    packages = [
      pkgs.rustup
      pkgs.bun
      pkgs.python3
      pkgs.wasm-bindgen-cli
    ];
  };
}
