//! Differential test against the original Nim implementation, used as an oracle.
//!
//! Run with:
//!     COLORCOL_ORIG=/path/to/nim/colorcol cargo test --test differential
//! Skipped when that variable is unset, so CI without the Nim binary still passes.
//!
//! The corpus avoids every *intentional* divergence, so a mismatch is a real bug.
//! Excluded on purpose:
//!   - adjacency (`#fff#000`)     Nim drops every other literal
//!   - `rgb:abc`, `rgb:deadbeef`  Nim accepts 3/4/8 hex; kakoune requires exactly 6
//!   - `rgba:ff000080`            Nim never scans `rgba:` as input at all, only `rgb:`
//!   - `xrgb:ff0000`              Nim has no left word boundary on `rgb:`
//!   - uppercase hex              Nim echoes input case; we normalise to lowercase
//!   - a color as the final byte  Nim crashes (IndexDefect)
//!   - `background` mode          we compute a contrast foreground; Nim emits `default`
//!   - `#rgba` / `#rrggbbaa`      Nim discards alpha; we emit an `rgba:` face

use std::path::{Path, PathBuf};
use std::process::Command;

// `background` left this list when auto-contrast landed: it now emits a computed
// foreground where Nim emits `default`. Its scanner behaviour is covered transitively
// by `foreground` (same Scanner), and its face logic by unit tests in emit.rs.
const MODES: &[&str] = &["foreground", "append"];

const CORPUS: &[(&str, &str)] = &[
    ("empty", ""),
    ("no_colors", "the quick brown fox\njumped over it\n"),
    ("hex3", "#fff\n"),
    ("hex6", "#ff0000\n"),
    ("kak_face", "rgb:00ff00\n"),
    ("mixed", "a #fff b rgb:123456 c #abcdef d\n"),
    ("multiline", "#f00\n\n  #0f0\nx #00f y\n"),
    ("multibyte_prefix", "héllo #ff0000 wörld\n"),
    ("midword", "word#ff0000 and .cls#abcdef\n"),
    ("crlf", "#fff\r\n#000\r\n"),
    ("negatives", "#define X\n### heading\n#abcde\n#abcdef0\n#fffg\n"),
    ("punctuation_terminators", "#fff. #000- #123, #456; (#789)\n"),
    ("dense", "#111 #222 #333 #444 #555 #666\n"),
    ("many_lines", "#f00\n#0f0\n#00f\n#ff0\n#0ff\n#f0f\n"),
];

fn run(bin: &Path, mode: &str, fixture: &Path) -> (String, bool) {
    let out = Command::new(bin)
        .args([mode, "3", "F", "M", "true"])
        .arg(fixture)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", bin.display()));
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.success())
}

fn oracle() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("COLORCOL_ORIG").ok()?);
    assert!(p.exists(), "COLORCOL_ORIG does not exist: {}", p.display());
    Some(p)
}

fn fixtures(dir: &str) -> PathBuf {
    let d = std::env::temp_dir().join(dir);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn matches_the_nim_oracle() {
    let Some(orig) = oracle() else {
        eprintln!("COLORCOL_ORIG unset — skipping differential test");
        return;
    };
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_colorcol"));
    let dir = fixtures("colorcol-differential");

    let mut compared = 0;
    for (name, content) in CORPUS {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        for mode in MODES {
            let (want, ok_a) = run(&orig, mode, &path);
            let (got, ok_b) = run(&ours, mode, &path);
            assert!(ok_a, "nim oracle failed on {name}/{mode}");
            assert!(ok_b, "colorcol failed on {name}/{mode}");
            assert_eq!(want, got, "mismatch on fixture {name:?} in {mode} mode");
            compared += 1;
        }
    }
    assert_eq!(compared, CORPUS.len() * MODES.len());
}

/// The inputs where we intentionally differ. Each asserts the Nim behaviour we are
/// fixing, so the test fails loudly if a future Nim build ever changes.
#[test]
fn documented_divergences_from_nim() {
    let Some(orig) = oracle() else { return };
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_colorcol"));
    let dir = fixtures("colorcol-divergence");
    let write = |name: &str, s: &str| {
        let p = dir.join(name);
        std::fs::write(&p, s).unwrap();
        p
    };

    // Nim crashes on a color literal as the final bytes of the buffer.
    let p = write("eof", "#ff0000");
    let (_, nim_ok) = run(&orig, "foreground", &p);
    let (got, our_ok) = run(&ours, "foreground", &p);
    assert!(!nim_ok, "expected the Nim oracle to crash on a trailing color literal");
    assert!(our_ok);
    assert!(got.contains("1.1+7|rgb:ff0000"));

    // Nim drops every other literal in a run.
    let p = write("adjacent", "#f00#0f0#00f\n");
    let (nim, _) = run(&orig, "foreground", &p);
    let (got, _) = run(&ours, "foreground", &p);
    assert_eq!(nim.matches("set -add").count(), 2, "nim should find only 2 of 3");
    assert_eq!(got.matches("set -add").count(), 3);

    // Nim previews `rgb:abc` as `aabbcc`; kakoune's own parser would reject it.
    let p = write("short_face", "rgb:abc\n");
    let (nim, _) = run(&orig, "foreground", &p);
    let (got, _) = run(&ours, "foreground", &p);
    assert!(nim.contains("rgb:aabbcc"));
    assert!(!got.contains("set -add"));

    // Nim discards alpha; we preserve it.
    let p = write("alpha", "#ff000080\n");
    let (nim, _) = run(&orig, "foreground", &p);
    let (got, _) = run(&ours, "foreground", &p);
    assert!(nim.contains("rgb:ff0000"), "nim flattens alpha");
    assert!(got.contains("rgba:ff000080"));

    // Nim has no left word boundary on `rgb:`.
    let p = write("glued_face", "xrgb:ff0000\n");
    let (nim, _) = run(&orig, "foreground", &p);
    let (got, _) = run(&ours, "foreground", &p);
    assert!(nim.contains("rgb:ff0000"));
    assert!(!got.contains("set -add"));

    // Nim never scans `rgba:` as input at all, only `rgb:`.
    let p = write("rgba_face", "rgba:ff000080\n");
    let (nim, _) = run(&orig, "foreground", &p);
    let (got, _) = run(&ours, "foreground", &p);
    assert!(!nim.contains("set -add"));
    assert!(got.contains("rgba:ff000080"));
}
