# Каталог Liquid Glass и Fluid UI — Очередь оставшихся компонентов (62)

Оставшиеся референсы и паттерны **iOS 26 Liquid Glass / матовое стекло / пружинные микроанимации**, ещё **не** портированные в Rustgramm (`v0.30.0`). Все уже реализованные компоненты (включая `#1–15`, `GlassEffect`/`GlassDock`, `NotificationPopover`, `ExpandableSidebar`, `AppleSpotlight`, `ChatComposer`, `SegmentedTabs`, `OtpInput`, а также ранее перенесённые `GlassCard`, `PillButton`, `FloatingTabBar`, `Toast`, `SlidingPill`, `KeypadKeys`, `Switch`, `FloatingDock`, `ActionSheet` и др.) удалены из очереди.

> **Стиль Rustgramm (`v0.30.0`):** строго монохромный чёрно-белый Liquid Glass (`#F2F2F6` акцент, `#0A0A0C` текст на белых элементах, матовый фон `#08080A` без цветных бликов). Для точности ссылки указывают на корневые каталоги библиотек (`21st.dev`, `reactbits.dev`, `ui.aceternity.com`, `magicui.design`, `motion-primitives.com`, `glasscn.app`, `uiverse.io`).

---

## 1. 21st.dev — Оставшиеся компоненты (3)

Источник: <https://21st.dev/s/liquid-glass> · <https://21st.dev/components>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 1 | Liquid Glass Modal Dialog Frame | <https://21st.dev/components> | Модальные окна поверх ленты (перенос на единую рамку `glass.rs`) |
| 2 | Liquid Glass Command Palette Kbar | <https://21st.dev/components> | Меню `/` команд бота в стиле Kbar |
| 3 | Liquid Glass Tooltip Card | <https://21st.dev/components> | Единая стеклянная карточка hover-подсказок |

---

## 2. ReactBits — Оставшиеся компоненты (12)

Источник: <https://reactbits.dev/>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 4 | Flowing Menu | <https://reactbits.dev/components> | Плавный hover-слой в строках списка диалогов |
| 5 | Elastic Slider | <https://reactbits.dev/components> | Ползунки прозрачности/скорости с резиновым отскоком в Настройках |
| 6 | Bounce Cards | <https://reactbits.dev/components> | Карточки историй и витрины подарков Stars |
| 7 | Animated List | <https://reactbits.dev/components> | Пружинное появление новых сообщений в ленте |
| 8 | Counter (Animated Digits) | <https://reactbits.dev/components> | Перелистывающиеся цифры счётчиков непрочитанных и реакций |
| 9 | Magnet Button | <https://reactbits.dev/components> | Магнитное притяжение указателя для FAB-кнопок |
| 10 | Star Border | <https://reactbits.dev/components> | Монохромный мерцающий контур платной звёздной реакции |
| 11 | Spotlight Card | <https://reactbits.dev/components> | Карточка предпросмотра чата (`Peek`) |
| 12 | Gooey Nav | <https://reactbits.dev/components> | Плавное слияние вкладок профиля |
| 13 | Gradual Blur | <https://reactbits.dev/components> | Мягкое растворение у верхнего и нижнего края ленты |
| 14 | Bubble Menu | <https://reactbits.dev/components> | Радиальная всплывающая палитра реакций |
| 15 | Folder Reveal | <https://reactbits.dev/components> | Папки чатов и архива |
| 16 | Tilted Card | <https://reactbits.dev/components> | Карточки подарков и стикерпаков |

---

## 3. Aceternity UI — Оставшиеся компоненты (12)

Источник: <https://ui.aceternity.com/components>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 17 | Floating Navbar | <https://ui.aceternity.com/components> | Верхняя плавающая шапка чата |
| 18 | Animated Tooltip | <https://ui.aceternity.com/components> | Подписи участников над аватарками |
| 19 | Animated Modal | <https://ui.aceternity.com/components> | Пружинные модалки Stars и подтверждений |
| 20 | Stateful Button | <https://ui.aceternity.com/components> | Кнопки с встроенным индикатором загрузки |
| 21 | Placeholders And Vanish Input | <https://ui.aceternity.com/components> | Анимированные подсказки поля ввода и поиска |
| 22 | Moving Border | <https://ui.aceternity.com/components> | Контур активной записи голоса |
| 23 | Focus Cards | <https://ui.aceternity.com/components> | Карточки тем и наборов стикеров |
| 24 | Glare Card | <https://ui.aceternity.com/components> | Карточка просмотра стикерпака |
| 25 | Expandable Card | <https://ui.aceternity.com/components> | Раскрытие комментариев поста и ответов |
| 26 | Timeline View | <https://ui.aceternity.com/components> | Журнал удалённых и правок сообщения |
| 27 | Hover Border Gradient | <https://ui.aceternity.com/components> | Световая кромка кнопок меню бота |
| 28 | Layout Grid | <https://ui.aceternity.com/components> | Сетка медиафайлов в правой панели |

