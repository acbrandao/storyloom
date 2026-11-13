//! Filename templates and rename planning.
//!
//! Template tokens (case-insensitive):
//!
//! | Token | Example | Notes |
//! |---|---|---|
//! | `{capture_date}` | 2026-07-14 | effective capture date (user correction wins); "undated" if unknown |
//! | `{capture_time}` | 153245 | HHMMSS; "000000" never invented — "undated" if unknown |
//! | `{year}` `{month}` `{day}` | 2026 07 14 | |
//! | `{original_name}` | GOPR0042 | the first filename ever recorded (without extension) |
//! | `{name}` | IMG_0001 | current filename without extension |
//! | `{camera}` | iPhone16Pro | model, spaces removed |
//! | `{location}` `{description}` | SandyHook | catalog fields, spaces removed |
//! | `{rating}` | 5 | |
//! | `{type}` | photo | |
//! | `{sequence}` / `{sequence:N}` | 001 | position in the batch (chronological), zero-padded to N (default 3) |
//!
//! The original extension is always kept (case preserved). Characters not allowed in
//! filenames are removed, and separators around empty values are collapsed.

use crate::core::errors::{CuratorError, Result};
use crate::database::now_ms;
use r2d2_sqlite::rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Values available to a template for one file.
#[derive(Debug, Clone, Default)]
pub struct NameContext {
    /// Effective local capture time "YYYY-MM-DDTHH:MM:SS[.fff]".
    pub capture_local: Option<String>,
    pub original_stem: String,
    pub current_stem: String,
    pub camera: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub rating: i64,
    pub media_type: String,
    pub sequence: u64,
}

const KNOWN: &[&str] = &[
    "capture_date", "capture_time", "year", "month", "day", "original_name", "name", "camera", "location", "description", "rating", "type", "sequence",
];

/// Check a template for unknown tokens / unbalanced braces before rendering anything.
pub fn validate_template(t: &str) -> Result<()> {
    if t.trim().is_empty() {
        return Err(CuratorError::invalid("the name template is empty"));
    }
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}').ok_or_else(|| CuratorError::invalid("a “{” in the template is not closed"))? + open;
        let token = rest[open + 1..close].to_ascii_lowercase();
        let base = token.split(':').next().unwrap_or("");
        if !KNOWN.contains(&base) {
            return Err(CuratorError::invalid(format!("unknown field {{{token}}}")));
        }
        if base == "sequence" {
            if let Some(n) = token.split(':').nth(1) {
                n.parse::<u8>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(|| CuratorError::invalid("sequence padding must be 1–9, e.g. {sequence:3}"))?;
            }
        }
        rest = &rest[close + 1..];
    }
    if rest.contains('}') || t.matches('}').count() != t.matches('{').count() {
        return Err(CuratorError::invalid("the template has an unmatched “}”"));
    }
    Ok(())
}

fn compact(s: &str) -> String {
    s.split_whitespace().map(|w| {
        let mut c = w.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }).collect::<String>()
}

/// Render a template into a file *stem* (no extension).
pub fn render(template: &str, ctx: &NameContext) -> Result<String> {
    validate_template(template)?;
    let (date, time) = match ctx.capture_local.as_deref() {
        Some(l) if l.len() >= 19 => (Some(l[0..10].to_string()), Some(l[11..19].replace(':', ""))),
        _ => (None, None),
    };
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}').unwrap() + open;
        let token = rest[open + 1..close].to_ascii_lowercase();
        let mut parts = token.split(':');
        let base = parts.next().unwrap_or("");
        let value = match base {
            "capture_date" => date.clone().unwrap_or_else(|| "undated".into()),
            "capture_time" => time.clone().unwrap_or_default(),
            "year" => date.as_ref().map(|d| d[0..4].to_string()).unwrap_or_default(),
            "month" => date.as_ref().map(|d| d[5..7].to_string()).unwrap_or_default(),
            "day" => date.as_ref().map(|d| d[8..10].to_string()).unwrap_or_default(),
            "original_name" => ctx.original_stem.clone(),
            "name" => ctx.current_stem.clone(),
            "camera" => ctx.camera.as_deref().map(compact).unwrap_or_default(),
            "location" => ctx.location.as_deref().map(compact).unwrap_or_default(),
            "description" => ctx.description.as_deref().map(|d| compact(&d.chars().take(40).collect::<String>())).unwrap_or_default(),
            "rating" => ctx.rating.to_string(),
            "type" => ctx.media_type.clone(),
            "sequence" => {
                let pad: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(3);
                format!("{:0pad$}", ctx.sequence, pad = pad)
            }
            _ => unreachable!("validated"),
        };
        out.push_str(&value);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    let stem = sanitize(&out);
    if stem.is_empty() {
        return Err(CuratorError::invalid("the template produced an empty name"));
    }
    Ok(stem)
}

