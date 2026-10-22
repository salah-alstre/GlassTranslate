//! Local translation history. Only written while the user has enabled it in Settings.
//! Stored as one JSON file in the app config directory; nothing leaves the machine.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub original: String,
    pub translation: String,
    /// screen | clipboard | manual
    pub source: String,
    /// Unix time in milliseconds.
    pub ts: u64,
    pub favorite: bool,
    /// "en" | "he". Records written before V2 default to English.
    pub source_lang: String,
    /// Always "ar" today.
    pub target_lang: String,
    /// "direct" (single model) | "pivot" (two-stage). Old records default to direct.
    pub method: String,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            id: 0,
            original: String::new(),
            translation: String::new(),
            source: "manual".into(),
            ts: 0,
            favorite: false,
            source_lang: "en".into(),
            target_lang: "ar".into(),
            method: "direct".into(),
        }
    }
}

/// Language pair + method recorded with an entry.
#[derive(Clone, Copy, Debug)]
pub struct Meta<'a> {
    pub source_lang: &'a str,
    pub target_lang: &'a str,
    pub method: &'a str,
}

impl Meta<'static> {
    #[allow(dead_code)]
    pub const EN_AR: Meta<'static> = Meta { source_lang: "en", target_lang: "ar", method: "direct" };
}

pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Entry>>(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &PathBuf, list: &[Entry]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(list).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Add a translation (newest first). An identical original+translation pair replaces the older
/// copy (keeping its favorite flag) so "Translate Again" does not create duplicates.
/// Returns the entry id and whether it is a favorite.
pub fn record(list: &mut Vec<Entry>, original: &str, translation: &str, source: &str, meta: Meta) -> (u64, bool) {
    let mut favorite = false;
    // Ids must never be reused, so take the maximum before the old duplicate is removed.
    let prev_max = list.iter().map(|e| e.id).max().unwrap_or(0);
    if let Some(i) = list.iter().position(|e| {
        e.original == original && e.translation == translation && e.source_lang == meta.source_lang && e.target_lang == meta.target_lang
    }) {
        favorite = list[i].favorite;
        list.remove(i);
    }
    let id = prev_max.max(now_ms()) + 1;
    list.insert(
        0,
        Entry {
            id,
            original: original.to_string(),
            translation: translation.to_string(),
            source: source.to_string(),
            ts: now_ms(),
            favorite,
            source_lang: meta.source_lang.to_string(),
            target_lang: meta.target_lang.to_string(),
            method: meta.method.to_string(),
        },
    );
    // Trim the oldest non-favorites beyond the cap.
    while list.len() > MAX_ENTRIES {
        if let Some(i) = list.iter().rposition(|e| !e.favorite) {
            list.remove(i);
        } else {
            list.pop();
        }
    }
    (id, favorite)
}

pub fn delete(list: &mut Vec<Entry>, id: u64) -> bool {
    let n = list.len();
    list.retain(|e| e.id != id);
    list.len() != n
}

pub fn set_favorite(list: &mut [Entry], id: u64, fav: bool) -> bool {
    match list.iter_mut().find(|e| e.id == id) {
        Some(e) => {
            e.favorite = fav;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_dedupes_and_keeps_favorite() {
        let mut l = Vec::new();
        let (id, _) = record(&mut l, "hello", "مرحبا", "manual", Meta::EN_AR);
        assert!(set_favorite(&mut l, id, true));
        let (id2, fav) = record(&mut l, "hello", "مرحبا", "screen", Meta::EN_AR);
        assert!(fav);
        assert_eq!(l.len(), 1);
        assert_ne!(id, id2);
        assert_eq!(l[0].source, "screen");
    }

    #[test]
    fn old_v1_entries_migrate_to_en_ar() {
        let l: Vec<Entry> = serde_json::from_str(
            r#"[{"id":5,"original":"hi","translation":"مرحبا","source":"screen","ts":9,"favorite":true}]"#,
        )
        .unwrap();
        assert_eq!((l[0].source_lang.as_str(), l[0].target_lang.as_str(), l[0].method.as_str()), ("en", "ar", "direct"));
        assert!(l[0].favorite);
    }

    #[test]
    fn hebrew_records_keep_pair_and_do_not_merge_with_english() {
        let he = Meta { source_lang: "he", target_lang: "ar", method: "direct" };
        let mut l = Vec::new();
        record(&mut l, "שלום", "مرحبا", "manual", he);
        record(&mut l, "שלום", "مرحبا", "manual", Meta::EN_AR);
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].source_lang, "he");
        let json = serde_json::to_string(&l).unwrap();
        let back: Vec<Entry> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
    }

    #[test]
    fn cap_trims_oldest_non_favorites() {
        let mut l = Vec::new();
        let (first, _) = record(&mut l, "keep", "x", "manual", Meta::EN_AR);
        set_favorite(&mut l, first, true);
        for i in 0..(MAX_ENTRIES + 20) {
            record(&mut l, &format!("t{i}"), "y", "manual", Meta::EN_AR);
        }
        assert_eq!(l.len(), MAX_ENTRIES);
        assert!(l.iter().any(|e| e.id == first));
        assert_eq!(l[0].original, format!("t{}", MAX_ENTRIES + 19));
    }

    #[test]
    fn delete_removes_only_target() {
        let mut l = Vec::new();
        let (a, _) = record(&mut l, "a", "1", "manual", Meta::EN_AR);
        record(&mut l, "b", "2", "manual", Meta::EN_AR);
        assert!(delete(&mut l, a));
        assert_eq!(l.len(), 1);
        assert!(!delete(&mut l, a));
    }
}
