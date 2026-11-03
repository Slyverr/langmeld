{
  description = "Langmeld framework and development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
    in {
      packages = forAllSystems (system:
        import ./nix/packages.nix {
          pkgs = pkgsFor system;
          source = self;
        });

      devShells = forAllSystems (system:
        import ./nix/dev-shell.nix {
          pkgs = pkgsFor system;
        });
    };
}
