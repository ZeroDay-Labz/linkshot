//! Minimal RTF support for `RTFEncodedText` (base64 RTF stored in text objects).
//!
//! We support the subset found in capture files: one font, a colour table, paragraph alignment, bold/italic/underline/
//! strike toggles and plain text with `\par`/`\line` breaks. Unknown control words are skipped.

use crate::color::Argb;
use crate::enums::HAlign;
use base64::Engine;

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Argb,
    pub font_family: String,
    pub font_size: f64,
    pub align: HAlign,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self { bold: false, italic: false, underline: false, strike: false, color: Argb::BLACK, font_family: "Arial".into(), font_size: 14.0, align: HAlign::Center }
    }
}

/// A run of text sharing one style.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub runs: Vec<Run>,
    pub align: HAlign,
}
impl RichText {
    pub fn plain(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn from_plain(text: &str, style: &TextStyle) -> Self {
        Self { runs: vec![Run { text: text.to_string(), style: style.clone() }], align: style.align }
    }
}

fn escape_rtf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\par\n"),
            '\r' => {}
            c if (c as u32) < 128 => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
    }
    out
}

/// Builds an RTF document from runs.
pub fn encode_rtf(rt: &RichText) -> String {
    let font = rt.runs.first().map(|r| r.style.font_family.clone()).unwrap_or_else(|| "Arial".into());
    // Colour table: index 1.. are run colours in order of first appearance.
    let mut colors: Vec<Argb> = Vec::new();
    for r in &rt.runs {
        if !colors.contains(&r.style.color) {
            colors.push(r.style.color);
        }
    }
    let mut s = String::new();
    s.push_str("{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0\\fswiss\\fcharset0 ");
    s.push_str(&font);
    s.push_str(";}}\n{\\colortbl;");
    for c in &colors {
        s.push_str(&format!("\\red{}\\green{}\\blue{};", c.r, c.g, c.b));
    }
    s.push_str("}\n\\pard");
    s.push_str(match rt.align { HAlign::Left => "\\ql", HAlign::Center => "\\qc", HAlign::Right => "\\qr" });
    for r in &rt.runs {
        let ci = colors.iter().position(|c| *c == r.style.color).unwrap_or(0) + 1;
        s.push_str(&format!("\\f0\\fs{}\\cf{}", (r.style.font_size * 2.0).round() as i64, ci));
        s.push_str(if r.style.bold { "\\b" } else { "\\b0" });
        s.push_str(if r.style.italic { "\\i" } else { "\\i0" });
        s.push_str(if r.style.underline { "\\ul" } else { "\\ulnone" });
        s.push_str(if r.style.strike { "\\strike" } else { "\\strike0" });
        s.push(' ');
        s.push_str(&escape_rtf(&r.text));
    }
    s.push('}');
    s
}

pub fn encode_plain_to_base64_rtf(text: &str, style: &TextStyle) -> String {
    let rtf = encode_rtf(&RichText::from_plain(text, style));
    base64::engine::general_purpose::STANDARD.encode(rtf.as_bytes())
}

