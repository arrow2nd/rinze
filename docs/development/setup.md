# 開発環境

NixOS / nix で `nix develop` に入ると、cargo、llama.cpp のビルドに要る cmake と libclang、Neovim、[検査](lint.md)の道具が揃い、blink.cmp のパスが `RINZE_BLINK` に入る。

```sh
nix develop
cargo test --workspace           # モデルの要らないテスト
tests/lint.sh                    # フォーマット、リンタ、不要なコード、複雑性の検査 (lua か rust を渡すと片方だけ)
nix build                        # result/bin/rinzed (モデルと辞書の既定値を持つラッパー)
nix flake check                  # rinzed とプラグインのビルド、tests/lint.sh、既定のソケットのパスの一致 (Lua 側)、cmd を省いた起動 (tests/nvim/default_cmd.lua)、Linux では fcitx5 のアドオンのテスト
```

rinzed と Neovim プラグインは既定のソケットのパスを別々に組み立てる。
両者が同じパスを返すことは `crates/rinze-socket/tests/fixtures/default_socket.json` の表で確かめる。Rust 側の規則は `rinze-socket` の 1 か所にあり、rinzed と rinze-client が使う。
Rust 側は `cargo test --workspace` (と `nix build`) に含まれ、Lua 側は次で走らせる。

```sh
nvim --headless --clean -l tests/nvim/default_socket.lua
```

x86_64 では flake が `RUSTFLAGS` で AVX2 などを明示する。指定しないと ggml の SIMD が無効になり、変換が約 5 倍遅くなる。
macOS では `GGML_METAL=OFF` で llama.cpp の Metal のバックエンドを外す ([Apple Silicon](../architecture/performance.md#apple-silicon))。`nix develop` の中の `cargo build` にも同じ値が入る。

## モデルと辞書の環境変数

[モデルと既定の辞書](../usage/dictionaries.md#モデルと既定の辞書)はリポジトリに入れず、flake が取得する。テストと測定では、モデルと辞書のパスを環境変数で渡す。

```sh
export RINZE_MODEL=$(nix build --no-link --print-out-paths .#rinze-zenz-model)
export RINZE_DICT=$(nix build --no-link --print-out-paths .#rinze-dictionaries)/share/skk/SKK-JISYO.L
```

## C ヘッダの再生成

fcitx5 アドオンが使うヘッダ `crates/rinze-client/include/rinze_client.h` は cbindgen で `crates/rinze-client/src/ffi.rs` から生成する (`cargo test -p rinze-client` がずれを検出する)。`ffi.rs` を変えたら次で作り直す。

```sh
RINZE_WRITE_HEADER=1 cargo test -p rinze-client header
```
