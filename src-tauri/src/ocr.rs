//! Local OCR. English goes through the Windows.Media.Ocr engine that ships with Windows 10/11;
//! Hebrew goes through a Tesseract build that is bundled with the app (Windows has no Hebrew OCR
//! pack unless the user installs one). Nothing leaves the machine.

use image::{imageops, GrayImage, ImageFormat, Luma, RgbaImage};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

#[derive(Debug)]
pub enum OcrError {
    /// No OCR recognizer language is installed in Windows.
    Unavailable(String),
    Failed(String),
}

pub struct OcrStatus {
    pub available: bool,
    /// "windows" | "tesseract"
    pub engine: &'static str,
    pub language: String,
    pub detail: String,
}

/// OCR recognizer language needed for a translation direction.
pub fn lang_for(direction: &str) -> &'static str {
    if direction == "he-ar" {
        "he-IL"
    } else {
        "en-US"
    }
}

fn is_hebrew(lang: &str) -> bool {
    lang.starts_with("he")
}

pub fn status(lang: &str) -> OcrStatus {
    if is_hebrew(lang) {
        return match locate_tesseract() {
            Ok(_) => OcrStatus {
                available: true,
                engine: "tesseract",
                language: "heb+eng".into(),
                detail: String::new(),
            },
            Err(e) => OcrStatus { available: false, engine: "tesseract", language: String::new(), detail: e },
        };
    }
    match create_engine() {
        Ok((_, lang)) => OcrStatus { available: true, engine: "windows", language: lang, detail: String::new() },
        Err(e) => OcrStatus { available: false, engine: "windows", language: String::new(), detail: format!("{e:?}") },
    }
}

fn create_engine() -> Result<(OcrEngine, String), OcrError> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    for tag in ["en-US", "en"] {
        if let Ok(lang) = Language::CreateLanguage(&HSTRING::from(tag)) {
            if OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
                if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                    return Ok((engine, tag.to_string()));
                }
            }
        }
    }
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        let tag = engine
            .RecognizerLanguage()
            .and_then(|l| l.LanguageTag())
            .map(|t| t.to_string())
            .unwrap_or_default();
        return Ok((engine, tag));
    }
    Err(OcrError::Unavailable("No Windows OCR language pack is installed".into()))
}

struct Line {
    text: String,
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
}

struct Pass {
    lines: Vec<Line>,
    score: usize,
}

fn luminance_mean(img: &RgbaImage) -> f32 {
    let mut sum = 0u64;
    let mut n = 0u64;
    for px in img.as_raw().chunks_exact(4).step_by(7) {
        sum += (px[0] as u64 * 30 + px[1] as u64 * 59 + px[2] as u64 * 11) / 100;
        n += 1;
    }
    if n == 0 {
        255.0
    } else {
        sum as f32 / n as f32
    }
}

fn recognize_pass(engine: &OcrEngine, img: &RgbaImage) -> Result<Pass, OcrError> {
    let (w, h) = (img.width(), img.height());
    let mut bgra = Vec::with_capacity(img.as_raw().len());
    for px in img.as_raw().chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    let buffer = CryptographicBuffer::CreateFromByteArray(&bgra)
        .map_err(|e| OcrError::Failed(e.to_string()))?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, w as i32, h as i32)
            .map_err(|e| OcrError::Failed(e.to_string()))?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|op| op.join())
        .map_err(|e| OcrError::Failed(e.to_string()))?;
    let mut lines = Vec::new();
    let mut score = 0usize;
    for line in result.Lines().map_err(|e| OcrError::Failed(e.to_string()))? {
        let text = line.Text().map(|t| t.to_string()).unwrap_or_default();
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, 0f32, 0f32);
        if let Ok(words) = line.Words() {
            for word in words {
                if let Ok(rc) = word.BoundingRect() {
                    l = l.min(rc.X);
                    t = t.min(rc.Y);
                    r = r.max(rc.X + rc.Width);
                    b = b.max(rc.Y + rc.Height);
                }
            }
        }
        if l == f32::MAX {
            l = 0.0;
            t = lines.len() as f32 * 20.0;
            r = 100.0;
            b = t + 18.0;
        }
        score += text.chars().filter(|c| c.is_alphanumeric()).count();
        lines.push(Line { text, top: t, bottom: b, left: l, right: r });
    }
    Ok(Pass { lines, score })
}

