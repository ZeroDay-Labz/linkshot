//! Automatic file naming ("Automatic File Name": `<Prefix><AutoNum>` etc.) and library names.

use crate::presets::ShareAutoNameSettings;
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

/// Context available to the name pattern.
#[derive(Clone, Debug, Default)]
pub struct NameContext {
    pub application: String,
    pub window_title: String,
    pub url: String,
}

/// Expands the naming pattern. `auto_num` is the number to use for `<AutoNum>`.
pub fn expand(pattern: &str, s: &ShareAutoNameSettings, auto_num: u32, now: DateTime<Local>, ctx: &NameContext) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else { out.push_str(&rest[start..]); rest = ""; break; };
        let tag = &rest[start + 1..start + end];
        let value = match tag {
            "Prefix" => s.prefix.clone(),
            "AutoNum" => format!("{:0width$}", auto_num, width = s.digits.max(1) as usize),
            "Date" => now.format("%Y-%m-%d").to_string(),
            "Time" => now.format("%H-%M-%S").to_string(),
            "Year" => now.format("%Y").to_string(),
            "Month" => now.format("%m").to_string(),
            "Day" => now.format("%d").to_string(),
            "Hour" => now.format("%H").to_string(),
            "Minute" => now.format("%M").to_string(),
            "Second" => now.format("%S").to_string(),
            "Application" => sanitize(&ctx.application),
            "WindowTitle" => sanitize(&ctx.window_title),
            "Url" => sanitize(&ctx.url),
            "OriginalFilename" => String::new(),
            other => format!("<{other}>"),
        };
        out.push_str(&value);
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Finds the next free `<AutoNum>` in `dir` for the given pattern and extension.
pub fn next_auto_name(dir: &Path, s: &ShareAutoNameSettings, ext: &str, ctx: &NameContext) -> PathBuf {
    let now = Local::now();
    let mut n = s.start_number;
    loop {
        let name = format!("{}.{}", expand(&s.file_format, s, n, now, ctx), ext.trim_start_matches('.'));
        let p = dir.join(&name);
        if s.overwrite || !p.exists() || !s.file_format.contains("<AutoNum>") {
            return p;
        }
        n += 1;
        if n > 999_999 { return p; }
    }
}

/// Library file name: `YYYY-MM-DD_HH-MM-SS` (+ ` (n)` on collision).
pub fn library_file_name(dir: &Path, when: DateTime<Local>, ext: &str) -> PathBuf {
    let base = when.format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut i = 1;
    while p.exists() {
        p = dir.join(format!("{base} ({i}).{ext}"));
        i += 1;
    }
    p
}

/// Removes characters that are illegal in file names on Windows/Linux.
pub fn sanitize(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c }).collect();
    out = out.trim().trim_end_matches('.').to_string();
    if out.len() > 120 { out.truncate(120); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands() {
        let s = ShareAutoNameSettings::default();
        let now = Local::now();
        assert_eq!(expand("<Prefix><AutoNum>", &s, 7, now, &NameContext::default()), "SNAG-0007");
        assert_eq!(sanitize("a/b:c?"), "a_b_c_");
    }
}
