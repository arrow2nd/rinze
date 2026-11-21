# Neovim

`nvim/` を runtimepath に加え (Nix では `rinze-nvim`、[Neovim に入れる](../install/neovim.md))、`setup` を呼ぶ。ソケットに接続できなかったときは `cmd` で rinzed を起動する。
`cmd` の既定は `{ 'rinzed' }` で、flake の `rinzed` が PATH にあればモデルと辞書の既定値で起動する。flake の外で入れた rinzed では、モデルと辞書を引数で渡す。

```lua
require('rinze').setup({
  -- flake の rinzed が PATH にあれば cmd は要らない
  cmd = { 'rinzed', '--model', '/path/to/ggml-model-Q5_K_M.gguf', '--dict', '/path/to/SKK-JISYO.L', '--dict', '/path/to/SKK-JISYO.jinmei' },
})
-- C-j は rinze の中で SKK の C-j (確定、ひらがなに戻る) に使うので、オンオフには別のキーを割り当てる
vim.keymap.set('i', '<C-l>', function() require('rinze').toggle() end)
vim.o.statusline = "%f %{v:lua.require'rinze'.status()}"
```

コマンドラインでの入力は [コマンドライン](neovim-cmdline.md)、blink.cmp との連携は [blink.cmp との連携](neovim-blink.md) を参照。

## 有効化とキー割り当て

`:RinzeEnable` `:RinzeDisable` `:RinzeToggle` でも切り替えられる。有効な間は挿入モードの印字可能文字と Space、Enter、BackSpace、`C-g`、`C-j`、`C-q`、Esc を rinze が受け取る。
無効な間は、rinze の autocmd もキーの割り当ても置かず、rinzed との接続も持たないので、打鍵やカーソルの移動に処理を足さない。`setup` で読み込むときは、`require` に約 1 ms、Lua のヒープに約 120 KB かかる (`nvim --clean` の `--startuptime` と `collectgarbage('count')`、5 回)。

- 割り当ては挿入モードの間だけ、そのバッファにバッファローカルの `<nowait>` で置き、挿入モードを抜けるか別のバッファへ移ると外して、置く前の割り当てを戻す。global な割り当て (mini.pairs など) は書き換えない。
- ほかのプラグインのバッファローカルの割り当て (nvim-ts-autotag の `>` など) より rinze が優先する。同じキーで始まる長い割り当て (llama.vim の挿入モードの `<Leader>llf` など) があっても、続きのキーを `timeoutlen` だけ待たない。
- ASCII モードの間は `C-j` 以外の割り当てを外すので、打鍵は rinze を経由せず、rinze が無効なときと同じに Neovim とほかのプラグインが処理する。
- rinze が処理しなかったキー (見出しの無いときの Enter、BackSpace、Space と、Esc) は、rinze が無いときに働く割り当て (mini.pairs の `<CR>` と `<BS>`、picker の `<CR>` と `<Esc>` など) に渡す。割り当てが無ければ Neovim がそのまま処理する。

有効にした直後はひらがなモードで始まる。挿入モードを抜けると入力途中のものを確定するが、モードはそのまま残り、次に挿入モードに入ったときも同じモードで続く。
`C-o` で一時的にノーマルモードに入ったとき (`C-o:` でコマンドラインに入ったときを含む) は確定せず、挿入モードに戻ると見出しの入力を続けられる。戻ったときにカーソルが動いていれば、元の位置に確定する。

確定した文字列は打鍵として Neovim に入力するので、`.` で繰り返せ、`u` は 1 回の挿入をまとめて戻す。制御文字は `C-v` を前に置いて文字として入れ、打鍵や Ex コマンドとして働かせない。
未確定文字列があるときにカーソルを動かすか、挿入モードのまま別のウィンドウへ移ると、未確定文字列を表示どおりに元の位置へ確定する。

`C-q` は端末のフロー制御 (XON) と同じコードだが、Neovim は端末を raw モードにしてフロー制御を切るので、Neovim の動いている間は rinze に届く (tmux の中で確かめた)。届かない場合は、端末エミュレータが `C-q` を自分で使っていないかを確かめる。

## ステータスラインと API

`require('rinze').status()` はステータスラインに出す文字列を返し、無効な間は空文字列を返す。表示は `setup` の `mode_labels` で変えられ、既定は次のとおり。`require('rinze').mode()` はモードの名前 (`hiragana` など、[通信](../architecture/protocol.md)の `mode` と同じ) を返し、無効な間は nil を返す。

| モード | 既定の表示 |
| --- | --- |
| `hiragana` | `かな` |
| `katakana` | `カナ` |
| `halfwidth-katakana` | `ｶﾅ` |
| `ascii` | `aA` |
| `fullwidth-latin` | `全英` |

## ソケットと単一プロセス

ソケットは既定で Linux では `$XDG_RUNTIME_DIR/rinze.sock` (無ければ `$XDG_CACHE_HOME/rinze/rinze.sock`、それも無ければ `~/.cache/rinze/rinze.sock`)、macOS では `~/Library/Caches/rinze/rinze.sock` で、`setup` の `socket` で変えられる。
macOS で `$TMPDIR` を使わないのは、nix develop などで値が変わり、同じ利用者のプロセスどうしで一致しないことがあるため。
Unix ドメインソケットのパスには上限 (Linux で 107 バイト、macOS で 103 バイト) があり、超えると rinzed はモデルを読む前に原因を書いて終了し、プラグインはそれをエラーとして表示する。

プラグインは接続前にソケットが自分の所有する Unix ドメインソケットであることを確かめ、`vim.uv` のパイプと `vim.mpack` で応答 (`kind == 1`) だけを読む片方向の通信を行う (`sockconnect` の `rpc = true` は使わず、接続先から Neovim API を呼ばせない)。応答の待ちは `request_timeout` (既定 10000 ms) で打ち切る。

Neovim を同時にいくつ開いても rinzed は 1 つだけ動く。rinzed はソケットのディレクトリを 0700、ロックファイル (`rinze.sock.lock`) とソケットを 0600 で作り、生きている間ずっとロックを保持する。ロックを取れなかったものは何もせずに 0 で終了し、プラグインは起動したものが 0 で終了した場合も、先に動いている rinzed が待ち受けを始めるまで待つ。
ロックは rinzed が落ちれば外れるので、ロックを取れた rinzed は自分の所有する残ったソケットを前の rinzed の残骸として消してから bind する。
