# fcitx5 で入れる

fcitx5 アドオンの振る舞いや診断の詳細は [fcitx5](../frontends/fcitx5.md) を参照。

## NixOS で入れる

flake の `fcitx5-rinze` を fcitx5 のアドオンに足す。アドオンは動かす fcitx5 と同じ版に対してビルドする必要があるので、`inputs.rinze.inputs.nixpkgs.follows` でシステムの nixpkgs に揃える。

```nix
# flake.nix の inputs
rinze = {
  url = "git+file:///path/to/rinze";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

```nix
# NixOS のモジュール
{ inputs, pkgs, ... }:
{
  i18n.inputMethod = {
    enable = true;
    type = "fcitx5";
    fcitx5.addons = [ inputs.rinze.packages.${pkgs.stdenv.hostPlatform.system}.fcitx5-rinze ];
  };
}
```

アドオンは PATH の `rinzed` を起動するので、flake の `rinzed` を home-manager の `home.packages` などで入れる。fcitx5 を systemd の user unit で動かしていて、unit の PATH に `/etc/profiles/per-user/$USER/bin` が入っていれば、home-manager で入れた rinzed が見つかる。

反映して fcitx5 を起動し直したら、fcitx5-configtool の入力メソッドの一覧 (または `~/.config/fcitx5/profile`) に rinze を足す。

## fcitx5-skk と並べて使う

fcitx5-skk と rinze は、同じグループにも別のグループにも置ける。

- 同じグループに `keyboard-us`、`skk`、`rinze` を並べた場合、`TriggerKeys` (既定は `Control+space`) は直接入力と、最後に使った入力メソッドを切り替える。skk と rinze の切り替えには `EnumerateForwardKeys` を割り当てるか、トレイのメニューを使う。
- グループを分けた場合 (`keyboard-us` と `skk` のグループ、`keyboard-us` と `rinze` のグループ)、`EnumerateGroupForwardKeys` (既定は `Super+space`) でグループを切り替える。
