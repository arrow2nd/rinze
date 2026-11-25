---
name: docs-maintenance
description: >-
  Use this skill when adding, updating, or reorganizing documentation in rinze
  (README.md and docs/), or whenever code changes affect specifications,
  keybindings, input modes, dictionaries, learning, RPC protocol, Nix flake
  outputs, frontends (Neovim / fcitx5 / macOS), architecture, or development workflow.
---

# rinze ドキュメント整備スキル

`rinze` のドキュメント（`README.md` と `docs/`）を、索引構造と文体を保ちながら更新・分割・検証するための手順。

## 執筆の方針（何を書くか・何を書かないか）

- **コードが変わっても陳腐化しにくい内容を書く**: 設計思想、アーキテクチャ、責務分担、概念、利用者向けの導入方法・基本操作を中心にまとめる。
- **コードを読めばわかる細部は書かない**: 内部の関数名や定数、設定ファイル上の個別しきい値、テストケースの列挙、一過性のベンチマーク表や環境依存の実測値、エラー文言の全文引用などはドキュメントに持ち込まない。

## ディレクトリ構成と索引の役割

ドキュメントは `docs/` 配下にカテゴリごとのディレクトリを切り、3 階層の索引で辿れる形を維持する。

```text
README.md                          # 概要、最小の試用コマンド、カテゴリ索引
docs/
├── README.md                      # 全カテゴリ・全ページの総索引
├── install/                       # 導入・Nix flake・環境別セットアップ
│   └── README.md                  # カテゴリ索引 + 各トピック .md
├── usage/                         # 打ち方・モード・ローマ字・辞書・学習の設計
│   └── README.md
├── frontends/                     # Neovim / コマンドライン / blink.cmp / fcitx5 / macOS
│   └── README.md
├── architecture/                  # クレート構成・通信設計・性能とリソース方針
│   └── README.md
└── development/                   # 開発環境・静的解析・テストと評価
    └── README.md
```

- **ルート `README.md`**: 概要・試用（`nix run ...#demo`）・カテゴリ表だけを残す。詳細な仕様や設定例は書かない。
- **`docs/README.md`**: 全カテゴリと全トピック `.md` を 1 行説明付きで列挙する総索引。
- **`docs/<category>/README.md`**: そのカテゴリ内の全トピック `.md` の索引。
- **`docs/<category>/<topic>.md`**: 単一トピックの解説。

## 変更箇所と更新先ドキュメントの対応

設計や利用者向けの仕様が変わった場合は、次の表から影響するドキュメントを特定して更新する（実装詳細の変更のみであれば更新不要）。

| 変更した領域 | 更新先ドキュメント |
| --- | --- |
| `flake.nix`, `nix/demo.*`, `nix/overlay.nix` | `docs/install/flake.md`, `docs/install/neovim.md`, `docs/development/setup.md` |
| `nix/fcitx5.nix`, NixOS / fcitx5 導入 | `docs/install/fcitx5.md` |
| `nix/macos.nix`, `nix/home-manager.nix`, macOS 導入 | `docs/install/macos.md`, `docs/install/flake.md` |
| `crates/rinze-engine/`（打ち方・abbrev・辞書登録・`X`） | `docs/usage/typing.md` |
| `crates/rinze-engine/`（モード・ローマ字） | `docs/usage/modes.md` |
| `crates/rinze-dict/`, 辞書構成・`user.jisyo` | `docs/usage/dictionaries.md` |
| `crates/rinze-zenz/`（表記生成・推論方針） | `docs/usage/typing.md`, `docs/architecture/performance.md` |
| `crates/rinzed/`（学習方針・履歴保存） | `docs/usage/learning.md` |
| `crates/rinzed/`（RPC 要求・応答・キャッシュ・セキュリティ） | `docs/architecture/protocol.md` |
| `crates/rinzed/`（スレッド・メモリ・フォールバック方針） | `docs/architecture/performance.md` |
| `crates/rinze-socket/`, クレート追加・責務変更 | `docs/architecture/crates.md`, `docs/development/setup.md` |
| `nvim/lua/rinze/init.lua`, `socket.lua` | `docs/frontends/neovim.md` |
| `nvim/lua/rinze/cmdline.lua` | `docs/frontends/neovim-cmdline.md` |
| `nvim/lua/rinze/blink.lua` | `docs/frontends/neovim-blink.md` |
| `fcitx5/`, `crates/rinze-client/` | `docs/frontends/fcitx5.md`, `docs/architecture/performance.md`, `docs/development/testing.md` |
| `macos/` | `docs/frontends/macos.md`, `docs/install/macos.md`, `docs/development/testing.md` |
| `tests/lint.sh`, lint 構成 | `docs/development/lint.md` |
| `tests/`, `corpus.tsv`, フロントエンドテスト構成 | `docs/development/testing.md` |

## 分割と配置のルール

1. **既存ページに収めるか、新ページを切るか**:
   - 既存トピックの設計変更や追記は該当ファイルに反映する。
   - 1 ファイルがおおむね 180 行を超える場合、または独立した機能・新しいフロントエンドを追加する場合は、同じカテゴリ下に `<topic>.md` を新設する。
   - 既存 5 カテゴリに収まらない大きな領域ができた場合のみ、新しいカテゴリディレクトリと `README.md` を追加する。
2. **重複を避ける**:
   - 同じ説明を複数ページに重複して書かず、1 ページに集約して他ページからは相対リンクで参照する。
3. **索引を必ず同期する**:
   - ページを追加・改名・削除したら、`docs/<category>/README.md`、`docs/README.md`、および必要ならルート `README.md` を同時に更新する。

## 文体・表記のルール

- **文体**: 常体（「〜する」「〜である」・簡潔な述語）で統一する。丁寧語（「です・ます」）は使わない。
- **自然な日本語**: 英語の直訳調（例: 「正本」「写し」「〜の道具」「〜を立てる」の多用）を避け、技術文書として読みやすい自然な表現を使う。
- **相対リンク**: `docs/` 内どうしは相対パス（`[学習](../usage/learning.md)`）、ルート `README.md` からは `docs/...` を使う。見出しアンカー（`#...`）を付ける場合は実在する見出しと一致させる。

## 作業手順

1. **影響範囲を特定する**:
   - 変更内容と上の対応表を突き合わせ、更新対象ファイルを選ぶ。
   - コードコメントからの参照がないか確認する（`git grep -n 'docs/'`）。
2. **本文を更新する**:
   - 該当ページを更新する。分割が必要なら新ファイルを作り、元ページからリンクする。
3. **索引と参照を更新する**:
   - `docs/<category>/README.md` と `docs/README.md`（必要ならルート `README.md`）の表・リンクを更新する。
   - 移動・改名したページを指している他のドキュメントやコードコメントのリンクも直す。
4. **検証スクリプトを実行する**:
   - リポジトリルートから [scripts/check_docs.sh](./scripts/check_docs.sh) を実行し、エラーが 0 件であることを確認する。

```sh
bash .agents/skills/docs-maintenance/scripts/check_docs.sh
```
