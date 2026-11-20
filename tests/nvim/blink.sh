#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
if [[ -z ${RINZE_BLINK:-} || -z ${RINZE_MODEL:-} || -z ${RINZE_DICT:-} ]]; then
  echo "RINZE_BLINK、RINZE_MODEL、RINZE_DICT のどれかが無いので飛ばす"
  exit 0
fi
rinzed=${RINZED:-$root/target/release/rinzed}
# nix develop の TMPDIR は長く、ソケットのパスの上限 (107 バイト) を超えうるので /tmp に置く
work=$(mktemp -d /tmp/rinze-blink-XXXXXX)
socket=$work/r.sock
trap 'pkill -f -- "--socket $socket" || true; rm -rf "$work"' EXIT

# blink.cmp は受け入れ履歴を stdpath('state') に書くので、Neovim の XDG の場所をすべて作業ディレクトリに向ける
for dir in config data state cache; do
  mkdir -p "$work/$dir"
done
export XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data XDG_STATE_HOME=$work/state XDG_CACHE_HOME=$work/cache

for preset in default enter-readme-override enter-unchanged; do
  RINZE_BLINK_PRESET=$preset nvim --headless --clean -l "$root/tests/nvim/blink.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$work/history.tsv" "$RINZE_BLINK"
done
nvim --headless --clean -l "$root/tests/nvim/blink_cmdline.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$work/history.tsv" "$RINZE_BLINK"
