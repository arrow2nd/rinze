{
  description = "rinze: 左文脈で候補を並べる SKK 方式の IME";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # llama-cpp-sys-2 の build.rs は同梱の llama.cpp を cmake でビルドし、bindgen (libclang) でヘッダを読む
      nativeDeps = pkgs: [ pkgs.cmake pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];
      # nix のビルドでは -march=native が効かず、指定しないと ggml の SIMD が無効になって約 5 倍遅くなる。
      # build.rs は CARGO_CFG_TARGET_FEATURE を見て GGML_AVX2 などを有効にする
      simdEnv = pkgs: pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isx86_64 {
        RUSTFLAGS = "-C target-feature=+avx,+avx2,+fma,+f16c,+bmi2";
      };
    in
    {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell ({
          nativeBuildInputs = nativeDeps pkgs ++ [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            pkgs.clippy
            pkgs.neovim
          ];
          # tests/nvim/blink.sh が blink.cmp との連携を確かめるのに使う
          RINZE_BLINK = "${pkgs.vimPlugins.blink-cmp}";
        } // simdEnv pkgs);
      });

      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage ({
          pname = "rinzed";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./crates ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "rinzed" ];
          nativeBuildInputs = nativeDeps pkgs;
          cargoTestFlags = [ "--workspace" ];
        } // simdEnv pkgs);
      });
    };
}