pub fn decode_base64(b64: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Parses RTF into runs. Robust against unknown groups/control words.
pub fn parse_rtf(rtf: &str) -> RichText {
    #[derive(Clone)]
    struct State {
        style: TextStyle,
        skip: bool, // inside a destination we ignore (fonttbl, colortbl, stylesheet…)
    }
    let mut fonts: Vec<(usize, String)> = Vec::new();
    let mut colors: Vec<Argb> = Vec::new();
    let mut cur_color = Argb::BLACK;
    let mut in_colortbl = false;
    let mut in_fonttbl = false;
    let mut font_name = String::new();
    let mut font_idx = 0usize;

    let mut stack: Vec<State> = Vec::new();
    let mut st = State { style: TextStyle::default(), skip: false };
    let mut runs: Vec<Run> = Vec::new();
    let mut text = String::new();
    let mut align = HAlign::Center;
    let mut pending_unicode_skip = 0usize;

    let flush = |runs: &mut Vec<Run>, text: &mut String, style: &TextStyle| {
        if !text.is_empty() {
            if let Some(last) = runs.last_mut() {
                if last.style == *style {
                    last.text.push_str(text);
                    text.clear();
                    return;
                }
            }
            runs.push(Run { text: std::mem::take(text), style: style.clone() });
        }
    };

    let chars: Vec<char> = rtf.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                stack.push(st.clone());
                i += 1;
            }
            '}' => {
                if in_colortbl { in_colortbl = false; }
                if in_fonttbl && stack.len() <= 2 { in_fonttbl = false; }
                if let Some(prev) = stack.pop() {
                    if prev.style != st.style { flush(&mut runs, &mut text, &st.style); }
                    st = prev;
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() { break; }
                let c2 = chars[i];
                if c2.is_ascii_alphabetic() {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_alphabetic() { i += 1; }
                    let word: String = chars[start..i].iter().collect();
                    let mut num: Option<i64> = None;
                    let ns = i;
                    if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
                        num = chars[ns..i].iter().collect::<String>().parse().ok();
                    }
                    if i < chars.len() && chars[i] == ' ' { i += 1; }
                    let on = num.unwrap_or(1) != 0;
                    match word.as_str() {
                        "fonttbl" => in_fonttbl = true,
                        "colortbl" => { in_colortbl = true; cur_color = Argb::BLACK; }
                        "stylesheet" | "info" | "pict" | "expandedcolortbl" | "listtable" => st.skip = true,
                        "red" if in_colortbl => cur_color.r = num.unwrap_or(0) as u8,
                        "green" if in_colortbl => cur_color.g = num.unwrap_or(0) as u8,
                        "blue" if in_colortbl => cur_color.b = num.unwrap_or(0) as u8,
                        "f" if in_fonttbl => { font_idx = num.unwrap_or(0) as usize; font_name.clear(); }
                        "f" => {
                            let n = num.unwrap_or(0) as usize;
                            if let Some((_, name)) = fonts.iter().find(|(i, _)| *i == n) {
                                flush(&mut runs, &mut text, &st.style);
                                st.style.font_family = name.clone();
                            }
                        }
                        "fs" => { flush(&mut runs, &mut text, &st.style); st.style.font_size = num.unwrap_or(24) as f64 / 2.0; }
                        "cf" => {
                            // Index 0 is the "auto" entry (first `;` in the table); we store it as black.
                            let n = num.unwrap_or(0) as usize;
                            if n < colors.len() { flush(&mut runs, &mut text, &st.style); st.style.color = colors[n]; }
                        }
                        "b" => { flush(&mut runs, &mut text, &st.style); st.style.bold = on; }
                        "i" => { flush(&mut runs, &mut text, &st.style); st.style.italic = on; }
                        "ul" => { flush(&mut runs, &mut text, &st.style); st.style.underline = true; }
                        "ulnone" => { flush(&mut runs, &mut text, &st.style); st.style.underline = false; }
                        "strike" => { flush(&mut runs, &mut text, &st.style); st.style.strike = on; }
                        "ql" => align = HAlign::Left,
                        "qc" => align = HAlign::Center,
                        "qr" => align = HAlign::Right,
                        "par" | "line" => if !st.skip && !in_fonttbl && !in_colortbl { text.push('\n') },
                        "tab" => if !st.skip { text.push('\t') },
                        "u" => {
                            if let Some(n) = num {
                                let u = if n < 0 { (n + 65536) as u32 } else { n as u32 };
                                if let Some(ch) = char::from_u32(u) { if !st.skip { text.push(ch); } }
                                pending_unicode_skip = 1;
                            }
                        }
                        "uc" => {}
                        _ => {}
                    }
                } else {
                    // control symbol
                    match c2 {
                        '\\' | '{' | '}' => if !st.skip && !in_fonttbl && !in_colortbl { text.push(c2) },
                        '\'' => {
                            let hex: String = chars.get(i + 1..i + 3).map(|s| s.iter().collect()).unwrap_or_default();
                            if let Ok(b) = u8::from_str_radix(&hex, 16) {
                                if !st.skip && !in_fonttbl && !in_colortbl { text.push(cp1252(b)); }
                            }
                            i += 2;
                        }
                        '~' => text.push('\u{a0}'),
                        '-' | '_' | '*' | ':' => {}
                        '\n' | '\r' => {}
                        _ => {}
                    }
                    i += 1;
                }
            }
            '\n' | '\r' => i += 1,
            ';' if in_colortbl => {
                colors.push(cur_color);
                cur_color = Argb::BLACK;
                i += 1;
            }
            ';' if in_fonttbl => {
                fonts.push((font_idx, font_name.trim().to_string()));
                font_name.clear();
                i += 1;
            }
            _ => {
                if pending_unicode_skip > 0 { pending_unicode_skip -= 1; i += 1; continue; }
                if in_fonttbl { font_name.push(c); }
                else if !in_colortbl && !st.skip { text.push(c); }
                i += 1;
            }
        }
    }
    flush(&mut runs, &mut text, &st.style);
    // Trim a single trailing newline produced by a final \par.
    if let Some(last) = runs.last_mut() {
        if last.text.ends_with('\n') { last.text.pop(); }
    }
    runs.retain(|r| !r.text.is_empty());
    RichText { runs, align }
}

