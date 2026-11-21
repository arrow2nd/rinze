# クレート構成

| crate | 役割 |
| --- | --- |
| `rinze-engine` | 入力の状態機械。入力モード、ローマ字の解釈、▽ / ▼ の状態、abbrev、候補の送り、見出しの入力中の候補の選択、辞書登録 (入れ子を含む)、辞書から消すかの問いを持つ。入出力を持たず、候補の取得、登録した語の保存と削除、確定した候補の通知は `Converter` trait に任せる |
| `rinze-dict` | SKK 辞書の読み込みと見出しの検索、SKK 辞書のテキストへの語の追加と削除 (`skk-ignore-dic-word` での除外を含む) |
| `rinze-zenz` | zenz による候補の点数づけと、ビーム探索による表記の生成 |
| `rinzed` | 常駐プロセス。システム辞書 (複数を合わせたもの)、ユーザー辞書、学習の履歴、zenz を 1 つずつ持ち、Unix ドメインソケットで msgpack-rpc を受ける。上の 3 つを繋ぐ唯一の場所で、ユーザー辞書と学習の履歴を書くのも rinzed だけ |
| `rinze-socket` | 既定のソケットのパスの規則。rinzed と rinze-client が共有する |
| `rinze-client` | rinzed のクライアント。接続、応答の解釈、rinzed の自動起動、左文脈から未確定文字列を除く処理を持ち、C ABI で fcitx5 のアドオンから使う ([fcitx5](../frontends/fcitx5.md)) |

フロントエンドは接続ごとに独立した入力状態を持ち、入力モードもその一部として rinzed が持つ。フロントエンドはモードを判断せず、キーを送って応答を表示する。Neovim のプラグインは `nvim/`、fcitx5 のアドオンは `fcitx5/` にある。
入力状態を分けたい場所ごとに接続を分ける。Neovim のプラグインは、挿入モード、[コマンドライン](../frontends/neovim-cmdline.md)、[blink.cmp](../frontends/neovim-blink.md) の問い合わせにそれぞれ別の接続を使い、rinze を無効にすると閉じる。fcitx5 のアドオンは、入力欄 (fcitx5 の入力コンテキスト) ごとの接続を使う。
