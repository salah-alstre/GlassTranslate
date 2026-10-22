# GlassTranslate Lite

An offline, privacy-first translator for Windows that turns **English → Arabic** and **Hebrew → Arabic** text into a
clean glass popup. Translate text you type, text on your clipboard, or text on your screen. Everything runs on your PC:
no cloud or paid translation service, and no account.

- Version: **1.1.1** (Windows 10/11, x64, per-user install, no administrator rights needed)
- Interface languages: English and Arabic (Hebrew is a *source* language only)
- Themes: dark (default) and light

## Features

- **Language direction selector** in the main window: `English → العربية` or `עברית → العربية`. The choice is used for
  manual, clipboard and screen translation, is remembered across restarts, and a default can be set in Settings.
- **Manual, clipboard and screen translation.** Screen translation captures the screen, runs OCR (Windows OCR for English,
  bundled Tesseract for Hebrew), and translates the recognised text.
- **Glass popup** with the result, copy, favorite and *Translate again* (reuses the original direction). Hebrew and
  Arabic text are shown right-to-left.
- **History and Favorites** (off by default). Each record stores the source language, target language, method, time and
  favorite state. Search works with Hebrew and Arabic text. Records from 1.0.0 are migrated automatically (as English → Arabic).
- **Model status** in Settings: state, size, checksum verification and reload for each model. Models load lazily; only the
  default direction's model is loaded at startup.
- System tray, configurable shortcuts, popup opacity and remembered popup position.

## Shortcuts (defaults)

| Action | Shortcut |
|---|---|
| Capture screen and translate | `Ctrl+Alt+G` |
| Translate clipboard | `Ctrl+Alt+V` |

Both can be rebound in Settings.

## Translation models

