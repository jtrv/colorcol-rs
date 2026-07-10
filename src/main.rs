use colorcol::emit::{render, Mode, Opts};
use colorcol::Color;

const KAK_SRC: &str = include_str!("colorcol.kak");

/// `""` -> passthrough. `RRGGBB` or `#RRGGBB` -> composite over that color.
///
/// Anything else is a typo in the user's kakrc. It must not abort and must not put
/// junk on stdout: stdout is fed to `evaluate-commands`, so a bad option value would
/// blank every preview in the buffer. Warn into the debug buffer and pass through.
fn parse_bg(s: &str, warnings: &mut String) -> Option<Color> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.is_empty() {
        return None;
    }
    if h.len() == 6 && h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Some(Color::from_hex6(h.as_bytes()));
    }
    warnings.push_str("echo -debug 'colorcol: colorcol_alpha_bg must be RRGGBB, ignoring: ");
    warnings.extend(s.chars().filter(|c| c.is_ascii_graphic() && *c != '\''));
    warnings.push_str("'\n");
    None
}

fn fail(msg: &str) -> ! {
    eprintln!("colorcol: {msg}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();

    let (mode, max_flags, flag_marker, append_marker, color_full, path, alpha_bg) = match a[..] {
        [] => {
            print!("{KAK_SRC}");
            return;
        }
        [m, x, f, ap, cf, p] => (m, x, f, ap, cf, p, ""),
        [m, x, f, ap, cf, p, bg] => (m, x, f, ap, cf, p, bg),
        _ => fail("expected 0, 6, or 7 arguments"),
    };

    let mode = Mode::parse(mode).unwrap_or_else(|| fail(&format!("unknown mode: {mode}")));
    let buf = std::fs::read(path).unwrap_or_else(|e| fail(&format!("cannot read {path}: {e}")));

    let mut out = String::new();
    let alpha_bg = parse_bg(alpha_bg, &mut out);

    let opts = Opts {
        mode,
        // A garbage option value should degrade, not crash the plugin.
        max_flags: max_flags.parse().unwrap_or(3),
        flag_marker,
        append_marker,
        color_full: color_full == "true",
        alpha_bg,
    };
    out.push_str(&render(&opts, &buf));
    print!("{out}");
}
