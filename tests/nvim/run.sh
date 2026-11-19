#!/usr/bin/env bash
# headless の Neovim で打鍵を確かめる。RINZE_MODEL と RINZE_DICT が必要。
# rinzed は RINZED (既定は target/release/rinzed) を使い、専用のソケットで自動起動させて最後に止める。
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
rinzed=${RINZED:-$root/target/release/rinzed}
socket=$(mktemp -u "${TMPDIR:-/tmp}/rinze-e2e-XXXXXX.sock")
trap 'pkill -f -- "--socket $socket" || true' EXIT

nvim --headless --clean -l "$root/tests/nvim/e2e.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket"
