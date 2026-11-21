# テストと評価

## モデルが要るテスト

事前に [モデルと辞書の環境変数](setup.md#モデルと辞書の環境変数) (`RINZE_MODEL`、`RINZE_DICT`) を設定する。

```sh
# 並べ替えが試作 (rust-spike) と同じ順になることを 40 件で確かめる。学習なしで並べる。点数が llama.cpp の最適化に依るので release で走らせる
cargo test --release -p rinzed

# headless の Neovim で rinzed を自動起動させ、実際の打鍵でバッファとコマンドラインに入力した結果を確かめる。
# 一時ディレクトリのソケット、ユーザー辞書、学習の履歴を使い、利用者のファイルには触れない。RINZED で rinzed を差し替えられる
cargo build --release -p rinzed
tests/nvim/run.sh
RINZED=result/bin/rinzed tests/nvim/run.sh

# blink.cmp との連携を、default preset と enter preset (上書きあり、なし) で確かめる。
# コマンドラインで rinze がかなのモードの間は blink.cmp のコマンドラインのメニューが出ず、ASCII モードの間は出ることも確かめる。RINZE_BLINK も要る
tests/nvim/blink.sh
```

`RINZE_MODEL` と `RINZE_DICT` (blink.sh では `RINZE_BLINK` も) が無ければ、モデルが要るテストは何もせずに通る。

## 日常の文章での 1 位正解率

`crates/rinze-engine/tests/fixtures/corpus.tsv` に、日常の文章を SKK の打鍵で書いたコーパスがある。1 行が 1 文で、期待する文と打鍵列をタブで区切り、打鍵列の `{表記}` はその直前までの打鍵で変換が始まり、表記が出るまで候補を送ることを表す。

```sh
cargo build --release -p rinzed
tests/nvim/corpus.sh             # RINZED で rinzed を、CORPUS でコーパスを差し替えられる。RINZE_DICT は : 区切りで複数の辞書を渡せる
PASSES=2 tests/nvim/corpus.sh    # 1 周目で学習した状態で 2 周目を打つ。LEARNING=off で学習を止める
```

headless の Neovim で実際の rinzed に打鍵を送り、変換する区間ごとに、期待する表記がその場か一覧に出るまで Space を送る。区間ごとに `文の番号 期待する表記 Space の回数 最初の候補` を出し、最後に次を出す。期待する文と違うバッファになった文があれば 1 で終了する。

- 1 位正解率: 最初の候補が期待する表記だった区間の割合。
- Space の回数の分布: 期待する表記が出るまでに送った Space の回数ごとの区間の数。4 回目から一覧で、一覧に出た回数で数える。
- 届かなかった区間: 生成した表記まで送り切っても期待する表記が出ず、辞書登録に入った区間。

数字は SKK 辞書、ユーザー辞書と学習の履歴 (どちらも空の一時ファイルから始める)、zenz のモデルと llama.cpp の版、左文脈 (Neovim の行のカーソルより前) で決まる。同じ版で走らせれば同じ数字になる。
周ごとに区間の結果と集計を出し、2 周目以降はそれまでの周で選び直した候補を覚えた状態で打つ。
状態機械だけで確かめられること (打鍵列が期待する文になること、変換の始まる位置) は、変換が常に期待する表記を返すものとして `cargo test -p rinze-engine` が同じコーパスで確かめる。
llama.cpp の版が変わると点数が 0.1〜1 nat 動くので、`llama-cpp-2` は `=` で版を固定している。

## fcitx5 のテスト

```sh
nix build .#checks.x86_64-linux.fcitx5-rinze -L
```

fcitx5 の testfrontend でアドオンを画面なしで読み込み、打鍵を送って、確定した文字列、未確定文字列、パネル、入力モードの表示を確かめる (`fcitx5/test/testrinze.cpp`)。
`fcitx5/test/run.sh` が場面ごとに一時ディレクトリのソケット、ユーザー辞書、学習の履歴で rinzed を用意し、利用者のファイルと既定のソケットには触れない。場面は次のとおりで、所要時間もログに出す。

- 打鍵: 変換と確定、送りあり、左文脈による順位、未確定文字列の混ざった surrounding text、候補一覧の選択とページ送り、辞書登録の入力欄、辞書から消すかの問い、5 つの入力モードとトレイのメニュー、ASCII モードで rinzed に送らないこと、Ctrl のキー、Escape、矢印、BackSpace、フォーカスが外れたときに捨てて学習に書かないこと、入力欄を閉じると接続を閉じること
- 自動起動: rinzed が動いていない状態で有効にしたとき、打鍵を待たせずにアプリへ渡し、待ち受けを始めると表示が戻ること
- 遅い rinzed (止めて 1.5 秒後に再開する): 1 秒を超えても待って変換すること。止めたままの rinzed: 10 秒で未確定文字列を残して諦め、ほかの入力欄はそのあと 1 秒で諦め、再開したあとの打鍵で遅れた応答の確定を失わないこと
- PATH に rinzed が無い場合、rinzed がモデルを読めずに終了する場合 (打鍵が無くても理由をパネルに出し、fcitx5 が入力メソッドの名前を出したあとも理由が戻ること)、応答しない rinzed (受け付けるだけのソケット) の場合 (最初の 10 秒のあとは打鍵ごとに 1 秒で諦めること)

あわせて、共有ライブラリが外に出すシンボルを確かめる (`fcitx5/test/exports.cmake`)。
手元で cmake でビルドする場合は、`cargo build --release -p rinze-client` のあとで `nix develop .#fcitx5-rinze` に入り、`-DRINZE_CLIENT_LIB=target/release/librinze_client.a -DRINZE_CLIENT_INCLUDE=crates/rinze-client/include` を渡して `fcitx5/` をビルドし、ビルドディレクトリで `RINZED=<rinzed のパス> ctest --output-on-failure` を走らせる。
