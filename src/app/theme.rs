//! Colours and metrics, approximating Microsoft Project 2016+'s default
//! Gantt Chart style.

use egui::Color32;

pub const SCROLLBAR: f32 = 14.0;
pub const SPLITTER_W: f32 = 6.0;

pub const DEFAULT_TEXT_SIZE: f32 = 13.0;
pub const MIN_TEXT_SIZE: f32 = 9.0;
pub const MAX_TEXT_SIZE: f32 = 28.0;

// Table/chart metrics follow the user's text size (View ▸ Text Size). The
// app sets it at the start of every frame; thread-local so parallel tests
// with different sizes don't interfere.
thread_local! {
    static TEXT_SIZE: std::cell::Cell<f32> = const { std::cell::Cell::new(DEFAULT_TEXT_SIZE) };
}

pub fn set_text_size(px: f32) {
    TEXT_SIZE.with(|t| t.set(px.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE)));
}
/// Font size for table cells and bar text.
pub fn text_size() -> f32 {
    TEXT_SIZE.with(|t| t.get())
}
/// Slightly smaller font (IDs, timescale labels, bar text).
pub fn small_size() -> f32 {
    text_size() - 1.0
}
pub fn row_h() -> f32 {
    (text_size() * 1.7).round()
}
pub fn header_h() -> f32 {
    (text_size() * 3.2).round()
}
pub fn row_header_w() -> f32 {
    (text_size() * 2.8).round()
}
pub fn indent_px() -> f32 {
    (text_size() * 1.08).round()
}

// ---------------------------------------------------------------- themes
//
// The interface palette is chosen per user (View ▸ Theme) and set at the
// start of every frame, thread-local like the text size. Colours stored in
// the project file (bar styles, gridlines …) are never rewritten; in dark
// mode they are adapted at draw time by `paper` and `ink` below, so a file
// looks right in both modes and on other people's machines.

use serde::{Deserialize, Serialize};

/// Accent colour of the interface (title band, headings, selection).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Navy,
    Green,
    Teal,
    Purple,
    Burgundy,
    Graphite,
}

impl Accent {
    pub const ALL: [Accent; 6] = [Accent::Navy, Accent::Green, Accent::Teal, Accent::Purple, Accent::Burgundy, Accent::Graphite];
    pub fn label(self) -> &'static str {
        match self {
            Accent::Navy => "Navy",
            Accent::Green => "Green (Project classic)",
            Accent::Teal => "Teal",
            Accent::Purple => "Purple",
            Accent::Burgundy => "Burgundy",
            Accent::Graphite => "Graphite",
        }
    }
    /// The accent's base colour (its light-mode title band).
    pub fn base(self) -> Color32 {
        match self {
            Accent::Navy => Color32::from_rgb(30, 58, 112),
            Accent::Green => Color32::from_rgb(49, 117, 47),
            Accent::Teal => Color32::from_rgb(0, 106, 114),
            Accent::Purple => Color32::from_rgb(96, 52, 148),
            Accent::Burgundy => Color32::from_rgb(140, 34, 52),
            Accent::Graphite => Color32::from_rgb(62, 68, 78),
        }
    }
}

/// Light, dark, or follow the desktop's colour-scheme preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System];
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::System => "Use system setting",
        }
    }
}

