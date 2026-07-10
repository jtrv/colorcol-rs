//! Finds color literals in arbitrary buffer text and delimits their byte extent.
//!
//! Kakoune columns are 1-based BYTE offsets (verified against a live kakoune: on the
//! buffer `ααα#ff0000`, a range-spec at column 7 highlights exactly `#ff0000`). So we
//! never count codepoints — the byte offset from the line start, plus one, is the column.

use crate::Color;

/// CSS functional-notation names we recognise. `color(...)` is deliberately absent:
/// wide-gamut colors cannot be shown honestly in 8-bit sRGB, and in practice every
/// `color(` occurrence turns out to be an unrelated function call, not a color literal.
const FUNCS: [&[u8]; 9] =
    [b"rgb", b"rgba", b"hsl", b"hsla", b"hwb", b"oklch", b"oklab", b"lab", b"lch"];

/// A functional literal longer than this is not a color. Bounds the lookahead so a
/// stray `hsl(` cannot make us scan to end-of-buffer.
const MAX_FUNC_SPAN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorMatch {
    pub line: u32,
    /// 1-based byte column of the literal's first byte.
    pub col: u32,
    /// Full matched span, including any prefix and any alpha nibbles.
    pub byte_len: u32,
    pub color: Color,
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The one place a delimited span becomes a color. `color::parse_color` rejects
/// trailing junk, so a span that isn't exactly a color yields `None`.
fn convert(span: &[u8]) -> Option<Color> {
    let s = std::str::from_utf8(span).ok()?;
    let c = color::parse_color(s).ok()?;
    let [r, g, b, a] = c.to_alpha_color::<color::Srgb>().to_rgba8().to_u8_array();
    Some(Color { r, g, b, a })
}

/// Index of the `)` closing the `(` at `open`, or None if the span is unterminated,
/// too long, contains a newline, or nests another `(` (which means `calc()`/`var()`).
fn find_close(b: &[u8], open: usize) -> Option<usize> {
    let end = (open + MAX_FUNC_SPAN).min(b.len());
    for (i, &c) in b.iter().enumerate().take(end).skip(open + 1) {
        match c {
            b')' => return Some(i),
            b'(' | b'\n' => return None,
            _ => {}
        }
    }
    None
}

pub struct Scanner<'a> {
    b: &'a [u8],
    i: usize,
    line: u32,
    line_start: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Scanner { b, i: 0, line: 1, line_start: 0 }
    }

    fn emit(&self, start: usize, end: usize, color: Color) -> ColorMatch {
        ColorMatch {
            line: self.line,
            col: (start - self.line_start) as u32 + 1,
            byte_len: (end - start) as u32,
            color,
        }
    }
}

