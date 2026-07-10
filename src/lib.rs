pub mod emit;
pub mod scan;

/// 8-bit sRGB with alpha. Alpha is retained (Nim discarded it), which is what makes
/// both auto-contrast and alpha compositing possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Six ASCII hex digits. Only used for Kakoune's `rgb:RRGGBB` face syntax,
    /// which is not CSS and so cannot go through `color::parse_color`.
    pub fn from_hex6(h: &[u8]) -> Color {
        Color { r: hex_byte(h, 0), g: hex_byte(h, 2), b: hex_byte(h, 4), a: 255 }
    }

    /// Eight ASCII hex digits. Kakoune's `rgba:RRGGBBAA` face syntax.
    pub fn from_hex8(h: &[u8]) -> Color {
        Color { r: hex_byte(h, 0), g: hex_byte(h, 2), b: hex_byte(h, 4), a: hex_byte(h, 6) }
    }

    /// Blend over `bg` in gamma (sRGB) space, matching how a browser paints
    /// `background-color: rgba(...)`. Physically wrong, but a preview exists to
    /// match what the user will see in a browser. Do not "fix" this to linear light.
    ///
    /// `bg == None` is passthrough. `a == 255` is a no-op against every background,
    /// which is what keeps opaque colors byte-identical to the Nim reference.
    pub fn composite(self, bg: Option<Color>) -> Color {
        let Some(bg) = bg else { return self };
        let a = self.a as f32 / 255.0;
        let mix = |f: u8, b: u8| (f as f32 * a + b as f32 * (1.0 - a)).round() as u8;
        Color { r: mix(self.r, bg.r), g: mix(self.g, bg.g), b: mix(self.b, bg.b), a: 255 }
    }

    /// Relative luminance, WCAG 2.x.
    fn luminance(self) -> f32 {
        let ch = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(self.r) + 0.7152 * ch(self.g) + 0.0722 * ch(self.b)
    }

    /// A legible foreground to draw on top of this color. Without this, `background`
    /// mode leaves fg at `default` and roughly half of all swatches are illegible.
    pub fn contrast_fg(self) -> &'static str {
        if self.luminance() > 0.179 { "rgb:000000" } else { "rgb:ffffff" }
    }

    /// `rgb:RRGGBB`, or `rgba:RRGGBBAA` when alpha survives to the face.
    ///
    /// Note a terminal renders `rgba:` identically to the opaque color — kakoune
    /// parses the alpha byte then drops it when writing the SGR escape. Passthrough
    /// is lossless, not visible. Set `colorcol_alpha_bg` to actually see alpha.
    pub fn face_token(self) -> String {
        if self.a == 255 {
            format!("rgb:{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("rgba:{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => c - b'A' + 10,
    }
}

fn hex_byte(h: &[u8], i: usize) -> u8 {
    nibble(h[i]) << 4 | nibble(h[i + 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    const HALF_RED: Color = Color { r: 255, g: 0, b: 0, a: 128 };
    const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };

    #[test]
    fn opaque_composite_is_a_noop_against_every_background() {
        assert_eq!(RED.composite(Some(BLACK)), RED);
        assert_eq!(RED.composite(Some(WHITE)), RED);
        assert_eq!(RED.composite(None), RED);
    }

    #[test]
    fn transparent_composites_to_the_background() {
        let clear = Color { a: 0, ..RED };
        assert_eq!(clear.composite(Some(BLACK)), BLACK);
        assert_eq!(clear.composite(Some(WHITE)), WHITE);
    }

    #[test]
    fn half_alpha() {
        assert_eq!(HALF_RED.composite(Some(BLACK)), Color { r: 128, g: 0, b: 0, a: 255 });
        assert_eq!(HALF_RED.composite(Some(WHITE)), Color { r: 255, g: 127, b: 127, a: 255 });
    }

    #[test]
    fn contrast_follows_the_composited_color() {
        // Opaque red is bright enough for black text...
        assert_eq!(RED.contrast_fg(), "rgb:000000");
        // ...but half-alpha red over black is dark, and needs white.
        assert_eq!(HALF_RED.composite(Some(BLACK)).contrast_fg(), "rgb:ffffff");
    }

    #[test]
    fn face_tokens() {
        assert_eq!(RED.face_token(), "rgb:ff0000");
        assert_eq!(HALF_RED.face_token(), "rgba:ff000080");
        assert_eq!(HALF_RED.composite(Some(BLACK)).face_token(), "rgb:800000");
    }

    #[test]
    fn from_hex6_is_case_insensitive() {
        assert_eq!(Color::from_hex6(b"ff0000"), RED);
        assert_eq!(Color::from_hex6(b"FF0000"), RED);
        assert_eq!(Color::from_hex6(b"0a0B0c"), Color { r: 10, g: 11, b: 12, a: 255 });
    }

    #[test]
    fn from_hex8_reads_alpha() {
        assert_eq!(Color::from_hex8(b"ff000080"), HALF_RED);
        assert_eq!(Color::from_hex8(b"FF000080"), HALF_RED);
    }
}
