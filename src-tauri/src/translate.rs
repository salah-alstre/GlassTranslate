//! Offline English -> Arabic and Hebrew -> Arabic translation with CTranslate2 and the
//! Helsinki-NLP opus-mt-en-ar / opus-mt-he-ar models (both direct, one engine per model).

use ct2rs::tokenizers::sentencepiece::Tokenizer as SpTokenizer;
use ct2rs::{Config, TranslationOptions, Translator};
use std::path::Path;

const LANG_TOKEN: &str = ">>ara<<";
const MAX_WORDS_PER_UNIT: usize = 80;

/// SentencePiece tokenizer that prepends the target-language token the model requires.
struct Tok(SpTokenizer);

impl ct2rs::Tokenizer for Tok {
    fn encode(&self, input: &str) -> anyhow::Result<Vec<String>> {
        let mut v = self.0.encode(input)?;
        v.insert(0, LANG_TOKEN.to_string());
        Ok(v)
    }
    fn decode(&self, tokens: Vec<String>) -> anyhow::Result<String> {
        self.0.decode(tokens)
    }
}

pub struct Engine {
    tr: Translator<Tok>,
}

/// CTranslate2 joins file names with '/', which Windows rejects after a `\\?\` verbatim prefix
/// (Tauri's resource_dir and fs::canonicalize both return such paths), so drop the prefix.
fn plain_path(p: &Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

pub fn load(dir: &Path) -> Result<Engine, String> {
    let dir = plain_path(dir);
    let dir = dir.as_path();
    for f in ["model.bin", "source.spm", "target.spm", "shared_vocabulary.json"] {
        if !dir.join(f).exists() {
            return Err(format!("Model file missing: {f}"));
        }
    }
    let tok = SpTokenizer::from_file(dir.join("source.spm"), dir.join("target.spm"))
        .map_err(|e| e.to_string())?;
    let mut cfg = Config::default();
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    cfg.num_threads_per_replica = cpus.clamp(1, 8);
    let tr = Translator::with_tokenizer(dir, Tok(tok), &cfg).map_err(|e| e.to_string())?;
    Ok(Engine { tr })
}

impl Engine {
    /// Translate multi-line text. Line breaks and blank lines are preserved.
    pub fn translate(&self, text: &str) -> Result<String, String> {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut units: Vec<String> = Vec::new();
        // per line: indices of its units (empty for blank lines)
        let mut layout: Vec<Vec<usize>> = Vec::new();
        for line in text.split('\n') {
            let mut idx = Vec::new();
            let line = line.trim();
            if !line.is_empty() {
                for s in split_sentences(line) {
                    for chunk in chunk_words(&s, MAX_WORDS_PER_UNIT) {
                        if chunk.chars().any(|c| c.is_alphanumeric()) {
                            idx.push(units.len());
                            units.push(chunk);
                        } else {
                            // punctuation / symbols only: nothing to translate, keep verbatim
                            idx.push(units.len());
                            units.push(String::new());
                        }
                    }
                }
            }
            layout.push(idx);
        }
        if units.iter().all(|u| u.is_empty()) && !units.is_empty() {
            return Ok(text.trim().to_string());
        }

        let work: Vec<(usize, &String)> =
            units.iter().enumerate().filter(|(_, u)| !u.is_empty()).collect();
        let inputs: Vec<&str> = work.iter().map(|(_, u)| u.as_str()).collect();
        let opts: TranslationOptions<String, String> = TranslationOptions {
            beam_size: 4,
            max_decoding_length: 256,
            repetition_penalty: 1.1,
            no_repeat_ngram_size: 4,
            ..Default::default()
        };
        let mut outputs: Vec<String> = vec![String::new(); units.len()];
        if !inputs.is_empty() {
            let res = self
                .tr
                .translate_batch(&inputs, &opts, None)
                .map_err(|e| e.to_string())?;
            for ((i, _), (t, _)) in work.iter().zip(res.into_iter()) {
                outputs[*i] = t.trim().to_string();
            }
        }
        // symbol-only units are echoed back
        for (i, u) in units.iter().enumerate() {
            if u.is_empty() && outputs[i].is_empty() {
                outputs[i] = String::new();
            }
        }

        let mut out_lines: Vec<String> = Vec::with_capacity(layout.len());
        for idx in &layout {
            let parts: Vec<&str> = idx
                .iter()
                .map(|&i| outputs[i].as_str())
                .filter(|s| !s.is_empty())
                .collect();
            out_lines.push(parts.join(" "));
        }
        Ok(out_lines.join("\n").trim().to_string())
    }
}

const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "etc", "e.g", "i.e", "inc", "ltd",
    "co", "no", "fig", "approx", "dept", "est", "mt", "u.s", "u.k", "a.m", "p.m",
];

fn is_abbrev(before: &str) -> bool {
    let word = before
        .rsplit(|c: char| c.is_whitespace() || c == '(' || c == '"' || c == '\'')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_lowercase();
    if word.is_empty() {
        return false;
    }
    if word.chars().count() == 1 && word.chars().all(|c| c.is_alphabetic()) {
        return true; // initials: "J. Smith"
    }
    ABBREVIATIONS.contains(&word.as_str())
}

