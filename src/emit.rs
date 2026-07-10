//! Turns matches into kakscript. Everything written here goes straight into
//! `evaluate-commands`, so a single unescaped byte is an editor-level parse error.

use std::fmt::Write as _;

use crate::scan::Scanner;
use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Background,
    Foreground,
    Append,
    Flag,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        Some(match s {
            "background" => Mode::Background,
            "foreground" => Mode::Foreground,
            "append" => Mode::Append,
            "flag" => Mode::Flag,
            _ => return None,
        })
    }

    fn option(self) -> &'static str {
        match self {
            Mode::Background | Mode::Foreground => "colorcol_ranges",
            Mode::Append => "colorcol_replace_ranges",
            Mode::Flag => "colorcol_flags",
        }
    }
}

/// Kakoune single-quoted string: `'` is escaped by doubling it.
///
/// The Nim original interpolates `colorcol_flag_str` and `colorcol_append_str` raw.
/// An apostrophe (append) or a space (flag, emitted bare) produces malformed kakscript,
/// the `set` aborts the whole `%sh{}` block, and *no* previews render at all.
fn push_quoted(out: &mut String, s: &str) {
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push('\'');
        }
        out.push(c);
    }
    out.push('\'');
}

pub struct Opts<'a> {
    pub mode: Mode,
    pub max_flags: usize,
    pub flag_marker: &'a str,
    pub append_marker: &'a str,
    pub color_full: bool,
    pub alpha_bg: Option<Color>,
}