/// Remove characters that are invalid on macOS/Windows/Linux, collapse repeated separators
/// left behind by empty values, and trim separators at the ends.
pub fn sanitize(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c }).collect();
    let mut out = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        let sep = matches!(c, '_' | '-' | ' ' | '.');
        if sep && out.chars().last().is_some_and(|l| matches!(l, '_' | '-' | ' ' | '.')) {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_matches(|c| matches!(c, '_' | '-' | ' ' | '.')).to_string();
    // Keep names comfortably below the 255-byte limit (extension is appended later).
    let mut end = out.len().min(200);
    while !out.is_char_boundary(end) {
        end -= 1;
    }
    out[..end].to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    /// Leave conflicting files unchanged.
    #[default]
    Skip,
    /// Add "-2", "-3"… to make the name unique.
    AddSuffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanStatus {
    Ready,
    /// The new name equals the current name.
    Unchanged,
    /// Another file already has that name (on disk or earlier in this batch); will be skipped.
    Conflict,
    /// The source file is missing/offline.
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub media_id: i64,
    pub from: String,
    pub to: String,
    pub status: PlanStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub template: String,
    pub items: Vec<PlanItem>,
    pub ready: usize,
    pub conflicts: usize,
    pub unchanged: usize,
    pub missing: usize,
}

/// Load naming contexts for media ids, ordered chronologically (the sequence order).
pub fn load_contexts(conn: &Connection, ids: &[i64]) -> Result<Vec<(i64, PathBuf, NameContext)>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT id, current_path, original_filename, current_filename, COALESCE(user_capture_datetime, capture_datetime),
            camera_model, location, description, rating, media_type
         FROM media WHERE id IN ({ph})
         ORDER BY COALESCE(user_capture_datetime, capture_datetime, sort_datetime), current_filename COLLATE NOCASE, id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(ids.iter()), |r| {
        let stem = |s: String| Path::new(&s).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or(s);
        Ok((
            r.get::<_, i64>(0)?,
            PathBuf::from(r.get::<_, String>(1)?),
            NameContext {
                original_stem: stem(r.get(2)?),
                current_stem: stem(r.get(3)?),
                capture_local: r.get(4)?,
                camera: r.get(5)?,
                location: r.get(6)?,
                description: r.get(7)?,
                rating: r.get(8)?,
                media_type: r.get(9)?,
                sequence: 0,
            },
        ))
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn with_suffix(path: &Path, n: u32) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    path.with_file_name(format!("{stem}-{n}{ext}"))
}

/// Does `target` exist as a *different* file than `source`? (Case-only renames on
/// case-insensitive volumes see the same file, which is fine.)
pub fn occupied_by_other(target: &Path, source: &Path) -> bool {
    match (std::fs::metadata(target), std::fs::metadata(source)) {
        (Ok(t), Ok(s)) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                !(t.ino() == s.ino() && t.dev() == s.dev())
            }
            #[cfg(not(unix))]
            {
                let _ = (t, s);
                target.to_string_lossy().to_lowercase() != source.to_string_lossy().to_lowercase()
            }
        }
        (Ok(_), Err(_)) => true,
        _ => false,
    }
}

