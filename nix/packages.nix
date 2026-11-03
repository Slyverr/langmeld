{ pkgs, source }:

let
  langmeld = pkgs.rustPlatform.buildRustPackage {
    pname = "langmeld";
    version =
      (builtins.fromTOML (builtins.readFile "${source}/Cargo.toml")).workspace.package.version;
    src = source;
    cargoLock.lockFile = "${source}/Cargo.lock";
    cargoBuildFlags = [ "--package" "langmeld-cli" ];
    cargoTestFlags = [ "--package" "langmeld-cli" "--bins" ];

    meta = {
      description = "Command-line tools for bootstrapping Langmeld language projects.";
      mainProgram = "langmeld";
      license = with pkgs.lib.licenses; [ mit asl20 ];
    };
  };
in {
  default = langmeld;
  inherit langmeld;
}
