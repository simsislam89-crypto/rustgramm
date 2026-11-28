# Каталог Liquid Glass и Fluid UI — Очередь оставшихся компонентов (62)

Оставшиеся референсы и паттерны **iOS 26 Liquid Glass / матовое стекло / пружинные микроанимации**, ещё **не** портированные в Rustgramm (`v0.31.0`). В разделе 1 сохранены **15** неприсланных компонентов `21st.dev` из расширенного списка библиотеки для последующих этапов, а далее — оставшиеся **47** позиций из `ReactBits`, `Aceternity UI`, `Magic UI`, `Motion Primitives`, `GlassCN` и `Uiverse`. Все уже перенесённые компоненты (включая пакет `v0.31.0`: `Modal Dialog Frame`, `Command Palette Kbar`, `Tooltip Card`, `Flowing Menu`, `Elastic Slider`, `Bounce Cards`, `Animated List`, `Counter`, `Magnet Button`, `Star Border`, `Spotlight Card`, `Gooey Nav`, `Gradual Blur`, `Bubble Menu`, `Folder Reveal`, а также `v0.28–v0.30`: `GlassEffect`/`GlassDock`, `NotificationPopover`, `ExpandableSidebar`, `AppleSpotlight`, `ChatComposer`, `SegmentedTabs`, `OtpInput`, `GlassCard`, `PillButton`, `FloatingTabBar`, `Toast`, `SlidingPill`, `KeypadKeys`, `Switch`, `FloatingDock`, `ActionSheet`) удалены из очереди.

> **Стиль Rustgramm (`v0.31.0`):** строго монохромный чёрно-белый Liquid Glass для всего системного хрома (`#F2F2F6` акцент, `#0A0A0C` текст на белых элементах, матовый фон `#08080A` без цветных бликов) при сохранении живых фирменных цветов Telegram для аватарок, ников пользователей, полос ответов, ссылок и значков verified/premium. Для точности ссылки указывают на корневые каталоги библиотек (`21st.dev`, `reactbits.dev`, `ui.aceternity.com`, `magicui.design`, `motion-primitives.com`, `glasscn.app`, `uiverse.io`).

---

## 1. 21st.dev — Неприсланные и дополнительные компоненты Liquid Glass (15)

Источник: <https://21st.dev/s/liquid-glass> · <https://21st.dev/components>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 1 | Liquid Glass Floating Navigation Bar | <https://21st.dev/components> | Плавающая верхняя панель навигации с прозрачностью при скролле |
| 2 | Liquid Glass Calendar & Date Range Picker | <https://21st.dev/components> | Стеклянный календарь перехода к дате и фильтров истории |
| 3 | Liquid Glass Media / Music Player Card | <https://21st.dev/components> | Плавающий мини-плеер музыки и голосовых заметок |
| 4 | Liquid Glass Profile Hover Card / Avatar Popover | <https://21st.dev/components> | Быстрая карточка участника при наведении на имя в группе |
| 5 | Liquid Glass Accordion / Disclosure List | <https://21st.dev/components> | Сворачиваемые группы настроек приватности и прокси |
| 6 | Liquid Glass Toast / Notification Stack | <https://21st.dev/components> | Стек всплывающих уведомлений поверх ленты |
| 7 | Liquid Glass Bottom Sheet / Action Drawer | <https://21st.dev/components> | Выезжающая снизу стеклянная шторка действий на телефоне |
| 8 | Liquid Glass Context / Action Menu Sheet | <https://21st.dev/components> | Многоуровневое стеклянное контекстное меню сообщений |
| 9 | Liquid Glass Audio Waveform Player Pill | <https://21st.dev/components> | Капсула воспроизведения и перемотки голосового с волной |
| 10 | Liquid Glass File Dropzone / Upload Card | <https://21st.dev/components> | Зона перетаскивания файлов и фото в чат (`Drag & Drop`) |
| 11 | Liquid Glass Skeleton Shimmer Loader | <https://21st.dev/components> | Мерцающий стеклянный плейсхолдер при догрузке медиа и ленты |
| 12 | Liquid Glass Stepper / Multi-Step Onboarding | <https://21st.dev/components> | Пошаговая стеклянная шкала экранов входа (`Телефон -> Код -> 2FA`) |
| 13 | Liquid Glass Radial Speed Dial FAB | <https://21st.dev/components> | Веер быстрых действий (новое сообщение / канал / группа) |
| 14 | Liquid Glass Glassmorphic Table / Data List | <https://21st.dev/components> | Таблицы и карточки журнала аудита, статистики и Stars |
| 15 | Liquid Glass Interactive Map / Geo Pin Card | <https://21st.dev/components> | Стеклянная карточка геолокации и превью точки на карте |

---

## 2. ReactBits — Оставшиеся компоненты (1)

Источник: <https://reactbits.dev/>

| № | Компонент | Ссылка | Потенциальное применение в Rustgramm |
|---|---|---|---|
| 16 | Tilted Card | <https://reactbits.dev/components> | Наклоняющиеся 3D-карточки подарков и стикерпаков |

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