/// Hebrew has no upper case, so a Hebrew letter after a full stop also starts a sentence.
fn is_hebrew_letter(c: char) -> bool {
    ('\u{05D0}'..='\u{05EA}').contains(&c)
}

/// Split a single line into sentences.
pub fn split_sentences(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?' | '…' | '؟') {
            // swallow closing punctuation / quotes
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j], '.' | '!' | '?' | '"' | '\'' | ')' | '”' | '’') {
                j += 1;
            }
            let at_end = j >= chars.len();
            let ws_after = !at_end && chars[j].is_whitespace();
            if at_end || ws_after {
                let mut k = j;
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                let next_starts_sentence = k >= chars.len()
                    || chars[k].is_uppercase()
                    || chars[k].is_numeric()
                    || is_hebrew_letter(chars[k])
                    || matches!(chars[k], '"' | '\'' | '“' | '(' | '[' | '¿' | '¡');
                let before: String = chars[start..i].iter().collect();
                let abbr = c == '.' && is_abbrev(&before);
                if next_starts_sentence && !abbr {
                    let s: String = chars[start..j].iter().collect();
                    let s = s.trim();
                    if !s.is_empty() {
                        out.push(s.to_string());
                    }
                    start = k;
                    i = k;
                    continue;
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    if start < chars.len() {
        let s: String = chars[start..].iter().collect();
        let s = s.trim();
        if !s.is_empty() {
            out.push(s.to_string());
        }
    }
    out
}

fn chunk_words(s: &str, max: usize) -> Vec<String> {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() <= max {
        return vec![s.to_string()];
    }
    words.chunks(max).map(|c| c.join(" ")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences() {
        let v = split_sentences("Hello there. How are you? I'm fine! Dr. Smith paid $3.50 today.");
        assert_eq!(
            v,
            vec![
                "Hello there.",
                "How are you?",
                "I'm fine!",
                "Dr. Smith paid $3.50 today."
            ]
        );
    }

    #[test]
    fn keeps_initials_together() {
        let v = split_sentences("J. R. Tolkien wrote it. Yes.");
        assert_eq!(v, vec!["J. R. Tolkien wrote it.", "Yes."]);
    }

    #[test]
    fn splits_hebrew_sentences() {
        let v = split_sentences("שלום, מה שלומך? אני רוצה ללמוד תכנות. תודה רבה! Hello World.");
        assert_eq!(v, vec!["שלום, מה שלומך?", "אני רוצה ללמוד תכנות.", "תודה רבה!", "Hello World."]);
    }

    #[test]
    fn chunks_long_text() {
        let s = vec!["word"; 200].join(" ");
        assert_eq!(chunk_words(&s, 80).len(), 3);
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    /// Runs against the real bundled model (src-tauri/resources/model).
    #[test]
    fn translates_with_real_model() {
        // canonicalize() yields a `\\?\` path on Windows, like Tauri's resource_dir in the installed app
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("model")
            .canonicalize()
            .unwrap();
        let t_load = std::time::Instant::now();
        let eng = load(&dir).expect("model loads");
        println!("load: {:?}", t_load.elapsed());
        let t0 = std::time::Instant::now();
        let out = eng
            .translate("Hello, how are you today?\n\nThe meeting starts at 5 PM. Please bring your laptop.")
            .expect("translates");
        println!("translate: {:?}\n{}", t0.elapsed(), out);
        assert!(out.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)));
        assert!(out.contains("\n\n"));
    }

    fn bundled(dir: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join(dir).canonicalize().unwrap()
    }

    fn has_arabic(s: &str) -> bool {
        s.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
    }

    /// Hebrew -> Arabic with the real direct he-ar model (the five acceptance sentences).
    #[test]
    fn translates_hebrew_with_real_model() {
        let eng = load(&bundled("model-he-ar")).expect("he-ar model loads");
        let sentences = [
            "שלום, מה שלומך?",
            "אני רוצה ללמוד תכנות.",
            "תודה רבה על העזרה.",
            "איפה אפשר למצוא את הקובץ?",
            "התוכנית לא עובדת במחשב שלי.",
        ];
        for s in sentences {
            let out = eng.translate(s).expect("translates");
            println!("{s}  =>  {out}");
            assert!(has_arabic(&out), "no Arabic in output for {s}");
            assert!(!out.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c)), "Hebrew leaked: {out}");
        }
        let multi = eng.translate("שלום, מה שלומך?\n\nתודה רבה על העזרה. איפה אפשר למצוא את הקובץ?").unwrap();
        println!("{multi}");
        assert!(multi.contains("\n\n"));
    }

    /// Both engines can be alive at the same time and keep their own behaviour.
    #[test]
    fn english_still_works_next_to_hebrew() {
        let he = load(&bundled("model-he-ar")).unwrap();
        let en = load(&bundled("model")).unwrap();
        assert!(has_arabic(&en.translate("Thank you very much for your help.").unwrap()));
        assert!(has_arabic(&he.translate("תודה רבה על העזרה.").unwrap()));
    }
}
