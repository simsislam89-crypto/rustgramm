# Каталог Liquid Glass и Fluid UI компонентов (108 элементов)

Сводный справочник из **108 UI-компонентов и паттернов** в эстетике **iOS 26 Liquid Glass / матовое стекло / пружинные микроанимации**, собранных из открытых каталогов интерфейсов. Статус в таблице показывает, какие компоненты уже адаптированы и портированы в нативный Rust/egui-движок Rustgramm (`glass.rs`, `theme.rs`, `dialogs_ui`, `chat.rs`, `features_ui.rs`).

> **Важно:** Rustgramm использует строго матовый тёмный фон без цветных пятен/бликов (в соответствии с требованиями дизайна) — перенесены геометрия капсул, спекулярная кромка, внутренний верхний блик, пружинная физика (`ios_spring`, `ios_spring_bounce`) и контраст оранжевого акцента `#FF7A1A`.

---

## 1. 21st.dev — Liquid Glass & iOS Frosted Components (22 компонента)

Источник: <https://21st.dev/s/liquid-glass> · <https://21st.dev/suraj-xd/liquid-glass> · <https://21st.dev/components>

| № | Компонент | Ссылка | Статус в Rustgramm |
|---|---|---|---|
| 1 | Liquid Glass Card Container | <https://21st.dev/suraj-xd/liquid-glass> | Портирован (`glass::paint_surface` + `GlassVariant::Regular`) |
| 2 | Liquid Glass Pill Button | <https://21st.dev/s/liquid-glass> | Портирован (`glass::pill_button`, `mobile_glass_active`) |
| 3 | Liquid Glass Floating Tab Bar | <https://21st.dev/s/liquid-glass> | Портирован (`glass::floating_tab_bar`) |
| 4 | Liquid Glass Toast Notification | <https://21st.dev/s/liquid-glass> | Портирован (`glass::paint_toast`) |
| 5 | Liquid Glass Sliding Selection Pill | <https://21st.dev/s/liquid-glass> | Портирован (`dialogs_ui` `dialogs-slide-row`) |
| 6 | Liquid Glass Specular Border Sheet | <https://21st.dev/s/liquid-glass> | Портирован (`glass::paint_surface` dual rim stroke) |
| 7 | Liquid Glass Keypad Keys (iPhone Dialer) | <https://21st.dev/s/liquid-glass> | Портирован (`paint_android_keypad` + вектор `BACK`) |
| 8 | Liquid Glass Search Capsule | <https://21st.dev/s/liquid-glass> | Портирован (поиск чатов / сообщений) |
| 9 | Liquid Glass Composer Pill Input | <https://21st.dev/s/liquid-glass> | Портирован (`chat.rs` `composer-input` с оранжевым фокусом) |
| 10 | Liquid Glass Quick-Action Header Dock | <https://21st.dev/s/liquid-glass> | Портирован (`chat.rs` `header_ui`) |
| 11 | Liquid Glass Context Menu Frame | <https://21st.dev/s/liquid-glass> | Портирован (`kit::compact_popup_frame`) |
| 12 | Liquid Glass Toggle Switch (iOS Spring) | <https://21st.dev/components> | Портирован (`theme::switch_ui` + `ios_spring`) |
| 13 | Liquid Glass Segmented Tab Switcher | <https://21st.dev/components> | Портирован (вкладки Эмодзи / Стикеры / GIF, Профиль) |
| 14 | Liquid Glass Bottom Sheet Scrim | <https://21st.dev/components> | Портирован (`paint_frosted_scrim`) |
| 15 | Liquid Glass Floating Jump-Down FAB | <https://21st.dev/s/liquid-glass> | Портирован (`scroll-down-fab`, `@`, `❤`) |
| 16 | Liquid Glass Reaction Pill Chip | <https://21st.dev/s/liquid-glass> | Портирован (`messages_ui` капсулы реакций) |
| 17 | Liquid Glass Media Preview Scrim | <https://21st.dev/components> | Портирован (`viewer_window`) |
| 18 | Liquid Glass Modal Dialog Frame | <https://21st.dev/components> | Референс для модальных окон |
| 19 | Liquid Glass Command Palette Kbar | <https://21st.dev/components> | Референс (команды бота `/` + меню) |
| 20 | Liquid Glass Tooltip Card | <https://21st.dev/components> | Референс (всплывающие подсказки) |
| 21 | Liquid Glass Avatar Presence Badge | <https://21st.dev/components> | Портирован (`online` кольцо и бейдж канала) |
| 22 | Liquid Glass Pinned Message Banner | <https://21st.dev/s/liquid-glass> | Портирован (шапка закреплённого сообщения во всю ширину) |

