# フロントエンド

フロントエンドは接続ごとに独立した入力状態を持ち、キーを `rinzed` へ送って応答を表示する。現在、Neovim プラグイン、Linux の fcitx5 アドオン、macOS の入力メソッドがある。

| ページ | 内容 |
| --- | --- |
| [neovim.md](neovim.md) | Neovim プラグインの設定、キー割り当て、モードと状態、ステータスライン、ソケット |
| [neovim-cmdline.md](neovim-cmdline.md) | Neovim のコマンドライン (`/` `?` `:`) での入力、フロート表示、左文脈、blink.cmp との兼ね合い |
| [neovim-blink.md](neovim-blink.md) | blink.cmp との連携 (`blink = true`)、`enter` preset、内部 API への依存 |
| [fcitx5.md](fcitx5.md) | fcitx5 アドオンの構成、打ち方・表示、rinzed の起動と応答待ち、左文脈、診断 |
| [macos.md](macos.md) | macOS の入力メソッドの構成、打ち方・表示、左文脈、設定と診断 |