fn cp1252(b: u8) -> char {
    match b {
        0x80 => '€', 0x82 => '‚', 0x83 => 'ƒ', 0x84 => '„', 0x85 => '…', 0x86 => '†', 0x87 => '‡', 0x88 => 'ˆ',
        0x89 => '‰', 0x8A => 'Š', 0x8B => '‹', 0x8C => 'Œ', 0x8E => 'Ž', 0x91 => '‘', 0x92 => '’', 0x93 => '“',
        0x94 => '”', 0x95 => '•', 0x96 => '–', 0x97 => '—', 0x98 => '˜', 0x99 => '™', 0x9A => 'š', 0x9B => '›',
        0x9C => 'œ', 0x9E => 'ž', 0x9F => 'Ÿ',
        _ => b as char,
    }
}

pub fn decode_base64_rtf(b64: &str) -> RichText {
    match decode_base64(b64) {
        Some(rtf) => parse_rtf(&rtf),
        None => RichText::default(),
    }
}

pub fn decode_base64_rtf_to_plain(b64: &str) -> String {
    decode_base64_rtf(b64).plain()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let style = TextStyle { bold: true, color: Argb::rgb(1, 2, 3), font_size: 16.0, ..Default::default() };
        let b64 = encode_plain_to_base64_rtf("Hello {world}\nline 2 — ünïcode", &style);
        let rt = decode_base64_rtf(&b64);
        assert_eq!(rt.plain(), "Hello {world}\nline 2 — ünïcode");
        assert!(rt.runs[0].style.bold);
        assert_eq!(rt.runs[0].style.color, Argb::rgb(1, 2, 3));
        assert_eq!(rt.runs[0].style.font_size, 16.0);
    }

    #[test]
    fn parses_mac_sample() {
        // From the sample ScreenshotExample.snagx (mac): "ABC", Arial 24pt, colour index 2 (black)
        let b64 = "e1xydGYxXGFuc2lcYW5zaWNwZzEyNTJcY29jb2FydGYyNzA5Clxjb2NvYXRleHRzY2FsaW5nMFxjb2NvYXBsYXRmb3JtMHtcZm9udHRibFxmMFxmc3dpc3NcZmNoYXJzZXQwIEFyaWFsO30Ke1xjb2xvcnRibDtccmVkMjU1XGdyZWVuMjU1XGJsdWUyNTU7XHJlZDBcZ3JlZW4wXGJsdWUwO30Ke1wqXGV4cGFuZGVkY29sb3J0Ymw7O1xjc3NyZ2JcYzBcYzBcYzA7fQpccGFyZFx0eDU2MFx0eDExMjBcdHgxNjgwXHR4MjI0MFx0eDI4MDBcdHgzMzYwXHR4MzkyMFx0eDQ0ODBcdHg1MDQwXHR4NTYwMFx0eDYxNjBcdHg2NzIwXHBhcmRpcm5hdHVyYWxccWNccGFydGlnaHRlbmZhY3RvcjAKClxmMFxmczQ4IFxjZjIgQUJDfQ==";
        let rt = decode_base64_rtf(b64);
        assert_eq!(rt.plain(), "ABC");
        assert_eq!(rt.align, HAlign::Center);
        assert_eq!(rt.runs[0].style.font_size, 24.0);
        assert_eq!(rt.runs[0].style.font_family, "Arial");
        assert_eq!(rt.runs[0].style.color, Argb::BLACK);
    }
}
