//! Model registry: where each translation model lives, whether it is usable, and integrity checks.
//!
//! Both models ship inside the installer (`resources/model`, `resources/model-he-ar`). A copy in the
//! app-data folder (`<app data>/models/<dir>/`) takes priority, so a repaired or manually supplied
//! model survives updates and uninstall/reinstall. Nothing is downloaded: no hosted CTranslate2
//! build of these checkpoints exists, and the converted files are verified against pinned SHA-256
//! sums instead (see scripts/prepare-model.mjs).

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub const FILES: [&str; 5] = ["model.bin", "config.json", "shared_vocabulary.json", "source.spm", "target.spm"];
/// Files the engine cannot start without.
pub const REQUIRED: [&str; 4] = ["model.bin", "source.spm", "target.spm", "shared_vocabulary.json"];

pub struct Spec {
    /// Translation direction id, also what the frontend sends.
    pub id: &'static str,
    /// Folder name below `resources/` and `<app data>/models/`.
    pub dir: &'static str,
    pub label: &'static str,
    pub upstream: &'static str,
    pub license: &'static str,
    sums: &'static str,
}

pub const SPECS: [Spec; 2] = [
    Spec {
        id: "en-ar",
        dir: "model",
        label: "English → Arabic",
        upstream: "Helsinki-NLP/opus-mt-en-ar",
        license: "Apache-2.0",
        sums: include_str!("../../scripts/model.sha256"),
    },
    Spec {
        id: "he-ar",
        dir: "model-he-ar",
        label: "Hebrew → Arabic",
        upstream: "Helsinki-NLP/opus-mt-he-ar",
        license: "Apache-2.0",
        sums: include_str!("../../scripts/model-he-ar.sha256"),
    },
];

pub fn spec(direction: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == direction)
}

impl Spec {
    fn expected(&self) -> Vec<(String, String)> {
        self.sums
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let hash = it.next()?;
                let name = it.next()?;
                Some((name.to_string(), hash.to_lowercase()))
            })
            .collect()
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// ok | missing | corrupt
    pub state: String,
    pub detail: String,
    pub size_bytes: u64,
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Cheap check used before loading: every required file exists and is not empty.
pub fn quick_check(dir: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    for f in REQUIRED {
        match std::fs::metadata(dir.join(f)) {
            Ok(m) if m.is_file() && m.len() > 0 => total += m.len(),
            Ok(_) => return Err(format!("{f} is empty or not a file")),
            Err(_) => return Err(format!("{f} is missing")),
        }
    }
    Ok(total)
}

/// Full integrity check (reads every file once and compares SHA-256).
pub fn verify(spec: &Spec, dir: &Path) -> Check {
    let size = match quick_check(dir) {
        Ok(s) => s,
        Err(d) => return Check { state: "missing".into(), detail: d, size_bytes: 0 },
    };
    for (name, want) in spec.expected() {
        match sha256_file(&dir.join(&name)) {
            Ok(got) if got == want => {}
            Ok(_) => {
                return Check { state: "corrupt".into(), detail: format!("{name}: checksum mismatch"), size_bytes: size }
            }
            Err(e) => return Check { state: "missing".into(), detail: format!("{name}: {e}"), size_bytes: size },
        }
    }
    Check { state: "ok".into(), detail: String::new(), size_bytes: size }
}

/// Candidate folders in priority order. `app_data` is the per-user `models` folder.
pub fn candidates(spec: &Spec, app_data: Option<&Path>, resource_dir: Option<&Path>) -> Vec<(PathBuf, &'static str)> {
    let mut c: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Ok(p) = std::env::var("GLASSTRANSLATE_MODEL_DIR") {
        // only honoured for the English model (developer override, kept from v1)
        if spec.id == "en-ar" {
            c.push((PathBuf::from(p), "override"));
        }
    }
    if let Some(a) = app_data {
        c.push((a.join(spec.dir), "installed"));
    }
    if let Some(r) = resource_dir {
        c.push((r.join(spec.dir), "bundled"));
        c.push((r.join("resources").join(spec.dir), "bundled"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            c.push((d.join(spec.dir), "bundled"));
        }
    }
    #[cfg(debug_assertions)]
    c.push((Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join(spec.dir), "bundled"));
    c
}

/// First candidate that passes the quick check; also reports why earlier ones were rejected.
pub fn locate(spec: &Spec, app_data: Option<&Path>, resource_dir: Option<&Path>) -> Result<(PathBuf, &'static str), String> {
    let mut why = String::new();
    for (p, kind) in candidates(spec, app_data, resource_dir) {
        if !p.join("model.bin").exists() {
            continue;
        }
        match quick_check(&p) {
            Ok(_) => return Ok((p, kind)),
            Err(e) => why = format!("{}: {e}", p.display()),
        }
    }
    if why.is_empty() {
        Err(format!("The {} model files were not found. Reinstall GlassTranslate Lite.", spec.label))
    } else {
        Err(format!("The {} model is incomplete ({why}). Reinstall GlassTranslate Lite.", spec.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_both_directions_with_all_sums() {
        for s in SPECS.iter() {
            assert_eq!(s.expected().len(), FILES.len(), "{}", s.id);
        }
        assert!(spec("he-ar").is_some() && spec("fr-ar").is_none());
    }

    #[test]
    fn bundled_models_pass_full_verification() {
        for s in SPECS.iter() {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join(s.dir);
            let c = verify(s, &dir);
            assert_eq!(c.state, "ok", "{}: {}", s.id, c.detail);
        }
    }

    #[test]
    fn missing_and_corrupt_models_are_detected() {
        let s = spec("he-ar").unwrap();
        let tmp = std::env::temp_dir().join(format!("gt-model-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert_eq!(verify(s, &tmp).state, "missing");
        for f in REQUIRED {
            std::fs::write(tmp.join(f), b"garbage").unwrap();
        }
        assert_eq!(verify(s, &tmp).state, "corrupt");
        std::fs::write(tmp.join("model.bin"), b"").unwrap();
        assert_eq!(verify(s, &tmp).state, "missing");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
