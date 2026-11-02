//! A small PDF writer: pages of vector drawing (lines, rectangles,
//! polygons) and text in the standard Helvetica fonts, which every PDF
//! viewer has, so nothing is embedded. Enough to print a Gantt chart.
//!
//! Coordinates are in points (1/72 in) from the page's *top-left*, y down,
//! like the screen; they're flipped when written.

use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);
    fn ops(self) -> String {
        format!("{:.3} {:.3} {:.3}", self.0 as f32 / 255.0, self.1 as f32 / 255.0, self.2 as f32 / 255.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    Regular,
    Bold,
}

/// Page sizes in points (portrait).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Paper {
    #[default]
    Letter,
    Legal,
    Tabloid,
    A4,
    A3,
}

impl Paper {
    pub const ALL: [Paper; 5] = [Paper::Letter, Paper::Legal, Paper::Tabloid, Paper::A4, Paper::A3];
    pub fn size(self) -> (f32, f32) {
        match self {
            Paper::Letter => (612.0, 792.0),
            Paper::Legal => (612.0, 1008.0),
            Paper::Tabloid => (792.0, 1224.0),
            Paper::A4 => (595.28, 841.89),
            Paper::A3 => (841.89, 1190.55),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Paper::Letter => "Letter",
            Paper::Legal => "Legal",
            Paper::Tabloid => "Tabloid (11×17)",
            Paper::A4 => "A4",
            Paper::A3 => "A3",
        }
    }
}

/// Helvetica advance widths (per 1000 em) for ASCII 32–126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // space … /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0–9
    278, 278, 584, 584, 584, 556, 1015, // : … @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // A–Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // a–z
    334, 260, 334, 584, // { | } ~
];
/// Helvetica-Bold, same range.
const HELV_B: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, //
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, //
    333, 333, 584, 584, 584, 611, 975, //
    722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, //
    333, 278, 333, 584, 556, 333, //
    556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, //
    389, 280, 389, 584,
];

/// Map a char to WinAnsi (cp1252); `None` if it has no code there.
fn win_ansi(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0x20..=0x7E | 0xA0..=0xFF => Some(u as u8),
        _ => Some(match c {
            '€' => 0x80,
            '‚' => 0x82,
            'ƒ' => 0x83,
            '„' => 0x84,
            '…' => 0x85,
            '†' => 0x86,
            '‡' => 0x87,
            'ˆ' => 0x88,
            '‰' => 0x89,
            'Š' => 0x8A,
            '‹' => 0x8B,
            'Œ' => 0x8C,
            'Ž' => 0x8E,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '˜' => 0x98,
            '™' => 0x99,
            'š' => 0x9A,
            '›' => 0x9B,
            'œ' => 0x9C,
            'ž' => 0x9E,
            'Ÿ' => 0x9F,
            _ => return None,
        }),
    }
}

/// Width of `text` at `size` points.
pub fn text_width(text: &str, font: Font, size: f32) -> f32 {
    let table = match font {
        Font::Regular => &HELV,
        Font::Bold => &HELV_B,
    };
    let units: u32 = text
        .chars()
        .map(|c| match c as u32 {
            0x20..=0x7E => table[(c as u32 - 0x20) as usize] as u32,
            _ => 556,
        })
        .sum();
    units as f32 * size / 1000.0
}

