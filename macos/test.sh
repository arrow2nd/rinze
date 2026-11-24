#!/bin/sh
# キーの対応づけや入力モードの対応など、rinzed にも入力欄にも触れない部分を確かめる。
# SWIFTC に swiftc の呼び方を渡せる。既定は Xcode のもの
set -eu
cd "$(dirname "$0")/.."

out=${TMPDIR:-/tmp}/rinze-macos-test
${SWIFTC:-env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun swiftc} -swift-version 5 \
  -import-objc-header crates/rinze-client/include/rinze_client.h \
  -o "$out" \
  macos/Sources/Keys.swift macos/Sources/Modes.swift macos/Tests/main.swift
"$out" macos/Info.plist macos/Resources
