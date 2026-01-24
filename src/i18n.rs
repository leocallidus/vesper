use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

impl Default for Language {
    fn default() -> Self {
        Language::Auto
    }
}

pub fn resolve_language(language: Language) -> Language {
    match language {
        Language::Auto => system_language(),
        other => other,
    }
}

pub fn system_language() -> Language {
    for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            let value = value.to_ascii_lowercase();
            let code = value.split('.').next().unwrap_or(&value);
            let code = code.split('_').next().unwrap_or(code);
            if code.starts_with("ru") {
                return Language::Russian;
            }
            if code.starts_with("en") {
                return Language::English;
            }
        }
    }
    Language::English
}

pub fn language_label(ui_language: Language, option: Language) -> &'static str {
    let ui_language = resolve_language(ui_language);
    match ui_language {
        Language::Russian => match option {
            Language::Auto => "Автоматически",
            Language::Russian => "Русский",
            Language::English => "Английский",
        },
        Language::English => match option {
            Language::Auto => "Automatic",
            Language::Russian => "Russian",
            Language::English => "English",
        },
        Language::Auto => "Automatic",
    }
}

pub fn language_index(language: Language) -> u32 {
    match language {
        Language::Auto => 0,
        Language::Russian => 1,
        Language::English => 2,
    }
}

pub fn language_from_index(index: u32) -> Language {
    match index {
        1 => Language::Russian,
        2 => Language::English,
        _ => Language::Auto,
    }
}

pub fn yes_no(language: Language, value: bool) -> &'static str {
    if value {
        tr(language, "Да")
    } else {
        tr(language, "Нет")
    }
}

pub fn profile_name(language: Language, index: usize) -> String {
    format!("{} {}", tr(language, "Профиль"), index)
}

pub fn cli_usage(language: Language) -> &'static str {
    match resolve_language(language) {
        Language::Russian => {
            "Управление Vesper:\n\n  vesper <команда> [аргументы]\n\nКоманды:\n  start                     Запустить скринсейвер\n  stop                      Остановить скринсейвер\n  show-settings             Открыть настройки\n  show                      Показать главное окно\n  enable | disable           Включить/выключить автозапуск\n  inhibit | uninhibit        Включить/выключить блокировку сна\n  set-enabled <bool>         Явно задать включение (true/false)\n  set-inhibit <bool>         Явно задать блокировку сна (true/false)\n  switch-profile <index>     Переключить профиль (0-254)\n  quit                       Завершить приложение\n  status | --status          Краткий статус\n  help                       Показать помощь"
        }
        Language::English => {
            "Vesper control:\n\n  vesper <command> [args]\n\nCommands:\n  start                     Start screensaver\n  stop                      Stop screensaver\n  show-settings             Open settings\n  show                      Show main window\n  enable | disable           Enable/disable auto start\n  inhibit | uninhibit        Enable/disable sleep inhibit\n  set-enabled <bool>         Explicitly set enabled (true/false)\n  set-inhibit <bool>         Explicitly set sleep inhibit (true/false)\n  switch-profile <index>     Switch profile (0-254)\n  quit                       Quit application\n  status | --status          Short status\n  help                       Show help"
        }
        Language::Auto => "",
    }
}

