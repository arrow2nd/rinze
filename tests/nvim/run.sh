#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
rinzed=${RINZED:-$root/target/release/rinzed}
work=$(mktemp -d "${TMPDIR:-/tmp}/rinze-e2e-XXXXXX")
socket=$work/r.sock
trap 'pkill -f -- "--socket $socket" || true; rm -rf "$work"' EXIT

nvim --headless --clean -l "$root/tests/nvim/e2e.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$work/history.tsv"
nvim --headless --clean -l "$root/tests/nvim/keymap.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$work/history.tsv"
nvim --headless --clean -l "$root/tests/nvim/cmdline.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$work/history.tsv"
nvim --headless --clean -l "$root/tests/nvim/security.lua"
