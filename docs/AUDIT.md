# Аудит AyuGram: куда уходят данные (2026-11-21)

Источники: AyuGramDesktop v7.2.9 (1 225 149 строк C++/h, 3052 файла, 36 сабмодулей; из них код AyuGram — 68 574 строки в `Telegram/SourceFiles/ayu`)
и `vanilla-AyuGram-full-20261104.apk` (133 МБ, 8 dex, нативные библиотеки 96 МБ, встроенный Python 3.11 через Chaquopy).

## ПК: соединения НЕ с серверами Telegram

| Хост | Файл | Что уходит | Решение в Rust-версии |
|---|---|---|---|
| `sentry.radolyn.com` (с зашитым `sentry_key`) | `core/crash_report_window.cpp:663` | минидамп, отчёт, версия, dump-id | удалить полностью |
| `update.ayugram.one`, `api.exteragram.app` | `ayu/utils/rc_manager.cpp:15-16` | GET раз в час: список «разработчиков», значки, донат-суммы | удалить, значки не нужны |
| `cdn.jsdelivr.net/gh/AyuGram/Languages` | `ayu/ayu_lang.cpp:121,124` | GET языковых файлов | вшить локали в бинарник |
| `dpaste.com/api/v2/` | `ayu/features/filters/filters_utils.cpp:366` | POST фильтров пользователя (публикация) | убрать, экспорт только в файл |
| `translate-pa.googleapis.com` | `ayu/features/translator/implementations/google.cpp:28` | текст сообщений на перевод | выключено по умолчанию, только по явному действию, показывать хост |
| `translate.yandex.net` | `.../yandex.cpp:78` | текст сообщений на перевод | то же |
| `itunes.apple.com/search` | `ayu/ui/utils/itunes_search.cpp:698` | запрос обложек | убрать |
| Геолокация (`current_geo_location.cpp`) | upstream tdesktop | токен + запрос карт | только по явному действию |
| Обновления (`update_checker.cpp`) | upstream | GET релизов | свой канал, без автозагрузки |

Серверы MTProto (DC 1–5) зашиты в `mtproto/mtproto_dc_options.cpp` (149.154.x.x, 91.108.x.x) — это Telegram.

## Android (APK): внешние хосты в dex

Помимо Telegram: Firebase Analytics (`app-measurement.com`), Google Maps/Places, `generativelanguage.googleapis.com`,
Stripe, Twitch/Spotify/last.fm/lrclib, `ss3.4sqi.net` (Foursquare), `filters.adtidy.org`, Yandex карты/переводчик,
Bing/TinEye, `cdn.jsdelivr.net`. Нативные модули: `libetgadblock.so`, `libextera_analyzer.so`, `libvosk.so`, Python.
Аналитику и рекламные SDK нужно не переносить, а исключить.

## Вывод для портирования

- Включаем только: MTProto к DC Telegram (+ прокси MTProxy/SOCKS5 по выбору пользователя).
- Всё остальное — по явному действию пользователя и с показом хоста; по умолчанию выключено.
- В CI добавить проверку: сборка падает, если в исходниках появляется URL вне белого списка (`t.me`, `telegram.org`, DC-адреса).
