# DictaFlow

Диктовка голосом в любое активное окно Windows/macOS с автоматической пунктуацией
и исправлением грамматики. Работает офлайн (Whisper через whisper.cpp),
облачный режим — опционально.

## Возможности

- Глобальная горячая клавиша (press & hold) — работает в любом приложении, поверх ОС.
- Распознавание русского, украинского и английского, включая микс, с автоопределением языка.
- Автоматическая пунктуация и капитализация, лёгкая грамматическая коррекция офлайн.
- Голосовые команды: «новая строка», «новый абзац», «точка», «запятая», «удали последнее предложение» (RU/UK/EN).
- Пользовательский словарь для имён, терминов, аббревиатур.
- История диктовок с поиском, локальная SQLite-база.
- Вставка текста через имитацию набора ИЛИ через буфер обмена + Ctrl+V.
- Опциональный облачный режим (грамматика через внешний API) — по умолчанию выключен.
- Автозапуск с системой, сворачивание в трей.

## Архитектура

```
dictaflow/
├── src/                      # React + TypeScript UI (Tauri webview)
│   ├── App.tsx
│   └── components/           # Recorder, Settings, History
├── src-tauri/                 # Rust-ядро
│   └── src/
│       ├── main.rs            # запуск, трей, регистрация хоткея
│       ├── audio.rs           # захват микрофона (cpal), ресемплинг до 16кГц
│       ├── whisper_engine.rs  # обёртка над whisper.cpp (whisper-rs), загрузка моделей
│       ├── textproc.rs        # пунктуация, голосовые команды, словарь, облачная грамматика
│       ├── inject.rs          # вставка текста в активное окно (enigo / буфер обмена)
│       ├── hotkey.rs          # низкоуровневый глобальный listener клавиатуры (rdev)
│       ├── history.rs         # SQLite-история диктовок
│       ├── config.rs          # настройки, JSON-конфиг
│       ├── state.rs           # общее состояние приложения
│       └── commands.rs        # Tauri-команды для UI
└── .github/workflows/build.yml # CI: авто-сборка MSI (Windows) и DMG (macOS)
```

## Как получить готовые .msi / .dmg — через GitHub Actions

Сборка на реальной Windows/macOS происходит не у меня локально, а на серверах
GitHub при пуше. Шаги:

1. Создай новый репозиторий на GitHub (пустой, без README).
2. В этой папке:
   ```bash
   git init
   git add .
   git commit -m "DictaFlow: initial scaffold"
   git branch -M main
   git remote add origin https://github.com/<твой-аккаунт>/dictaflow.git
   git push -u origin main
   ```
3. Открой вкладку **Actions** в репозитории на GitHub — workflow `Build DictaFlow`
   запустится автоматически при пуше в `main`.
4. Через 10–20 минут (Rust собирается небыстро) в **Actions → (последний run) →
   Artifacts** появятся:
   - `dictaflow-windows-latest` — внутри `.msi` и портативный `.exe` (NSIS).
   - `dictaflow-macos-latest` — внутри `.dmg`.
5. Чтобы получить именно **релиз** с версией (а не просто артефакт),
   запушь тег:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
   Появится Draft Release с прикреплёнными установщиками — опубликуй его вручную
   на GitHub (кнопка "Publish release").

### Если хочешь собрать локально (без ожидания CI)

**Windows:**
```powershell
# Однократно: Rust, Node.js 20+, Visual Studio Build Tools (C++), WebView2 (обычно уже есть в Win10/11)
rustup default stable
npm install
npx tauri icon src-tauri/icons/icon-source.png -o src-tauri/icons
npm run tauri build
# Готовые файлы: src-tauri/target/release/bundle/msi/*.msi
#                src-tauri/target/release/bundle/nsis/*.exe
```

**macOS:**
```bash
xcode-select --install
npm install
npx tauri icon src-tauri/icons/icon-source.png -o src-tauri/icons
npm run tauri build
# Готовый файл: src-tauri/target/release/bundle/dmg/*.dmg
```

## Поддержка Windows 7

Windows 7 **опционально и с оговорками**: WebView2 (движок интерфейса Tauri)
на Win7 требует отдельной установки Evergreen-runtime и Microsoft прекратил
для него официальную поддержку. Ядро на Rust соберётся и заработает, но
стабильность интерфейса на Win7 не гарантируется современными версиями Tauri/WebView2.
Рекомендация: ориентироваться на Windows 10/11 как основные таргеты,
Windows 7 — best-effort без гарантий из коробки.

## Модели распознавания (Whisper)

Модели скачиваются автоматически при первом выборе размера в Настройках
(с Hugging Face, `ggerganov/whisper.cpp`) и хранятся локально в
`%APPDATA%/DictaFlow/models` (Windows) или `~/Library/Application Support/DictaFlow/models` (macOS).
Рекомендуемый размер для RU/UK/EN микса — `small` (баланс) или `medium` (точнее, медленнее).

## Известные ограничения текущего скелета

- Grammar-correction офлайн — облегчённая (правила капитализации/пунктуации).
  Полноценная офлайн-грамматика (например, локальный LanguageTool-сервер) — следующий шаг,
  папка `textproc.rs` уже спроектирована для замены на него.
- Голосовые команды на фиксированном списке фраз — можно расширять словарь в `textproc.rs`.
- Первый прогон CI может потребовать точечных версий крейтов (whisper-rs/rdev/enigo
  довольно активно обновляются) — если сборка упадёт на конкретной версии, GitHub Actions
  покажет точную ошибку компиляции, и это будет предметно чинить, а не гадать.

## Лицензия

MIT — см. `LICENSE`.
