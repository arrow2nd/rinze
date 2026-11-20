#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
rinzed=${RINZED:-$root/target/release/rinzed}
rounds=${ROUNDS:-50}
history_lines=${HISTORY_LINES:-5000}
# 履歴の fsync を実際の書き込み先と同じファイルシステムで測るため、TMPDIR で置き場所を変えられる
work=$(mktemp -d "${TMPDIR:-/tmp}/rinze-timing-XXXXXX")
trap 'pkill -f -- "--socket $work/" || true; rm -rf "$work"' EXIT

nvim --headless --clean -l "$root/tests/nvim/commit_timing.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$work" "$rounds" "$history_lines"
