# macOS で入れる

macOS 14 以降の Apple Silicon 環境に対応している。動作仕様や設定の詳細は [macOS](../frontends/macos.md) を参照。

## home-manager で導入する

[rinzed を常駐させる](flake.md#rinzed-を常駐させる)モジュールで `services.rinzed.macos.enable = true` を設定する。

```nix
{ inputs, ... }:
{
  imports = [ inputs.rinze.homeManagerModules.default ];
  services.rinzed = {
    enable = true;
    macos.enable = true;
  };
}
```

設定を反映すると、`rinze.app` が `~/Library/Input Methods/` へ複製・ad-hoc 署名され、macOS へ登録される。macOS の入力メソッドはシンボリックリンク経由では読み込まれないため、Nix ストアから実体をコピーし、更新があったときだけ差し替えてプロセスを再起動する仕組みにしている。アプリが利用する `rinzed` には `services.rinzed.package` が引き継がれる。

## 入力ソースを有効にする

初回導入時は、システム設定の「キーボード」→「入力ソース」→「編集」→「＋」から「日本語」配下の `rinze` を追加し、メニューバーの入力メニューから選択する（一覧に出ない場合は再ログインする）。macOS はユーザーがシステム設定から追加した入力ソースのみを永続化するため、コマンドからの有効化ではなく手動追加が必要となる。

## ローカルビルドして試す

Nix パッケージを使わずにソースから直接ビルドする場合は、Xcode の `swiftc` を使って組み立てる。

```sh
nix build .#rinzed
RINZED=$PWD/result/bin/rinzed nix develop -c macos/build.sh
```

`macos/build/rinze.app` が生成される。稼働中のアプリへ上書きコピーすると署名が壊れて起動できなくなるため、古いバンドルを削除してから配置し、プロセスを再起動して登録する。

```sh
trash ~/Library/Input\ Methods/rinze.app
cp -R macos/build/rinze.app ~/Library/Input\ Methods/
pkill -x rinze
~/Library/Input\ Methods/rinze.app/Contents/MacOS/rinze --register
```

登録後に[入力ソースを有効にする](#入力ソースを有効にする)。

## アンインストール

システム設定の入力ソースから `rinze` を削除し、`services.rinzed.macos.enable` を外して設定を反映したあと、`~/Library/Input Methods/` 配下の `rinze.app` と `.rinze.app.source` を手動で削除する。
