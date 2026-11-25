#!/bin/sh
# testrinze の場面ごとに、一時ディレクトリのソケット、ユーザー辞書、学習の履歴で rinzed を用意して走らせる。
# 使い方: RINZED=<モデルと辞書の既定値を持つ rinzed> run.sh <testrinze>
set -eu
testrinze=$1
rinzed=${RINZED:?RINZED に rinzed のパスを渡す}

# ソケットのパスには上限 (107 バイト) があるので短い場所に置く
dir=$(mktemp -d "${RINZE_TEST_DIR:-/tmp}/rzt-XXXXXX")
stop_rinzed() {
  for proc in /proc/[0-9]*; do
    if tr '\0' ' ' < "$proc/cmdline" 2>/dev/null | grep -q -- "--socket $dir/"; then
      kill "${proc#/proc/}" 2>/dev/null || true
    fi
  done
}
trap 'stop_rinzed; rm -rf "$dir"' EXIT

wait_for_socket() {
  for _ in $(seq 300); do
    [ -S "$1" ] && return 0
    sleep 0.1
  done
  echo "$1 で待ち受けが始まらない" >&2
  return 1
}

"$rinzed" --socket "$dir/t.sock" --user-dict "$dir/t-user.jisyo" --history "$dir/t-history.tsv" 2>"$dir/t-rinzed.log" &
typing_rinzed=$!
wait_for_socket "$dir/t.sock"

# 応答を待ち切るまで 10 秒かかる場面は、ほかの場面と並べて走らせる
RINZE_SOCKET=$dir/u.sock RINZE_RINZED=$dir/not-launched "$testrinze" unresponsive >"$dir/u.log" 2>&1 &
unresponsive=$!

echo "--- 打鍵"
echo preset > "$dir/t-direct-programs"
RINZE_SOCKET=$dir/t.sock RINZE_RINZED=$dir/not-launched RINZE_FCITX_DEBUG=1 \
  RINZE_TEST_HISTORY=$dir/t-history.tsv RINZE_TEST_RINZED_PID=$typing_rinzed RINZE_DIRECT_PROGRAMS=$dir/t-direct-programs \
  "$testrinze" typing

echo "--- 自動起動"
mkdir -p "$dir/a-state" "$dir/a-data"
XDG_STATE_HOME=$dir/a-state XDG_DATA_HOME=$dir/a-data RINZE_SOCKET=$dir/a.sock RINZE_RINZED=$rinzed \
  "$testrinze" autostart

echo "--- rinzed が PATH に無い"
mkdir "$dir/empty-bin"
PATH=$dir/empty-bin RINZE_SOCKET=$dir/m.sock "$testrinze" missing

echo "--- rinzed が辞書を読めずに終了する"
cat > "$dir/rinzed-without-dict" <<EOF
#!/bin/sh
exec "$rinzed" --dict /missing.jisyo "\$@"
EOF
chmod +x "$dir/rinzed-without-dict"
mkdir -p "$dir/f-state" "$dir/f-data"
XDG_STATE_HOME=$dir/f-state XDG_DATA_HOME=$dir/f-data RINZE_SOCKET=$dir/f.sock RINZE_RINZED=$dir/rinzed-without-dict \
  "$testrinze" failing

echo "--- rinzed が応答しない"
status=0
wait "$unresponsive" || status=$?
cat "$dir/u.log"
exit "$status"
