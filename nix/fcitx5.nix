{
  lib,
  stdenv,
  rustPlatform,
  cmake,
  kdePackages,
  pkg-config,
  fcitx5,
  rinzed,
}:
let
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  rinze-client = rustPlatform.buildRustPackage {
    pname = "rinze-client";
    inherit version;
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
  };
in
stdenv.mkDerivation {
  pname = "fcitx5-rinze";
  inherit version;
  src = ../fcitx5;

  nativeBuildInputs = [
    cmake
    kdePackages.extra-cmake-modules
    pkg-config
  ];
  buildInputs = [ fcitx5 ];

  cmakeFlags = [
    "-DRINZE_CLIENT_LIB=${rinze-client}/lib/librinze_client.a"
    "-DRINZE_CLIENT_INCLUDE=${rinze-client}/include"
  ];

  # testfrontend で打鍵を送る。test/run.sh が rinzed をビルドの一時ディレクトリのソケット、ユーザー辞書、学習の履歴で起動する
  nativeCheckInputs = [ rinzed ];
  checkPhase = ''
    runHook preCheck
    RINZED=${lib.getExe rinzed} RINZE_TEST_DIR=$TMPDIR ctest --output-on-failure --verbose
    runHook postCheck
  '';

  passthru = { inherit rinze-client; };

  meta = {
    description = "rinze の fcitx5 アドオン";
    platforms = lib.platforms.linux;
  };
}
