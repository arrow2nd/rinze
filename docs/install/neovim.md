# Neovim に入れる

設定と使い方の詳細は [Neovim](../frontends/neovim.md) を参照。

## home-manager と lazy.nvim

Neovim の設定を Nix の外 (out-of-store symlink など) に置き、lazy.nvim でプラグインを管理している場合の入れ方。home-manager でプラグインを決まった場所に置き、lazy.nvim の `dir` で読む。
lazy.nvim は既定で runtimepath と packpath を作り直すので、`pack/*/start` に置いたプラグインは読まれない。

```nix
# flake.nix の inputs
rinze.url = "git+file:///path/to/rinze";
```

```nix
# home-manager のモジュール
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
-- lazy.nvim のプラグインの一覧
{
  dir = vim.fn.stdpath("data") .. "/rinze",
  name = "rinze",
  config = function()
    require("rinze").setup({ blink = true })
    vim.keymap.set("i", "<C-l>", function() require("rinze").toggle() end)
  end,
},
```

`inputs.rinze.inputs.nixpkgs.follows` で nixpkgs を揃えると、rinzed をビルドするコンパイラも変わる。[複数の辞書](../usage/dictionaries.md#複数の辞書)などの測定の数字は rinze の `flake.lock` の nixpkgs で出したもの。nixos-26.05 (`6dadaf43`) に揃えてビルドした rinzed でも、コーパスの全区間の結果は同じだった。

## overlay を使う (NixOS、nix-darwin、home-manager の programs.neovim)

```nix
nixpkgs.overlays = [ inputs.rinze.overlays.default ];
```

```nix
# home-manager
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

`extraPackages` は Neovim の PATH だけに rinzed を足す。端末から rinzed を使わないならこれで足りる。

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
