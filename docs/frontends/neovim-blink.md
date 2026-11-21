# blink.cmp との連携

[blink.cmp](https://github.com/saghen/blink.cmp) を使っているなら、`setup` に `blink = true` を渡すと、見出しの入力中 (▽) にその読みの変換候補を blink.cmp のメニューに出す。blink.cmp が無いか、`blink` を渡さなければ何も変わらない。

```lua
require('rinze').setup({ blink = true })
```

- メニューの並びは Space で変換したときと同じ (zenz の点数と学習の加点) で、`Asita` と打てば `Asita` の Space で送る順に「明日」「朝」「晨」が出る。出すのは送りがなの無い見出しの辞書の候補だけで、送りがなの入力中、abbrev、辞書登録の入力欄では出さない。生成した表記も出さない。
- メニューで受け入れる (`default` preset なら `C-y`) と、その候補で見出しを確定する。Space で送って確定したのと同じに扱い、先頭以外の候補なら[学習](../usage/learning.md)の履歴に書く。
- `C-n` などで選ぶと、blink.cmp は候補をバッファに仮に入れる (`completion.list.selection.auto_insert`)。その状態で rinze のキーを打つと、SKK で候補の表示中に次の文字を打ったときと同じく、仮に入れた候補を確定してからそのキーを処理する。Enter はその確定だけを行い、改行しない。
- メニューに候補が見えていても、選ぶまで何も確定しない。Space での変換、5 候補目からの一覧、Enter での読みのままの確定はそのまま使える。メニューで並べた読みをそのまま Space で変換すると、並べ替えをやり直さない (`かんじ` の Space の中央値が、メニューを出さないときの 34〜36 ms から 0.12〜0.19 ms になった。`tests/nvim/blink.sh` で 5 回ずつ)。
- 候補はキーを送る接続とは別の接続で `lookup` で問い合わせ、キーの処理は問い合わせを待たない。問い合わせの途中に来た問い合わせは最新の 1 件だけ残す。`tests/nvim/blink.sh` では、Space を除く 1 打鍵の処理の中央値が連携ありで 0.06〜0.10 ms、なしで 0.02 ms (それぞれ 95 打鍵、`cargo build --release` のバイナリで 2 回と `nix build` のバイナリで 1 回) で、`Nihongonyuuryoku` を一気に打ち終えてから最後の読みのメニューが出るまでは 20〜22 ms だった。
- ソースは `rinze` という名前で rinze が blink.cmp に足すので、blink.cmp の設定の `sources` に書かなくてよい。メニューにはこのソースだけを出し、ほかのソース (buffer、LSP など) の候補は混ぜない。確定した文字列は打鍵としてバッファに入るので、そのあとは普段どおりほかのソースの補完が働く。

## preset とキー割り当て

`default` preset のキーは rinze のキーと重ならない。`enter` preset では、メニューの表示中の Enter が先頭の候補の受け入れになり、SKK の「▽ で Enter は読みのまま確定」と意味が変わる。読みのまま確定したい場合は、見出しの入力中だけ Enter を rinze に渡すよう上書きする。

```lua
require('blink.cmp').setup({
  keymap = {
    preset = 'enter',
    ['<CR>'] = {
      function(cmp)
        if require('rinze').preedit() == '' then
          return cmp.accept()
        end
      end,
      'fallback',
    },
  },
})
```

ほかに rinze が受け取るキー (印字可能文字、Space、BackSpace、`C-g`、`C-j`、`C-q`、Esc) を blink.cmp の `keymap` に割り当てる場合も、同じく見出しの入力中は `fallback` に回す。rinze のキーの割り当ては `<Cmd>` を右辺にしているので、blink.cmp の `fallback` から呼ばれても、続けて打ったキーと順序が入れ替わらない。

blink.cmp が割り当てたキーは、ほかのバッファローカルの割り当てと違って rinze が奪わず、blink.cmp が先に受け取る。blink.cmp の `fallback` は挿入モードに入ったときに無かったバッファローカルの割り当ての代わりに global な割り当てを探すので、rinze はそのキーだけ挿入モードの間 global に置き、抜けるときに置く前の割り当てを戻す。

## 内部 API への依存

blink.cmp は受け入れ (`C-y`) と選択 (`C-n`) を `vim.schedule` で後回しにするので、rinze はキーを処理する前に `vim.wait(0)` で後回しの処理を先に済ませ、受け入れの直後に間を空けずに打ったキーを受け入れのあとに処理する。仮に入れた候補を確かめて取り消すのに blink.cmp の内部の `blink.cmp.completion.list` (`preview_undo`、`get_selected_item`、`undo_preview`) を使っている。blink.cmp 1.10.2 で確かめ、`tests/nvim/blink.sh` が守る。