| Direction | Model | Method | License |
|---|---|---|---|
| English → Arabic | [Helsinki-NLP/opus-mt-en-ar](https://huggingface.co/Helsinki-NLP/opus-mt-en-ar) | direct | Apache-2.0 |
| Hebrew → Arabic | [Helsinki-NLP/opus-mt-he-ar](https://huggingface.co/Helsinki-NLP/opus-mt-he-ar) | **direct** (no pivot through English) | Apache-2.0 |

Both are converted to CTranslate2 int8 and run with [CTranslate2](https://github.com/OpenNMT/CTranslate2). No pivot
through English is used for Hebrew: a Hebrew → English model (`opus-mt-he-en`) does not exist. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for attribution.

**Both models are bundled in the installer.** There is no model downloader in this release, so there is no extra download
after install and no network use at run time. The 1.1.1 installer (`GlassTranslate Lite_1.1.1_x64-setup.exe`,
168,306,466 bytes) is about 31 MB larger than 1.1.0 because it also carries the Hebrew OCR engine. Each model is verified against a SHA-256 list
(`scripts/model.sha256`, `scripts/model-he-ar.sha256`).

Translation quality: these are small Marian models. Short sentences translate well; long or technical text may be
imperfect. Always check important translations.

## OCR and Hebrew

OCR engine is chosen by the selected direction:

- **English → Arabic** uses the Windows built-in OCR engine (`en-US`, available on a standard Windows install).
- **Hebrew → Arabic** uses a **bundled, offline Tesseract 5.4.0** with the official `heb.traineddata` (`tessdata_best`)
  and `eng.traineddata`. It needs **no Windows Hebrew language pack, no global Tesseract install and no PATH entry**, and
  nothing is sent over the network. Tesseract runs as a hidden child process; the screenshot is passed in memory.
  Dark UIs (e.g. Discord) are handled by automatic inversion and contrast stretching; mixed Hebrew/English text,
  numbers and punctuation are supported.
- If the bundled Hebrew OCR files are missing or damaged, the app says so and suggests reinstalling. Manual input and
  clipboard translation keep working regardless (copy Hebrew text and press `Ctrl+Alt+V`).
- OCR accuracy on very small or low-contrast text is limited; prefer selecting a tight region around the text.

## Install

Run `GlassTranslate Lite_1.1.1_x64-setup.exe` (it can be installed over 1.1.0). It installs per user to `%LOCALAPPDATA%\GlassTranslate Lite\`. No Python,
Rust or Node.js is needed. Settings and history are stored in `%APPDATA%\com.glasstranslate.lite\`; uninstalling removes
the program but leaves these files.

## Build from source

Requires Node.js, Rust (MSVC) and CMake.

```bash
npm install
node scripts/prepare-model.mjs en-ar   # builds src-tauri/resources/model
node scripts/prepare-model.mjs he-ar   # builds src-tauri/resources/model-he-ar
npm run tauri build
```

Model files are not committed (see `.gitignore`). Run `npx tsc --noEmit` and `cargo test --release` in `src-tauri` to check.

## Screenshots

Images were captured from the installed 1.1.0 build. 1.1.1 does not change the layout (only the OCR engine line in
Settings and the OCR error text differ), so they were not recaptured.

| | |
|---|---|
| ![Main window, dark](docs/screenshots/main-dark.png) | ![Main window, light](docs/screenshots/main-light.png) |
| ![Popup, English to Arabic](docs/screenshots/popup.png) | ![Hebrew to Arabic result](docs/screenshots/hebrew-translation.png) |
| ![Settings](docs/screenshots/settings.png) | ![Arabic interface (RTL)](docs/screenshots/arabic-rtl.png) |
| ![History and favorites](docs/screenshots/history-favorites.png) | |

## Testing status

**1.1.1 – automated tests and static checks:**

- `cargo test --release --lib`: 24 passed, 0 failed. This includes the bundled-Tesseract integrity check, the OCR
  post-processing tests and Hebrew OCR on rendered test images (not real screen captures): 13 of 18 lines exact,
  96.0 % character accuracy. Typical errors are `ב`/`כ` confusions at small sizes; 12 px text is the weakest case.
- Hebrew → Arabic translation with the real bundled model, for example `אני רוצה ללמוד תכנות.` → `أريد أن أتعلم البرمجة`
  and `התוכנית לא עובדת במחשב שלי.` → `البرنامج لا يعمل على حاسوبي`.
- TypeScript typecheck, frontend build, Rust build, Tauri production build and NSIS installer build.
- Offline: a code and dependency inspection only (no HTTP client in the app, no network calls from the UI, restrictive
  CSP). A run with the network blocked was **not** performed.
- Uninstaller: the generated NSIS script was reviewed (per-user, opt-in "delete app data" checkbox, explicit deletion of
  the installed files including `tesseract/`). It was **not** run.

**1.1.1 – verified by hand on the installed app** (installed over 1.1.0 and then re-installed over 1.1.1 with the final
build; models and settings were kept both times):

- Hebrew screen OCR with `Ctrl+Alt+G` on a dark Notepad window at 150 % zoom. Recognised text:
  `שלום, מה שלומך?` / `אני רוצה ללמוד תכנות. תודה רבה על העזרה. איפה אפשר למצוא את הקובץ? התוכנית לא עובדת במחשב שלי. שלום 2024 תודה Hello`.
  All five test sentences were recognised and translated, e.g. `שלום, מה שלומך?` → `مرحباً، كيف حالك؟`.
  This was a controlled Notepad window, **not** Discord.
- English screen OCR with `Ctrl+Alt+G` (`The weather is very nice today.` → `الطقس لطيف جداً اليوم`), clipboard
  translation with `Ctrl+Alt+V` in both directions, and manual input in both directions (including Hebrew in the
  right-to-left text box).
- Switching direction without a restart, *Translate again*, *Copy translation*, *Copy original*, dragging, pin/unpin,
  `Esc`, popup opacity, remembered position, focus staying in the app you are typing in, shortcut rebinding, history and
  favorites, Arabic RTL interface, light/dark theme.
- Tray icon with tooltip, and left-click reopening the main window after it was closed to the tray.
- 1.1.0 → 1.1.1 upgrade with settings and both models preserved.

**1.1.0 (verified by hand on the installed build):** upgrade from 1.0.0 with settings kept; both directions via manual
input, clipboard hotkey and *Translate again*; English screen OCR; popup opacity, remembered position, dragging and
shortcut rebinding; close to tray and reopening from the tray icon; Arabic RTL interface, light theme and default
direction persisting across restarts; history and favorites.

## Known limitations

- **Not verified on the installed 1.1.1 app:** Hebrew capture in Discord itself (only a controlled Notepad window was
  used), the tray icon's right-click menu, its menu actions and *Quit* (the tray's right-click could not be driven by
  the test tooling; the code was reviewed), settings persistence across an app restart (checked on 1.1.0, not
  re-checked through the UI on 1.1.1), and the uninstaller.
- Hebrew OCR accuracy depends on text size and contrast; small text (about 12 px) can have letter confusions. Select a
  tight region, or copy the text and use `Ctrl+Alt+V`.
- The Hebrew → Arabic model often leaves out the full stop at the end of a sentence, so several sentences can run
  together in the Arabic text. A line that Tesseract reports as a separate paragraph is shown after a blank line, and
  in mixed Hebrew/English lines the Arabic word order can differ from the on-screen order.
- No test with the network disconnected has been run; "offline" is based on the absence of any network code.
- Windows only; x64 only.

## License

MIT. Third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md); see [CHANGELOG.md](CHANGELOG.md).
