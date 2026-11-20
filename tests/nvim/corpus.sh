#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
rinzed=${RINZED:-$root/target/release/rinzed}
corpus=${CORPUS:-$root/crates/rinze-engine/tests/fixtures/corpus.tsv}
work=$(mktemp -d "${TMPDIR:-/tmp}/rinze-corpus-XXXXXX")
socket=$work/r.sock
trap 'pkill -f -- "--socket $socket" || true; rm -rf "$work"' EXIT

nvim --headless --clean -l "$root/tests/nvim/corpus.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$corpus"
