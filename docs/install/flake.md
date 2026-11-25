# flake で導入する

rinze は Nix flake として `rinzed`、zenz モデル、SKK 辞書、各フロントエンドのパッケージと home-manager モジュールを提供している。対応環境は `x86_64-linux`、`aarch64-linux`、`aarch64-darwin` である（バイナリキャッシュは用意していないため、初回はローカルでビルドが走る）。

## 試用する

```sh
nix run /path/to/rinze#demo
```

`--clean` の Neovim に rinze と blink.cmp を設定した状態で起動し、すぐに変換を試せる。ソケットや辞書・履歴などの状態はすべて一時ディレクトリ（`/tmp/rinze-demo-*`）に隔離され、終了時に自動削除されるため、既存環境には一切影響しない。

## flake の主な出力

| 出力 | 内容 |
| --- | --- |
| `packages.<system>.rinzed`（`default`） | モデルと辞書の既定値を組み込んだ `rinzed` ラッパー |
| `packages.<system>.rinzed-unwrapped` | 既定値を持たない `rinzed` 本体 |
| `packages.<system>.rinze-zenz-model` | 既定の zenz モデル（[モデルと既定の辞書](../usage/dictionaries.md#モデルと既定の辞書)） |
| `packages.<system>.rinze-dictionaries` | 既定の SKK 辞書群 |
| `packages.<system>.rinze-nvim` | Neovim プラグイン |
| `packages.<system>.fcitx5-rinze` | fcitx5 アドオン（Linux のみ） |
| `packages.<system>.rinze-macos` | macOS 入力メソッド（macOS のみ） |
| `packages.<system>.demo` / `apps.<system>.demo` | 試用環境を起動するデモ |
| `homeManagerModules.default` | `rinzed` の常駐サービス化および macOS 入力メソッド導入モジュール |
| `overlays.default` | `rinzed` / `rinzed-unwrapped` / `rinze-zenz-model` / `rinze-nvim` を `pkgs` に追加するオーバーレイ |
| `checks.<system>` | ビルド・静的解析・ソケットパス一致・各フロントエンドの自動検証 |

### 既定値の変更

既定の辞書やモデルをカスタマイズしたい場合は、`rinzed.override` で差し替えられる。`dictionaries` には辞書パッケージと単体の辞書ファイルを混在して指定でき、並べた順が[複数辞書の統合順](../usage/dictionaries.md#複数辞書の統合ルール)になる。

```nix
rinzed.override {
  dictionaries = rinzed.dictionaries ++ [ ./skk-jisyo.imas.utf8 ];
  # model = ...;
}
```

## rinzed を常駐させる

`rinzed` は一度起動すると自動終了しないため、各フロントエンドからオンデマンド起動する運用では、アップデート後も古いバージョンのプロセスが残り続けることがある。home-manager モジュールで常駐サービス化しておくと、ログイン時に起動し、環境更新時に新しいバージョンへ自動的に切り替わるため、すべてのフロントエンドでバージョンを揃えられる。

```nix
{ inputs, ... }:
{
  imports = [ inputs.rinze.homeManagerModules.default ];
  services.rinzed.enable = true;
  # services.rinzed.package = ...;  # 既定は flake の rinzed
}
```

- Linux では systemd ユーザーサービス、macOS では launchd エージェントとして登録される。
- `services.rinzed.package` は `home.packages` にも自動追加されるため、別途追加する必要はない。
- サービスが待ち受けている間、各フロントエンドは既存ソケットへ接続するだけで自前起動は行わない。
- 常駐サービスはログイン時からメモリを保持する（[常駐メモリの抑制](../architecture/performance.md#常駐メモリの抑制)）。起動エラーの確認時は端末から `rinzed --verbose` を直接実行してログを確認する。

## バージョン管理と固定

バージョン番号は `Cargo.toml` の `[workspace.package].version` で一元管理し、Rust クレート・Nix パッケージ・fcitx5 アドオン・macOS アプリの間で常に一致させる方針としている。リリース時には `vX.Y.Z` 形式の Git タグを付与する。

更新タイミングを固定したい場合は flake の URL でタグを指定する。

```nix
inputs.rinze.url = "github:arrow2nd/rinze/v0.1.0";
```

稼働中のバージョンは `rinzed --version` で確認できる。