/// Every colour the interface draws with. Fields are semantic, so a new
/// theme is just a new `Palette`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub accent_kind: Accent,
    /// Title/tab strip at the top of the window, and text on it.
    pub band: Color32,
    pub band_hover: Color32,
    pub on_band: Color32,
    /// Interactive accent on the normal background (links, focus, hover).
    pub accent: Color32,
    /// Dialog titles and ribbon glyphs.
    pub heading: Color32,
    /// Fill and outline of checked / hovered ribbon buttons.
    pub tint: Color32,
    pub tint_border: Color32,
    /// Text selection.
    pub selection: Color32,
    /// Sheet and chart background.
    pub bg: Color32,
    /// Ribbon, status bar, side panels.
    pub panel: Color32,
    /// Ribbon body (and the selected tab).
    pub ribbon: Color32,
    pub status_bar: Color32,
    pub header_bg: Color32,
    pub header_hover: Color32,
    pub header_border: Color32,
    pub grid: Color32,
    pub splitter: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub selected_row: Color32,
    pub active_cell: Color32,
    /// Outline glyphs (▾ ▸) in the tables.
    pub glyph: Color32,
    pub manual_text: Color32,
    pub warn: Color32,
    pub note: Color32,
    pub today: Color32,
    pub project_line: Color32,
    /// Overallocated cells in the usage views.
    pub over_bg: Color32,
    /// Resource / task rows in the usage views.
    pub group_row: Color32,
    pub tooltip_bg: Color32,
    pub tooltip_border: Color32,
    /// Drag feedback in the chart (link rubber band, drop outline).
    pub drag: Color32,
    pub timeline_band: Color32,
    pub timeline_border: Color32,
    pub timeline_summary: Color32,
    pub bar_fill: Color32,
    pub bar_border: Color32,
    pub milestone: Color32,
    /// Working-time dialog calendar cells.
    pub cal_working: Color32,
    pub cal_nonworking: Color32,
    pub cal_exception: Color32,
    pub scroll_track: Color32,
    pub scroll_thumb: Color32,
    pub scroll_thumb_hover: Color32,
    /// Window / dialog background and text-field background.
    pub window: Color32,
    pub field: Color32,
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl Palette {
    pub fn new(accent: Accent, dark: bool) -> Palette {
        let base = accent.base();
        let w = Color32::WHITE;
        let k = Color32::BLACK;
        if !dark {
            let bg = Color32::WHITE;
            Palette {
                dark,
                accent_kind: accent,
                band: base,
                band_hover: mix(base, k, 0.28),
                on_band: w,
                accent: base,
                heading: mix(base, k, 0.3),
                tint: mix(base, w, 0.86),
                tint_border: mix(base, w, 0.58),
                selection: mix(base, w, 0.62),
                bg,
                panel: Color32::from_rgb(250, 250, 250),
                ribbon: Color32::WHITE,
                status_bar: Color32::from_rgb(240, 240, 240),
                header_bg: Color32::from_rgb(243, 243, 243),
                header_hover: Color32::from_rgb(226, 226, 226),
                header_border: Color32::from_rgb(198, 198, 198),
                grid: Color32::from_rgb(218, 218, 218),
                splitter: Color32::from_rgb(225, 225, 225),
                text: Color32::from_rgb(38, 38, 38),
                text_dim: Color32::from_rgb(108, 108, 108),
                text_disabled: Color32::from_gray(165),
                selected_row: mix(base, w, 0.83),
                active_cell: base,
                glyph: Color32::from_gray(90),
                manual_text: Color32::from_rgb(30, 90, 100),
                warn: Color32::from_rgb(200, 40, 40),
                note: Color32::from_rgb(200, 170, 60),
                today: Color32::from_rgb(205, 105, 15),
                project_line: Color32::from_rgb(120, 160, 120),
                over_bg: Color32::from_rgb(255, 222, 222),
                group_row: Color32::from_rgb(248, 248, 242),
                tooltip_bg: Color32::from_rgb(255, 255, 240),
                tooltip_border: Color32::from_gray(150),
                drag: Color32::from_rgb(40, 40, 40),
                timeline_band: Color32::from_rgb(232, 238, 246),
                timeline_border: Color32::from_rgb(200, 210, 225),
                timeline_summary: Color32::from_rgb(160, 190, 230),
                bar_fill: Color32::from_rgb(138, 180, 232),
                bar_border: Color32::from_rgb(73, 123, 190),
                milestone: Color32::from_rgb(30, 30, 30),
                cal_working: Color32::WHITE,
                cal_nonworking: Color32::from_rgb(210, 210, 210),
                cal_exception: Color32::from_rgb(160, 200, 240),
                scroll_track: Color32::from_rgb(240, 240, 240),
                scroll_thumb: Color32::from_rgb(190, 190, 190),
                scroll_thumb_hover: Color32::from_rgb(150, 150, 150),
                window: Color32::from_rgb(248, 248, 248),
                field: Color32::WHITE,
            }
        } else {
            let bg = Color32::from_rgb(30, 31, 34);
            Palette {
                dark,
                accent_kind: accent,
                band: mix(base, k, 0.25),
                band_hover: mix(base, k, 0.5),
                on_band: Color32::from_rgb(240, 240, 240),
                accent: mix(base, w, 0.5),
                heading: mix(base, w, 0.6),
                tint: mix(base, bg, 0.62),
                tint_border: mix(base, w, 0.2),
                selection: mix(base, bg, 0.35),
                bg,
                panel: Color32::from_rgb(38, 39, 43),
                ribbon: Color32::from_rgb(43, 44, 49),
                status_bar: Color32::from_rgb(34, 35, 38),
                header_bg: Color32::from_rgb(44, 45, 50),
                header_hover: Color32::from_rgb(60, 62, 68),
                header_border: Color32::from_rgb(72, 74, 80),
                grid: Color32::from_rgb(56, 58, 63),
                splitter: Color32::from_rgb(52, 54, 59),
                text: Color32::from_rgb(226, 227, 230),
                text_dim: Color32::from_rgb(160, 163, 170),
                text_disabled: Color32::from_rgb(100, 103, 110),
                selected_row: mix(base, bg, 0.6),
                active_cell: mix(base, w, 0.45),
                glyph: Color32::from_gray(175),
                manual_text: Color32::from_rgb(120, 200, 205),
                warn: Color32::from_rgb(240, 110, 110),
                note: Color32::from_rgb(220, 190, 90),
                today: Color32::from_rgb(240, 155, 70),
                project_line: Color32::from_rgb(130, 180, 130),
                over_bg: Color32::from_rgb(92, 40, 42),
                group_row: Color32::from_rgb(37, 38, 42),
                tooltip_bg: Color32::from_rgb(52, 52, 46),
                tooltip_border: Color32::from_gray(110),
                drag: Color32::from_rgb(225, 225, 225),
                timeline_band: Color32::from_rgb(40, 48, 62),
                timeline_border: Color32::from_rgb(70, 84, 108),
                timeline_summary: Color32::from_rgb(90, 120, 170),
                bar_fill: Color32::from_rgb(138, 180, 232),
                bar_border: Color32::from_rgb(110, 160, 225),
                milestone: Color32::from_rgb(220, 220, 220),
                cal_working: Color32::from_rgb(52, 54, 58),
                cal_nonworking: Color32::from_rgb(28, 28, 30),
                cal_exception: Color32::from_rgb(50, 90, 140),
                scroll_track: Color32::from_rgb(36, 37, 40),
                scroll_thumb: Color32::from_rgb(82, 84, 90),
                scroll_thumb_hover: Color32::from_rgb(118, 121, 128),
                window: Color32::from_rgb(40, 41, 45),
                field: Color32::from_rgb(24, 25, 27),
            }
        }
    }
}