---

## 2. ReactBits — Fluid Animations, Optical Glass & Micro-Interactions (20 компонентов)

Источник: <https://reactbits.dev/>

| № | Компонент | Ссылка | Назначение / статус |
|---|---|---|---|
| 23 | Glass Surface | <https://reactbits.dev/components/glass-surface> | Портирован (верхний блик преломления в `glass.rs`) |
| 24 | Glass Icons | <https://reactbits.dev/components/glass-icons> | Портирован (иконки быстрой панели профиля) |
| 25 | Dock (macOS / iOS Fluid Dock) | <https://reactbits.dev/components/dock> | Портирован (увеличение при наведении/нажатии `spring_scale`) |
| 26 | Pill Nav | <https://reactbits.dev/components/pill-nav> | Портирован (скользящий индикатор выбранного чата) |
| 27 | Flowing Menu | <https://reactbits.dev/components/flowing-menu> | Референс (список диалогов с плавным hover-слоем) |
| 28 | Elastic Slider | <https://reactbits.dev/components/elastic-slider> | Референс (ползунки прозрачности в Настройках) |
| 29 | Bounce Cards | <https://reactbits.dev/components/bounce-cards> | Референс (карточки историй / подарков Stars) |
| 30 | Animated List | <https://reactbits.dev/components/animated-list> | Референс (появление новых сообщений в ленте) |
| 31 | Counter (Animated Digits) | <https://reactbits.dev/components/counter> | Референс (счётчики непрочитанных и реакций) |
| 32 | Magnet Button | <https://reactbits.dev/components/magnet> | Референс (кнопка отправки и FAB-кнопки) |
| 33 | Glare Hover | <https://reactbits.dev/animations/glare-hover> | Адаптирован (нейтральный верхний блик без цветных пятен) |
| 34 | Click Spark | <https://reactbits.dev/animations/click-spark> | Портирован (`rx_pop` анимация клика по реакции) |
| 35 | Star Border | <https://reactbits.dev/animations/star-border> | Референс (рамка звёздной платной реакции `⭐`) |
| 36 | Spotlight Card | <https://reactbits.dev/components/spotlight-card> | Референс (карточка предпросмотра чата) |
| 37 | Gooey Nav | <https://reactbits.dev/components/gooey-nav> | Референс (плавный переход вкладок профиля) |
| 38 | Gradual Blur | <https://reactbits.dev/animations/gradual-blur> | Референс (мягкий градиент у верхнего/нижнего края ленты) |
| 39 | Bubble Menu | <https://reactbits.dev/components/bubble-menu> | Референс (всплывающая палитра реакций) |
| 40 | Stepper (Smooth Wizard) | <https://reactbits.dev/components/stepper> | Портирован (шаги входа Телефон → Код → 2FA Пароль) |
| 41 | Folder Reveal | <https://reactbits.dev/components/folder> | Референс (папки чатов и архива) |
| 42 | Tilted Card | <https://reactbits.dev/components/tilted-card> | Референс (карточки подарков и стикеров) |

---

## 3. Aceternity UI — Depth, Floating Docks & Tactile Panels (16 компонентов)

Источник: <https://ui.aceternity.com/components>

