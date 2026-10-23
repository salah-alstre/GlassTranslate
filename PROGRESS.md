# PROGRESS — GlassTranslate Lite V2 (Hebrew integration)

Working notes so work survives context compaction. Keep this file current.

## Paths
- Project: `C:\Users\error\Desktop\GlassTranslate` (not a git repo; never push anything)
- Source backup (v1.0.0): `C:\Users\error\Desktop\GlassTranslate-backup-v1.0.0\` (+ rollback installer in `installer\`)
- Bundled models: `src-tauri/resources/model/` (en-ar), `src-tauri/resources/model-he-ar/` (he-ar, direct, int8 CT2)
- Checksums: `scripts/model.sha256`, `scripts/model-he-ar.sha256`; rebuild: `node scripts/prepare-model.mjs [en-ar|he-ar]`
- App data: `%APPDATA%\com.glasstranslate.lite\` (settings.json, history.json); install: `%LOCALAPPDATA%\GlassTranslate Lite\`
- cmake for cargo (if needed): `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin`

## Decisions
- Hebrew→Arabic is DIRECT (Helsinki-NLP/opus-mt-he-ar, Apache-2.0, BLEU 23.6), no pivot (opus-mt-he-en does not exist).
- Marian multi-target needs `>>ara<<` token; ct2rs encoder already appends `</s>`.
- Decode: beam 4, repetition_penalty 1.1, no_repeat_ngram_size 4, max_decoding_length 256.
- Windows OCR on this PC: only ar-SA, en-US. he-IL NOT available. Hebrew OCR needs Tesseract+heb.traineddata (download, needs permission) or he-IL OCR pack (needs permission). Neither done without approval.
- Direction values: `en-ar`, `he-ar`. Engines lazy-loaded per direction.
- History entries get `sourceLang`, `targetLang`, `method` (serde defaults en/ar/direct => safe migration).

## Stages
- [x] 1 Inspect/back up
- [x] 2 Convert he-ar model; OCR language check
- [x] 3 Rust: settings.direction, history meta, translate (Hebrew splitter), models.rs, ocr lang, lib.rs plumbing, tauri.conf resources
- [x] 4 UI: selector, settings default direction, model status, history meta, popup lang pair (tsc + vite build clean)
- [x] 5a cargo test (20 pass), tsc, vite build, real he-ar/en-ar model tests
- [x] 5b Native GUI tests on installed 1.1.0 (computer-use granted later): see status log. NOT verified: tray right-click menu (Explorer click-tier), uninstall (un.exe access denied), offline test, Hebrew screen OCR (no he-IL pack)
- [x] 6a 7 screenshots captured from installed 1.1.0 app into docs/screenshots (main-dark, main-light, popup, settings, history-favorites, arabic-rtl, hebrew-translation)
- [x] 6b README + CHANGELOG updated to match tested/untested status
- [x] 7 Version 1.1.0, installer built; installed via GUI installer (upgrade from 1.0.0); installed model SHA-256 all OK
- Installer: src-tauri\target\release\bundle\nsis\GlassTranslate Lite_1.1.0_x64-setup.exe (137,571,543 bytes); rollback: ..\GlassTranslate-backup-v1.0.0\installer\
- GT_MODE: no such identifier exists in the project; nothing to verify.
- Stale root `screenshots/` folder = V1 images, unreferenced; left in place.

## Status log
(append below)
- Stage 3 DONE: lib.rs plumbing complete (slots per direction, lazy load, popup_for/cur_dir, set_direction, verify_model, per-model get_engine_status {models[], ocr[]}, reload_engine/reveal_model_dir take direction, translate_text takes optional direction). `cargo test`: 20 passed (real en-ar + he-ar models, SHA verification, migrations).
- API for frontend: get_engine_status -> {models:[{id,label,upstream,license,state,error,files,sizeBytes,modelDir,modelDirDisplay}], ocr:[{direction,available,language,detail}]}; verify_model(direction)->{state,detail,sizeBytes}; set_direction(direction); translate_text(text, direction?); reload_engine(direction); reveal_model_dir(direction). PopupState now has direction/sourceLang/targetLang. History entries have sourceLang/targetLang/method.
- MSIX FINDING: Claude desktop is an MSIX app; my shell's writes to %LOCALAPPDATA% were redirected into Claude's package LocalCache, so my earlier silent "install" never reached the real dir. Fixed by running the real installer via GUI (double-click): upgraded 1.0.0 -> 1.1.0 (old uninstaller, "delete app data" unchecked), installed to %LOCALAPPDATA%\GlassTranslate Lite, app launched, status "Ready - works offline".
- GUI TEST PASS (installed 1.1.0, Hebrew manual): שלום, מה שלומך? -> مرحباً، كيف حالك؟ | אני רוצה ללמוד תכנות. -> أريد أن أتعلم البرمجة | תודה רבה על העזרה. -> شكراً جزيلاً على المساعدة | איפה אפשר למצוא את הקובץ? -> أين يمكن أن نجد الملف؟ | התוכנית לא עובדת במחשב שלי. -> البرنامج لا يعمل على حاسوبي
- Hebrew direction was persisted from earlier session (app started on he-ar); upgrade preserved settings.
- GUI TEST PASS (installed 1.1.0): Hebrew + English clipboard hotkey (HE->AR / EN->AR), Translate Again keeps direction, favorite (blocked with hint while history off; works when on), History/Favorites with mixed pairs, Settings verify files, engine status (en-US OCR ok, he-IL unavailable with clear message), popup opacity, remember position, drag, shortcut rebinding, English manual + screen OCR (Notepad), close-to-tray, tray left-click reopen, Hebrew screen-area flow shows corrected Hebrew OCR-unavailable message, default direction persisted across kill+relaunch, Arabic RTL UI persisted across restart, light theme, 7 screenshots.
- Fix found in testing: popup showed English-pack OCR message for Hebrew direction -> errorMessage(t, kind, direction) now maps to error.ocr_unavailable_he; rebuilt, re-verified.
- Final state restored: English UI, Dark, default direction en-ar, history off, remember position off, test history cleared.
- NOT VERIFIED: tray right-click menu; uninstall (the uninstaller process un.exe was denied computer-use access, so it was cancelled; app still installed); offline test (needs firewall/adapter change, not done); Hebrew screen OCR.
- Cleanup: Desktop\GTL-1.1.0-setup.exe temp copy removed.

## v1.1.2 (Hebrew OCR routing report follow-up)
- Report: Hebrew → Arabic on Discord showed "Windows OCR has no English language pack … he-IL pack is not installed".
- Findings: that text exists only in the 1.1.0 binary. The installed exe was still 1.1.0 (file and process version, running since 6:20 AM) while the `tesseract` folder from the 1.1.1 install was present, i.e. a stale running instance / partial upgrade. 1.1.1 source routes `he-ar` → bundled Tesseract; direction is read once from settings; capture is a full-monitor grab with nothing app-specific.
- Hardening: backend now emits `ocr_unavailable_he` for `he-ar`; version-free messages; version 1.1.2 so a stale install is visible.
- Previous installer/exes preserved in `Desktop\GlassTranslate-backup-v1.1.1\` (1.1.1 installer 735c0165…, 1.1.1 exe 10136805…, installed 1.1.0 exe 7efe0d9f…).
- Build: `GlassTranslate Lite_1.1.2_x64-setup.exe`, 168,287,857 bytes, SHA-256 a2ed8231c18927c840ae25eab498e9ef4cba5ffb484299afcca8f2c927fe7a75; release exe SHA-256 02c244a2…aa63. `cargo test --release --lib` 24/24, tsc, vite, tauri build OK.
- Installed 1.1.2 via the GUI installer (upgrade, "Uninstall before installing" path). Verified in the GUI: Settings engine status (EN→AR Windows OCR en-US; HE→AR bundled Tesseract), native Hebrew test (Notepad, Ctrl+Alt+G → Arabic translation, no language-pack error), native English regression after switching direction without restart (Windows OCR → Arabic translation).
- Caveat: shell checks still reported FileVersion 1.1.0 / the old hash for the installed exe (MSIX shell virtualization of %LOCALAPPDATA%); GUI behavior is the evidence.
- NOT verified: Discord window itself, tray right-click menu/Quit, uninstaller, network-blocked run.

## v1.1.1 (Hebrew OCR fix)
- Root cause of the 1.1.0 limitation: Windows OCR has no he-IL pack on this PC (only ar-SA, en-US).
- Fix: bundled Tesseract 5.4.0 (UB Mannheim, Apache-2.0) + official heb.traineddata (tessdata_best, SHA-256 dbaa827a...abdae5) + eng.traineddata, in src-tauri/resources/tesseract. Hidden child process, PNG on stdin, TSV on stdout.
- OCR routing: en-ar -> Windows OCR; he-ar -> Tesseract. Preprocessing: greyscale, auto-invert dark UIs, contrast stretch, padding, 1x/2x passes.
- cargo OCR tests on synthetic Hebrew images: 13/18 exact lines, 96.0% char accuracy (small sizes misread some letters).
- Versions bumped to 1.1.1; README, CHANGELOG, THIRD_PARTY_NOTICES updated.
- Build: `GlassTranslate Lite_1.1.1_x64-setup.exe`, 168,306,466 bytes (final rebuild). The generated installer.nsi has 46 `File /a` entries (tesseract.exe, 25 DLLs, both traineddata files, both models) and matching Delete lines.
- Tests run: `cargo test --release --lib` 24 passed / 0 failed (includes the bundle integrity check and Hebrew OCR on rendered images); real HE and EN model translations; `tsc --noEmit`; frontend build.
- Static audit (not a network-blocked run): no HTTP client in the app's source, Cargo.toml or the Windows build graph (`cargo tree -i reqwest` is empty); the frontend has no fetch/XHR/WebSocket; the CSP allows only self, ipc and the local `gtshot` protocol.
- Static tray audit: tray built with left-click -> show main window; menu Open / Select area / Translate clipboard / Settings / Quit; Quit wipes the capture buffer, removes the tray icon and calls `app.exit(0)`.
- Later the user granted computer-use access. Installed 1.1.1 via the GUI (upgrade over 1.1.0, then reinstall of the final build over 1.1.1); models and settings kept. MSIX virtualization means shell reads of %APPDATA%/%LOCALAPPDATA% are stale; verify installed state through the GUI only.
- Live bug found + fixed in ocr.rs: RTL line-final punctuation at line start, dropped sentences on dark UI. Added fix_rtl_line, noise-line drop, 3x pass for small text; 24/24 cargo tests pass. Rebuilt installer 168,306,466 bytes.
- Live installed-app Hebrew OCR (dark Notepad, 150%): all 5 sentences recognised and translated. Quirks: sentence-final full stops missing in Arabic, blank line after first line, mixed-line word order.
- Regression PASS on installed 1.1.1: EN/HE manual, clipboard, EN screen OCR, direction switch, Translate Again, copy translation/original, drag, pin, Esc, opacity, remember position, focus persistence, rebinding, history/favorites, Arabic RTL, themes, tray icon + left-click.
- NOT verified: Discord scenario, tray right-click menu/actions/Quit (Explorer is click-tier), restart persistence via UI (access to the relaunched process was denied), network-blocked run, uninstaller run (static NSIS review only; not run to avoid uninstalling the working install).
- Text size was left at Large by the test; user can set it back to Medium.