/// Shorten `text` with "…" to fit `max` points.
pub fn fit_text(text: &str, font: Font, size: f32, max: f32) -> String {
    if text_width(text, font, size) <= max {
        return text.to_string();
    }
    let ell = text_width("…", font, size);
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let cw = text_width(c.encode_utf8(&mut [0; 4]), font, size);
        if w + cw + ell > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

/// One page's drawing.
pub struct Page {
    pub w: f32,
    pub h: f32,
    ops: String,
}

impl Page {
    pub fn new(w: f32, h: f32) -> Self {
        Page { w, h, ops: String::new() }
    }

    fn y(&self, y: f32) -> f32 {
        self.h - y
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Rgb) {
        let _ = writeln!(self.ops, "{} rg {:.2} {:.2} {:.2} {:.2} re f", fill.ops(), x, self.y(y + h), w, h);
    }

    pub fn rect_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb, width: f32) {
        let _ = writeln!(self.ops, "{} RG {:.2} w {:.2} {:.2} {:.2} {:.2} re S", color.ops(), width, x, self.y(y + h), w, h);
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb, width: f32) {
        let _ = writeln!(self.ops, "{} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S", color.ops(), width, x0, self.y(y0), x1, self.y(y1));
    }

    /// A polyline (open).
    pub fn path(&mut self, pts: &[(f32, f32)], color: Rgb, width: f32) {
        if pts.len() < 2 {
            return;
        }
        let mut s = format!("{} RG {:.2} w ", color.ops(), width);
        for (k, (x, y)) in pts.iter().enumerate() {
            let _ = write!(s, "{:.2} {:.2} {} ", x, self.y(*y), if k == 0 { "m" } else { "l" });
        }
        s.push_str("S\n");
        self.ops.push_str(&s);
    }

    /// A filled polygon.
    pub fn polygon(&mut self, pts: &[(f32, f32)], fill: Rgb) {
        if pts.len() < 3 {
            return;
        }
        let mut s = format!("{} rg ", fill.ops());
        for (k, (x, y)) in pts.iter().enumerate() {
            let _ = write!(s, "{:.2} {:.2} {} ", x, self.y(*y), if k == 0 { "m" } else { "l" });
        }
        s.push_str("h f\n");
        self.ops.push_str(&s);
    }

    /// Text with its baseline at `y`.
    pub fn text(&mut self, x: f32, y: f32, text: &str, font: Font, size: f32, color: Rgb) {
        let mut bytes = String::new();
        for c in text.chars() {
            let b = win_ansi(c).unwrap_or(b'?');
            match b {
                b'(' | b')' | b'\\' => {
                    bytes.push('\\');
                    bytes.push(b as char);
                }
                0x20..=0x7E => bytes.push(b as char),
                _ => {
                    let _ = write!(bytes, "\\{b:03o}");
                }
            }
        }
        let f = match font {
            Font::Regular => "F1",
            Font::Bold => "F2",
        };
        let _ = writeln!(self.ops, "BT {} rg /{f} {:.2} Tf {:.2} {:.2} Td ({bytes}) Tj ET", color.ops(), size, x, self.y(y));
    }

    /// Text right-aligned to `x`.
    pub fn text_right(&mut self, x: f32, y: f32, text: &str, font: Font, size: f32, color: Rgb) {
        let w = text_width(text, font, size);
        self.text(x - w, y, text, font, size, color);
    }

    /// Clip what follows to a rectangle (until `unclip`).
    pub fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let _ = writeln!(self.ops, "q {:.2} {:.2} {:.2} {:.2} re W n", x, self.y(y + h), w, h);
    }

    pub fn unclip(&mut self) {
        self.ops.push_str("Q\n");
    }
}

/// Assemble pages into a PDF file.
pub fn write(pages: &[Page], title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = vec![];
    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
        n
    };
    // 1 catalog, 2 pages, 3 F1, 4 F2, 5 info, then (content, page) pairs
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|k| format!("{} 0 R", 8 + 2 * k)).collect();
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, format!("<< /Type /Pages /Count {n} /Kids [{}] >>", kids.join(" ")).as_bytes());
    obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
    let esc: String = title.chars().map(|c| if c.is_ascii() && !matches!(c, '(' | ')' | '\\') { c } else { '?' }).collect();
    obj(&mut out, format!("<< /Title ({esc}) /Producer (ProjectFCG {}) >>", env!("CARGO_PKG_VERSION")).as_bytes());
    obj(&mut out, b"<< >>"); // 6: spare (keeps page numbering simple)
    for p in pages {
        let data = miniz_oxide::deflate::compress_to_vec_zlib(p.ops.as_bytes(), 6);
        let mut body = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(&data);
        body.extend_from_slice(b"\nendstream");
        let c = obj(&mut out, &body);
        obj(
            &mut out,
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {c} 0 R >>", p.w, p.h).as_bytes(),
        );
    }
    let xref = out.len();
    let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for o in &offsets {
        let _ = writeln!(x, "{o:010} 00000 n ");
    }
    let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1);
    out.extend_from_slice(x.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_fitting() {
        assert!((text_width("Hello", Font::Regular, 10.0) - 22.78).abs() < 0.01);
        let s = fit_text("A rather long task name", Font::Regular, 8.0, 50.0);
        assert!(s.ends_with('…') && text_width(&s, Font::Regular, 8.0) <= 50.0);
        assert_eq!(fit_text("Short", Font::Regular, 8.0, 50.0), "Short");
    }

    #[test]
    fn structure_is_valid() {
        let mut p = Page::new(612.0, 792.0);
        p.text(10.0, 20.0, "Café (draft) \\ – ok ✓", Font::Bold, 9.0, Rgb::BLACK);
        p.rect(0.0, 0.0, 10.0, 10.0, Rgb(10, 20, 30));
        let pdf = write(&[p, Page::new(612.0, 792.0)], "Plan");
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 2"));
        assert!(s.contains("/Kids [8 0 R 10 0 R]"));
        assert!(s.contains("8 0 obj\n<< /Type /Page") && s.contains("10 0 obj\n<< /Type /Page"));
        // every xref offset points at its object
        let start: usize = s.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let table = &s[start..];
        for (k, line) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(s[off..].starts_with(&format!("{} 0 obj", k + 1)), "object {}", k + 1);
        }
    }
}
