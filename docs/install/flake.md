# flake で入れる

flake から `rinzed`、zenz のモデル、SKK 辞書、Neovim プラグインを入れる。flake の `rinzed` はモデルと辞書の既定値を持つので、PATH に入れればプラグインは引数なしで起動できる。

## 試す

```sh
nix run /path/to/rinze#demo
```

`--clean` の Neovim に rinze と blink.cmp を設定して起動し、rinze を有効にした状態で挿入モードに入る。`Asita` で blink.cmp のメニューに候補が出て、Space で変換、`C-y` でメニューの候補を確定、`C-l` でオンオフを切り替える。
ソケット、ユーザー辞書、学習の履歴、Neovim と blink.cmp の状態は `/tmp/rinze-demo-*` に置き、終了すると rinzed を止めてディレクトリごと消す。利用者のユーザー辞書、履歴、既定のソケットには触れない。

## flake の出力

| 出力 | 中身 |
| --- | --- |
| `packages.<system>.rinzed` (`default`) | モデルと辞書の既定値を持つ rinzed。`rinzed-unwrapped` のラッパー |
| `packages.<system>.rinzed-unwrapped` | 既定値を持たない rinzed |
| `packages.<system>.rinze-zenz-model` | zenz-v3.2-small の GGUF ([モデルと既定の辞書](../usage/dictionaries.md#モデルと既定の辞書)) |
| `packages.<system>.rinze-dictionaries` | 既定の SKK 辞書を `share/skk` にまとめたもの |
| `packages.<system>.rinze-nvim` | Neovim プラグイン (`vimUtils.buildVimPlugin`) |
| `packages.<system>.fcitx5-rinze` | fcitx5 のアドオン ([fcitx5](../frontends/fcitx5.md))。Linux だけ |
| `packages.<system>.demo`、`apps.<system>.demo` | 上の試用の入口 |
| `overlays.default` | 上の `rinzed`、`rinzed-unwrapped`、`rinze-zenz-model`、`rinze-nvim` を pkgs に足す |
| `checks.<system>` | rinzed とプラグインのビルド、Lua と Rust の[検査](../development/lint.md)、既定のソケットのパスの一致 (Lua 側)、`setup` に何も渡さずに PATH の rinzed を起動して候補を引けること。Linux では fcitx5 のアドオンを testfrontend で打って確かめること ([fcitx5 のテスト](../development/testing.md#fcitx5-のテスト)) |

system は `x86_64-linux`、`aarch64-linux`、`aarch64-darwin`。バイナリキャッシュは無いので、rinzed は手元で llama.cpp ごとビルドする。

### 既定値の渡し方

`rinzed` は `rinzed-unwrapped` を `makeBinaryWrapper` で包み、環境変数 `RINZED_DEFAULT_MODEL` にモデルのパスを、`RINZED_DEFAULT_DICTS` に辞書のパスを `:` 区切りで入れる (`--set-default` なので、起動時の環境にあればそちらを使う)。
rinzed は `--model` を省くと `RINZED_DEFAULT_MODEL` を、`--dict` を 1 つも渡さないと `RINZED_DEFAULT_DICTS` を使う。`--dict` を 1 つでも渡すと既定の辞書は使わない。

既定の辞書を変えるには `override` で `dictionaries` を渡す。辞書のパッケージ (`share/skk` の下の辞書をすべて使う) と辞書のファイルを混ぜて並べられ、並べた順が[複数の辞書](../usage/dictionaries.md#複数の辞書)の順になる。

```nix
rinzed.override {
  dictionaries = rinzed.dictionaries ++ [ ./skk-jisyo.imas.utf8 ];
  # model = ...;
}
```
