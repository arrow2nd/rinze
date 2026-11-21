---
name: docs-maintenance
description: >-
  Use this skill when adding, updating, or reorganizing documentation in rinze
  (README.md and docs/), or whenever code changes affect specifications,
  keybindings, input modes, dictionaries, learning, RPC protocol, Nix flake
  outputs, frontends (Neovim / fcitx5), performance measurements, or tests.
---

# rinze ドキュメント整備スキル

`rinze` のドキュメント (`README.md` と `docs/`) を、索引構造と文体を保ちながら更新・分割・検証するための手順。

## ディレクトリ構成と索引の役割

ドキュメントは `docs/` 配下にカテゴリごとのディレクトリを切り、3 階層の索引で辿れる形を維持する。

```text
README.md                          # 概要、最小の試用コマンド、カテゴリ索引
docs/
├── README.md                      # 全カテゴリ・全ページの総索引
├── install/                       # 導入・Nix flake・環境別セットアップ
│   └── README.md                  # カテゴリ索引 + 各トピック .md
├── usage/                         # 打ち方・モード・ローマ字・辞書・学習
│   └── README.md
├── frontends/                     # Neovim / コマンドライン / blink.cmp / fcitx5
│   └── README.md
├── architecture/                  # クレート構成・msgpack-rpc 通信・性能と資源
│   └── README.md
└── development/                   # 開発環境・検査・テスト・コーパス評価
    └── README.md
```

- **ルート `README.md`**: 概要・試用 (`nix run ...#demo`)・カテゴリ表だけを残す。詳細な仕様・設定例・測定表は書かない。
- **`docs/README.md`**: 全カテゴリと全トピック `.md` を 1 行説明付きで列挙する総索引。
- **`docs/<category>/README.md`**: そのカテゴリ内の全トピック `.md` の索引。
- **`docs/<category>/<topic>.md`**: 単一トピックの詳細。

## 変更箇所と更新先ドキュメントの対応

コードを修正したら、次の表から影響するドキュメントを特定して同期する。

| 変更したコード / 話題 | 更新先ドキュメント |
| --- | --- |
| `flake.nix`, `nix/demo.*`, `nix/overlay.nix` | `docs/install/flake.md`, `docs/install/neovim.md`, `docs/development/setup.md` |
| `nix/fcitx5.nix`, NixOS / fcitx5 導入 | `docs/install/fcitx5.md` |
| `crates/rinze-engine/` (打ち方・abbrev・辞書登録・`X`) | `docs/usage/typing.md` |
| `crates/rinze-engine/src/romaji.rs`, `script.rs` (モード・ローマ字) | `docs/usage/modes.md` |
| `crates/rinze-dict/`, 既定辞書・複数辞書・`user.jisyo` | `docs/usage/dictionaries.md` |
| `crates/rinze-zenz/` (生成フィルタ・点数づけ性能) | `docs/usage/typing.md`, `docs/architecture/performance.md` |
| `crates/rinzed/` (学習・`history.tsv`・加点) | `docs/usage/learning.md` |
| `crates/rinzed/` (RPC 要求・応答・キャッシュ) | `docs/architecture/protocol.md` |
| `crates/rinzed/` (スレッド・常駐メモリ) | `docs/architecture/performance.md` |
| `crates/rinze-socket/`, クレート追加・役割変更 | `docs/architecture/crates.md`, `docs/development/setup.md` |
| `nvim/lua/rinze/init.lua`, `socket.lua` | `docs/frontends/neovim.md` |
| `nvim/lua/rinze/cmdline.lua` | `docs/frontends/neovim-cmdline.md` |
| `nvim/lua/rinze/blink.lua` | `docs/frontends/neovim-blink.md` |
| `fcitx5/`, `crates/rinze-client/` | `docs/frontends/fcitx5.md`, `docs/architecture/performance.md`, `docs/development/testing.md` |
| `tests/lint.sh`, `.stylua.toml`, `.luacheckrc`, `clippy.toml` | `docs/development/lint.md` |
| `tests/nvim/`, `corpus.tsv`, `fcitx5/test/` | `docs/development/testing.md` |

## 分割と配置のルール

1. **既存ページに収めるか、新ページに切るか**:
   - 既存トピックの仕様変更や小規模な追記は該当ファイルに書く。
   - 1 ファイルがおおむね 180 行を超える場合、または独立した機能・新しいフロントエンドを追加する場合は、同じカテゴリ下に `<topic>.md` を新設する。
   - 既存 5 カテゴリに収まらない大きな領域ができた場合のみ、新しいカテゴリディレクトリと `README.md` を追加する。
2. **重複を避ける**:
   - 同じ仕様や数値を複数ページに複製せず、正本 (single source of truth) を 1 ページに置いて他からは相対リンクで参照する。
3. **索引を必ず同期する**:
   - ページを追加・改名・削除したら、`docs/<category>/README.md`、`docs/README.md`、および必要ならルート `README.md` を同時に更新する。

## 文体・表記のルール

既存の `rinze` ドキュメントと揃えるため、次を守る。

- **文体**: 常体 (「〜する」「〜である」・簡潔な述語)。丁寧語 (「です・ます」) は使わない。
- **事実と実測**: 性能や正解率の数値は推測で書かず、測定条件 (回数・環境・辞書・モデル・版) と再現コマンドを併記する。未確認の項目は「確かめていない」と明記する。
- **相対リンク**: `docs/` 内どうしは相対パス (`[学習](../usage/learning.md)`、`[検査](../development/lint.md)`)、ルート `README.md` からは `docs/...` を使う。見出しアンカー (`#...`) を付ける場合は実在する見出しと一致させる。

## 作業手順

1. **影響範囲を特定する**:
   - 変更内容と上の対応表を突き合わせ、更新対象ファイルを選ぶ。
   - 関連するコードコメントがドキュメントを参照していないか確認する (`git grep -n 'docs/'`)。
2. **本文を更新する**:
   - 仕様変更・新規項目を正本ファイルへ反映する。分割が必要なら新ファイルを作り、元ページからリンクする。
3. **索引と参照を更新する**:
   - `docs/<category>/README.md` と `docs/README.md` (必要ならルート `README.md`) の表・リンクを更新する。
   - 移動・改名したページを指している他のドキュメントやコードコメントのリンクも直す。
4. **検証スクリプトを走らせる**:
   - [scripts/check_docs.sh](./scripts/check_docs.sh) をリポジトリルートから実行する。

```sh
bash .agents/skills/docs-maintenance/scripts/check_docs.sh
```

   - エラーが 0 件であることを確認する (リンク切れ、アンカー不一致、索引漏れ、コードコメントからの不正パス参照がすべて検出される)。
   - 行数警告が出た場合は、分割すべきか検討する。
