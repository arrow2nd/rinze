# rinze

🖋️ zenz で単語変換だけを文脈からよしなにやってくれる SKK 実装

## 対応環境

- [Neovim](docs/frontends/neovim.md)
- [fcitx5](docs/frontends/fcitx5.md)
- [macOS](docs/frontends/macos.md)

## 試す

```sh
nix run /path/to/rinze#demo
```

`--clean` の Neovim に rinze と blink.cmp を設定して起動し、一時ディレクトリ
(`/tmp/rinze-demo-*`)
の状態で試せる。終了するとディレクトリごと消すため、利用者のファイルには触れない。導入の詳細は
[flake で入れる](docs/install/flake.md) を参照。

## ドキュメント

全体の索引は [docs/README.md](docs/README.md) にある。

| カテゴリ                                      | 主な内容                                                                                                                                                                   |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [入れ方](docs/install/README.md)              | [flake](docs/install/flake.md) / [Neovim](docs/install/neovim.md) / [fcitx5](docs/install/fcitx5.md) / [macOS](docs/install/macos.md)                                                                       |
| [打ち方と変換の仕組み](docs/usage/README.md)  | [打ち方・abbrev・生成](docs/usage/typing.md) / [入力モード・ローマ字](docs/usage/modes.md) / [モデルと辞書](docs/usage/dictionaries.md) / [学習](docs/usage/learning.md)   |
| [フロントエンド](docs/frontends/README.md)    | [Neovim](docs/frontends/neovim.md) / [コマンドライン](docs/frontends/neovim-cmdline.md) / [blink.cmp](docs/frontends/neovim-blink.md) / [fcitx5](docs/frontends/fcitx5.md) / [macOS](docs/frontends/macos.md) |
| [構成と内部仕様](docs/architecture/README.md) | [クレート構成](docs/architecture/crates.md) / [通信](docs/architecture/protocol.md) / [性能と資源](docs/architecture/performance.md)                                       |
| [開発とテスト](docs/development/README.md)    | [開発環境](docs/development/setup.md) / [検査](docs/development/lint.md) / [テストと評価](docs/development/testing.md)                                                     |