/// Small crops OCR much better when enlarged; anything above `max_dim` is shrunk to fit.
fn prepare(src: &RgbaImage, max_dim: u32) -> RgbaImage {
    let mut scale = 1.0f32;
    if src.height() < 220 || src.width() < 400 {
        scale = 3.0;
    } else if src.height() < 500 {
        scale = 2.0;
    }
    while scale > 1.0
        && ((src.width() as f32 * scale) as u32 > max_dim
            || (src.height() as f32 * scale) as u32 > max_dim)
    {
        scale -= 0.5;
    }
    if scale > 1.0 {
        imageops::resize(
            src,
            (src.width() as f32 * scale) as u32,
            (src.height() as f32 * scale) as u32,
            imageops::FilterType::CatmullRom,
        )
    } else if src.width() > max_dim || src.height() > max_dim {
        let k = (max_dim as f32 / src.width().max(src.height()) as f32).min(1.0);
        imageops::resize(
            src,
            ((src.width() as f32 * k) as u32).max(1),
            ((src.height() as f32 * k) as u32).max(1),
            imageops::FilterType::CatmullRom,
        )
    } else {
        src.clone()
    }
}

/// Recognise text and return paragraphs (lines of one paragraph are joined by a space).
pub fn recognize(src: &RgbaImage, lang: &str) -> Result<Vec<String>, OcrError> {
    if is_hebrew(lang) {
        return recognize_tesseract(src);
    }
    let (engine, _) = create_engine()?;
    let max_dim = OcrEngine::MaxImageDimension().unwrap_or(10000).min(10000);
    let img = prepare(src, max_dim);

    let mut best = recognize_pass(&engine, &img)?;
    // Light text on a dark background: also try the inverted image and keep the better pass.
    if luminance_mean(&img) < 128.0 {
        let mut inv = img.clone();
        for px in inv.as_mut().chunks_exact_mut(4) {
            px[0] = 255 - px[0];
            px[1] = 255 - px[1];
            px[2] = 255 - px[2];
        }
        if let Ok(p) = recognize_pass(&engine, &inv) {
            if p.score > best.score {
                best = p;
            }
        }
    }
    Ok(group_paragraphs(best.lines))
}

