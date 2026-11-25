# Neovim に入れる

設定オプションや動作仕様の詳細は [Neovim](../frontends/neovim.md) を参照。

## home-manager と lazy.nvim

Neovim の設定を Nix の外に置き、lazy.nvim でプラグインを管理している場合は、home-manager でプラグインを固定パスへ配置して lazy.nvim の `dir` から読み込む（lazy.nvim は起動時に runtimepath と packpath を再構築するため、`pack/*/start` 配置は使えない）。

```nix
# flake.nix の inputs
rinze.url = "git+file:///path/to/rinze";
```

```nix
# home-manager モジュール
{ inputs, pkgs, ... }:
let
  rinze = inputs.rinze.packages.${pkgs.stdenv.hostPlatform.system};
in
{
  home.packages = [ rinze.rinzed ];
  xdg.dataFile."nvim/rinze".source = rinze.rinze-nvim;
}
```

```lua
-- lazy.nvim のプラグイン定義
{
  dir = vim.fn.stdpath("data") .. "/rinze",
  name = "rinze",
  config = function()
    require("rinze").setup({ blink = true })
    vim.keymap.set("i", "<C-l>", function() require("rinze").toggle() end)
  end,
},
```

## overlay を使う（programs.neovim）

NixOS、nix-darwin、home-manager の `programs.neovim` を使う場合は、`overlays.default` を適用すると `pkgs.rinzed` と `pkgs.rinze-nvim` を直接参照できる。

```nix
nixpkgs.overlays = [ inputs.rinze.overlays.default ];
```

```nix
programs.neovim = {
  enable = true;
  extraPackages = [ pkgs.rinzed ];
  plugins = [ pkgs.rinze-nvim ];
  initLua = ''
    require('rinze').setup({})
    vim.keymap.set('i', '<C-l>', function() require('rinze').toggle() end)
  '';
};
```

`extraPackages` を指定すると Neovim 実行時の PATH にのみ `rinzed` が追加されるため、端末から直接 `rinzed` を使う予定がなければこれで十分である。

## nixvim

```nix
programs.nixvim = {
  extraPlugins = [ pkgs.rinze-nvim ];
  extraPackages = [ pkgs.rinzed ];
  extraConfigLua = ''
    require('rinze').setup({})
  '';
};
```