pub fn tr(language: Language, key: &'static str) -> &'static str {
    if matches!(resolve_language(language), Language::Russian) {
        return key;
    }
    match key {
        "Настройки" => "Settings",
        "Программа работает в фоновом режиме" => "The app is running in the background",
        "Запустить сейчас" => "Start now",
        "Выход" => "Exit",
        "Укажите индекс профиля (0-254)." => "Specify a profile index (0-254).",
        "Не удалось выполнить D-Bus команду" => "Failed to execute D-Bus command",
        "Не удалось запустить сервис" => "Failed to start service",
        "Статус: не запущен" => "Status: not running",
        "Мониторы" => "Monitors",
        "Монитор" => "Monitor",
        "Мониторы не найдены" => "No monitors found",
        "Не удалось получить список мониторов" => "Failed to get monitor list",
        "По умолчанию (активный профиль)" => "Default (active profile)",
        "Назначьте профиль для каждого монитора (по умолчанию — активный профиль)." => {
            "Assign a profile for each monitor (default is the active profile)."
        }
        "Интерактивный веб" => "Interactive web",
        "Разрешить управление мышью (курсор будет видим)" => {
            "Allow mouse interaction (cursor will be visible)"
        }
        "Сначала выберите паттерн: Водная рябь" => "Select pattern first: Water ripples",
        "Да" => "Yes",
        "Нет" => "No",
        "Активен: {active} • Включен: {enabled} • Сон: {inhibit} • Профиль: {} ({}) • Режим: {}" => {
            "Active: {active} • Enabled: {enabled} • Sleep: {inhibit} • Profile: {} ({}) • Mode: {}"
        }
        "Неизвестное значение: {value} (ожидается true/false)." => {
            "Unknown value: {value} (expected true/false)."
        }
        "Ошибка: {message}" => "Error: {message}",
        "Не удалось отправить команду" => "Failed to send command",
        "Запустить скринсейвер" => "Start screensaver",
        "Остановить скринсейвер" => "Stop screensaver",
        "Цвет" => "Color",
        "Градиент" => "Gradient",
        "Паттерн" => "Pattern",
        "Паттерны" => "Patterns",
        "Веб-страница" => "Web page",
        "Видео по URL" => "Video by URL",
        "Изображение" => "Image",
        "Видео" => "Video",
        "Слайдшоу" => "Slideshow",
        "Python скрипт" => "Python script",
        "GLSL шейдер" => "GLSL shader",
        "Список: {count}" => "List: {count}",
        "Список: {list_count}" => "List: {list_count}",
        "Список: {}" => "List: {}",
        "Матрица" => "Matrix",
        "Звезды" => "Stars",
        "Звёзды" => "Stars",
        "Геометрия" => "Geometry",
        "Дым/Чернила" => "Smoke/Ink",
        "Водная рябь" => "Water ripples",
        "Матрица 2.0" => "Matrix Rain 2.0",
        "LCARS" => "LCARS",
        "Терминал" => "Terminal",
        "Фракталы" => "Fractals",
        "Реакция-диффузия" => "Reaction-Diffusion",
        "Фоновое изображение" => "Background image",
        "Скринсейвер не запущен: {message}" => "Screensaver not started: {message}",
        "список медиа пуст или недоступен" => "media list is empty or unavailable",
        "Файл не выбран" => "File not selected",
        "Файл не найден: {path}" => "File not found: {path}",
        "Путь не является файлом" => "Path is not a file",
        "Неподдерживаемый формат" => "Unsupported format",
        "не настроено" => "not configured",
        "Папка не выбрана" => "Folder not selected",
        "Папка" => "Folder",
        "Предпросмотр не поддерживается" => "Preview not supported",
        "Папка не найдена: {path}" => "Folder not found: {path}",
        "Путь не является папкой" => "Path is not a folder",
        "В папке нет изображений" => "Folder has no images",
        "В папке нет шейдера Image" => "Folder has no Image shader",
        "Предупреждение: {message}" => "Warning: {message}",
        "Обнаружены составные GLSL шейдеры. Если скринсейвер зависнет, используйте принудительное закрытие: {hotkey}" => {
            "Composite GLSL shaders detected. If the screensaver hangs, use force close: {hotkey}"
        }
        "URL не указан" => "URL not specified",
        "Неверный URL потока" => "Invalid stream URL",
        "Профили" => "Profiles",
        "Быстрое переключение между наборами настроек" => {
            "Quick switching between settings profiles"
        }
        "Активный профиль" => "Active profile",
        "Название профиля" => "Profile name",
        "Добавить профиль" => "Add profile",
        "Добавить" => "Add",
        "Общие" => "General",
        "Интервал неактивности" => "Idle interval",
        "Время бездействия в секундах" => "Idle time in seconds",
        "Задержка реакции на мышь" => "Mouse wake delay",
        "Миллисекунды" => "Milliseconds",
        "Плавное появление/исчезновение" => "Fade in/out",
        "Горячие клавиши" => "Hotkeys",
        "Работают при активном окне приложения. Backspace — очистить" => {
            "Work when the app window is focused. Backspace clears."
        }
        "Запуск скринсейвера" => "Start screensaver",
        "Нажмите комбинацию" => "Press shortcut",
        "Остановка скринсейвера" => "Stop screensaver",
        "Принудительное закрытие" => "Force close",
        "На случай зависания/ошибок GPU" => "In case of hangs/GPU errors",
        "Запустить" => "Start",
        "Блокировать сон" => "Block sleep",
        "Контент" => "Content",
        "Внешний вид" => "Appearance",
        "Режим" => "Mode",
        "Выбор цвета" => "Color selection",
        "Цвет 1" => "Color 1",
        "Цвет 2" => "Color 2",
        "URL потока" => "Stream URL",
        "Путь к файлу" => "File path",
        "Выберите изображение, видео или папку" => "Choose an image, video, or folder",
        "Выбрать..." => "Browse...",
        "Проверить шейдеры" => "Check shaders",
        "Показать найденные BufferA-D" => "Show detected BufferA-D",
        "Проверить" => "Check",
        "Проверка шейдеров" => "Shader check",
        "Информация" => "Info",
        "Нет данных" => "No data",
        "Интервал слайдшоу" => "Slideshow interval",
        "Секунды между сменой изображений" => "Seconds between image changes",
        "Без звука" => "Mute",
        "Громкость видео" => "Video volume",
        "Проценты" => "Percent",
        "Случайный выбор" => "Random selection",
        "Список медиа" => "Media list",
        "Использовать список медиафайлов вместо одного файла" => {
            "Use a media list instead of a single file"
        }
        "Включить случайный выбор" => "Enable random selection",
        "Плейлист" => "Playlist",
        "Очистить" => "Clear",
        "Предпросмотр" => "Preview",
        "Превью" => "Preview",
        "Сейчас играет" => "Now playing",
        "Обложка и название трека (MPRIS)" => "Album art and track info (MPRIS)",
        "Показывать «Сейчас играет»" => "Show “Now playing”",
        "RSS/Новости" => "RSS/News",
        "Бегущая строка из RSS лент" => "Scrolling ticker from RSS feeds",
        "Показывать RSS-строку" => "Show RSS ticker",
        "Скорость прокрутки" => "Scroll speed",
        "пикс/с" => "px/s",
        "Интервал обновления" => "Refresh interval",
        "Минуты" => "Minutes",
        "Добавить RSS ленту" => "Add RSS feed",
        "Нет RSS лент" => "No RSS feeds",
        "Системные показатели" => "System stats",
        "Графики CPU/RAM (retro-tech)" => "CPU/RAM graphs (retro-tech)",
        "Показывать System Stats" => "Show system stats",
        "CPU" => "CPU",
        "RAM" => "RAM",
        "Пауза предпросмотра" => "Pause preview",
        "Нет воспроизведения" => "Nothing is playing",
        "Путь скопирован" => "Path copied",
        "Питание" => "Power",
        "Громкость" => "Volume",
        "Система" => "System",
        "Блокировать спящий режим" => "Block sleep mode",
        "Предотвращает переход системы в сон" => "Prevents the system from sleeping",
        "Интеграция с настройками питания" => "Power settings integration",
        "KDE/GNOME: управление энергосбережением при скринсейвере" => {
            "KDE/GNOME: manage power saving while screensaver is active"
        }
        "Блокировать экран при активации" => "Lock screen on activation",
        "KDE/GNOME: интеграция с системным экраном блокировки" => {
            "KDE/GNOME: integrate with the system lock screen"
        }
        "Приостанавливать медиаплееры (MPRIS)" => "Pause media players (MPRIS)",
        "Останавливает воспроизведение при запуске скринсейвера" => {
            "Stops playback when the screensaver starts"
        }
        "Исключения приложений" => "Application exclusions",
        "Скринсейвер не запускается автоматически, если эти приложения запущены" => {
            "The screensaver will not auto-start if these apps are running"
        }
        "Добавить приложение" => "Add application",
        "Имя процесса или приложения" => "Process or application name",
        "Обновить список" => "Refresh list",
        "Обновить" => "Refresh",
        "Часы" => "Clock",
        "Отображаются поверх скринсейвера" => "Shown on top of the screensaver",
        "Показывать часы" => "Show clock",
        "Пользовательский" => "Custom",
        "24ч: %H:%M" => "24h: %H:%M",
        "24ч: %H:%M:%S" => "24h: %H:%M:%S",
        "12ч: %I:%M %p" => "12h: %I:%M %p",
        "Дата: %d.%m.%Y" => "Date: %d.%m.%Y",
        "Дата и время: %d.%m.%Y %H:%M" => "Date and time: %d.%m.%Y %H:%M",
        "День недели: %a %H:%M" => "Weekday: %a %H:%M",
        "Полная дата: %A, %d %B %Y" => "Full date: %A, %d %B %Y",
        "ISO: %F %T" => "ISO: %F %T",
        "24ч (без нуля): %-H:%M" => "24h (no zero): %-H:%M",
        "24ч с секундами и датой: %H:%M:%S  %d.%m" => "24h with seconds + date: %H:%M:%S  %d.%m",
        "12ч с секундами: %I:%M:%S %p" => "12h with seconds: %I:%M:%S %p",
        "День недели и дата: %a, %d.%m.%Y" => "Weekday + date: %a, %d.%m.%Y",
        "Полный день и дата: %A  %d.%m" => "Full weekday + date: %A  %d.%m",
        "ISO дата: %F" => "ISO date: %F",
        "ISO дата и время: %F %R" => "ISO date + time: %F %R",
        "Номер недели и день: Неделя %V, %a" => "Week number + day: Week %V, %a",
        "День года: День %j" => "Day of year: Day %j",
        "Время и часовой пояс: %H:%M %z" => "Time + timezone: %H:%M %z",
        "Формат" => "Format",
        "Строка формата" => "Format string",
        "Положение" => "Position",
        "Сверху слева" => "Top left",
        "Сверху по центру" => "Top center",
        "Сверху справа" => "Top right",
        "По центру слева" => "Center left",
        "По центру" => "Center",
        "По центру справа" => "Center right",
        "Снизу слева" => "Bottom left",
        "Снизу по центру" => "Bottom center",
        "Снизу справа" => "Bottom right",
        "Размер текста" => "Text size",
        "Часы в две строки" => "Two-line clock",
        "Время (формат)" => "Time format",
        "Дата (формат)" => "Date format",
        "Пункты" => "Points",
        "Перемещать часы" => "Move clock",
        "Перемещать виджет" => "Move widget",
        "Меняет положение по кругу" => "Cycles through positions",
        "Интервал перемещения" => "Move interval",
        "Секунды" => "Seconds",
        "Команды панелей" => "Panel commands",
        "Команды для скрытия/показа панелей при активации скринсейвера" => {
            "Commands to hide/show panels when the screensaver activates"
        }
        "Добавить из списка" => "Add from presets",
        "Добавить свою" => "Add custom",
        "Статус" => "Status",
        "Текущие настройки" => "Current settings",
        "Статистика" => "Statistics",
        "Общее время работы" => "Total runtime",
        "История активаций" => "Activation history",
        "Экспорт/импорт" => "Export/Import",
        "Сохранить настройки в JSON" => "Save settings to JSON",
        "Экспорт" => "Export",
        "Загрузить настройки из JSON" => "Load settings from JSON",
        "Импорт" => "Import",
        "Сбросить" => "Reset",
        "О программе" => "About",
        "Простой скринсейвер сделанный на Rust и GTK4" => {
            "A simple screensaver built with Rust and GTK4"
        }
        "Разработчик:" => "Developer:",
        "История очищена" => "History cleared",
        "Экспорт настроек" => "Export settings",
        "Отмена" => "Cancel",
        "Сохранить" => "Save",
        "Ошибка экспорта: {err}" => "Export failed: {err}",
        "Не удалось сохранить файл: {err}" => "Failed to save file: {err}",
        "Настройки экспортированы" => "Settings exported",
        "Импорт настроек" => "Import settings",
        "Импортировать" => "Import",
        "Не удалось прочитать файл: {err}" => "Failed to read file: {err}",
        "Ошибка формата JSON: {err}" => "JSON format error: {err}",
        "Настройки импортированы" => "Settings imported",
        "Сначала выберите режим" => "Select a mode first",
        "Выбрать файл" => "Choose file",
        "Открыть" => "Open",
        "Список доступен только для изображения или видео" => {
            "List is available only for image or video"
        }
        "Добавить медиафайлы" => "Add media files",
        "Некоторые файлы пропущены" => "Some files were skipped",
        "Плейлисты доступны только для изображений или видео" => {
            "Playlists are available only for images or videos"
        }
        "Импорт плейлиста" => "Import playlist",
        "Плейлист пуст" => "Playlist is empty",
        "Некоторые элементы плейлиста пропущены" => "Some playlist entries were skipped",
        "Достигнут лимит профилей" => "Profile limit reached",
        "Настройки сохранены" => "Settings saved",
        "Удалить" => "Delete",
        "Удалить профиль?" => "Delete profile?",
        "Профиль будет удалён без возможности восстановления." => {
            "The profile will be permanently deleted."
        }
        "Нельзя удалить последний профиль" => "Cannot delete the last profile",
        "Сбросить профиль?" => "Reset profile?",
        "Настройки текущего профиля будут сброшены к значениям по умолчанию." => {
            "Current profile settings will be reset to defaults."
        }
        "Сохранить изменения?" => "Save changes?",
        "Есть несохранённые изменения." => "There are unsaved changes.",
        "Не сохранять" => "Don't save",
        "Профиль" => "Profile",
        "Интервал: {slideshow_interval}с" => "Interval: {slideshow_interval}s",
        "Громкость: {volume}%" => "Volume: {volume}%",
        "Да ({clock_format}, {clock_position}, {clock_size}пт, {interval}с)" => {
            "Yes ({clock_format}, {clock_position}, {clock_size}pt, {interval}s)"
        }
        "Да ({clock_format}, {clock_position}, {clock_size}пт)" => {
            "Yes ({clock_format}, {clock_position}, {clock_size}pt)"
        }
        "Профиль: {profile_name} • Режим: {mode_text}{slideshow_suffix} • Таймер: {inactivity}с • Задержка мыши: {mouse_delay_ms}мс • Без звука: {mute}{volume_suffix} • Сон: {inhibit} • Интеграция питания: {power_integration} • Блокировка: {lock_screen} • Часы: {clock} • Fade: {fade} • ГК: {start_hotkey}/{stop_hotkey}/{panic_hotkey}" => {
            "Profile: {profile_name} • Mode: {mode_text}{slideshow_suffix} • Timer: {inactivity}s • Mouse delay: {mouse_delay_ms}ms • Mute: {mute}{volume_suffix} • Sleep: {inhibit} • Power integration: {power_integration} • Lock screen: {lock_screen} • Clock: {clock} • Fade: {fade} • Hotkeys: {start_hotkey}/{stop_hotkey}/{panic_hotkey}"
        }
        "Неизвестно" => "Unknown",
        "Некорректный формат" => "Invalid format",
        "Неподдерживаемый формат плейлиста" => "Unsupported playlist format",
        "Не удалось прочитать плейлист" => "Failed to read playlist",
        "Включено" => "Enabled",
        "Отключено" => "Disabled",
        "Все пресеты уже добавлены" => "All presets are already added",
        "Выберите среду" => "Choose environment",
        "Новая команда" => "New command",
        "Редактировать" => "Edit",
        "Название" => "Name",
        "Команда скрытия" => "Hide command",
        "Команда показа" => "Show command",
        "ОК" => "OK",
        "Формат не определён" => "Format not determined",
        "Файл не является корректным изображением" => "File is not a valid image",
        "Разрешение: {resolution_text} • Размер: {size_text}" => {
            "Resolution: {resolution_text} • Size: {size_text}"
        }
        "Файлов: {} • Размер: {}" => "Files: {} • Size: {}",
        "Б" => "B",
        "КБ" => "KB",
        "МБ" => "MB",
        "ГБ" => "GB",
        "ТБ" => "TB",
        "д" => "d",
        "ч" => "h",
        "м" => "m",
        "с" => "s",
        "Нет записей" => "No entries",
        "Последние {shown} запусков" => "Last {shown} launches",
        "Последние {shown} из {total}" => "Last {shown} of {total}",
        "Записей пока нет" => "No entries yet",
        "{path}\nНажмите, чтобы скопировать" => "{path}\nClick to copy",
        "Некорректный URL" => "Invalid URL",
        "Режим не поддерживается" => "Mode not supported",
        "Язык интерфейса" => "Interface language",
        "Автозапуск" => "Autostart",
        "Запускать при входе в систему" => "Start at login",
        "Создает запись в ~/.config/autostart" => "Creates an entry in ~/.config/autostart",
        "Запускать в фоне" => "Start in background",
        "Не показывать главное окно при запуске" => {
            "Do not show the main window on startup"
        }
        "Не удалось обновить автозапуск: {err}" => {
            "Failed to update autostart: {err}"
        }
        "Автоматически" => "Automatic",
        "Русский" => "Russian",
        "Английский" => "English",
        "Язык будет применён после перезапуска приложения" => {
            "Language will be applied after restarting the app"
        }
        _ => key,
    }
}
