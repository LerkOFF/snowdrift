# Snowdrift ❄️

Снегопад поверх всех окон на macOS и Windows. Снег ложится на верхние кромки окон, сползает вслед за окном, тает, разлетается от курсора. Когда сугробы внизу вырастают, выезжает снегоуборщик. Клики проходят сквозь, иконки в Dock нет, управление из меню-бара (macOS) или системного трея (Windows).

Snowfall for macOS and Windows: snow piles up on top of your windows, slides off when you move them, melts, flies apart under the cursor, and a snowplow clears the pile at the bottom of the screen. Click-through, menu bar or tray only.

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

## Что умеет

- Снег с глубиной и разной плотностью: от лёгкого до метели; режимы снега, осенних листьев, дождя, сакуры и Хеллоуина.
- Сугробы на верхней кромке каждого окна. Окно двигается, и снег сдвигается вместе с ним, рывок стряхивает часть, закрытое окно осыпается целиком.
- Курсор разгребает сугроб и поднимает снежную пыль.
- Снегоуборщик: когда снега внизу много, едет вдоль нижнего края, сгребает его в вал и сбрасывает за край экрана. Вызвать можно из меню.
- Курсор над Dock или панелью задач: снег рядом разлетается в стороны.
- Таяние, прозрачность, скрытие от скриншотов, запуск при входе в систему.
- Круглое стартовое окно, которое при нажатии рассыпается снегом.
- Девять языков интерфейса по языку системы.

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

Общая симуляция снега лежит в `core/` (Rust, `cargo test` в этой папке), версия в файле `VERSION`.

## Лицензия

Код, добавленный в этом репозитории, под MIT, см. [LICENSE](LICENSE). Движок снегопада основан на [Snowhush](https://github.com/mbezruchko/Snowhush) Михаила Безручко (PolyForm Noncommercial 1.0.0) и используется с разрешения автора, подробности в [NOTICE](NOTICE).
