# フロントエンド

各フロントエンドは `rinzed` との接続ごとに独立した入力状態を持ち、キー入力と左文脈を送って返ってきた表示指示を描画する薄いクライアントとして設計している。

| ページ | 内容 |
| --- | --- |
| [neovim.md](neovim.md) | Neovim プラグインの基本設定、キー制御の方針、ステータスライン API、デーモン管理 |
| [neovim-cmdline.md](neovim-cmdline.md) | Neovim のコマンドライン入力（`/` `?` `:`）の設計、フロート表示と左文脈の扱い |
| [neovim-blink.md](neovim-blink.md) | blink.cmp 連携の仕組み、`enter` preset との共存方法 |
| [fcitx5.md](fcitx5.md) | Linux 向け fcitx5 アドオンの構成、挙動、デーモン起動・タイムアウト設計、診断方法 |
| [macos.md](macos.md) | macOS 向け入力メソッドの構成、挙動、設定と診断方法 |
