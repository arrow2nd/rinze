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

echo "--- 打鍵"
RINZE_SOCKET=$dir/t.sock RINZE_RINZED=$dir/not-launched RINZE_FCITX_DEBUG=1 \
  RINZE_TEST_HISTORY=$dir/t-history.tsv RINZE_TEST_RINZED_PID=$typing_rinzed \
  "$testrinze" typing

echo "--- 自動起動"
mkdir -p "$dir/a-state" "$dir/a-data"
XDG_STATE_HOME=$dir/a-state XDG_DATA_HOME=$dir/a-data RINZE_SOCKET=$dir/a.sock RINZE_RINZED=$rinzed \
  "$testrinze" autostart

echo "--- 先読みあり"
RINZE_SOCKET=$dir/t.sock RINZE_RINZED=$dir/not-launched "$testrinze" prefetch
echo "--- 先読みなし"
RINZE_SOCKET=$dir/t.sock RINZE_RINZED=$dir/not-launched RINZE_FCITX_NO_PREFETCH=1 "$testrinze" prefetch

echo "--- rinzed が PATH に無い"
mkdir "$dir/empty-bin"
PATH=$dir/empty-bin RINZE_SOCKET=$dir/m.sock "$testrinze" missing

echo "--- rinzed がモデルを読めずに終了する"
cat > "$dir/rinzed-without-model" <<EOF
#!/bin/sh
exec "$rinzed" --model /missing.gguf "\$@"
EOF
chmod +x "$dir/rinzed-without-model"
mkdir -p "$dir/f-state" "$dir/f-data"
XDG_STATE_HOME=$dir/f-state XDG_DATA_HOME=$dir/f-data RINZE_SOCKET=$dir/f.sock RINZE_RINZED=$dir/rinzed-without-model \
  "$testrinze" failing

echo "--- rinzed が応答しない"
RINZE_SOCKET=$dir/u.sock RINZE_RINZED=$dir/not-launched "$testrinze" unresponsive
