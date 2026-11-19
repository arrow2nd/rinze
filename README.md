# rinze

SKK 方式の IME。打ち方は SKK と同じで、変換候補の順位を「直前に使った候補」ではなく、カーソルより前の文章 (左文脈) で決める。
候補は SKK 辞書とユーザー辞書から引き、ニューラルかな漢字変換モデル zenz で各候補の対数尤度を計算して並べ替える。
辞書の候補を送り切ると zenz が生成した表記を出し、それも送り切ると SKK と同じ辞書登録に入る。

今はひらがなモードと Neovim のフロントエンドだけがある。

## 構成

| crate | 役割 |
| --- | --- |
| `rinze-engine` | 入力の状態機械。ローマ字の解釈、▽ / ▼ の状態、候補の送り、辞書登録 (入れ子を含む) を持つ。入出力を持たず、候補の取得と登録した語の保存は `Converter` trait に任せる |
| `rinze-dict` | SKK 辞書の読み込みと見出しの検索、SKK 辞書のテキストへの語の追加 |
| `rinze-zenz` | zenz による候補の点数づけと、ビーム探索による表記の生成 |
| `rinzed` | 常駐プロセス。辞書、ユーザー辞書、zenz を 1 つずつ持ち、Unix ドメインソケットで msgpack-rpc を受ける。上の 3 つを繋ぐ唯一の場所で、ユーザー辞書を書くのも rinzed だけ |

フロントエンドは接続ごとに独立した入力状態を持つ。Neovim のプラグインは `nvim/` にある。

### 通信

msgpack-rpc の要求を 2 種類受け付ける。

- `key(key, left_context)`: `key` は 1 文字か `space` `enter` `backspace` `ctrl-g`。`left_context` はカーソルより前の文字列で、zenz にはその末尾 40 字を渡す。
- `flush()`: 入力途中のものを表示どおりに確定し、状態を空にする。

どちらも次の map を返す。

| キー | 内容 |
| --- | --- |
| `handled` | false ならフロントエンドは `commit` を挿入したあと、元のキーを通常どおり処理する |
| `commit` | 確定した文字列 |
| `preedit` | 未確定文字列 (`▽あし`、`▽あ*t`、`▼明日` など) |
| `candidates` | 候補一覧の表示中だけ `{items = [[選択キー, 候補], ...], page, pages}`、それ以外は nil。辞書登録中は入力欄の変換の一覧 |
| `registration` | 辞書登録中だけ、いちばん内側の入力欄 `{prompt, text, preedit}`、それ以外は nil。`prompt` (`[辞書登録] まよねーず: `、入れ子では `[[辞書登録]] まよ: `)、`text` (入力欄に確定した文字列)、`preedit` (入力欄の未確定文字列) をこの順に並べて表示する |

辞書登録中も `preedit` は元の位置の未確定文字列 (`▼まよねーず`) で、入れ子の状態はすべて rinzed が持つ。

## 打ち方

SKK と同じ。

- 大文字で見出しの入力を始める。送りがなの無い語は Space で変換する。
- 送りがなのある語は、送りがなの開始を大文字で打つと変換が始まる (`AEmasu` は「あ*え」、`ATte` は「あ*っ」)。
- 候補の表示中に Space で次の候補、`x` で前の候補。最初の 4 候補はその場に表示し、5 候補目からは一覧を出して `asdfjkl` で選ぶ。
- 候補の表示中に次の文字を打つか Enter で確定する。`C-g` で取り消す。
- 辞書の候補を送り切ると、zenz が生成した表記を続けて出す。辞書に見出しの無い読みでは生成した表記が最初の候補になる (`Mayone-zu` の Space で「マヨネーズ」)。
- 生成した表記も送り切ると辞書登録に入り、`[辞書登録] まよねーず: ` の入力欄で登録する語を打つ。入力欄でも変換でき、そこで辞書登録に入れば入れ子になる。Enter で登録して元の位置に確定し、空のまま Enter するか `C-g` で登録をやめて元の状態に戻る。送りありの語は送りがなを除いて打つ (打っても除く)。

### 生成する表記

zenz に幅 8 のビーム探索で表記を生成させ、次のものを捨てる。

- 最も尤もらしい表記から対数尤度が 6 以上低いもの。これより下は読みと無関係な語が増える。
- 送りありで、送りがなで終わらないもの。
- 読みに対応しないもの。表記の中のかなが読みの同じ位置のかなと一致し、かな以外の連なりが読みの 1 字以上に当たるかで調べる (「天気がよい」は「てんきがいい」に対応しない)。
- 読みと同じひらがなのもの、辞書の候補と重複するもの。