/// Preview a batch rename. Reads only; nothing is renamed.
pub fn plan_rename(conn: &Connection, ids: &[i64], template: &str, start: u64, policy: ConflictPolicy) -> Result<RenamePlan> {
    validate_template(template)?;
    let contexts = load_contexts(conn, ids)?;
    let sources: HashSet<PathBuf> = contexts.iter().map(|(_, p, _)| p.clone()).collect();
    let mut claimed: HashMap<String, i64> = HashMap::new(); // lowercase target -> media id
    let mut items = vec![];
    for (i, (id, from, mut ctx)) in contexts.into_iter().enumerate() {
        ctx.sequence = start + i as u64;
        let ext = from.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let stem = render(template, &ctx)?;
        let mut to = from.with_file_name(format!("{stem}{ext}"));
        let mut status = PlanStatus::Ready;
        let mut message = None;
        if !from.exists() {
            status = PlanStatus::Missing;
            message = Some("file is missing or its drive is offline".into());
        } else if to == from {
            status = PlanStatus::Unchanged;
        } else {
            // A target is taken if another batch item already claimed it, or a file that is not
            // being renamed away in this batch occupies it.
            let taken = |p: &Path, claimed: &HashMap<String, i64>| {
                claimed.contains_key(&p.to_string_lossy().to_lowercase()) || (occupied_by_other(p, &from) && !sources.contains(p))
            };
            if taken(&to, &claimed) {
                match policy {
                    ConflictPolicy::Skip => {
                        status = PlanStatus::Conflict;
                        message = Some(format!("“{}” already exists", to.file_name().unwrap_or_default().to_string_lossy()));
                    }
                    ConflictPolicy::AddSuffix => {
                        let base = to.clone();
                        let mut n = 2;
                        while taken(&to, &claimed) {
                            to = with_suffix(&base, n);
                            n += 1;
                        }
                        message = Some("name was taken; a number was added".into());
                    }
                }
            }
        }
        if status == PlanStatus::Ready {
            claimed.insert(to.to_string_lossy().to_lowercase(), id);
        }
        items.push(PlanItem { media_id: id, from: from.to_string_lossy().into(), to: to.to_string_lossy().into(), status, message });
    }
    let count = |s: PlanStatus| items.iter().filter(|i| i.status == s).count();
    Ok(RenamePlan {
        template: template.into(),
        ready: count(PlanStatus::Ready),
        conflicts: count(PlanStatus::Conflict),
        unchanged: count(PlanStatus::Unchanged),
        missing: count(PlanStatus::Missing),
        items,
    })
}

pub fn batch_id(kind: &str) -> String {
    format!("{kind}-{}", now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> NameContext {
        NameContext {
            capture_local: Some("2026-07-14T15:32:45.250".into()),
            original_stem: "GOPR0042".into(),
            current_stem: "GOPR0042".into(),
            camera: Some("HERO12 Black".into()),
            location: Some("Sandy Hook".into()),
            description: Some("windsurf jump".into()),
            rating: 5,
            media_type: "video".into(),
            sequence: 1,
        }
    }

    #[test]
    fn spec_example() {
        let s = render("{capture_date}_{capture_time}_{location}_{description}_{sequence}", &ctx()).unwrap();
        assert_eq!(s, "2026-07-14_153245_SandyHook_WindsurfJump_001");
    }

    #[test]
    fn empty_values_collapse_and_undated_is_explicit() {
        let mut c = ctx();
        c.location = None;
        c.capture_local = None;
        assert_eq!(render("{capture_date}_{location}_{name}", &c).unwrap(), "undated_GOPR0042");
        assert_eq!(render("{year}-{month}_{sequence:5}", &c).unwrap(), "00001");
    }

    #[test]
    fn rejects_bad_templates() {
        assert!(validate_template("{nope}").is_err());
        assert!(validate_template("{name").is_err());
        assert!(validate_template("name}").is_err());
        assert!(validate_template("{sequence:0}").is_err());
        assert!(validate_template("  ").is_err());
        assert!(validate_template("{Capture_Date} trip {SEQUENCE:4}").is_ok());
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize("a/b:c*?"), "a_b_c");
        assert_eq!(sanitize("__x__-__y__"), "x_y");
        assert_eq!(sanitize("..hidden"), "hidden");
    }
}
