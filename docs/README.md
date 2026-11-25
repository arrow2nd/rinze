# rinze ドキュメント索引

rinze の導入手順、操作と変換の仕組み、各フロントエンドの設計、アーキテクチャ、開発手順の索引。

## [導入方法](install/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [flake で導入する](install/flake.md) | 試用（`demo`）、flake の出力一覧、辞書のカスタマイズ、`rinzed` の常駐設定 |
| [Neovim に入れる](install/neovim.md) | home-manager + lazy.nvim、overlay（`programs.neovim`）、nixvim での導入手順 |
| [fcitx5 で入れる](install/fcitx5.md) | NixOS での fcitx5 アドオン導入と `fcitx5-skk` との併用設定 |
| [macOS で入れる](install/macos.md) | home-manager による macOS 入力メソッドの導入とローカルビルド手順 |

## [打ち方と変換の仕組み](usage/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [打ち方と変換の流れ](usage/typing.md) | SKK の基本操作、辞書登録・候補削除、abbrev、zenz による表記生成とフォールバック |
| [入力モードとローマ字](usage/modes.md) | 5 つの入力モードの切り替えと、ddskk とのローマ字規則の違い |
| [モデルと辞書](usage/dictionaries.md) | zenz モデルと既定の SKK 辞書、複数辞書の統合方針、ユーザー辞書の扱い |
| [学習の設計](usage/learning.md) | 学習機能の設計思想（対象・重み付け・保存の考え方） |

## [フロントエンド](frontends/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [Neovim](frontends/neovim.md) | 基本設定、キー制御の方針、ステータスライン API、デーモン共有 |
| [Neovim のコマンドライン](frontends/neovim-cmdline.md) | `/` `?` `:` での入力設計、フロート表示と左文脈の扱い |
| [blink.cmp との連携](frontends/neovim-blink.md) | 非同期先読みの仕組み、`enter` preset との共存方法 |
| [fcitx5](frontends/fcitx5.md) | C++ / `rinze-client` の責務分担、挙動、デーモン起動・応答待ち設計、診断方法 |
| [macOS](frontends/macos.md) | Swift / `rinze-client` の責務分担、挙動、設定と診断方法 |

## [アーキテクチャ](architecture/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [クレート構成と責務分担](architecture/crates.md) | ワークスペース構成と各クレート・フロントエンドの役割 |
| [通信設計](architecture/protocol.md) | msgpack-rpc による要求・応答の構造、状態管理とセキュリティ境界 |
| [性能とリソース管理の方針](architecture/performance.md) | 推論スレッドの設計、フォールバック戦略、メモリとプラットフォーム最適化 |

## [開発とテスト](development/README.md)

| ドキュメント | 内容 |
| --- | --- |
| [開発環境](development/setup.md) | `nix develop`、基本コマンド、ビルドの前提、モデル・辞書の環境変数 |
| [静的解析と品質検査](development/lint.md) | `tests/lint.sh` による Lua / Rust のフォーマット・静的解析・複雑度チェック |
| [テストと評価](development/testing.md) | テストの層、日常文コーパスによる精度評価、フロントエンド別の検証方法 |