pub fn render(o: &Opts, buf: &[u8]) -> String {
    let mut out = String::new();
    let opt = o.mode.option();
    // Re-stamp the option with the current buffer timestamp before appending.
    let _ = writeln!(out, "unset-option buffer {opt}");
    let _ = writeln!(out, "update-option buffer {opt}");

    match o.mode {
        Mode::Background | Mode::Foreground => {
            for m in Scanner::new(buf) {
                let c = m.color.composite(o.alpha_bg);
                let face = match o.mode {
                    Mode::Background => format!("{},{}", c.contrast_fg(), c.face_token()),
                    _ => c.face_token(),
                };
                let width = if o.color_full { m.byte_len } else { 1 };
                let _ = write!(out, "set -add buffer {opt} ");
                push_quoted(&mut out, &format!("{}.{}+{}|{}", m.line, m.col, width, face));
                out.push('\n');
            }
        }
        Mode::Append => {
            for m in Scanner::new(buf) {
                let c = m.color.composite(o.alpha_bg);
                // Zero-width range immediately after the literal.
                let col = m.col + m.byte_len;
                let spec = format!("{}.{}+0|{{{}}}{}", m.line, col, c.face_token(), o.append_marker);
                let _ = write!(out, "set -add buffer {opt} ");
                push_quoted(&mut out, &spec);
                out.push('\n');
            }
        }
        Mode::Flag => {
            // line-specs: one spec per line, each carrying up to max_flags markers.
            let mut specs: Vec<(u32, String)> = Vec::new();
            for m in Scanner::new(buf) {
                let c = m.color.composite(o.alpha_bg);
                match specs.last_mut() {
                    Some((line, s)) if *line == m.line => {
                        if s.matches('{').count() < o.max_flags {
                            let _ = write!(s, "{{{}}}{}", c.face_token(), o.flag_marker);
                        }
                    }
                    _ => {
                        if o.max_flags > 0 {
                            let mut s = format!("{}|", m.line);
                            let _ = write!(s, "{{{}}}{}", c.face_token(), o.flag_marker);
                            specs.push((m.line, s));
                        }
                    }
                }
            }
            // An empty `set -add` is an arity error; emit nothing when there are no colors.
            if !specs.is_empty() {
                let _ = write!(out, "set -add buffer {opt}");
                for (_, s) in &specs {
                    out.push(' ');
                    push_quoted(&mut out, s);
                }
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(mode: Mode) -> Opts<'static> {
        Opts { mode, max_flags: 3, flag_marker: "F", append_marker: "M", color_full: true, alpha_bg: None }
    }
    fn body(s: &str) -> Vec<String> {
        // drop the two unset/update preamble lines
        s.lines().skip(2).map(str::to_owned).collect()
    }

    #[test]
    fn quoting_escapes_apostrophes() {
        let mut s = String::new();
        push_quoted(&mut s, "it's");
        assert_eq!(s, "'it''s'");
    }

    #[test]
    fn append_marker_with_apostrophe_does_not_break_out() {
        let o = Opts { append_marker: "ab'cd", ..opts(Mode::Append) };
        let out = render(&o, b"#ff0000 ");
        assert_eq!(body(&out), ["set -add buffer colorcol_replace_ranges '1.8+0|{rgb:ff0000}ab''cd'"]);
    }

    #[test]
    fn flag_marker_with_space_stays_one_argument() {
        let o = Opts { flag_marker: "a b", ..opts(Mode::Flag) };
        let out = render(&o, b"#ff0000 ");
        assert_eq!(body(&out), ["set -add buffer colorcol_flags '1|{rgb:ff0000}a b'"]);
    }

    #[test]
    fn flag_mode_emits_nothing_when_there_are_no_colors() {
        let out = render(&opts(Mode::Flag), b"nothing here\n");
        assert_eq!(body(&out), Vec::<String>::new());
        assert!(out.ends_with("update-option buffer colorcol_flags\n"));
    }

    #[test]
    fn flag_mode_groups_by_line_and_caps() {
        let o = Opts { max_flags: 2, ..opts(Mode::Flag) };
        let out = render(&o, b"#f00 #0f0 #00f\n#fff\n");
        assert_eq!(
            body(&out),
            ["set -add buffer colorcol_flags '1|{rgb:ff0000}F{rgb:00ff00}F' '2|{rgb:ffffff}F'"]
        );
    }

    #[test]
    fn background_gets_a_contrast_foreground() {
        let out = render(&opts(Mode::Background), b"#111111 #eeeeee ");
        assert_eq!(
            body(&out),
            [
                "set -add buffer colorcol_ranges '1.1+7|rgb:ffffff,rgb:111111'",
                "set -add buffer colorcol_ranges '1.9+7|rgb:000000,rgb:eeeeee'",
            ]
        );
    }

    #[test]
    fn foreground_is_the_color_itself() {
        let out = render(&opts(Mode::Foreground), b"#ff0000 ");
        assert_eq!(body(&out), ["set -add buffer colorcol_ranges '1.1+7|rgb:ff0000'"]);
    }

    #[test]
    fn color_full_false_highlights_one_byte() {
        let o = Opts { color_full: false, ..opts(Mode::Foreground) };
        let out = render(&o, b"#ff0000 ");
        assert_eq!(body(&out), ["set -add buffer colorcol_ranges '1.1+1|rgb:ff0000'"]);
    }

    #[test]
    fn alpha_passthrough_emits_rgba() {
        let out = render(&opts(Mode::Foreground), b"#ff000080 ");
        assert_eq!(body(&out), ["set -add buffer colorcol_ranges '1.1+9|rgba:ff000080'"]);
    }

    #[test]
    fn alpha_bg_composites_and_drops_alpha() {
        let bg = Some(Color { r: 0, g: 0, b: 0, a: 255 });
        let o = Opts { alpha_bg: bg, ..opts(Mode::Background) };
        let out = render(&o, b"#ff000080 ");
        // 50% red over black -> 800000, which is dark, so the contrast fg flips to white.
        assert_eq!(body(&out), ["set -add buffer colorcol_ranges '1.1+9|rgb:ffffff,rgb:800000'"]);
    }

    #[test]
    fn alpha_bg_leaves_opaque_colors_untouched() {
        let bg = Some(Color { r: 0, g: 0, b: 0, a: 255 });
        let with = render(&Opts { alpha_bg: bg, ..opts(Mode::Foreground) }, b"#ff0000 ");
        let without = render(&opts(Mode::Foreground), b"#ff0000 ");
        assert_eq!(with, without);
    }

    #[test]
    fn append_column_is_one_past_the_literal() {
        let out = render(&opts(Mode::Append), b"ab #f00 ");
        // '#' at col 4, len 4 -> marker at col 8
        assert_eq!(body(&out), ["set -add buffer colorcol_replace_ranges '1.8+0|{rgb:ff0000}M'"]);
    }
}
