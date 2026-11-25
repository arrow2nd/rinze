# テストと評価

rinze のテストは「モデル不要で高速に回る単体テスト」「実モデルと実プロセスを使う結合テスト」「日常文コーパスによる変換精度評価」「フロントエンド固有の検証」の 4 層に分かれている。

## モデル不要の単体テスト

`cargo test --workspace` で実行する。`rinze-engine` の状態遷移、`rinze-dict` の辞書検索・更新、`rinze-socket` のパス解決や検証ロジックなど、モデルに依存しない部分を網羅的に検証する。

## 実モデルを使う結合テスト

事前に[モデルと辞書の環境変数](setup.md#モデルと辞書の環境変数)（`RINZE_MODEL`、`RINZE_DICT`）を設定して実行する。環境変数が未設定の場合、モデルが必要なテストは自動的にスキップされる。

```sh
cargo test --release -p rinzed   # 実モデルを用いたランキング検証（最適化の影響を避けるため release で実行）
cargo build --release -p rinzed
tests/nvim/run.sh                # headless Neovim と実 rinzed の結合テスト・セキュリティ検証
tests/nvim/blink.sh              # blink.cmp 連携テスト（RINZE_BLINK が必要）
```

結合テストはいずれも一時ディレクトリ上のソケット・ユーザー辞書・学習履歴を使うため、利用者の環境には影響しない。

## 日常文コーパスによる変換精度評価

`crates/rinze-engine/tests/fixtures/corpus.tsv` に、日常的な日本語の文章と SKK の打鍵列を対応付けた評価用コーパスを用意している。headless の Neovim から実際の `rinzed` に打鍵を送り、変換精度を定量的に確認できる。

```sh
cargo build --release -p rinzed
tests/nvim/corpus.sh             # RINZED / CORPUS / RINZE_DICT（: 区切り）で差し替え可能
PASSES=2 tests/nvim/corpus.sh    # 1 周目の学習を引き継いで 2 周目を評価（LEARNING=off で学習無効）
```

評価では以下の指標を集計する。

- **1 位正解率**: 最初の変換候補が期待する表記と一致した区間の割合
- **Space 回数の分布**: 期待する表記に到達するまでに要した候補送りの回数
- **未到達区間**: 生成候補まで送り切っても期待する表記が出ず、辞書登録に入った区間

なお、推論ライブラリのバージョンが変わるとスコアがわずかに変動するため、評価の再現性を保つ目的で `llama-cpp-2` のバージョンを固定している。

## フロントエンド別の検証

### fcitx5

```sh
nix build .#checks.x86_64-linux.fcitx5-rinze -L
```

fcitx5 の `testfrontend` を使い、画面なしでアドオンを読み込んで実 `rinzed` と通信させる結合テストを実行する。通常の打鍵やモード切り替えに加え、自動起動、応答遅延・タイムアウト時の復旧、起動失敗時のエラー表示、共有ライブラリのエクスポートシンボル検査まで自動化している。

### macOS

```sh
macos/test.sh
nix build .#checks.aarch64-darwin.rinze-macos -L
```

キー変換、左文脈の抽出範囲、直接入力アプリの設定管理など、InputMethodKit に依存しないロジックを単体テストで検証する。InputMethodKit を介した実際の打鍵やパネル・メニューバー表示は、ビルドしたアプリをインストールして手動で確認する。
