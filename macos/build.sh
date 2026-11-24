#!/bin/sh
# rinze.app を macos/build に作る。cargo と Xcode の swiftc が要る。
# RINZED に rinzed の絶対パスを渡すと、アプリが起動する rinzed として Info.plist に書く
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p rinze-client
app=macos/build/rinze.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp macos/Info.plist "$app/Contents/Info.plist"
cp -R macos/Resources/. "$app/Contents/Resources/"
if [ -n "${RINZED:-}" ]; then
  plutil -replace RinzedPath -string "$RINZED" "$app/Contents/Info.plist"
fi
# nix develop の中では DEVELOPER_DIR と SDKROOT が nix の SDK を指し、swiftc が見つからない
env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun swiftc -O -swift-version 5 \
  -target "$(uname -m)-apple-macosx14.0" \
  -import-objc-header crates/rinze-client/include/rinze_client.h \
  -framework Cocoa -framework InputMethodKit \
  -o "$app/Contents/MacOS/rinze" \
  macos/Sources/*.swift target/release/librinze_client.a
codesign --force --sign - "$app"
echo "$app"
