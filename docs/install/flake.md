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
| `packages.<system>.rinze-macos` | macOS の入力メソッド ([macOS](../frontends/macos.md))。`Library/Input Methods/rinze.app` を持つ。macOS だけ |
| `packages.<system>.demo`、`apps.<system>.demo` | 上の試用の入口 |
| `homeManagerModules.default` | rinzed をログイン時に起動し、macOS では入力メソッドも入れる home-manager のモジュール ([rinzed を常駐させる](#rinzed-を常駐させる)) |
| `overlays.default` | 上の `rinzed`、`rinzed-unwrapped`、`rinze-zenz-model`、`rinze-nvim` を pkgs に足す |
| `checks.<system>` | rinzed とプラグインのビルド、Lua と Rust の[検査](../development/lint.md)、既定のソケットのパスの一致 (Lua 側)、`setup` に何も渡さずに PATH の rinzed を起動して候補を引けること。Linux では fcitx5 のアドオンを testfrontend で打って確かめること ([fcitx5 のテスト](../development/testing.md#fcitx5-のテスト))。macOS では入力メソッドを組んで、キーの対応づけなどを確かめること ([macOS のテスト](../development/testing.md#macos-のテスト)) |

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

## rinzed を常駐させる

rinzed は一度起動すると自分からは終了しない。フロントエンドが起動した rinzed は rinze を更新したあとも動き続け、フロントエンドごとに別の版の rinzed を指していると、先に起動したものが使われる。
home-manager のモジュールで rinzed をサービスにすると、ログイン時に起動し、home-manager で更新したときに新しい版で起動し直すので、どのフロントエンドも同じ版の rinzed に繋がる。

```nix
# home-manager のモジュール
{ inputs, ... }:
{
  imports = [ inputs.rinze.homeManagerModules.default ];
  services.rinzed.enable = true;
  # services.rinzed.package = ...;  # 既定は flake の rinzed
}
```

- Linux では systemd のユーザーサービス (`rinzed.service`)、macOS では launchd のエージェント (`org.nix-community.home.rinzed`) になる。
- `services.rinzed.package` を `home.packages` にも入れるので、`home.packages` に rinzed を書く必要は無い。サービスが止まっているときにフロントエンドが PATH から起動する rinzed も同じ版になる。
- フロントエンドの設定は変えなくてよい。サービスの rinzed が待ち受けていれば、フロントエンドはそれに繋ぐだけで起動しない。
- Linux ではユーザー辞書と学習の履歴の場所を home-manager の `xdg.dataHome` と `xdg.stateHome` から引数で渡す。サービスの環境には利用者のシェルの `XDG_DATA_HOME` と `XDG_STATE_HOME` が入らないため。
- rinzed は変換のたびに読みと候補を標準エラー出力へ書くので、Linux では journal に残さず捨てる。起動に失敗したときの理由は、`rinzed` を端末から起動して確かめる。
- macOS では `services.rinzed.macos.enable` で入力メソッドも入れる ([macOS で入れる](macos.md))。
- ログインした時点から rinzed が常駐し、メモリを持ち続ける ([常駐メモリ](../architecture/performance.md#常駐メモリ))。