| № | Компонент | Ссылка | Назначение / статус |
|---|---|---|---|
| 43 | Floating Dock | <https://ui.aceternity.com/components/floating-dock> | Портирован (нижняя навигационная панель на телефоне) |
| 44 | Floating Navbar | <https://ui.aceternity.com/components/floating-navbar> | Референс (верхняя шапка чата) |
| 45 | Tabs (Sliding Pill Tabs) | <https://ui.aceternity.com/components/tabs> | Портирован (папки и вкладки настроек) |
| 46 | Animated Tooltip | <https://ui.aceternity.com/components/animated-tooltip> | Референс (подписи при наведении на участников) |
| 47 | Animated Modal | <https://ui.aceternity.com/components/animated-modal> | Референс (модалки подтверждения и Stars) |
| 48 | Stateful Button | <https://ui.aceternity.com/components/stateful-button> | Референс (кнопка отправки с состоянием загрузки) |
| 49 | Placeholders And Vanish Input | <https://ui.aceternity.com/components/placeholders-and-vanish-input> | Референс (поле ввода и поиск) |
| 50 | Sidebar (Collapsible Navigation) | <https://ui.aceternity.com/components/sidebar> | Портирован (левая боковая панель папок и аккаунтов) |
| 51 | File Upload Dropzone | <https://ui.aceternity.com/components/file-upload> | Портирован (drag-and-drop вложений с превью) |
| 52 | Moving Border | <https://ui.aceternity.com/components/moving-border> | Референс (активный контур записи голосового) |
| 53 | Focus Cards | <https://ui.aceternity.com/components/focus-cards> | Референс (карточки тем и паков стикеров) |
| 54 | Glare Card | <https://ui.aceternity.com/components/glare-card> | Референс (просмотр стикерпака) |
| 55 | Expandable Card | <https://ui.aceternity.com/components/expandable-card> | Референс (раскрытие комментариев поста и ответов) |
| 56 | Timeline View | <https://ui.aceternity.com/components/timeline> | Референс (журнал удалённых/правок сообщения) |
| 57 | Hover Border Gradient | <https://ui.aceternity.com/components/hover-border-gradient> | Референс (обводка кнопок меню бота) |
| 58 | Layout Grid | <https://ui.aceternity.com/components/layout-grid> | Референс (сетка медиафайлов в правой панели) |

---

## 4. Magic UI — Tactile Surface Micro-Motion & Dock Physics (16 компонентов)

Источник: <https://magicui.design/docs/components/dock>

| № | Компонент | Ссылка | Назначение / статус |
|---|---|---|---|
| 59 | Magic Dock | <https://magicui.design/docs/components/dock> | Портирован (панель вкладок с физикой масштаба) |
| 60 | Animated List | <https://magicui.design/docs/components/animated-list> | Референс (лента входящих сообщений) |
| 61 | Animated Beam | <https://magicui.design/docs/components/animated-beam> | Референс (пересылка / ответы на сообщения) |
| 62 | Shimmer Button | <https://magicui.design/docs/components/shimmer-button> | Референс (CTA «Запустить бота /start» и «ДАЛЕЕ») |
| 63 | Shine Border | <https://magicui.design/docs/components/shine-border> | Референс (кромка активного стекла) |
| 64 | Border Beam | <https://magicui.design/docs/components/border-beam> | Референс (индикация звонка / синхронизации) |
| 65 | Ripple Button | <https://magicui.design/docs/components/ripple-button> | Референс (отклик нажатия клавиатуры кода/номера) |
| 66 | Pulsating Button | <https://magicui.design/docs/components/pulsating-button> | Адаптирован (строго для кнопки активной записи голоса) |
| 67 | Number Ticker | <https://magicui.design/docs/components/number-ticker> | Референс (плавный счётчик звёзд/просмотров) |
| 68 | Bento Grid | <https://magicui.design/docs/components/bento-grid> | Референс (обзор аккаунта / профиля) |
| 69 | Marquee | <https://magicui.design/docs/components/marquee> | Референс (бегущая строка длинного названия трека) |
| 70 | Avatar Circles | <https://magicui.design/docs/components/avatar-circles> | Референс (прочитавшие сообщение в «Сведениях») |
| 71 | Confetti Burst | <https://magicui.design/docs/components/confetti> | Портирован (эффекты сообщений и `⭐`) |
| 72 | Blur Fade | <https://magicui.design/docs/components/blur-fade> | Референс (плавное открытие вьювера медиа) |
| 73 | Lens Magnifier | <https://magicui.design/docs/components/lens> | Референс (зум вьювера фото) |
| 74 | Hero Video Dialog | <https://magicui.design/docs/components/hero-video-dialog> | Референс (встроенный видео-просмотрщик) |

---

## 5. Motion Primitives — Smooth Springs, Morphing Popovers & Sliders (14 компонентов)

Источник: <https://motion-primitives.com/docs>

