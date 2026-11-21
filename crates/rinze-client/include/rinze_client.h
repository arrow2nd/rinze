#ifndef RINZE_CLIENT_H
#define RINZE_CLIENT_H

/* cbindgen で src/ffi.rs から作る。手で書き換えず、RINZE_WRITE_HEADER=1 cargo test -p rinze-client header で作り直す */

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef enum {
  RINZE_STATUS_READY,
  RINZE_STATUS_STARTING,
  RINZE_STATUS_UNAVAILABLE,
} RinzeStatus;

typedef enum {
  RINZE_MODE_HIRAGANA,
  RINZE_MODE_KATAKANA,
  RINZE_MODE_HALFWIDTH_KATAKANA,
  RINZE_MODE_ASCII,
  RINZE_MODE_FULLWIDTH_LATIN,
} RinzeMode;

typedef enum {
  RINZE_FAILURE_NONE,
  // rinzed を起動中で、要求を処理していない。`rinze_service_poll` で待ち受けを始めたかを確かめる。
  RINZE_FAILURE_STARTING,
  RINZE_FAILURE_UNAVAILABLE,
  // 時間内に応答が無かったが、要求は rinzed に届いていて入力状態も残っている。表示はそのままにし、打鍵はアプリへ通さない。
  // 応答は次の要求の前に読む。そのときも応答が無ければ、入力状態を捨てて Lost を返す。
  RINZE_FAILURE_DELAYED,
  // 接続が切れたか応答を待ち切れず、rinzed 側の入力状態は失われた。
  RINZE_FAILURE_LOST,
} RinzeFailure;

typedef enum {
  // surrounding text を渡さなかったので、空の左文脈を送った。
  RINZE_LEFT_CONTEXT_UNKNOWN,
  RINZE_LEFT_CONTEXT_SURROUNDING,
  // surrounding text に入っていた rinze の未確定文字列を除いた。
  RINZE_LEFT_CONTEXT_PREEDIT_REMOVED,
} RinzeLeftContext;

typedef struct RinzeClient RinzeClient;

typedef struct RinzeResponse RinzeResponse;

typedef struct RinzeService RinzeService;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// `socket` が NULL なら既定のソケット。`program` は接続できなかったときに `--socket` を付けて起動する rinzed で、NULL なら起動しない。
// 既定のソケットを決められなければ NULL を返す。
RinzeService *rinze_service_new(const char *socket,
                                const char *program);

void rinze_service_free(RinzeService *service);

// rinzed の起動を待っている間に呼び、待ち受けを始めたか、起動に失敗したかを返す。待たずに戻る。
// `message` には UNAVAILABLE のときの理由を入れる。
RinzeStatus rinze_service_poll(RinzeService *service,
                               const char **message);

// rinzed の入力状態 1 つ (1 接続) に当たる。入力欄ごとに作る。接続は最初の要求のときに張る。
RinzeClient *rinze_client_new(const RinzeService *service);

void rinze_client_free(RinzeClient *client);

// `key` は rinzed のキーの名前 (1 文字か `space` `enter` `backspace` `ctrl-g` `ctrl-j` `ctrl-q`)。
// `surrounding` は入力先のカーソルの前後の文字列、`cursor` はその中のカーソルの位置 (文字数)。取れなければ `surrounding` に NULL を渡す。
// 失敗すると NULL を返し、理由は `rinze_client_failure` で読む。
RinzeResponse *rinze_client_key(RinzeClient *client,
                                const char *key,
                                const char *surrounding,
                                size_t cursor);

// 入力途中のものを確定せずに捨てる。入力モードは変えない。
RinzeResponse *rinze_client_discard(RinzeClient *client);

// 入力途中のものを表示どおりに確定する。入力モードは変えない。
RinzeResponse *rinze_client_flush(RinzeClient *client);

// 入力途中のものを確定してから、切り替えのキーを送って `mode` にする。
RinzeResponse *rinze_client_switch_mode(RinzeClient *client,
                                        RinzeMode mode);

// 直前の要求が失敗した理由。`message` には利用者に見せる文を入れる。
RinzeFailure rinze_client_failure(const RinzeClient *client, const char **message);

// 直前の `rinze_client_key` で送った左文脈と、その取り方。診断に使う。
RinzeLeftContext rinze_client_left_context(const RinzeClient *client, const char **text);

void rinze_response_free(RinzeResponse *response);

// false なら、`commit` を確定したあと元のキーをアプリにそのまま渡す。
bool rinze_response_handled(const RinzeResponse *response);

// true の間 (ASCII モード) は `ctrl-j` 以外のキーを rinzed に送らずアプリに渡してよい。
bool rinze_response_passthrough(const RinzeResponse *response);

// 未確定文字列、候補一覧、入力欄のどれも無い。
bool rinze_response_idle(const RinzeResponse *response);

const char *rinze_response_commit(const RinzeResponse *response);

const char *rinze_response_preedit(const RinzeResponse *response);

RinzeMode rinze_response_mode(const RinzeResponse *response);

// 候補一覧を表示していなければ 0。
size_t rinze_response_candidate_count(const RinzeResponse *response);

// 候補を選ぶキー。`rinze_client_key` にそのまま渡すと、その候補を確定する。範囲の外なら空文字列。
const char *rinze_response_candidate_label(const RinzeResponse *response,
                                           size_t index);

// 送りがなを含めた候補。範囲の外なら空文字列。
const char *rinze_response_candidate_word(const RinzeResponse *response, size_t index);

// 表示中のページ (0 から)。
size_t rinze_response_page(const RinzeResponse *response);

size_t rinze_response_pages(const RinzeResponse *response);

// 辞書登録の入力欄か、辞書から消すかの問いがあれば true を返し、`prompt` `text` `preedit` を入れる。この順に並べて表示する。
bool rinze_response_minibuffer(const RinzeResponse *response,
                               const char **prompt,
                               const char **text,
                               const char **preedit);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* RINZE_CLIENT_H */
