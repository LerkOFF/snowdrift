# Snowdrift ❄️

Снегопад поверх всех окон macOS. Снег ложится на верхние кромки окон, сползает при движении окна, а когда сугробы внизу вырастают, выезжает снегоуборщик. Клики проходят сквозь, иконки в Dock нет, управление из меню-бара.

Snowfall for macOS: snow piles up on top of your windows, slides off when you move them, and a snowplow clears the pile at the bottom of the screen. Click-through, menu bar only.

## Установка

Одной командой:

```bash
curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash
```

Приложение ставится в `~/Applications/Snowdrift.app` и сразу запускается. Другая папка: `SNOWDRIFT_DIR=/Applications`.
Удалить: `curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash -s -- --uninstall`.

Нужна macOS 13 или новее, Apple Silicon или Intel. Можно и вручную: скачать `Snowdrift.zip` из [Releases](https://github.com/LerkOFF/snowdrift/releases/latest), распаковать и открыть.

## Что умеет

- Снежинки с глубиной: дальние мелкие и медленные, ближние крупные, у каждой своё покачивание.
- Сугробы на верхней кромке каждого окна. Окно двигается, и снег сдвигается вместе с ним. Резкий рывок стряхивает часть снега, пропавшее окно осыпается целиком.
- Снегоуборщик: когда снега внизу много, едет вдоль нижнего края, сгребает его в вал и сбрасывает за край экрана.
- Курсор над Dock: под ним вырастает иконка, и снег рядом разлетается в стороны.
- Меню-бар: пауза, три плотности снега (лёгкий, обычный, метель), копить снег на окнах или нет, стряхнуть сугробы, вызвать снегоуборщик.

Интерфейс на русском или английском по языку системы.

## Сборка из исходников

Нужны Command Line Tools (`xcode-select --install`).

```bash
./build.sh                 # build/Snowdrift.app под архитектуру этого Mac
UNIVERSAL=1 ./build.sh     # arm64 и x86_64 сразу
open build/Snowdrift.app
```

Весь код в `Sources/main.swift`, без зависимостей. `Snowdrift --version` печатает версию.

## Лицензия

MIT, см. [LICENSE](LICENSE).