impl Iterator for Scanner<'_> {
    type Item = ColorMatch;

    fn next(&mut self) -> Option<ColorMatch> {
        let b = self.b;
        while self.i < b.len() {
            let c = b[self.i];

            if c == b'\n' {
                self.line += 1;
                self.i += 1;
                self.line_start = self.i;
                continue;
            }

            // #rgb, #rgba, #rrggbb, #rrggbbaa
            //
            // Deliberately no left word boundary here, matching Nim: `word#ff0000` matches,
            // and so does the `#abc123` of a URL fragment. The false positives this lets
            // through (URL fragments, the rare Ada base literal) are too rare to justify
            // diverging from the differential oracle. Contrast `rgb:` below, which DOES take
            // a left boundary: `rgb` is a common identifier fragment, `#` is not.
            if c == b'#' {
                let start = self.i;
                let mut j = start + 1;
                while j < b.len() && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
                let hexlen = j - start - 1;
                // Land ON the terminator, never past it: `#fff#000` must yield both.
                self.i = j;
                let bounded = j == b.len() || !b[j].is_ascii_alphanumeric();
                if bounded && matches!(hexlen, 3 | 4 | 6 | 8) {
                    if let Some(color) = convert(&b[start..j]) {
                        return Some(self.emit(start, j, color));
                    }
                }
                continue;
            }

            // An identifier run: `rgb:`, or one of the CSS functions.
            if c.is_ascii_alphabetic() || c == b'_' {
                let start = self.i;
                // A run that continues an earlier identifier is not an anchor:
                // `myhsl(` and `xrgb:ff0000` must not match.
                let continues_ident = start > 0 && is_ident(b[start - 1]);
                let mut e = start;
                while e < b.len() && is_ident(b[e]) {
                    e += 1;
                }
                self.i = e; // always consume the run; guarantees progress
                if continues_ident {
                    continue;
                }
                let ident = &b[start..e];

                // Kakoune's own face syntax: `rgb:RRGGBB`, exactly 6 hex digits.
                // Not CSS, so `color::parse_color` cannot help us here.
                if ident == b"rgb" && e < b.len() && b[e] == b':' {
                    let hs = e + 1;
                    let mut j = hs;
                    while j < b.len() && j < hs + 6 && b[j].is_ascii_hexdigit() {
                        j += 1;
                    }
                    let bounded = j == b.len() || !b[j].is_ascii_alphanumeric();
                    if j == hs + 6 && bounded {
                        let color = Color::from_hex6(&b[hs..j]);
                        self.i = j;
                        return Some(self.emit(start, j, color));
                    }
                    continue;
                }

                // Kakoune's own face syntax: `rgba:RRGGBBAA`, exactly 8 hex digits.
                if ident == b"rgba" && e < b.len() && b[e] == b':' {
                    let hs = e + 1;
                    let mut j = hs;
                    while j < b.len() && j < hs + 8 && b[j].is_ascii_hexdigit() {
                        j += 1;
                    }
                    let bounded = j == b.len() || !b[j].is_ascii_alphanumeric();
                    if j == hs + 8 && bounded {
                        let color = Color::from_hex8(&b[hs..j]);
                        self.i = j;
                        return Some(self.emit(start, j, color));
                    }
                    continue;
                }

                // hsl(...), oklch(...), rgb(...), ...
                let lower = ident.to_ascii_lowercase();
                if e < b.len() && b[e] == b'(' && FUNCS.contains(&lower.as_slice()) {
                    if let Some(close) = find_close(b, e) {
                        if let Some(color) = convert(&b[start..=close]) {
                            self.i = close + 1;
                            return Some(self.emit(start, close + 1, color));
                        }
                    }
                }
                continue;
            }

            self.i += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(s: &str) -> Vec<ColorMatch> {
        Scanner::new(s.as_bytes()).collect()
    }
    fn cols(s: &str) -> Vec<(u32, u32, u32)> {
        scan(s).iter().map(|m| (m.line, m.col, m.byte_len)).collect()
    }
    fn rgba(s: &str) -> Vec<(u8, u8, u8, u8)> {
        scan(s).iter().map(|m| (m.color.r, m.color.g, m.color.b, m.color.a)).collect()
    }

    #[test]
    fn hex_lengths() {
        assert_eq!(rgba("#f00 "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("#ff0000 "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("#ff000080 "), [(255, 0, 0, 128)]);
        assert_eq!(rgba("#f008 "), [(255, 0, 0, 136)]);
        // 5 and 7 hex digits are not colors
        assert_eq!(scan("#abcde "), []);
        assert_eq!(scan("#abcdef0 "), []);
    }

    #[test]
    fn color_at_end_of_buffer_does_not_panic() {
        // The Nim original crashes here with IndexDefect (reads one byte past the end).
        assert_eq!(rgba("#ff0000"), [(255, 0, 0, 255)]);
        assert_eq!(rgba("#fff"), [(255, 255, 255, 255)]);
        // `#ff00` is a valid 4-digit #rgba literal (yellow, alpha 0) — which is exactly
        // why Nim crashes on this input rather than merely mis-parsing it.
        assert_eq!(rgba("x#ff00"), [(255, 255, 0, 0)]);
        assert_eq!(scan(""), []);
        assert_eq!(scan("#ab"), []); // 2 hex is not a color, and must not read past the end
    }

    #[test]
    fn adjacent_literals_all_match() {
        // Nim drops every other one: the terminator is consumed as it closes a match.
        assert_eq!(cols("#fff#000"), [(1, 1, 4), (1, 5, 4)]);
        assert_eq!(cols("#f00#0f0#00f"), [(1, 1, 4), (1, 5, 4), (1, 9, 4)]);
    }

    #[test]
    fn byte_columns_after_multibyte() {
        // ααα is 6 bytes; the '#' is byte 6, so column 7. Kakoune columns are bytes.
        assert_eq!(cols("ααα#ff0000 "), [(1, 7, 7)]);
    }

    #[test]
    fn lines_and_columns() {
        assert_eq!(cols("a\n  #fff\n\n#000 "), [(2, 3, 4), (4, 1, 4)]);
    }

    #[test]
    fn kakoune_face_syntax() {
        assert_eq!(rgba("rgb:ff0000 "), [(255, 0, 0, 255)]);
        assert_eq!(cols("rgb:ff0000 "), [(1, 1, 10)]);
        // exactly 6 hex; Nim wrongly accepts 3/4/8
        assert_eq!(scan("rgb:abc "), []);
        assert_eq!(scan("rgb:deadbeef "), []);
        // must be an identifier start
        assert_eq!(scan("xrgb:ff0000 "), []);
    }

    #[test]
    fn kakoune_rgba_face_syntax() {
        assert_eq!(rgba("rgba:ff000080 "), [(255, 0, 0, 128)]);
        assert_eq!(cols("rgba:ff000080 "), [(1, 1, 13)]);
        // exactly 8 hex; Kakoune's str_to_color has no shorthand branch
        assert_eq!(scan("rgba:abcd "), []);
        assert_eq!(scan("rgba:ff0000 "), []);
        // must be an identifier start
        assert_eq!(scan("xrgba:ff000080 "), []);
        // `rgba(...)` CSS function still works alongside `rgba:` face syntax
        assert_eq!(rgba("rgba(255,0,0,0.5) "), [(255, 0, 0, 128)]);
    }

    #[test]
    fn css_functions() {
        assert_eq!(rgba("hsl(0 100% 50%) "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("hsl(0, 100%, 50%) "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("rgb(255 0 0 / 50%) "), [(255, 0, 0, 128)]);
        assert_eq!(rgba("hwb(0 0% 0%) "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("oklch(0.7 0.1 200) "), [(64, 177, 183, 255)]);
        assert_eq!(cols("hsl(0 100% 50%) "), [(1, 1, 15)]);
    }

    #[test]
    fn lab_and_lch() {
        assert_eq!(rgba("lab(29.568302% 68.28737 -112.02971) "), [(0, 0, 255, 255)]);
        assert_eq!(rgba("lch(29.568302% 131.20145 301.36426) "), [(0, 0, 255, 255)]);
        assert_eq!(rgba("lab(50% 0 0 / 0.5) "), [(119, 119, 119, 128)]);
    }

    /// `#` takes no left word boundary, matching Nim. These are the known false
    /// positives that buys us; changing any of them is a deliberate divergence from
    /// the differential oracle, not a bug fix.
    #[test]
    fn hash_has_no_left_word_boundary() {
        assert_eq!(cols("word#ff0000 "), [(1, 5, 7)]);
        assert_eq!(cols(".cls#abcdef "), [(1, 5, 7)]);
        // URL fragment: a real false positive we accept for parity.
        assert_eq!(cols("example.com/#abc123 "), [(1, 13, 7)]);
        // Ada/VHDL base literal: likewise.
        assert_eq!(cols("2#192422 "), [(1, 2, 7)]);
    }

    #[test]
    fn function_anchors_need_a_boundary() {
        // `rgb(` inside `Color::rgb(...)` is a feature: it previews.
        assert_eq!(rgba("Color::rgb(255, 0, 0) "), [(255, 0, 0, 255)]);
        // but a longer identifier ending in a function name is not an anchor
        assert_eq!(scan("myhsl(0 100% 50%) "), []);
        assert_eq!(scan("_rgb(255 0 0) "), []);
    }

    #[test]
    fn rejects_nested_calls_and_runaway_spans() {
        assert_eq!(scan("hsl(calc(1 + 1) 100% 50%) "), []);
        assert_eq!(scan("rgb(var(--x)) "), []);
        assert_eq!(scan("hsl(0 100% 50%\nnot a color) "), []); // newline inside
        let runaway = format!("hsl({}", "9".repeat(MAX_FUNC_SPAN + 10));
        assert_eq!(scan(&runaway), []);
        assert_eq!(scan("hsl("), []); // unterminated at EOF
    }

    #[test]
    fn not_colors() {
        assert_eq!(scan("#define X 1\n"), []);
        assert_eq!(scan("### heading\n"), []);
        assert_eq!(scan("#fffg "), []); // alphanumeric terminator
        assert_eq!(scan("0xff0000 "), []); // 0x is rejected: mostly masks and addresses
        assert_eq!(scan("tan "), []); // named colors are opt-in, and never anchored
    }

    #[test]
    fn crlf_terminates() {
        assert_eq!(cols("#fff\r\n#000\r\n"), [(1, 1, 4), (2, 1, 4)]);
    }

    #[test]
    fn uppercase() {
        assert_eq!(rgba("#FF0000 "), [(255, 0, 0, 255)]);
        assert_eq!(rgba("HSL(0, 100%, 50%) "), [(255, 0, 0, 255)]);
    }
}
