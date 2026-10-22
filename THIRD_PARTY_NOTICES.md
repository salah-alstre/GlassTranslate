# Third-party notices

GlassTranslate Lite is MIT-licensed. It bundles or links the following components.

| Component | Use | License |
|---|---|---|
| [Helsinki-NLP/opus-mt-en-ar](https://huggingface.co/Helsinki-NLP/opus-mt-en-ar) (converted to CTranslate2 int8) | English → Arabic translation model, bundled in `model/` | Apache-2.0 (Hugging Face card); upstream Tatoeba-Challenge release lists CC-BY 4.0. Attribution: Language Technology Research Group, University of Helsinki. |
| [Helsinki-NLP/opus-mt-he-ar](https://huggingface.co/Helsinki-NLP/opus-mt-he-ar) (converted locally to CTranslate2 int8) | Hebrew → Arabic translation model (direct, no pivot), bundled in `model-he-ar/` | Apache-2.0 (Hugging Face card); training data (OPUS / Tatoeba-Challenge) is CC-BY 4.0. Attribution: Language Technology Research Group, University of Helsinki. |
| [CTranslate2](https://github.com/OpenNMT/CTranslate2) via [`ct2rs`](https://crates.io/crates/ct2rs) | Inference engine | MIT |
| [SentencePiece](https://github.com/google/sentencepiece) | Tokenisation | Apache-2.0 |
| [Tauri](https://tauri.app) 2 and plugins | Application framework | MIT / Apache-2.0 |
| [`windows`](https://crates.io/crates/windows) crate | Windows.Media.Ocr (OCR provided by the OS language pack) | MIT / Apache-2.0 |
| [Tesseract OCR](https://github.com/tesseract-ocr/tesseract) 5.4.0 (Windows build by [UB Mannheim](https://github.com/UB-Mannheim/tesseract/wiki), `tesseract.exe` + `libtesseract-5.dll`), run as a hidden child process | Hebrew screen OCR, bundled in `tesseract/` | Apache-2.0 (copy shipped as `tesseract/LICENSE-tesseract.txt`) |
| [`heb.traineddata`](https://github.com/tesseract-ocr/tessdata_best) (official `tessdata_best`, SHA-256 `dbaa827a…abdae5`) | Hebrew OCR language data, bundled in `tesseract/tessdata/` | Apache-2.0 (Tesseract project) |
| [`eng.traineddata`](https://github.com/tesseract-ocr/tessdata_fast) (official `tessdata_fast`) | Latin/English language data used alongside Hebrew for mixed Hebrew/English text, bundled in `tesseract/tessdata/` | Apache-2.0 (Tesseract project) |
| [Leptonica](http://www.leptonica.org/) (`libleptonica-6.dll`) and the codec / support DLLs shipped with the UB Mannheim build: libarchive, libb2, libbz2, libdeflate, libexpat, libgif, libiconv, libjbig, libjpeg, libLerc, liblz4, liblzma, libopenjp2, libpng, libsharpyuv, libtiff, libwebp, libwebpmux, libzstd, zlib, OpenSSL (`libcrypto-3`) | Image loading and compression used by Tesseract | Leptonica: BSD-2-Clause; the others: their respective permissive licenses (BSD, MIT, zlib, Apache-2.0, libpng, IJG, libtiff, OpenSSL Apache-2.0) |
| MinGW-w64 runtime (`libgcc_s_seh-1`, `libstdc++-6`, `libwinpthread-1`) | C/C++ runtime for the Tesseract build | GPL-3.0 with GCC Runtime Library Exception / MIT-style (winpthreads) |
| [`xcap`](https://crates.io/crates/xcap) | Screen capture | Apache-2.0 |
| React, Vite, Tailwind CSS, lucide-react | User interface | MIT / ISC |
| [Inter](https://rsms.me/inter/), [Noto Sans Arabic](https://fonts.google.com/noto/specimen/Noto+Sans+Arabic) (via Fontsource) | Fonts, bundled | SIL OFL 1.1 |

English screen OCR is performed by the Windows built-in engine (Windows.Media.Ocr). Hebrew screen OCR is performed
offline by the bundled Tesseract above; no cloud OCR is used. The Tesseract binaries and language data are unmodified
upstream releases (download sources: the UB Mannheim installer for 5.4.0.20240606 and the official `tessdata_best`
repository); the app verifies the size and SHA-256 of `heb.traineddata` before using it.
Run `cargo license` / `npm ls` for the complete transitive dependency list.
