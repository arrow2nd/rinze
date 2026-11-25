# 静的解析と品質検査

`tests/lint.sh` で Lua と Rust のフォーマット、静的解析、不要コード検出、複雑度チェックを一括実行する。引数に `lua` または `rust` を渡すと片方だけ実行でき、`nix flake check` でも同じ検査が走る。

## 検査の対象と方針

- **Lua（`nvim/`、`tests/`、`nix/`）**: `stylua` でフォーマット、`luacheck` で未使用変数・引数や循環的複雑度を検査する。
- **Rust（ワークスペース全体）**: `rustfmt` でフォーマット、`clippy` で警告をエラー扱いとしつつ可視性（`unreachable_pub`）や関数の複雑度・長さ・引数数を検査する。未使用の依存クレートは `cargo-shear` で検出する。

具体的なルールや複雑度のしきい値は、リポジトリ直下の `.stylua.toml`、`.luacheckrc`、`Cargo.toml`（`[workspace.lints]`）、`clippy.toml` に定義している。

## フォーマット修正と例外の扱い

フォーマットの自動修正は以下のコマンドで行う。

```sh
stylua nvim tests nix
cargo fmt
```

やむを得ず複雑度などのしきい値を超える関数には、Rust では `#[expect(clippy::...)]`、Lua では `-- luacheck: ignore ...` を付けて明示的に許可する。特に Rust の `#[expect]` はリファクタリングで警告が解消されると逆に通知されるため、不要になった例外指定を残さずに済む。
