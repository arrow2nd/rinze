# 開発環境

Nix の開発シェル（`nix develop`）に、Rust ツールチェイン、llama.cpp のビルドに必要なツール、Neovim、リンタ類、連携テスト用の `blink.cmp`（`RINZE_BLINK`）がすべて揃っている。

## 基本コマンド

```sh
nix develop
cargo test --workspace   # モデル不要の単体テスト
tests/lint.sh            # Lua / Rust のフォーマット・静的解析・複雑度チェック
nix build                # モデルと辞書の既定値付き rinzed のビルド
nix flake check          # ビルド・lint・ソケットパス一致・各フロントエンドの検証を一括実行
```

## ビルド時の前提

- **既定ソケットパスの一致**: Rust 側（`rinze-socket`）と Neovim プラグイン側（Lua）は既定ソケットパスをそれぞれ解決する。両者の規則がずれないよう、共通のフィクスチャで一致を検証している。
- **CPU 最適化フラグ**: x86_64 環境では SIMD 命令（AVX2 等）を有効にするため、flake 側でビルドフラグを指定している。macOS では [Apple Silicon 向けの方針](../architecture/performance.md#apple-silicon-向けの方針)に従い Metal バックエンドを無効化する。`nix develop` 内でも同じ設定が適用される。
- **C ヘッダの同期**: ネイティブフロントエンド向けの C ヘッダ（`crates/rinze-client/include/rinze_client.h`）は Rust の FFI 定義から自動生成しており、テストで差分を検出する。FFI を変更した際は `RINZE_WRITE_HEADER=1 cargo test -p rinze-client header` で再生成する。

## モデルと辞書の環境変数

[モデルと既定の辞書](../usage/dictionaries.md#モデルと既定の辞書)はリポジトリに含めず flake から取得する。実モデルを使うテストや評価を手元で走らせる際は、以下のようにパスを環境変数へ渡す。

```sh
export RINZE_MODEL=$(nix build --no-link --print-out-paths .#rinze-zenz-model)
export RINZE_DICT=$(nix build --no-link --print-out-paths .#rinze-dictionaries)/share/skk/SKK-JISYO.L
```
