{
  description = "Langmeld, a Rust framework for building languages";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, self, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/package.nix { };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/langmeld";
        };
      });

      overlays.default = final: prev: {
        langmeld = (prev.extend rust-overlay.overlays.default).callPackage ./nix/package.nix { };
      };

      devShells = forAllSystems (
        pkgs:
        let
          manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        in
        {
          default = pkgs.callPackage ./nix/shell.nix { };
          # The oldest compiler the crates support.
          msrv = pkgs.callPackage ./nix/shell.nix {
            rustToolchain = pkgs.rust-bin.stable.${manifest.workspace.package.rust-version}.minimal;
          };
          # `cargo fuzz` needs a nightly compiler.
          fuzz = pkgs.callPackage ./nix/shell.nix {
            rustToolchain = pkgs.rust-bin.nightly."2026-11-25".minimal;
          };
        }
      );
    };
}
