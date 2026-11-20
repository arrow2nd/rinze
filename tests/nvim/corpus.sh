#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
rinzed=${RINZED:-$root/target/release/rinzed}
corpus=${CORPUS:-$root/crates/rinze-engine/tests/fixtures/corpus.tsv}
passes=${PASSES:-1}
work=$(mktemp -d "${TMPDIR:-/tmp}/rinze-corpus-XXXXXX")
socket=$work/r.sock
trap 'pkill -f -- "--socket $socket" || true; rm -rf "$work"' EXIT

case ${LEARNING:-on} in
  on) learning=(--history "$work/history.tsv") ;;
  off) learning=(--no-learning) ;;
  *)
    echo "LEARNING は on か off: $LEARNING" >&2
    exit 2
    ;;
esac

nvim --headless --clean -l "$root/tests/nvim/corpus.lua" "$rinzed" "$RINZE_MODEL" "$RINZE_DICT" "$socket" "$work/user.jisyo" "$corpus" "$passes" "${learning[@]}"
