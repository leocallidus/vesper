# Suggested Clock Format Presets (10)

These are `glib::DateTime::format()` / `strftime`-style patterns that would be useful as additional presets in the UI.

1. `24h (no leading zero): %-H:%M`
   - Why: cleaner look on large clocks; avoids `08:05` -> `8:05`.

2. `24h with seconds + date: %H:%M:%S  %d.%m`
   - Why: compact “today + precise time” for always-on displays.

3. `12h with seconds: %I:%M:%S %p`
   - Why: completes the 12h options; useful for US locale.

4. `Weekday + full date: %a, %d.%m.%Y`
   - Why: very common human-readable date header.

5. `Long weekday + short date: %A  %d.%m`
   - Why: big typography friendly; looks good in two-line mode.

6. `ISO date only: %F`
   - Why: standard unambiguous date (`YYYY-MM-DD`).

7. `ISO date + time (minutes): %F %R`
   - Why: standard “log-friendly” timestamp without seconds.

8. `Week number (ISO) + weekday: Week %V, %a`
   - Why: helpful for planning / workweeks.

9. `Day of year: Day %j`
   - Why: niche but useful for tracking / hobby projects.

10. `Time + timezone offset: %H:%M %z`
   - Why: for multi-timezone setups; explicitly shows offset.

Notes
- Locale-dependent names: `%a/%A/%b/%B/%p` depend on system locale.
- Some `strftime` modifiers (like `%-H`) can be platform/GLib dependent; if it’s not supported on your target GLib, you can skip those presets.
