# 導入方法

Nix flake を使って `rinzed`、zenz モデル、SKK 辞書、および各フロントエンドを導入する手順。flake が提供する `rinzed` にはモデルと辞書の既定パスが組み込まれているため、PATH に通せば各フロントエンドから引数なしで起動できる。

| ページ | 内容 |
| --- | --- |
| [flake.md](flake.md) | 試用（`nix run ...#demo`）、flake の出力一覧、辞書のカスタマイズ、`rinzed` の常駐設定 |
| [neovim.md](neovim.md) | home-manager + lazy.nvim、overlay（`programs.neovim`）、nixvim での導入手順 |
| [fcitx5.md](fcitx5.md) | NixOS での fcitx5 アドオン導入と `fcitx5-skk` との併用設定 |
| [macos.md](macos.md) | home-manager による macOS 入力メソッドの導入とローカルビルド手順 |
