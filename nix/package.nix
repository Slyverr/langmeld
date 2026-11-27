{
  lib,
  makeRustPlatform,
  rust-bin,
}:

let
  manifest = builtins.fromTOML (builtins.readFile ../Cargo.toml);
  rustPlatform = makeRustPlatform (
    let
      toolchain = rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
    in
    {
      cargo = toolchain;
      rustc = toolchain;
    }
  );
  root = ../.;
in
rustPlatform.buildRustPackage {
  pname = "langmeld";
  inherit (manifest.workspace.package) version;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (root + "/rust-toolchain.toml")
      (root + "/crates")
      (root + "/examples")
      (root + "/tests")
      (root + "/LICENSE-MIT")
      (root + "/LICENSE-APACHE")
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "langmeld-cli"
  ];
  doCheck = false;

  meta = {
    description = "Experimental Rust framework for building languages";
    homepage = manifest.workspace.package.repository;
    license = [
      lib.licenses.mit
      lib.licenses.asl20
    ];
    mainProgram = "langmeld";
  };
}