| № | Компонент | Ссылка | Назначение / статус |
|---|---|---|---|
| 75 | Animated Background (Sliding Highlight) | <https://motion-primitives.com/docs> | Портирован (`dialogs-slide-row` скользящая плашка чатов) |
| 76 | Morphing Popover | <https://motion-primitives.com/docs> | Референс (меню выбора эмодзи и реакций) |
| 77 | Morphing Dialog | <https://motion-primitives.com/docs> | Референс (переход в предпросмотр чата и профиль) |
| 78 | Disclosure Collapse | <https://motion-primitives.com/docs> | Референс (сворачиваемые разделы настроек AyuGram) |
| 79 | Sliding Number | <https://motion-primitives.com/docs> | Референс (счётчики непрочитанных сообщений) |
| 80 | Text Morph | <https://motion-primitives.com/docs> | Референс (переключение статуса «в сети» / «печатает…») |
| 81 | Magnetic Action Trigger | <https://motion-primitives.com/docs> | Референс (кнопка отправки / микрофона) |
| 82 | Transition Panel | <https://motion-primitives.com/docs> | Референс (смена экрана Телефон → Код → Чаты) |
| 83 | Toolbar Dynamic | <https://motion-primitives.com/docs> | Референс (панель форматирования `[Aa]`) |
| 84 | Toolbar Expandable | <https://motion-primitives.com/docs> | Референс (панель множественного выделения) |
| 85 | Scroll Progress Bar | <https://motion-primitives.com/docs> | Референс (индикатор прокрутки историй) |
| 86 | Cursor Context Tag | <https://motion-primitives.com/docs> | Референс (автоскролл по средней кнопке мыши) |
| 87 | Border Trail | <https://motion-primitives.com/docs> | Референс (рамка активной загрузки файла) |
| 88 | Glow Effect (Monochrome Specular) | <https://motion-primitives.com/docs> | Адаптирован (только белый спекулярный контур, без цветного свечения) |

---

## 6. GlassCN & Uiverse — Матовые стеклянные примитивы (20 компонентов)

Источники: <https://glasscn.app> · <https://uiverse.io/elements?search=glassmorphism>

| № | Компонент | Ссылка | Назначение / статус |
|---|---|---|---|
| 89 | GlassCN Frost Card | <https://glasscn.app> | Портирован (`GlassVariant::Regular`) |
| 90 | GlassCN Clear Pill | <https://glasscn.app> | Портирован (`GlassVariant::Clear`) |
| 91 | GlassCN Tinted Surface | <https://glasscn.app> | Портирован (`GlassVariant::Tinted`) |
| 92 | GlassCN Modal Drawer | <https://glasscn.app> | Референс (мобильная шторка) |
| 93 | GlassCN Action Sheet | <https://glasscn.app> | Портирован (контекстные действия сообщения) |
| 94 | GlassCN Dropdown Menu | <https://glasscn.app> | Референс (меню заголовка чата) |
| 95 | GlassCN Slider Track | <https://glasscn.app> | Референс (настройки прозрачности) |
| 96 | GlassCN Switch Pill | <https://glasscn.app> | Портирован (`switch_ui`) |
| 97 | GlassCN Badge Chip | <https://glasscn.app> | Портирован (бейджи `BOT`, `ADMIN`, реакции) |
| 98 | GlassCN Breadcrumb Bar | <https://glasscn.app> | Референс (ветка форума и комментарии) |
| 99 | Uiverse Frosted Dark Button | <https://uiverse.io/elements?search=glassmorphism> | Портирован (клавиатура цифр входа) |
| 100 | Uiverse Translucent Input Bar | <https://uiverse.io/elements?search=glassmorphism> | Портирован (`composer-input`) |
| 101 | Uiverse Frosted Search Box | <https://uiverse.io/elements?search=glassmorphism> | Портирован (поиск в списке чатов) |
| 102 | Uiverse Specular Neon Edge Card | <https://uiverse.io/elements?search=glassmorphism> | Адаптирован (монохромный + `#FF7A1A` фокус) |
| 103 | Uiverse Glass Toggle Group | <https://uiverse.io/elements?search=glassmorphism> | Референс (фильтры поиска по чату) |
| 104 | Uiverse Glass Profile Card | <https://uiverse.io/elements?search=glassmorphism> | Референс (правая панель сведений о чате) |
| 105 | Uiverse Glass Audio Mini-Player | <https://uiverse.io/elements?search=glassmorphism> | Портирован (плеер трека и голосовых сообщений) |
| 106 | Uiverse Glass Audio Progress Pill | <https://uiverse.io/elements?search=glassmorphism> | Референс (прогресс воспроизведения) |
| 107 | Uiverse Glass Call Control Dock | <https://uiverse.io/elements?search=glassmorphism> | Референс (оверлей активного звонка) |
| 108 | Uiverse Glass Floating Banner | <https://uiverse.io/elements?search=glassmorphism> | Портирован (верхний закреплённый баннер) |