thread_local! {
    static PALETTE: std::cell::Cell<Palette> = std::cell::Cell::new(Palette::new(Accent::default(), false));
}

/// The palette for this frame.
pub fn pal() -> Palette {
    PALETTE.with(|p| p.get())
}
pub fn set_palette(p: Palette) {
    PALETTE.with(|c| c.set(p));
}

// ---------------------------------------------------------------- colour maths

/// WCAG relative luminance (0 = black, 1 = white).
pub fn rel_luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// WCAG contrast ratio, 1–21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (rel_luminance(a), rel_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn to_hsl(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color32 {
    let l = l.clamp(0.0, 1.0);
    if s <= 1e-6 {
        let v = (l * 255.0).round() as u8;
        return Color32::from_rgb(v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// Mirror a colour's lightness for a dark background, keeping its hue and
/// saturation: white paper becomes the dark background, light greys become
/// slightly-lighter-than-background greys, black becomes near white.
fn flip_lightness(c: Color32, bg: Color32) -> Color32 {
    let (h, s, l) = to_hsl(c);
    let (_, _, lb) = to_hsl(bg);
    let out = from_hsl(h, s, lb + (1.0 - l) * (1.0 - lb));
    // carry the background's own tint, fully for white paper and fading
    // out towards black ink, so white maps exactly onto the background
    let grey = from_hsl(0.0, 0.0, lb);
    let tint = |o: u8, b: u8, g: u8| (o as f32 + (b as f32 - g as f32) * l).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(tint(out.r(), bg.r(), grey.r()), tint(out.g(), bg.g(), grey.g()), tint(out.b(), bg.b(), grey.b()))
}

/// A *surface* colour from the project file (gridlines, nonworking shading,
/// highlight fill) as it should look on the current background.
pub fn paper(c: crate::format::Rgb) -> Color32 {
    let col = Color32::from_rgb(c.0, c.1, c.2);
    let p = pal();
    if p.dark {
        flip_lightness(col, p.bg)
    } else {
        col
    }
}

/// A *content* colour from the project file (bars, link lines, text) drawn
/// directly on the background: kept as is when it already stands out by
/// `min_contrast`, otherwise its lightness is mirrored. Light mode is
/// always unchanged, so printing and screenshots match what was chosen.
pub fn ink_min(c: crate::format::Rgb, min_contrast: f32) -> Color32 {
    let col = Color32::from_rgb(c.0, c.1, c.2);
    let p = pal();
    if !p.dark || contrast(col, p.bg) >= min_contrast {
        return col;
    }
    let flipped = flip_lightness(col, p.bg);
    if contrast(flipped, p.bg) >= min_contrast {
        return flipped;
    }
    // very desaturated mid-tones: push towards white until readable
    let mut t = 0.1;
    let mut out = flipped;
    while contrast(out, p.bg) < min_contrast && t <= 1.0 {
        out = mix(flipped, Color32::WHITE, t);
        t += 0.1;
    }
    out
}
/// Bars and lines: WCAG's 3:1 for graphical objects.
pub fn ink(c: crate::format::Rgb) -> Color32 {
    ink_min(c, 3.0)
}
/// Text: WCAG's 4.5:1.
pub fn ink_text(c: crate::format::Rgb) -> Color32 {
    ink_min(c, 4.5)
}

/// Readable text colour on top of a filled shape (bar labels).
pub fn on_fill(fill: Color32) -> Color32 {
    if contrast(fill, Color32::from_rgb(30, 30, 30)) >= contrast(fill, Color32::WHITE) {
        Color32::from_rgb(30, 30, 30)
    } else {
        Color32::WHITE
    }
}

/// Back to the file's colour type (for code that derives shades from it).
pub fn to_rgb(c: Color32) -> crate::format::Rgb {
    crate::format::Rgb(c.r(), c.g(), c.b())
}

/// One-time setup: fonts and options. Colours are applied by
/// [`apply_palette`] whenever the theme changes.
pub fn apply(ctx: &egui::Context) {
    // Geometric shapes and arrows (▾ ▸ ◀ ▶ ⬅ ➡) exist only in the bundled
    // monospace font; make it the proportional family's last fallback.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        if !list.iter().any(|f| f == "Hack") {
            list.push("Hack".to_owned());
        }
    }
    ctx.set_fonts(fonts);
    // egui's built-in Ctrl+Plus/Minus/0 zoom would collide with Project's
    // Ctrl+Minus (delete row); ProjectFCG handles zoom keys itself.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

/// Configure egui's own widgets (buttons, text fields, windows, menus) to
/// match `p`.
pub fn apply_palette(ctx: &egui::Context, p: &Palette) {
    let theme = if p.dark { egui::Theme::Dark } else { egui::Theme::Light };
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.selection.bg_fill = p.selection;
    v.selection.stroke.color = if p.dark { p.on_band } else { p.heading };
    v.hyperlink_color = p.accent;
    v.window_corner_radius = 4.into();
    v.panel_fill = p.panel;
    v.window_fill = p.window;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.override_text_color = None;
    v.widgets.noninteractive.fg_stroke.color = p.text;
    v.widgets.inactive.fg_stroke.color = p.text;
    v.widgets.hovered.bg_stroke.color = p.accent;
    v.widgets.active.bg_stroke.color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.warn;
    if p.dark {
        v.faint_bg_color = p.header_bg;
        v.window_stroke.color = p.header_border;
    }
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, v);
    ctx.style_mut_of(theme, |s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{BarCategory, Rgb};

    fn all() -> Vec<Palette> {
        let mut v = vec![];
        for a in Accent::ALL {
            for d in [false, true] {
                v.push(Palette::new(a, d));
            }
        }
        v
    }

    fn need(name: &str, p: &Palette, fg: Color32, bg: Color32, min: f32, fails: &mut Vec<String>) {
        let c = contrast(fg, bg);
        if c < min {
            fails.push(format!("{:?}/{}: {name} {c:.2} < {min}", p.accent_kind, if p.dark { "dark" } else { "light" }));
        }
    }

    /// WCAG AA: 4.5:1 for text, 3:1 for graphics and large text, for every
    /// accent in both modes.
    #[test]
    fn every_theme_meets_contrast_targets() {
        let mut f = vec![];
        for p in all() {
            for (n, bg) in [("bg", p.bg), ("panel", p.panel), ("ribbon", p.ribbon), ("header", p.header_bg), ("window", p.window), ("status", p.status_bar), ("selected row", p.selected_row), ("group row", p.group_row)] {
                need(&format!("text on {n}"), &p, p.text, bg, 4.5, &mut f);
            }
            for (n, bg) in [("bg", p.bg), ("panel", p.panel), ("ribbon", p.ribbon), ("window", p.window), ("header", p.header_bg), ("status", p.status_bar)] {
                need(&format!("dim text on {n}"), &p, p.text_dim, bg, 4.5, &mut f);
                need(&format!("warning on {n}"), &p, p.warn, bg, 4.5, &mut f);
            }
            need("tab text on band", &p, p.on_band, p.band, 4.5, &mut f);
            need("tab text on hovered tab", &p, p.on_band, p.band_hover, 4.5, &mut f);
            need("selected tab text", &p, p.heading, p.ribbon, 4.5, &mut f);
            need("dialog title", &p, p.heading, p.window, 4.5, &mut f);
            need("text on checked button", &p, p.text, p.tint, 4.5, &mut f);
            need("glyph on checked button", &p, p.heading, p.tint, 3.0, &mut f);
            need("link", &p, p.accent, p.window, 4.5, &mut f);
            need("link on ribbon", &p, p.accent, p.ribbon, 4.5, &mut f);
            need("active cell outline", &p, p.active_cell, p.bg, 3.0, &mut f);
            need("text in field", &p, p.text, p.field, 4.5, &mut f);
            need("text on overallocated cell", &p, p.text, p.over_bg, 4.5, &mut f);
            need("tooltip", &p, p.text, p.tooltip_bg, 4.5, &mut f);
            need("manual task text", &p, p.manual_text, p.bg, 4.5, &mut f);
            need("today line", &p, p.today, p.bg, 3.0, &mut f);
            need("milestone", &p, p.milestone, p.bg, 3.0, &mut f);
            need("drag feedback", &p, p.drag, p.bg, 3.0, &mut f);
            need("scroll thumb", &p, p.scroll_thumb, p.scroll_track, 1.5, &mut f);
            need("calendar day text", &p, p.text, p.cal_working, 4.5, &mut f);
            need("calendar nonworking text", &p, p.text, p.cal_nonworking, 4.5, &mut f);
            need("calendar exception text", &p, p.text, p.cal_exception, 4.5, &mut f);
            // structure must be visible but quiet
            need("gridlines", &p, p.grid, p.bg, 1.15, &mut f);
            need("selected row band", &p, p.selected_row, p.bg, 1.1, &mut f);
            need("header vs body", &p, p.header_bg, p.bg, 1.05, &mut f);
        }
        assert!(f.is_empty(), "contrast failures:\n{}", f.join("\n"));
    }

    #[test]
    fn accents_are_distinguishable() {
        for d in [false, true] {
            for a in Accent::ALL {
                for b in Accent::ALL {
                    if a != b {
                        let (pa, pb) = (Palette::new(a, d), Palette::new(b, d));
                        let diff = |x: Color32, y: Color32| {
                            (x.r() as i32 - y.r() as i32).abs() + (x.g() as i32 - y.g() as i32).abs() + (x.b() as i32 - y.b() as i32).abs()
                        };
                        assert!(diff(pa.band, pb.band) > 40, "{a:?} vs {b:?} title bands look alike (dark={d})");
                    }
                }
            }
        }
    }

    #[test]
    fn light_mode_never_changes_file_colours() {
        set_palette(Palette::new(Accent::Navy, false));
        for c in [Rgb(0, 0, 0), Rgb(58, 58, 58), Rgb(255, 255, 255), Rgb(236, 236, 236), Rgb(200, 40, 40)] {
            assert_eq!(to_rgb(paper(c)), c);
            assert_eq!(to_rgb(ink(c)), c);
            assert_eq!(to_rgb(ink_text(c)), c);
        }
    }

    #[test]
    fn dark_mode_adapts_file_colours() {
        let p = Palette::new(Accent::Navy, true);
        set_palette(p);
        // white paper becomes the background; light shading stays a shade
        assert_eq!(paper(Rgb(255, 255, 255)), p.bg);
        let nw = paper(Rgb(236, 236, 236));
        assert!(rel_luminance(nw) > rel_luminance(p.bg) && contrast(nw, p.bg) < 1.6, "nonworking shading {nw:?}");
        // every default bar colour stands out against the dark chart
        for cat in BarCategory::ALL {
            if cat == BarCategory::Progress {
                continue;
            }
            let st = cat.default_style();
            assert!(contrast(ink(st.fill), p.bg) >= 3.0, "{cat:?} fill");
            assert!(contrast(ink(st.border), p.bg) >= 3.0, "{cat:?} border");
        }
        // colours that already stand out are left alone (critical red, task blue)
        assert_eq!(to_rgb(ink(Rgb(234, 128, 128))), Rgb(234, 128, 128));
        assert_eq!(to_rgb(ink(Rgb(138, 180, 232))), Rgb(138, 180, 232));
        // a black summary bar becomes light grey, keeping hue (none)
        let s = ink(Rgb(58, 58, 58));
        assert!(s.r() == s.g() && s.g() == s.b() && s.r() > 150, "{s:?}");
        // hue is kept: a dark red stays red
        let r = ink_text(Rgb(120, 0, 0));
        assert!(r.r() > r.g() + 60 && contrast(r, p.bg) >= 4.5, "{r:?}");
        // even mid-grey text becomes readable
        assert!(contrast(ink_text(Rgb(90, 90, 90)), p.bg) >= 4.5);
        set_palette(Palette::new(Accent::Navy, false));
    }

    #[test]
    fn bar_label_colour_follows_fill() {
        assert_eq!(on_fill(Color32::from_rgb(138, 180, 232)), Color32::from_rgb(30, 30, 30));
        assert_eq!(on_fill(Color32::from_rgb(28, 52, 98)), Color32::WHITE);
    }
}
