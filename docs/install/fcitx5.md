# fcitx5 で入れる

fcitx5 アドオンの挙動や診断方法の詳細は [fcitx5](../frontends/fcitx5.md) を参照。

## NixOS への導入

flake の `fcitx5-rinze` を fcitx5 のアドオンとして追加する。アドオンは実行環境の fcitx5 本体と ABI を揃えてビルドする必要があるため、`inputs.rinze.inputs.nixpkgs.follows` でシステムの `nixpkgs` に合わせる。

```nix
# flake.nix の inputs
rinze = {
  url = "git+file:///path/to/rinze";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

```nix
# NixOS モジュール
{ inputs, pkgs, ... }:
{
  i18n.inputMethod = {
    enable = true;
    type = "fcitx5";
    fcitx5.addons = [ inputs.rinze.packages.${pkgs.stdenv.hostPlatform.system}.fcitx5-rinze ];
  };
}
```

あわせて `rinzed` を home-manager の `home.packages` や[常駐サービス](flake.md#rinzed-を常駐させる)として導入し、fcitx5 の環境から見つかるようにする。設定を反映して fcitx5 を再起動したら、`fcitx5-configtool`（または `~/.config/fcitx5/profile`）から入力メソッドに `rinze` を追加する。

## fcitx5-skk と併用する

`fcitx5-skk` と `rinze` は同じグループにも別グループにも配置できる。

- **同じグループに並べる場合**: `TriggerKeys`（既定 `Control+space`）は直接入力と直前に使った入力メソッドを切り替え、`skk` と `rinze` の切り替えには `EnumerateForwardKeys` やトレイメニューを使う。
- **グループを分ける場合**: `keyboard-us` + `skk` と `keyboard-us` + `rinze` に分け、`EnumerateGroupForwardKeys`（既定 `Super+space`）でグループごと切り替える。
