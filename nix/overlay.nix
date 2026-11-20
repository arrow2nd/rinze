final: prev:
let
  inherit (final) lib;
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
in
{
  rinzed-unwrapped = final.rustPlatform.buildRustPackage (
    {
      pname = "rinzed-unwrapped";
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
        "rinzed"
      ];
      cargoTestFlags = [ "--workspace" ];
      # llama-cpp-sys-2 の build.rs は同梱の llama.cpp を cmake でビルドし、bindgen (libclang) でヘッダを読む
      nativeBuildInputs = [
        final.cmake
        final.pkg-config
        final.rustPlatform.bindgenHook
      ];
      meta = {
        description = "rinze の常駐プロセス (モデルと辞書の既定値なし)";
        mainProgram = "rinzed";
      };
    }
    # nix のビルドでは -march=native が効かず、指定しないと ggml の SIMD が無効になって約 5 倍遅くなる。
    # build.rs は CARGO_CFG_TARGET_FEATURE を見て GGML_AVX2 などを有効にする
    // lib.optionalAttrs final.stdenv.hostPlatform.isx86_64 {
      RUSTFLAGS = "-C target-feature=+avx,+avx2,+fma,+f16c,+bmi2";
    }
  );

  rinze-zenz-model = final.fetchurl {
    name = "zenz-v3.2-small-Q5_K_M.gguf";
    url = "https://huggingface.co/Miwa-Keita/zenz-v3.2-small-gguf/resolve/ef3e66e3665fe6a19598e6578e9b88a994b647fd/ggml-model-Q5_K_M.gguf";
    hash = "sha256-KcIj1MIzJ7gP0T67WrJVUFekYxeZfV2jkVhP++8NtnM=";
    meta = {
      description = "ニューラルかな漢字変換モデル zenz-v3.2-small の GGUF (Q5_K_M)";
      homepage = "https://huggingface.co/Miwa-Keita/zenz-v3.2-small-gguf";
      license = lib.licenses.cc-by-sa-40;
    };
  };

  rinzed = final.callPackage (
    {
      runCommand,
      makeBinaryWrapper,
      rinzed-unwrapped,
      model ? final.rinze-zenz-model,
      # 辞書のパッケージ (share/skk の下の辞書をすべて使う) か辞書のファイル。同じ見出しの候補はこの順に並べてから zenz で並べ替える
      dictionaries ? with final.skkDictionaries; [
        l
        jinmei
        geo
        station
        propernoun
      ],
    }:
    runCommand "rinzed-${version}"
      {
        nativeBuildInputs = [ makeBinaryWrapper ];
        passthru = {
          inherit model dictionaries;
          unwrapped = rinzed-unwrapped;
        };
        meta = rinzed-unwrapped.meta // {
          description = "rinze の常駐プロセス (zenz のモデルと SKK 辞書を既定値に持つ)";
        };
      }
      ''
        dicts=()
        for source in ${lib.escapeShellArgs (map (source: "${source}") dictionaries)}; do
          if [ -d "$source" ]; then
            dicts+=("$source"/share/skk/*)
          else
            dicts+=("$source")
          fi
        done
        makeBinaryWrapper ${rinzed-unwrapped}/bin/rinzed $out/bin/rinzed \
          --set-default RINZED_DEFAULT_MODEL ${model} \
          --set-default RINZED_DEFAULT_DICTS "$(IFS=:; echo "''${dicts[*]}")"
      ''
  ) { };

  rinze-nvim = final.vimUtils.buildVimPlugin {
    pname = "rinze";
    inherit version;
    src = ../nvim;
  };
}
