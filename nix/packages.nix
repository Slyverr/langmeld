{ pkgs, source }:

let
  workspace = builtins.fromTOML (builtins.readFile "${source}/Cargo.toml");
  langmeld = pkgs.rustPlatform.buildRustPackage {
    pname = "langmeld";
    version = workspace.workspace.package.version;
    src = source;
    cargoLock.lockFile = "${source}/Cargo.lock";
    cargoBuildFlags = [ "--package" "langmeld-cli" ];
    cargoTestFlags = [ "--package" "langmeld-cli" ];
    nativeCheckInputs = [ pkgs.rustfmt ];

    meta = {
      description = "Command-line tools for bootstrapping Langmeld language projects.";
      mainProgram = "langmeld";
      license = with pkgs.lib.licenses; [ mit asl20 ];
    };
  };
in
{
  default = langmeld;
  inherit langmeld;
}
