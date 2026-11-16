#!/usr/bin/env bash
# Собирает Snowdrift.app. Использование: ./build.sh [папка-для-результата]
# UNIVERSAL=1 ./build.sh   - сборка под arm64 и x86_64 сразу.
set -euo pipefail
cd "$(dirname "$0")"

OUT="${1:-build}"
APP="$OUT/Snowdrift.app"
VERSION="$(tr -d '[:space:]' < VERSION)"

if ! command -v swiftc >/dev/null 2>&1 || ! xcrun --find swiftc >/dev/null 2>&1; then
    echo "Не найден компилятор Swift. Поставь Command Line Tools: xcode-select --install" >&2
    exit 1
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
sed "s/__VERSION__/$VERSION/g" Resources/Info.plist > "$APP/Contents/Info.plist"

BIN="$APP/Contents/MacOS/Snowdrift"
if [ "${UNIVERSAL:-0}" = "1" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    for arch in arm64 x86_64; do
        swiftc -O -target "$arch-apple-macos13.0" Sources/main.swift -o "$TMP/Snowdrift-$arch"
    done
    lipo -create "$TMP/Snowdrift-arm64" "$TMP/Snowdrift-x86_64" -output "$BIN"
else
    swiftc -O -target "$(uname -m)-apple-macos13.0" Sources/main.swift -o "$BIN"
fi

codesign --force --sign - "$APP" >/dev/null 2>&1 || true
echo "$APP"
