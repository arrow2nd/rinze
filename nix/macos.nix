{
  lib,
  stdenv,
  callPackage,
  swift,
  rinzed,
}:
let
  rinze-client = callPackage ./rinze-client.nix { };
  header = "${rinze-client}/include/rinze_client.h";
  # 指定しないと swiftc はビルドした機械の macOS の版を下限にする。Info.plist の LSMinimumSystemVersion に合わせる
  target = "${stdenv.hostPlatform.darwinArch}-apple-macosx14.0";
in
stdenv.mkDerivation {
  pname = "rinze-macos";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = ../macos;

  nativeBuildInputs = [ swift ];

  buildPhase = ''
    runHook preBuild
    swiftc -O -swift-version 5 -target ${target} -import-objc-header ${header} \
      -framework Cocoa -framework InputMethodKit \
      -o rinze Sources/*.swift ${rinze-client}/lib/librinze_client.a
    runHook postBuild
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck
    swiftc -swift-version 5 -target ${target} -import-objc-header ${header} \
      -o test Sources/Keys.swift Sources/Modes.swift Tests/main.swift
    ./test Info.plist Resources
    runHook postCheck
  '';

  # アプリが起動する rinzed を Info.plist の RinzedPath に書く。入力メソッドのプロセスの PATH には利用者のシェルの設定が入らない
  installPhase = ''
    runHook preInstall
    app="$out/Library/Input Methods/rinze.app/Contents"
    mkdir -p "$app/MacOS" "$app/Resources"
    cp rinze "$app/MacOS/rinze"
    cp Resources/*.tiff "$app/Resources/"
    sed 's|^</dict>$|\t<key>RinzedPath</key>\n\t<string>${lib.getExe rinzed}</string>\n</dict>|' \
      Info.plist > "$app/Info.plist"
    runHook postInstall
  '';

  passthru = { inherit rinze-client; };

  meta = {
    description = "rinze の macOS 向けの入力メソッド";
    platforms = lib.platforms.darwin;
  };
}
