{ pkgs }:

{
  default = pkgs.mkShell {
    packages = [
      pkgs.rustup
      pkgs.stdenv.cc
      pkgs.nodejs_22
    ];
  };
}
