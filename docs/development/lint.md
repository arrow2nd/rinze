# 検査

`tests/lint.sh` は次の道具を走らせ、`nix flake check` の `lint-lua` と `lint-rust` も同じものを走らせる。

| 対象 | フォーマッタ | リンタと不要なコード | 複雑性 |
| --- | --- | --- | --- |
| Lua (`nvim`、`tests`、`nix/demo.lua`) | stylua (`.stylua.toml`) | luacheck (`.luacheckrc`)。未使用の変数、引数、値 | luacheck の循環的複雑度 (10 まで) |
| Rust | rustfmt | clippy (`-D warnings`、lint の設定は `Cargo.toml` の `[workspace.lints]`)。`unreachable_pub` で外から見えない `pub` を `pub(crate)` にさせ、使われていなければ rustc の `dead_code` が見つける。使われていない依存クレートは cargo-shear | clippy の `cognitive_complexity` (15 まで)、`too_many_lines` (100 行まで)、`too_many_arguments` (7 個まで)。しきい値は `clippy.toml` |

フォーマットを直すには `stylua nvim tests nix` と `cargo fmt` を走らせる。
しきい値を超える既存の関数は、その関数に印を付けて許している。Lua は `-- luacheck: ignore 561`、Rust は `#[expect(clippy::…)]` で、`git grep -e 'luacheck: ignore' -e 'expect(clippy::'` で一覧できる。`#[expect]` は lint が出なくなると警告になるので、関数を分けたら印も外す。
