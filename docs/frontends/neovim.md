# Neovim

Neovim プラグイン（`nvim/`）を runtimepath に追加し、`setup` を呼んで利用する。ソケットに接続できない場合は `cmd`（既定は `{ 'rinzed' }`）で `rinzed` を自動起動する。flake で導入した `rinzed` が PATH にあれば、モデルや辞書の引数は不要である。

```lua
require('rinze').setup({
  -- flake の rinzed が PATH にあれば cmd は省略できる
  cmd = { 'rinzed', '--model', '/path/to/model.gguf', '--dict', '/path/to/SKK-JISYO.L' },
})
-- C-j は SKK の確定・ひらがな復帰に使うため、有効/無効の切り替えには別のキーを割り当てる
vim.keymap.set('i', '<C-l>', function() require('rinze').toggle() end)
vim.o.statusline = "%f %{v:lua.require'rinze'.status()}"
```

導入方法は [Neovim に入れる](../install/neovim.md)、コマンドラインでの入力は [Neovim のコマンドライン](neovim-cmdline.md)、補完メニューとの連携は [blink.cmp との連携](neovim-blink.md) を参照。

## 設計の方針

- **無効時のオーバーヘッドゼロ**: rinze が無効な間は autocmd もキーマッピングも設定せず、`rinzed` との接続も保持しない。有効化するまで通常の編集操作に一切介入しない設計である。
- **他プラグインとの共存**: キーマッピングは挿入モードの間だけ対象バッファへ `<nowait>` 付きでバッファローカルに設定し、挿入モードを抜けるか別バッファへ移動すると元のマッピングを復元する。グローバルマッピングは書き換えず、rinze が処理しなかったキー（未確定文字列がないときの Enter や BackSpace、Esc など）は元のマッピングや Neovim の既定動作へフォールバックする。
- **ASCII モードでの素通しと復帰**: ASCII モード中は `C-j` 以外のマッピングを外すため、rinze が無効なときと同様に Neovim 本体や他プラグインへキーがそのまま届く。有効化直後はひらがなモードで始まり、挿入モードを抜けたとき（`C-o` による一時的なノーマルモード遷移を除く）やウィンドウ・バッファを移動したとき、Neovim がフォーカスを失ったときは未確定文字列を確定したうえで ASCII モードへ戻る。
- **Neovim の編集モデルとの統合**: 確定文字列はキー入力として流し込むため、`.` による繰り返しや `u` によるアンドゥが自然に機能する。制御文字はエスケープして挿入し、意図しないコマンド実行を防ぐ。また、`C-o` による一時的なノーマルモード遷移では未確定状態を維持し、元の位置へ戻れば見出し入力を続けられる。

## ステータスラインと API

`:RinzeEnable` `:RinzeDisable` `:RinzeToggle` コマンド、または Lua API で有効・無効を切り替えられる。

- `require('rinze').status()`: ステータスライン用のモード表示文字列を返す（無効時は空文字列）。表示ラベルは `setup` の `mode_labels` で変更できる（既定: `かな` / `カナ` / `ｶﾅ` / `aA` / `全英`）。
- `require('rinze').mode()`: 現在のモード名（[通信設計](../architecture/protocol.md)の `mode` と同じ）を返す（無効時は nil）。

## ソケットと単一デーモンの共有

既定のソケットパスは Linux で `$XDG_RUNTIME_DIR/rinze.sock`（未設定時はキャッシュディレクトリ配下）、macOS で `~/Library/Caches/rinze/rinze.sock` を使用する。macOS で `$TMPDIR` を避けているのは、`nix develop` 等でプロセスごとに値が変わり、同じユーザー間でもソケットを共有できなくなるためである。

複数の Neovim インスタンスや他フロントエンドから同時に起動されても、`rinzed` はロックファイルによる排他制御で常に 1 プロセスのみが常駐する。また、プラグインは応答のみを読む片方向通信を採用しており、ソケット経由で Neovim API を呼ばれない安全設計となっている。
