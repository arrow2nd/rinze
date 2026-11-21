# 入れ方

flake から `rinzed`、zenz のモデル、SKK 辞書、Neovim プラグイン、fcitx5 のアドオンを入れる。flake の `rinzed` はモデルと辞書の既定値を持つので、PATH に入れればフロントエンドは引数なしで起動できる。

| ページ | 内容 |
| --- | --- |
| [flake.md](flake.md) | 試用 (`nix run ...#demo`)、flake の出力一覧、モデルや辞書の既定値の変え方 (`override`) |
| [neovim.md](neovim.md) | home-manager と lazy.nvim、overlay (`programs.neovim`)、nixvim での導入手順 |
| [fcitx5.md](fcitx5.md) | NixOS での fcitx5 アドオンの導入、`fcitx5-skk` と並べて使う設定 |
