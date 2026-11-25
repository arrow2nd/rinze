# blink.cmp との連携

[blink.cmp](https://github.com/saghen/blink.cmp) を利用している場合、`setup` に `blink = true` を指定すると、見出し入力中（▽）に変換候補を blink.cmp の補完メニューへ表示できる。

```lua
require('rinze').setup({ blink = true })
```

## 連携の仕組みと設計方針

- **非同期の先読みとキャッシュ活用**: 変換候補はキー入力用の接続とは別の接続から `lookup` で非同期に問い合わせる。キー処理自体は問い合わせの完了を待たないため打鍵感を損なわず、メニューに表示された読みをそのまま Space で変換した場合は `rinzed` 側の[スコアキャッシュ](../architecture/protocol.md#状態管理とキャッシュ)が効いて即座に候補が出る。
- **SKK の操作感を壊さない共存**: メニューには送り仮名のない見出しの辞書候補のみを zenz のスコア順で表示し（送り仮名入力中・abbrev・辞書登録中・生成候補は対象外）、他ソース（LSP や buffer 等）の候補は混ぜない。メニューが出ていても、選ばない限り従来の Space 変換や Enter 確定がそのまま使える。
- **確定と学習の統一**: メニューから候補を受け入れた場合も、Space で送って確定した場合と同様に扱い、先頭以外の候補であれば[学習](../usage/learning.md)履歴に記録する。プレビュー表示中に次の文字を打った場合も、SKK で候補表示中に次文字を打ったときと同じく確定してから次のキーを処理する。

## preset とキー割り当て

blink.cmp の `default` preset は rinze のキーと衝突しない。一方、`enter` preset ではメニュー表示中の Enter が先頭候補の受け入れになるため、SKK の「見出し入力中の Enter は読みのまま確定」という挙動と競合する。SKK 側の挙動を優先したい場合は、見出し入力中だけ `fallback` に回すよう設定する。

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

その他のキー（印字可能文字、Space、BackSpace、`C-g`、`C-j`、`C-q`、Esc）を blink.cmp に割り当てる場合も同様に見出し入力中を `fallback` に回す。なお、blink.cmp のキー処理との順序保証やプレビュー取り消しには blink.cmp の内部 API に依存する部分があるため、結合テスト（`tests/nvim/blink.sh`）で互換性を検証している。
