#!/usr/bin/env bash
# Установка Snowdrift одной командой:
#   curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash
# Удаление:
#   curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash -s -- --uninstall
# macOS: папка установки по умолчанию ~/Applications, меняется переменной SNOWDRIFT_DIR.
# Linux: программа ставится в ~/.local/bin, ярлык и значок в ~/.local/share.
set -euo pipefail

REPO="LerkOFF/snowdrift"

if [ "$(uname -s)" = "Linux" ]; then
    case "$(uname -m)" in
        x86_64 | amd64) ARCH=x86_64 ;;
        aarch64 | arm64) ARCH=aarch64 ;;
        *) echo "Для Linux есть сборки только под x86_64 и aarch64, а тут $(uname -m)." >&2; exit 1 ;;
    esac
    BIN="$HOME/.local/bin/snowdrift"
    if [ "${1:-}" = "--uninstall" ]; then
        [ -x "$BIN" ] && "$BIN" uninstall || true
        echo "Snowdrift удалён."
        exit 0
    fi
    [ -x "$BIN" ] && "$BIN" stop 2>/dev/null || true
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    echo "Скачиваю последний релиз Snowdrift для Linux ($ARCH)..."
    curl -fsSL "https://github.com/$REPO/releases/latest/download/Snowdrift-linux-$ARCH.tar.gz" -o "$TMP/snowdrift.tar.gz"
    tar -xzf "$TMP/snowdrift.tar.gz" -C "$TMP"
    "$TMP"/snowdrift-*/snowdrift install
    "$BIN" start || true
    echo "Команды: snowdrift stop, plow, clear, set intensity 70, autostart on. Подробнее: snowdrift --help"
    case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Добавьте ~/.local/bin в PATH, чтобы команда snowdrift находилась." ;; esac
    exit 0
fi

DEST="${SNOWDRIFT_DIR:-$HOME/Applications}"
APP="$DEST/Snowdrift.app"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Snowdrift работает на macOS, Windows и Linux." >&2
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
