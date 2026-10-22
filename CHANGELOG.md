# Changelog

All notable changes to GlassTranslate Lite. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [1.1.1]

### Added
- **Hebrew screen OCR that works out of the box.** Hebrew → Arabic screen translation now uses a **bundled, offline
  Tesseract 5.4.0** (UB Mannheim Windows build, Apache-2.0) with the official `heb.traineddata` from `tessdata_best`
  (plus `eng.traineddata` for mixed Hebrew/English lines). No Windows Hebrew OCR language pack, global Tesseract install,
  PATH entry or network access is needed. Tesseract runs as a hidden child process; the screenshot is piped to it in
  memory and nothing is written to disk.
- Image preprocessing for Tesseract (greyscale, auto-invert for dark UIs such as Discord, contrast stretch, white
  padding, 1× and 2× passes keeping the more confident result). Words are read from Tesseract's TSV output in logical
  order, so right-to-left reading order, punctuation, numbers and mixed Hebrew/English text come out correctly.
- The bundled OCR files (`tesseract.exe`, DLLs, `heb`/`eng` data) are verified on use (presence, size, SHA-256 of
  `heb.traineddata`). If they are missing or damaged, Settings and the popup show a clear message with the recovery
  action (reinstall; meanwhile use clipboard or manual translation).
- Settings shows which OCR engine is active for the selected direction.
- OCR post-processing for right-to-left lines: line-final punctuation that Tesseract emits at the start of a Hebrew line
  (`?שלום`) is moved back to the end, stray `|` characters are removed and lines without letters or digits are dropped.
  Small text (source width ≤ 1400 px, low confidence) gets an extra 3× pass.

### Changed
- OCR routing by direction: English → Arabic keeps the Windows OCR engine; Hebrew → Arabic uses bundled Tesseract.
  Switching direction needs no restart, and *Translate again* uses the original direction's engine.
- Version bumped to 1.1.1 (`package.json`, `Cargo.toml`, `tauri.conf.json`). Installer resources now include
  `tesseract/` (explicit resource paths, including `tesseract/tessdata/`).
- `THIRD_PARTY_NOTICES.md` lists Tesseract, the language data, Leptonica and the support DLLs.

### Fixed
- Hebrew screen OCR previously depended on the Windows `he-IL` language pack, which is not installed by default (root
  cause of the 1.1.0 limitation). It no longer does.
- Found while testing the installed app: Hebrew lines came out with their punctuation at the start of the line and some
  sentences were dropped on dark backgrounds. Fixed with the contrast stretch and the punctuation handling above.

### Notes
- The installer grows by about 31 MB (168,306,466 bytes) because it carries Tesseract and the Hebrew data.
- Not verified: Discord itself, tray right-click menu / *Quit*, a network-blocked run and the uninstaller. See README.

## [1.1.0]

### Added
- **Hebrew → Arabic offline translation**, alongside English → Arabic. Hebrew is translated *directly* with
  `Helsinki-NLP/opus-mt-he-ar` (converted to CTranslate2 int8); there is no pivot through English.
- **Language-direction selector** in the main window ("English → العربية" / "עברית → العربية"). The direction is used
  for manual, clipboard and screen translation, is persisted across restarts, and has a default in Settings.
- **Per-model status** in Settings / engine info: state, size on disk, SHA-256 verification, reveal-folder and reload.
  Models are loaded lazily; only the default direction's model is loaded at startup.
- **Hebrew OCR plumbing (not demonstrated)**: the screen-capture path asks Windows OCR for `he-IL` when the direction is
  Hebrew → Arabic. If the Windows Hebrew OCR language pack is not installed, a clear error is shown (no silent fallback
  to another language). On the development PC the `he-IL` pack is not installed, so Hebrew screen OCR has **not** been
  shown to work end to end.
- **History / Favorites** records now store source language, target language and translation method; the list shows
  the language pair and renders Hebrew originals right-to-left.
- Popup shows the active language pair; Hebrew originals are RTL; *Translate again* reuses the original direction.
- `CHANGELOG.md`, `PROGRESS.md`, `scripts/model-he-ar.sha256`.

### Changed
- Version bumped from 1.0.0 to 1.1.0 (`package.json`, `Cargo.toml`, `tauri.conf.json`).
- `scripts/prepare-model.mjs` accepts a model id (`en-ar` | `he-ar`).
- Decoding for Marian models uses beam 4, repetition penalty 1.1, no-repeat-ngram 4 (prevents repeated-token output).

### Fixed
- Popup showed the English-pack "OCR unavailable" message when Hebrew OCR was unavailable; it now shows the Hebrew
  language-pack message (found while testing the installed build).

### Migration
- Existing `settings.json` and `history.json` files load unchanged: missing fields default to English → Arabic / direct.

### Notes
- Both models ship **inside the installer** (installer about 67 MB larger than 1.0.0: 70,011,183 → 137,571,543 bytes). There is no model downloader in this
  release: no hash-pinned hosted CTranslate2 build of these models exists, so none is invented.
- Hebrew screen OCR depends on the Windows `he-IL` OCR language pack, which is a Windows feature the user installs.

## [1.0.0]
- Initial release: offline English → Arabic screen / clipboard / manual translation, glass popup, history and
  favorites, English and Arabic interface, dark and light themes, tray, per-user NSIS installer.