---

## 4. Magic UI — Оставшиеся компоненты (13)

Источник: <https://magicui.design/docs/components/dock>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 29 | Animated List | <https://magicui.design/docs/components/dock> | Плавное появление входящих сообщений |
| 30 | Animated Beam | <https://magicui.design/docs/components/dock> | Связь ответов и пересылки |
| 31 | Shimmer Button | <https://magicui.design/docs/components/dock> | CTA «Запустить бота /start» и «ДАЛЕЕ» |
| 32 | Shine Border | <https://magicui.design/docs/components/dock> | Монохромная световая кромка стекла |
| 33 | Border Beam | <https://magicui.design/docs/components/dock> | Индикатор активного звонка / синхронизации |
| 34 | Ripple Button | <https://magicui.design/docs/components/dock> | Волна нажатия клавиш набора |
| 35 | Number Ticker | <https://magicui.design/docs/components/dock> | Плавный счётчик звёзд и просмотров |
| 36 | Bento Grid | <https://magicui.design/docs/components/dock> | Сетка профиля и обзора аккаунта |
| 37 | Marquee | <https://magicui.design/docs/components/dock> | Бегущая строка длинного названия трека |
| 38 | Avatar Circles | <https://magicui.design/docs/components/dock> | Прочитавшие сообщение в «Сведениях» |
| 39 | Blur Fade | <https://magicui.design/docs/components/dock> | Плавное открытие просмотрщика медиа |
| 40 | Lens Magnifier | <https://magicui.design/docs/components/dock> | Лупа масштабирования фото во вьювере |
| 41 | Hero Video Dialog | <https://magicui.design/docs/components/dock> | Модальный видео-просмотрщик |

---

## 5. Motion Primitives — Оставшиеся компоненты (12)

Источник: <https://motion-primitives.com/docs>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 42 | Morphing Popover | <https://motion-primitives.com/docs> | Меню выбора эмодзи и реакций |
| 43 | Morphing Dialog | <https://motion-primitives.com/docs> | Переход в предпросмотр чата и профиль |
| 44 | Disclosure Collapse | <https://motion-primitives.com/docs> | Сворачиваемые разделы настроек AyuGram |
| 45 | Sliding Number | <https://motion-primitives.com/docs> | Скользящие цифры счётчиков непрочитанных |
| 46 | Text Morph | <https://motion-primitives.com/docs> | Переход статуса «в сети» ↔ «печатает…» |
| 47 | Magnetic Action Trigger | <https://motion-primitives.com/docs> | Кнопка отправки / микрофона |
| 48 | Transition Panel | <https://motion-primitives.com/docs> | Переходы экранов Телефон → Код → Чаты |
| 49 | Toolbar Dynamic | <https://motion-primitives.com/docs> | Динамическая панель форматирования `[Aa]` |
| 50 | Toolbar Expandable | <https://motion-primitives.com/docs> | Расширяемая панель пакетного выделения |
| 51 | Scroll Progress Bar | <https://motion-primitives.com/docs> | Полоса прогресса историй |
| 52 | Cursor Context Tag | <https://motion-primitives.com/docs> | Иконка автоскролла у курсора |
| 53 | Border Trail | <https://motion-primitives.com/docs> | Индикатор активной загрузки файла |

---

## 6. GlassCN & Uiverse — Оставшиеся компоненты (9)

Источники: <https://glasscn.app> · <https://uiverse.io/elements?search=glassmorphism>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 54 | GlassCN Modal Drawer | <https://glasscn.app> | Мобильная нижняя шторка |
| 55 | GlassCN Dropdown Menu | <https://glasscn.app> | Выпадающее меню заголовка чата |
| 56 | GlassCN Slider Track | <https://glasscn.app> | Стеклянные треки ползунков настроек |
| 57 | GlassCN Breadcrumb Bar | <https://glasscn.app> | Навигация по темам форума и комментариям |
| 58 | Uiverse Glass Toggle Group | <https://uiverse.io/elements?search=glassmorphism> | Фильтры поиска по чату |
| 59 | Uiverse Glass Profile Card | <https://uiverse.io/elements?search=glassmorphism> | Правая панель сведений о чате |
| 60 | Uiverse Glass Audio Progress Pill | <https://uiverse.io/elements?search=glassmorphism> | Пилюля прогресса аудио/голосовых |
| 61 | Uiverse Glass Call Control Dock | <https://uiverse.io/elements?search=glassmorphism> | Оверлей управления звонком |
| 62 | Uiverse Specular Card Overlay | <https://uiverse.io/elements?search=glassmorphism> | Двойная кромка карточек галереи медиа |
