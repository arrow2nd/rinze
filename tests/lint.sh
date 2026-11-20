#!/usr/bin/env bash
# フォーマット、リンタ、不要なコード、複雑性を検査する。引数で lua か rust に絞れる
set -euo pipefail

cd "$(dirname "$0")/.."

lua() {
  stylua --check nvim tests nix
  luacheck nvim tests nix
}

rust() {
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo shear
}

case ${1:-all} in
  lua) lua ;;
  rust) rust ;;
  all)
    lua
    rust
    ;;
  *)
    echo "usage: $0 [lua|rust]" >&2
    exit 2
    ;;
esac
