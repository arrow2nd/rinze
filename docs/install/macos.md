# macOS で入れる

入力メソッドの振る舞いや診断の詳細は [macOS](../frontends/macos.md) を参照。macOS 14 以降の Apple silicon で使える。

## home-manager で入れる

[rinzed を常駐させる](flake.md#rinzed-を常駐させる)モジュールで `services.rinzed.macos.enable` を立てる。

```nix
# home-manager のモジュール
{ inputs, ... }:
{
  imports = [ inputs.rinze.homeManagerModules.default ];
  services.rinzed = {
    enable = true;
    macos.enable = true;
  };
}
```

反映すると、flake の `rinze-macos` を `~/Library/Input Methods/rinze.app` に複製し、署名し直して、入力ソースを macOS に登録する。

- macOS はシンボリックリンクの入力メソッドを読まないので、ストアのものを複製して置く。複製元が変わったときだけ入れ替え、動いている rinze を終了する。次の打鍵で macOS が新しいものを起動する。
- アプリが起動する rinzed は `services.rinzed.package` になる。辞書を `override` で変えた rinzed を渡せば、アプリもそれを使う。
- 署名は手元だけで通る ad-hoc のもので、Apple の公証は受けていない。

初めて入れたときは、システム設定の「キーボード」→「入力ソース」→「編集」で「日本語」と「英語」に並ぶ rinze の入力モードが入っていることを確かめ、メニューバーの入力メニューから rinze を選ぶ。一覧に出ないときはログインし直す。

## 手元で組んで入れる

Xcode の `swiftc` で組む。nix のパッケージを使わずにソースから試すときに使う。

```sh
nix build .#rinzed
RINZED=$PWD/result/bin/rinzed nix develop -c macos/build.sh
```

`macos/build/rinze.app` ができる。動いているアプリを上書きすると署名が合わなくなって起動できないので、前のものを消してから複製する。

```sh
trash ~/Library/Input\ Methods/rinze.app
cp -R macos/build/rinze.app ~/Library/Input\ Methods/
pkill -x rinze
~/Library/Input\ Methods/rinze.app/Contents/MacOS/rinze --register
```

`rinze --register` は入力ソースを macOS に登録して有効にし、すぐ終わる。`RINZED` を渡さずに組んだときは、[`defaults` の `rinzed`](../frontends/macos.md#rinzed-の起動と応答の待ち) で rinzed の場所を渡す。

## 外す

システム設定の入力ソースから rinze を外し、`services.rinzed.macos.enable` を消して反映したあと、`~/Library/Input Methods/rinze.app` と `~/Library/Input Methods/.rinze.app.source` を消す。モジュールは複製したアプリを消さない。
