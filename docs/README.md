# rinze ドキュメント索引

rinze の導入・使い方・フロントエンド別仕様・内部構成・開発手順の索引。

## [入れ方](install/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [flake で入れる](install/flake.md) | 試用 (`demo`)、flake の出力一覧、モデルと辞書の既定値の変更 (`override`) |
| [Neovim に入れる](install/neovim.md) | home-manager と lazy.nvim、overlay (`programs.neovim`)、nixvim での導入 |
| [fcitx5 で入れる](install/fcitx5.md) | NixOS での fcitx5 アドオン導入、`fcitx5-skk` と並べて使う設定 |
| [macOS で入れる](install/macos.md) | home-manager での入力メソッドの導入、手元で組んで入れる手順 |

## [打ち方と変換の仕組み](usage/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [打ち方](usage/typing.md) | 基本操作、辞書登録・削除 (`X`)、`Q`、Enter、abbrev、zenz の表記生成 |
| [入力モードとローマ字](usage/modes.md) | 5 つの入力モードと切り替え表、ローマ字規則 (ddskk との違い) |
| [モデルと辞書](usage/dictionaries.md) | zenz モデルと既定の SKK 辞書、複数辞書の結合と測定、ユーザー辞書 |
| [学習](usage/learning.md) | 学習の仕組み、`history.tsv` の保存と性能、加点の形と大きさの評価 |

## [フロントエンド](frontends/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [Neovim](frontends/neovim.md) | `setup`、キー割り当て、モードとステータスライン、ソケットと単一プロセス |
| [Neovim のコマンドライン](frontends/neovim-cmdline.md) | `/` `?` `:` での入力、フロート表示、左文脈、blink.cmp 補完との兼ね合い |
| [blink.cmp との連携](frontends/neovim-blink.md) | `blink = true`、`enter` preset の上書き、内部 API 依存 |
| [fcitx5](frontends/fcitx5.md) | C++ / `rinze-client` 構成、打ち方・表示、rinzed の起動と応答待ち、左文脈、診断 |
| [macOS](frontends/macos.md) | Swift / InputMethodKit / `rinze-client` 構成、打ち方・表示、左文脈、設定と診断 |

## [構成と内部仕様](architecture/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [クレート構成](architecture/crates.md) | 各クレートの役割、フロントエンドと接続の分離 |
| [通信](architecture/protocol.md) | msgpack-rpc の 6 種類の要求、応答形式、入れ子状態とスコアキャッシュ |
| [性能と資源](architecture/performance.md) | 点数づけスレッド (`OMP_WAIT_POLICY`)、rinzed の常駐メモリ、fcitx5 の資源 |

## [開発とテスト](development/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [開発環境](development/setup.md) | `nix develop`、基本コマンド、既定ソケットの一致テスト、`RUSTFLAGS`、環境変数 |
| [検査](development/lint.md) | `tests/lint.sh` による Lua / Rust のフォーマット・リンタ・複雑性検査 |
| [テストと評価](development/testing.md) | モデルが要るテスト、日常の文章での 1 位正解率 (`corpus.sh`)、fcitx5 のテスト、macOS のテスト |
