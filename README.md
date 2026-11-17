# Snowdrift ❄️

Снегопад поверх всех окон на macOS, Windows и Linux. Снег ложится на верхние кромки окон, сползает вслед за окном, тает, разлетается от курсора. Когда сугробы внизу вырастают, выезжает снегоуборщик. Клики проходят сквозь, иконки в Dock нет, управление из меню-бара (macOS), системного трея (Windows) или командной строки (Linux).

Snowfall for macOS, Windows and Linux: snow piles up on top of your windows, slides off when you move them, melts, flies apart under the cursor, and a snowplow clears the pile at the bottom of the screen. Click-through, menu bar or tray only.

## Установка

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash
```

Приложение ставится в `~/Applications/Snowdrift.app` и сразу запускается. Другая папка: `SNOWDRIFT_DIR=/Applications`.
Удалить: `curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash -s -- --uninstall`.

Нужна macOS 13 или новее, Apple Silicon или Intel. Можно и вручную: скачать `Snowdrift.zip` из [Releases](https://github.com/LerkOFF/snowdrift/releases/latest), распаковать и открыть.

### Windows

В PowerShell:

```powershell
irm https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.ps1 | iex
```

Скрипт скачивает `Snowdrift.exe`, копирует его в `%LOCALAPPDATA%\Programs\Snowdrift`, добавляет ярлык в меню «Пуск» и запись в «Установку и удаление программ» и запускает. Права администратора не нужны.
Удалить: «Параметры, Приложения» или `& "$env:LOCALAPPDATA\Programs\Snowdrift\Snowdrift.exe" --uninstall`.

Нужна Windows 10 или новее, x64. Можно и вручную: скачать `Snowdrift.exe` из [Releases](https://github.com/LerkOFF/snowdrift/releases/latest) и запустить с ключом `--install`.

### Linux

Одной командой (x86_64 и aarch64, под себя в `~/.local`, права root не нужны):

```bash
curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash
```

Или пакетом из [Releases](https://github.com/LerkOFF/snowdrift/releases/latest): `snowdrift_<версия>_amd64.deb` и `_arm64.deb` (Debian, Ubuntu, Mint), `snowdrift-<версия>-1.x86_64.rpm` и `.aarch64.rpm` (Fedora, openSUSE), либо `Snowdrift-linux-<arch>.tar.gz` с готовым бинарником (собран статически, запускается на любом дистрибутиве). Для Arch есть `linux/packaging/PKGBUILD` (на Arch не проверялся).

Управление: программа ищется в меню приложений как «Snowdrift», действия (снегоуборщик, сдуть снег, выйти) есть в контекстном меню её ярлыка, а из терминала:

```bash
snowdrift start            # запустить в фоне (snowdrift run: в этом терминале)
snowdrift stop             # выйти
snowdrift plow             # вызвать снегоуборщик
snowdrift clear            # сдуть снег
snowdrift set intensity 70 # 0-100; ещё opacity, ground, melt, windows, fullscreen, panels, mode snow|autumn|rain|blossom, mixed
snowdrift autostart on     # запускать при входе
```

Настройки лежат в `~/.config/snowdrift/config`. Удалить: `snowdrift uninstall` или `install.sh --uninstall`.

Что нужно: сеанс X11 (Xorg) или Wayland с XWayland и **композитор** (в GNOME, KDE, Xfce, Cinnamon, MATE он уже есть; в i3, bspwm, dwm, qtile, openbox его нет, запустите `picom -b`). Без композитора программа скажет об этом и не запустится.
Тайловые менеджеры работают: рамки окон берутся у X-сервера, окна вплотную друг к другу снега на кромке не держат, зазоры между окнами (gaps) он использует. Панель внизу экрана (рабочая область) снег обходит, ложится на неё сверху.
Ограничения: в сеансе Wayland снег ложится только на окна приложений XWayland, окна родных Wayland-приложений сугробов не получают (Wayland не отдаёт положение чужих окон), а снег поверх них падает и накапливается на дне экрана. Значка в трее и Хэллоуина в Linux-версии пока нет.

## Что умеет

- Снег с глубиной и разной плотностью: от лёгкого до метели; режимы снега, осенних листьев, дождя, сакуры и Хеллоуина.
- Сугробы на верхней кромке каждого окна. Окно двигается, и снег сдвигается вместе с ним, рывок стряхивает часть, закрытое окно осыпается целиком.
- Снег на панелях внутри окон (по желанию, пункт «Снег на панелях» в настройках): диалоги, всплывающие окна и большие картинки, открытые поверх страницы, получают свой сугроб. Положение панелей берётся у специальных возможностей системы: на macOS нужно разрешение «Универсальный доступ», на Windows работает через UI Automation, на Linux через шину AT-SPI (`snowdrift set panels on`). Что приложение не показывает системе, снега не получит.
- Курсор разгребает сугроб и поднимает снежную пыль.
- Снегоуборщик: когда снега внизу много, едет вдоль нижнего края, сгребает его в вал и сбрасывает за край экрана. Вызвать можно из меню.
- Курсор над Dock или панелью задач: снег рядом разлетается в стороны.
- Таяние, прозрачность, скрытие от скриншотов, запуск при входе в систему.
- Круглое стартовое окно, которое при нажатии рассыпается снегом.
- Девять языков интерфейса по языку системы (macOS и Windows).

## Сборка из исходников

macOS (нужны Command Line Tools: `xcode-select --install`):

```bash
./build.sh                 # build/Snowdrift.app, arm64 и x86_64
open build/Snowdrift.app
```

Windows (Rust, из любой ОС, кросс-сборка через `cargo-xwin`):

```bash
cd windows
cargo xwin build --release --target x86_64-pc-windows-msvc --bin Snowdrift
```

Linux (Rust, нужен Docker, собирает статические бинарники и пакеты deb, rpm, tar.gz для x86_64 и aarch64):

```bash
./linux/build.sh           # dist/linux
```

Общая симуляция снега лежит в `core/` (Rust, `cargo test` в этой папке), версия в файле `VERSION`.

## Лицензия

Код под MIT, см. [LICENSE](LICENSE).