fn group_paragraphs(lines: Vec<Line>) -> Vec<String> {
    if lines.is_empty() {
        return vec![];
    }
    let mut heights: Vec<f32> = lines.iter().map(|l| (l.bottom - l.top).max(1.0)).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med_h = heights[heights.len() / 2];
    let max_w = lines.iter().map(|l| l.right - l.left).fold(0f32, f32::max);

    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<&Line> = None;
    for l in &lines {
        let mut brk = false;
        if let Some(p) = prev {
            let gap = l.top - p.bottom;
            let big_gap = gap > med_h * 0.75;
            let new_column = l.top < p.bottom - med_h * 0.6;
            let prev_short = (p.right - p.left) < max_w * 0.6;
            let prev_ends = p.text.ends_with(|c: char| matches!(c, '.' | '!' | '?' | ':' | '؟'));
            brk = big_gap || new_column || (prev_short && prev_ends);
        }
        if brk && !cur.is_empty() {
            paras.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&l.text);
        prev = Some(l);
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras
}

// ---------------------------------------------------------------------------------------------
// Hebrew: bundled Tesseract (Apache-2.0) with the official `heb` traineddata, run as a hidden
// child process. The screenshot is piped through stdin and the result read from stdout, so no
// screen content touches the disk.
// ---------------------------------------------------------------------------------------------

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TESS_TIMEOUT: Duration = Duration::from_secs(45);
/// Official tessdata_best `heb.traineddata` (tesseract-ocr/tessdata_best, Apache-2.0).
const HEB_SIZE: u64 = 3_704_077;
const HEB_SHA256: &str = "dbaa827aea6bc21215638447f17783a1004987c2d0bf5573d111fee397abdae5";
/// Windows reports a missing or unloadable DLL with these exit codes.
const STATUS_DLL_NOT_FOUND: u32 = 0xC000_0135;
const STATUS_ENTRYPOINT_NOT_FOUND: u32 = 0xC000_0139;
/// Files that must be present for Hebrew OCR (the other runtime DLLs are caught at launch).
const TESS_FILES: [&str; 4] =
    ["tesseract.exe", "libtesseract-5.dll", "libleptonica-6.dll", "tessdata/eng.traineddata"];

static TESS_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();

fn plain(p: &Path) -> PathBuf {
    let s = p.display().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// Folders that may hold the bundled Tesseract (installed resources first).
pub fn init_tesseract_dirs(dirs: Vec<PathBuf>) {
    let _ = TESS_DIRS.set(dirs.iter().map(|d| plain(d)).collect());
}

fn tess_candidates() -> Vec<PathBuf> {
    let mut v = TESS_DIRS.get().cloned().unwrap_or_default();
    if cfg!(debug_assertions) || cfg!(test) {
        v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("tesseract"));
    }
    v
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn verify_tesseract(dir: &Path) -> Result<(), String> {
    for f in TESS_FILES {
        if !dir.join(f).is_file() {
            return Err(format!("Hebrew OCR file missing: {f}"));
        }
    }
    let heb = dir.join("tessdata").join("heb.traineddata");
    let data = std::fs::read(&heb)
        .map_err(|_| "Hebrew OCR file missing: tessdata/heb.traineddata".to_string())?;
    if data.len() as u64 != HEB_SIZE || sha256_hex(&data) != HEB_SHA256 {
        return Err("Hebrew OCR data (heb.traineddata) is damaged".into());
    }
    Ok(())
}

/// First candidate folder that holds an intact Tesseract bundle.
fn locate_tesseract() -> Result<PathBuf, String> {
    let mut problem = String::from("The bundled Hebrew OCR engine (Tesseract) was not found");
    for dir in tess_candidates() {
        if !dir.join("tesseract.exe").is_file() {
            continue;
        }
        match verify_tesseract(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) => problem = e,
        }
    }
    Err(problem)
}

/// Grey, dark-on-light with full contrast and a white margin: what Tesseract's LSTM reads best.
fn tess_input(img: &RgbaImage) -> GrayImage {
    const PAD: u32 = 16;
    let invert = luminance_mean(img) < 128.0;
    let grey: Vec<u8> = img
        .pixels()
        .map(|px| {
            let g = ((px[0] as u32 * 30 + px[1] as u32 * 59 + px[2] as u32 * 11) / 100) as u8;
            if invert { 255 - g } else { g }
        })
        .collect();
    // Stretch the 2nd..98th percentile to 0..255 so a dim dark-mode UI becomes plain black on white.
    let mut hist = [0usize; 256];
    for &g in &grey {
        hist[g as usize] += 1;
    }
    let pct = |frac: f32| {
        let target = (grey.len() as f32 * frac) as usize;
        let mut acc = 0;
        for (v, &c) in hist.iter().enumerate() {
            acc += c;
            if acc > target {
                return v as i32;
            }
        }
        255
    };
    let (lo, hi) = (pct(0.02), pct(0.98));
    let span = (hi - lo).max(1);
    let mut out = GrayImage::from_pixel(img.width() + 2 * PAD, img.height() + 2 * PAD, Luma([255]));
    for (i, &g) in grey.iter().enumerate() {
        let v = (((g as i32 - lo) * 255) / span).clamp(0, 255) as u8;
        out.put_pixel(i as u32 % img.width() + PAD, i as u32 / img.width() + PAD, Luma([v]));
    }
    out
}

fn run_tesseract(dir: &Path, png: Vec<u8>) -> Result<String, OcrError> {
    // The working directory is the Tesseract folder and the tessdata path is relative: tesseract.exe
    // reads its arguments in the ANSI code page, so an install path with Arabic/Hebrew letters
    // (a user name, say) would otherwise break.
    let mut child = Command::new(dir.join("tesseract.exe"))
        .current_dir(dir)
        .args(["stdin", "stdout", "--tessdata-dir", "tessdata", "-l", "heb+eng", "--psm", "6"])
        .args(["-c", "tessedit_create_tsv=1"])
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| OcrError::Unavailable(format!("Could not start the bundled Tesseract: {e}")))?;

    let mut stdin = child.stdin.take().ok_or_else(|| OcrError::Failed("no stdin".into()))?;
    let mut stdout = child.stdout.take().ok_or_else(|| OcrError::Failed("no stdout".into()))?;
    let mut stderr = child.stderr.take().ok_or_else(|| OcrError::Failed("no stderr".into()))?;
    // Feed and drain on their own threads so a full pipe can never stall the child.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&png);
    });
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });

    let deadline = Instant::now() + TESS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OcrError::Failed("Hebrew OCR timed out".into()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(OcrError::Failed(e.to_string())),
        }
    };
    let _ = writer.join();
    let out = out_t.join().unwrap_or_default();
    let err = err_t.join().unwrap_or_default();

    if !status.success() {
        let code = status.code().map(|c| c as u32);
        if matches!(code, Some(STATUS_DLL_NOT_FOUND) | Some(STATUS_ENTRYPOINT_NOT_FOUND)) {
            return Err(OcrError::Unavailable("A Hebrew OCR runtime DLL is missing or damaged".into()));
        }
        let msg: String = String::from_utf8_lossy(&err).chars().take(200).collect();
        return Err(OcrError::Failed(format!("Tesseract exited with {status}: {}", msg.trim())));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Tesseract inserts directional marks around mixed-script runs; they only confuse the translator.
fn clean_word(w: &str) -> String {
    w.chars()
        .filter(|c| {
            !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn is_sentence_punct(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | ',' | ';' | ':' | '׃' | '؟')
}

/// On right-to-left lines Tesseract emits the line-final punctuation (the visually left-most
/// glyph) at the start of the line's text, and sometimes a stray `|` from a UI border. Move the
/// leading punctuation run to the end of mostly-Hebrew lines so sentence splitting works.
fn fix_rtl_line(text: &str) -> String {
    let t = text.trim_matches(|c: char| c == '|' || c.is_whitespace());
    let (heb, lat) = t.chars().fold((0, 0), |(h, l), c| {
        if ('\u{05D0}'..='\u{05EA}').contains(&c) {
            (h + 1, l)
        } else if c.is_ascii_alphabetic() {
            (h, l + 1)
        } else {
            (h, l)
        }
    });
    if heb == 0 || heb < lat {
        return t.to_string();
    }
    let lead: String = t.chars().take_while(|&c| is_sentence_punct(c)).collect();
    if lead.is_empty() {
        return t.to_string();
    }
    let rest = t[lead.len()..].trim();
    if rest.ends_with(is_sentence_punct) {
        return rest.to_string();
    }
    format!("{rest}{lead}")
}

/// Words come out of Tesseract's TSV in logical (reading) order, so joining them per line is enough.
/// Also returns the mean word confidence (0..100) to compare runs.
fn parse_tsv(tsv: &str) -> (Vec<Line>, f32) {
    let mut lines: Vec<(String, Line)> = Vec::new();
    let (mut conf_sum, mut conf_n) = (0f32, 0f32);
    for row in tsv.lines().skip(1) {
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() < 12 || f[0] != "5" {
            continue;
        }
        let text = clean_word(f[11]);
        let conf: f32 = f[10].parse().unwrap_or(-1.0);
        if text.is_empty() || conf < 0.0 {
            continue;
        }
        conf_sum += conf;
        conf_n += 1.0;
        let n = |i: usize| f[i].parse::<f32>().unwrap_or(0.0);
        let (l, t, w, h) = (n(6), n(7), n(8), n(9));
        let key = format!("{}.{}.{}", f[2], f[3], f[4]);
        match lines.iter_mut().find(|(k, _)| *k == key) {
            Some((_, line)) => {
                line.text.push(' ');
                line.text.push_str(&text);
                line.left = line.left.min(l);
                line.top = line.top.min(t);
                line.right = line.right.max(l + w);
                line.bottom = line.bottom.max(t + h);
            }
            None => lines.push((key, Line { text, top: t, bottom: t + h, left: l, right: l + w })),
        }
    }
    let mean = if conf_n > 0.0 { conf_sum / conf_n } else { 0.0 };
    let lines = lines
        .into_iter()
        .map(|(_, mut l)| {
            l.text = fix_rtl_line(&l.text);
            l
        })
        .filter(|l| l.text.chars().any(|c| c.is_alphanumeric()))
        .collect();
    (lines, mean)
}

fn scaled(src: &RgbaImage, scale: f32) -> RgbaImage {
    if (scale - 1.0).abs() < 0.01 {
        return src.clone();
    }
    imageops::resize(
        src,
        ((src.width() as f32 * scale) as u32).max(1),
        ((src.height() as f32 * scale) as u32).max(1),
        imageops::FilterType::CatmullRom,
    )
}

/// One Tesseract run at one scale: the lines and the mean word confidence.
fn tess_pass(dir: &Path, src: &RgbaImage, scale: f32) -> Result<(Vec<Line>, f32), OcrError> {
    let gray = tess_input(&scaled(src, scale));
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(gray)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|e| OcrError::Failed(e.to_string()))?;
    let tsv = run_tesseract(dir, png)?;
    Ok(parse_tsv(&tsv))
}

fn recognize_tesseract(src: &RgbaImage) -> Result<Vec<String>, OcrError> {
    let dir = locate_tesseract().map_err(OcrError::Unavailable)?;
    let scales: Vec<f32> = std::env::var("GT_TESS_SCALES")
        .ok()
        .map(|v| v.split(',').filter_map(|s| s.parse().ok()).collect())
        .unwrap_or_else(|| vec![1.0, 2.0]);
    let mut best: Option<(Vec<Line>, f32)> = None;
    for s in scales {
        let (lines, conf) = tess_pass(&dir, src, s)?;
        if std::env::var("GT_TESS_DEBUG").is_ok() {
            let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
            eprintln!("  scale {s}: conf {conf:.1}: {}", t.join(" | "));
        }
        if best.as_ref().map_or(true, |b| conf > b.1) {
            best = Some((lines, conf));
        }
    }
    // Small UI text: a third, larger pass when the first two were not confident.
    if let Some(b) = &best {
        if b.1 < 80.0 && src.width() <= 1400 && std::env::var("GT_TESS_SCALES").is_err() {
            if let Ok((lines, conf)) = tess_pass(&dir, src, 3.0) {
                if conf > b.1 {
                    best = Some((lines, conf));
                }
            }
        }
    }
    Ok(group_paragraphs(best.map(|b| b.0).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the real Windows OCR engine on PNGs in $GT_OCR_DIR (light.png, dark.png, small.png).
    #[test]
    fn recognises_rendered_text() {
        let Ok(dir) = std::env::var("GT_OCR_DIR") else {
            eprintln!("GT_OCR_DIR not set; skipping");
            return;
        };
        let st = status("en-US");
        println!("ocr available={} lang={} {}", st.available, st.language, st.detail);
        assert!(st.available, "no Windows OCR engine");
        for (name, want) in [
            ("light.png", "meeting starts"),
            ("dark.png", "report before friday"),
            ("small.png", "save changes"),
        ] {
            let img = image::open(std::path::Path::new(&dir).join(name)).unwrap().to_rgba8();
            let paras = recognize(&img, "en-US").expect("ocr ok");
            let joined = paras.join(" | ");
            println!("{name}: {joined}");
            assert!(joined.to_lowercase().contains(want), "{name}: got {joined:?}");
        }
    }

    #[test]
    fn moves_leading_punctuation_to_line_end() {
        assert_eq!(fix_rtl_line("?שלום, מה שלומך"), "שלום, מה שלומך?");
        assert_eq!(fix_rtl_line(".תודה רבה על העזרה"), "תודה רבה על העזרה.");
        assert_eq!(fix_rtl_line("|שלום 2024 תודה Hello"), "שלום 2024 תודה Hello");
        assert_eq!(fix_rtl_line("שלום."), "שלום.");
        assert_eq!(fix_rtl_line(".שלום."), "שלום.");
        assert_eq!(fix_rtl_line("...Hello world"), "...Hello world");
    }

    #[test]
    fn parse_tsv_fixes_rtl_punctuation_and_drops_noise() {
        let hdr = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n";
        let row = |line: u32, n: u32, w: &str| {
            format!("5\t1\t1\t1\t{line}\t{n}\t{}\t{}\t10\t10\t90\t{w}\n", n * 20, line * 20)
        };
        let tsv = format!("{hdr}{}{}{}{}", row(1, 1, "?שלום,"), row(1, 2, "מה"), row(2, 1, "|"), row(3, 1, ".תודה"));
        let (lines, _) = parse_tsv(&tsv);
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t, vec!["שלום, מה?", "תודה."]);
    }

    fn norm(s: &str) -> String {
        s.chars().filter(|c| c.is_alphanumeric()).collect()
    }

    #[test]
    fn tesseract_bundle_is_intact() {
        let st = status("he-IL");
        assert!(st.available, "{}", st.detail);
        assert_eq!(st.engine, "tesseract");
    }

    /// Real bundled Tesseract on rendered Hebrew in $GT_OCR_HE_DIR:`sentences.txt` (one sentence per
    /// line) and `l<n>_12.png`, `d<n>_12.png` (dark UI), `l<n>_20.png`.
    #[test]
    fn recognises_hebrew_sentences() {
        let Ok(dir) = std::env::var("GT_OCR_HE_DIR") else {
            eprintln!("GT_OCR_HE_DIR not set; skipping");
            return;
        };
        let dir = std::path::Path::new(&dir);
        let Ok(text) = std::fs::read_to_string(dir.join("sentences.txt")) else {
            eprintln!("no sentences.txt; skipping");
            return;
        };
        let sentences: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let (mut ok, mut total, mut dist, mut chars) = (0, 0, 0usize, 0usize);
        for (i, want) in sentences.iter().enumerate() {
            for kind in ["l", "d"].iter().flat_map(|k| ["12", "20"].map(move |s| format!("{k}{}_{s}", i + 1))) {
                let Ok(img) = image::open(dir.join(format!("{kind}.png"))) else { continue };
                let t = Instant::now();
                let got = recognize(&img.to_rgba8(), "he-IL").expect("ocr ok").join(" ");
                let hit = norm(&got) == norm(want);
                total += 1;
                ok += hit as i32;
                dist += edit_distance(&norm(&got), &norm(want));
                chars += norm(want).chars().count();
                println!("{kind} {} {}ms: {got}", if hit { "OK  " } else { "DIFF" }, t.elapsed().as_millis());
            }
        }
        let acc = 100.0 - 100.0 * dist as f32 / chars.max(1) as f32;
        println!("exact matches: {ok}/{total}, character accuracy {acc:.1}%");
        assert!(total > 0);
        assert!(acc >= 90.0, "character accuracy only {acc:.1}%");
    }

    fn edit_distance(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut cur = vec![i; b.len() + 1];
            for j in 1..=b.len() {
                let sub = prev[j - 1] + (a[i - 1] != b[j - 1]) as usize;
                cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
            }
            prev = cur;
        }
        prev[b.len()]
    }

    #[test]
    fn hebrew_ocr_fails_cleanly_when_files_are_missing() {
        let tmp = std::env::temp_dir().join("gt_no_tess");
        let _ = std::fs::create_dir_all(&tmp);
        assert!(verify_tesseract(&tmp).is_err());
    }
}
