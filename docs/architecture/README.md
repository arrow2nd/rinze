# 構成と内部仕様

クレートの役割分担、msgpack-rpc の通信仕様、スレッドと常駐メモリ・資源の設計。

| ページ | 内容 |
| --- | --- |
| [crates.md](crates.md) | クレート一覧とフロントエンド・接続の役割分担 |
| [protocol.md](protocol.md) | msgpack-rpc の 6 種類の要求、応答フォーマット、キャッシュと切断時の挙動 |
| [performance.md](performance.md) | 点数づけのスレッド (`OMP_WAIT_POLICY`)、rinzed の常駐メモリ、fcitx5 アドオンの資源 |
