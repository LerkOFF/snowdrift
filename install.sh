#!/usr/bin/env bash
# Установка Snowdrift одной командой:
#   curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash
# Удаление:
#   curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash -s -- --uninstall
# Папка установки по умолчанию ~/Applications, меняется переменной SNOWDRIFT_DIR.
set -euo pipefail

REPO="LerkOFF/snowdrift"
DEST="${SNOWDRIFT_DIR:-$HOME/Applications}"
APP="$DEST/Snowdrift.app"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Snowdrift работает только на macOS." >&2
    exit 1
fi

pkill -x Snowdrift 2>/dev/null || true

if [ "${1:-}" = "--uninstall" ]; then
    rm -rf "$APP"
    echo "Snowdrift удалён: $APP"
    exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Скачиваю последний релиз Snowdrift..."
curl -fsSL "https://github.com/$REPO/releases/latest/download/Snowdrift.zip" -o "$TMP/Snowdrift.zip"

mkdir -p "$DEST"
rm -rf "$APP"
ditto -x -k "$TMP/Snowdrift.zip" "$DEST"
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

open "$APP"
echo "Готово: $APP"
echo "Снежинка появилась в меню-баре, там же настройки."
