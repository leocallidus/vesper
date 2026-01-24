# Vesper

[English](README.md) | [Русский](README_ru.md)

Десктопный скринсейвер для Linux, написанный на Rust и GTK4.

## Возможности

- Приложение в системном трее с быстрыми действиями, профилями и статусами
- Режимы: цвет, градиент, паттерн, веб‑страница, потоковое видео, изображение, видео, слайдшоу (папка)
- Список случайных медиа для изображений/видео
- Оверлей часов с настройкой формата, позиции, размера и движения
- Профили с быстрым переключением (в том числе из трея)
- Горячие клавиши запуска/остановки
- Автозапуск
- Интеграция питания (блокировка сна, блокировка экрана в KDE/GNOME)
- Импорт/экспорт настроек (JSON)
- История активаций
- Команды панелей (скрыть/показать панели при активации)
- Определение простоя X11 + Wayland (ext-idle-notify-v1, fallback через D-Bus GNOME)
- Интерфейс на русском и английском (автоопределение)

## Требования

- Rust toolchain (stable)
- GTK4 и libadwaita (dev пакеты)
- Опционально: WebKitGTK 6 для режима веб‑страниц (название пакета зависит от дистрибутива)
- Runtime: GStreamer плагины для воспроизведения видео, если их нет в системе

### Пакеты Linux (примеры)

**Ubuntu/Debian:**
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

**Fedora:**
```bash
sudo dnf install gtk4-devel libadwaita-devel
```

**Arch Linux:**
```bash
sudo pacman -S gtk4 libadwaita
```

## Сборка

```bash
cargo build --release
```

## Запуск

```bash
# Из исходников
cargo run --release

# Или напрямую
./target/release/vesper
```

## AppImage

Нужен установленный `appimagetool`.

```bash
bash scripts/build_appimage.sh
```

AppImage появится в `dist/vesper-<arch>.AppImage`.

## CLI управление (D-Bus)

```bash
./target/release/vesper status
./target/release/vesper start
./target/release/vesper stop
./target/release/vesper show-settings
./target/release/vesper show
./target/release/vesper enable
./target/release/vesper disable
./target/release/vesper inhibit
./target/release/vesper uninhibit
./target/release/vesper set-enabled true
./target/release/vesper set-inhibit false
./target/release/vesper switch-profile 2
./target/release/vesper quit
```

## Меню в трее

- Переключатели «Включено» и «Блокировать сон»
- Подменю профилей
- Настройки, Запустить, Выход

## Скриншоты

![Главное окно](screenshots/main.png)
![Настройки](screenshots/settings.png)
![Скринсейвер](screenshots/screensaver.png)

## Конфигурация

- Файл настроек: `~/.config/vesper/config.json`
- Профили и параметры режимов хранятся отдельно для каждого профиля

## Архитектура

- `src/main.rs`: вход в приложение, D-Bus, трей
- `src/config.rs`: модель настроек и сохранение
- `src/idle.rs`: определение простоя X11/Wayland/GNOME
- `src/ui/settings/*`: окно настроек
- `src/ui/saver.rs`: полноэкранное окно скринсейвера

## Примечания

- На Wayland требуется поддержка `ext-idle-notify-v1` в композиторе.
- В GNOME используется fallback через D-Bus (Mutter IdleMonitor).
- Если видео не воспроизводится, установите необходимые GStreamer плагины.

## Известные ограничения

- Работа трея зависит от поддержки вашего окружения рабочего стола.
- Для некоторых форматов видео нужны дополнительные плагины GStreamer.
- На Wayland не будет определения простоя, если композитор не поддерживает `ext-idle-notify-v1`.
- Режим веб‑страниц требует установленный WebKitGTK.

## Лицензия

MIT License
