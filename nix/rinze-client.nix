{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "rinze-client";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "rinze-client"
  ];
  cargoTestFlags = [
    "-p"
    "rinze-client"
  ];
  postInstall = ''
    mkdir -p $out/include
    cp crates/rinze-client/include/rinze_client.h $out/include/
  '';
}
