# rinze

SKK 方式の IME。打ち方は SKK と同じで、変換候補の順位を「直前に使った候補」ではなく、カーソルより前の文章 (左文脈) で決める。
候補は SKK 辞書から引き、ニューラルかな漢字変換モデル zenz で各候補の対数尤度を計算して並べ替える。

今はひらがなモードと Neovim のフロントエンドだけがある。

## 構成

| crate | 役割 |
| --- | --- |
| `rinze-engine` | 入力の状態機械。ローマ字の解釈、▽ / ▼ の状態、候補の送りを持つ。入出力を持たず、候補は `Converter` trait から受け取る |
| `rinze-dict` | SKK 辞書の読み込みと見出しの検索 |
| `rinze-zenz` | zenz による候補の点数づけ |
| `rinzed` | 常駐プロセス。辞書と zenz を 1 つずつ持ち、Unix ドメインソケットで msgpack-rpc を受ける。上の 3 つを繋ぐ唯一の場所 |

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
| `candidates` | 候補一覧の表示中だけ `{items = [[選択キー, 候補], ...], page, pages}`、それ以外は nil |

## 打ち方

SKK と同じ。

- 大文字で見出しの入力を始める。送りがなの無い語は Space で変換する。
- 送りがなのある語は、送りがなの開始を大文字で打つと変換が始まる (`AEmasu` は「あ*え」、`ATte` は「あ*っ」)。
- 候補の表示中に Space で次の候補、`x` で前の候補。最初の 4 候補はその場に表示し、5 候補目からは一覧を出して `asdfjkl` で選ぶ。
- 候補の表示中に次の文字を打つか Enter で確定する。`C-g` で取り消す。
- 送りなしの読みが辞書に無いときは、読みのカタカナ表記を候補にする (`Mayone-zu` の Space で「マヨネーズ」)。

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

# headless の Neovim で rinzed を自動起動させ、実際の打鍵でバッファに入力した結果を確かめる
cargo build --release -p rinzed
tests/nvim/run.sh
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
ソケットは既定で `$XDG_RUNTIME_DIR/rinze.sock` (無ければ `/tmp/rinze-$USER.sock`) で、`setup` の `socket` で変えられる。
