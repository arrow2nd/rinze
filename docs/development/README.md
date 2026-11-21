# 開発とテスト

開発環境の準備、フォーマット・リンタ・複雑性の検査、モデルや fcitx5 を使ったテストと精度評価。

| ページ | 内容 |
| --- | --- |
| [setup.md](setup.md) | `nix develop`、基本コマンド、既定ソケットの一致テスト、`RUSTFLAGS`、モデルと辞書の環境変数 |
| [lint.md](lint.md) | `tests/lint.sh` による Lua / Rust のフォーマット、リンタ、不要コード、複雑性検査 |
| [testing.md](testing.md) | モデルが要るテスト、日常の文章での 1 位正解率 (`corpus.sh`)、fcitx5 のテスト |