生成には 1 回 24〜110ms かかり、辞書の候補の並べ替え (12〜23ms) より遅いので、辞書の候補を送り切ったときに初めて計算する。辞書に見出しが無いときだけ変換の開始時に計算する。
`cargo run --release -p rinzed --example timing` で読みごとの所要時間と生成した候補を測れる。

### ユーザー辞書

登録した語は SKK 辞書と同じ書式の UTF-8 のテキストに書く。既定の場所は Linux で `$XDG_DATA_HOME/rinze/user.jisyo` (無ければ `~/.local/share/rinze/user.jisyo`)、macOS で `~/Library/Application Support/rinze/user.jisyo` で、`rinzed --user-dict` で変えられる。
書くのは rinzed だけで、一時ファイルに書いて fsync してから rename するので、途中で落ちても壊れない。ユーザー辞書の語はシステム辞書の候補と合わせて zenz で並べ替え、先頭には固定しない。

## 開発環境

NixOS / nix で `nix develop` に入ると、cargo、llama.cpp のビルドに要る cmake と libclang、Neovim が揃う。

```sh
nix develop
cargo test --workspace           # モデルの要らないテスト
nix build                        # result/bin/rinzed
```

x86_64 では flake が `RUSTFLAGS` で AVX2 などを明示する。指定しないと ggml の SIMD が無効になり、変換が約 5 倍遅くなる。

### モデルと辞書

どちらもリポジトリには入れない。開発時は次のように用意する。

- モデル: zenz-v3.2-small の GGUF (`ggml-model-Q5_K_M.gguf`) を Hugging Face から取得する。
- 辞書: skk-dev/dict の `SKK-JISYO.L`。EUC-JP のままでも UTF-8 に変換したものでも読める。

```sh
export RINZE_MODEL=/path/to/ggml-model-Q5_K_M.gguf
export RINZE_DICT=/path/to/SKK-JISYO.L
```

### モデルが要るテスト

```sh
# 並べ替えが試作 (rust-spike) と同じ順になることを 40 件で確かめる。点数が llama.cpp の最適化に依るので release で走らせる
cargo test --release -p rinzed

# headless の Neovim で rinzed を自動起動させ、実際の打鍵でバッファに入力した結果を確かめる。
# 一時ディレクトリのソケットとユーザー辞書を使い、利用者のユーザー辞書には触れない。RINZED で rinzed を差し替えられる
cargo build --release -p rinzed
tests/nvim/run.sh
RINZED=result/bin/rinzed tests/nvim/run.sh
```

`RINZE_MODEL` と `RINZE_DICT` が無ければ、モデルが要るテストは何もせずに通る。
llama.cpp の版が変わると点数が 0.1〜1 nat 動くので、`llama-cpp-2` は `=` で版を固定している。

## Neovim

`nvim/` を runtimepath に加え、起動コマンドを渡す。接続できなかったときにこのコマンドで rinzed を起動する。

```lua
require('rinze').setup({
  cmd = { 'rinzed', '--model', '/path/to/ggml-model-Q5_K_M.gguf', '--dict', '/path/to/SKK-JISYO.L' },
})
vim.keymap.set('i', '<C-j>', function() require('rinze').toggle() end)
```

`:RinzeEnable` `:RinzeDisable` `:RinzeToggle` でも切り替えられる。有効な間は挿入モードの印字可能文字と Space、Enter、BackSpace、`C-g`、Esc を rinze が受け取る。
ソケットは既定で Linux では `$XDG_RUNTIME_DIR/rinze.sock` (無ければ `/tmp/rinze-$USER.sock`)、macOS では `~/Library/Caches/rinze/rinze.sock` で、`setup` の `socket` で変えられる。
macOS で `$TMPDIR` を使わないのは、nix develop などで値が変わり、同じ利用者のプロセスどうしで一致しないことがあるため。
Unix ドメインソケットのパスには上限 (Linux で 107 バイト、macOS で 103 バイト) があり、超えると rinzed はモデルを読む前に原因を書いて終了し、プラグインはそれをエラーとして表示する。

Neovim を同時にいくつ開いても rinzed は 1 つだけ動く。rinzed はソケットごとのロックファイル (`rinze.sock.lock`) を生きている間ずっとロックし、ロックを取れなかったものは何もせずに 0 で終了する。プラグインは起動したものが 0 で終了した場合も、先に動いている rinzed が待ち受けを始めるまで待つ。
ロックは rinzed が落ちれば外れるので、ロックを取れた rinzed は残っているソケットを前の rinzed の残骸として消してから bind する。
