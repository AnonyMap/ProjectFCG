//! Bar rendering driven by the project's bar styles (Format ▸ Bar Styles).

use super::columns::{self, ColumnId};
use super::theme::*;
use super::ProjectFcgApp;
use crate::format::{BarCategory, BarField, BarStyle, EndShape, Pattern, Rgb};
use chrono::{Datelike, NaiveDateTime};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

pub fn c32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// A bar style as it should be drawn on the current theme's background.
/// Light mode: unchanged. Dark mode: colours that would vanish against the
/// dark chart (black summary bars, dark milestones) have their lightness
/// mirrored. Progress is drawn on top of the task bar, not the
/// background, so it keeps the file's colour.
pub fn adapt_style(mut st: BarStyle, cat: BarCategory) -> BarStyle {
    if !pal().dark || cat == BarCategory::Progress {
        return st;
    }
    st.fill = to_rgb(ink(st.fill));
    st.border = to_rgb(ink(st.border));
    st
}

/// The project's style for `cat`, adapted to the theme.
pub fn style(f: &crate::format::GanttFormat, cat: BarCategory) -> BarStyle {
    adapt_style(f.styles.get(cat), cat)
}

/// Pale version of a fill for the Light and Hatched patterns: towards the
/// chart background (white in light mode, as before).
fn soften(c: Rgb, t: f32) -> Color32 {
    mix(c32(c), pal().bg, t)
}

/// Task Path roles (Format ▸ Task Path) and their highlight colours,
/// matching Project's orange/purple scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRole {
    Predecessor,
    DrivingPredecessor,
    Successor,
    DrivenSuccessor,
}

impl PathRole {
    pub fn color(&self) -> Rgb {
        match self {
            PathRole::Predecessor => Rgb(250, 200, 140),
            PathRole::DrivingPredecessor => Rgb(237, 125, 49),
            PathRole::Successor => Rgb(200, 180, 235),
            PathRole::DrivenSuccessor => Rgb(112, 48, 160),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            PathRole::Predecessor => "Predecessors",
            PathRole::DrivingPredecessor => "Driving Predecessors",
            PathRole::Successor => "Successors",
            PathRole::DrivenSuccessor => "Driven Successors",
        }
    }
}

/// Which bar style draws a task's main bar.
pub fn category_for(app: &ProjectFcgApp, i: usize) -> BarCategory {
    let t = &app.project.tasks[i];
    let f = &app.project.format;
    if t.calc.is_summary {
        BarCategory::Summary
    } else if t.is_milestone() {
        BarCategory::Milestone
    } else if t.mode == crate::model::TaskMode::Manual {
        BarCategory::ManualTask
    } else if f.show_late && t.calc.late {
        BarCategory::LateTask
    } else if f.critical_on() && t.calc.critical {
        BarCategory::CriticalTask
    } else {
        BarCategory::Task
    }
}

/// Effective style for the main bar, after per-task colour and Task Path
/// overrides.
pub fn style_for(app: &ProjectFcgApp, i: usize, role: Option<PathRole>) -> (BarCategory, BarStyle) {
    let t = &app.project.tasks[i];
    let f = &app.project.format;
    let cat = category_for(app, i);
    let mut st = f.styles.get(cat);
    if cat == BarCategory::Milestone && f.critical_on() && t.calc.critical {
        // critical milestones take the critical colour, as in Project
        let c = f.styles.get(BarCategory::CriticalTask);
        st.fill = c.border;
        st.border = c.border;
    }
    if let Some(c) = t.bar_color {
        st.fill = c;
        st.border = if cat == BarCategory::Summary || cat == BarCategory::Milestone { c } else { c.darker(0.35) };
    }
    if let Some(r) = role {
        st.fill = r.color();
        st.border = r.color().darker(0.35);
        if st.pattern == Pattern::Outline {
            st.pattern = Pattern::Solid;
        }
    }
    (cat, adapt_style(st, cat))
}

/// Pixel half-height of a style's bar.
pub fn half_h(app: &ProjectFcgApp, st: &BarStyle) -> f32 {
    (row_h() * st.height * app.project.format.layout.bar_scale / 2.0).clamp(1.0, row_h() * 0.48)
}

/// Draw a bar body between x0..x1 centred on `mid`. Returns the bar rect.
pub fn paint_bar(p: &Painter, x0: f32, x1: f32, mid: f32, hh: f32, st: &BarStyle) -> Rect {
    let r = Rect::from_min_max(pos2(x0, mid - hh), pos2(x1.max(x0 + 1.0), mid + hh));
    let fill = c32(st.fill);
    let border = c32(st.border);
    match st.pattern {
        Pattern::Solid => {
            p.rect_filled(r, 1.0, fill);
            p.rect_stroke(r, 1.0, Stroke::new(1.0, border), StrokeKind::Inside);
        }
        Pattern::Light => {
            p.rect_filled(r, 1.0, soften(st.fill, 0.55));
            p.rect_stroke(r, 1.0, Stroke::new(1.0, border), StrokeKind::Inside);
        }
        Pattern::Hatched => {
            p.rect_filled(r, 1.0, soften(st.fill, 0.6));
            let clip = p.with_clip_rect(r.intersect(p.clip_rect()));
            let step = 5.0;
            let mut x = r.left() - r.height();
            while x < r.right() {
                clip.line_segment(
                    [pos2(x, r.bottom()), pos2(x + r.height(), r.top())],
                    Stroke::new(1.0, fill),
                );
                x += step;
            }
            p.rect_stroke(r, 1.0, Stroke::new(1.0, border), StrokeKind::Inside);
        }
        Pattern::Outline => {
            p.rect_stroke(r, 1.0, Stroke::new(1.5, border), StrokeKind::Inside);
        }
    }
    r
}

/// Draw an end shape at x (start or end of a bar).
pub fn paint_end(p: &Painter, shape: EndShape, x: f32, r: Rect, color: Color32, at_start: bool) {
    let h = r.height().max(4.0);
    let mid = r.center().y;
    match shape {
        EndShape::None => {}
        EndShape::Bracket => {
            let s = Stroke::new(1.6, color);
            p.line_segment([pos2(x, r.top() - 2.5), pos2(x, r.bottom() + 2.5)], s);
            let dx = if at_start { 3.0 } else { -3.0 };
            p.line_segment([pos2(x, r.top() - 2.5), pos2(x + dx, r.top() - 2.5)], s);
            p.line_segment([pos2(x, r.bottom() + 2.5), pos2(x + dx, r.bottom() + 2.5)], s);
        }
        EndShape::TriangleDown => {
            let w = (h * 0.9).max(4.0);
            p.add(Shape::convex_polygon(
                vec![pos2(x - w, r.top()), pos2(x + w, r.top()), pos2(x, r.bottom() + w)],
                color,
                Stroke::NONE,
            ));
        }
        EndShape::Diamond => {
            let s = (h * 0.85).max(4.0);
            p.add(Shape::convex_polygon(
                vec![pos2(x, mid - s), pos2(x + s, mid), pos2(x, mid + s), pos2(x - s, mid)],
                color,
                Stroke::NONE,
            ));
        }
        EndShape::Circle => {
            p.circle_filled(pos2(x, mid), (h * 0.7).max(3.0), color);
        }
        EndShape::Arrow => {
            let s = (h * 0.6).max(4.0);
            let tip = pos2(x, r.top());
            p.add(Shape::convex_polygon(
                vec![tip + vec2(-s, -s * 1.2), tip + vec2(s, -s * 1.2), tip],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// Text for a bar label field.
pub fn bar_text(app: &ProjectFcgApp, i: usize, field: BarField) -> String {
    let p = &app.project;
    let t = &p.tasks[i];
    let short = p.format.layout.short_bar_dates;
    let date = |d: Option<NaiveDateTime>| -> String {
        match d {
            Some(d) if short => format!("{}/{}", d.month(), d.day()),
            Some(d) => p.format.layout.date_format.format(d),
            None => String::new(),
        }
    };
    match field {
        BarField::None => String::new(),
        BarField::Name => t.name.clone(),
        BarField::Id => (i + 1).to_string(),
        BarField::Duration => columns::cell_text(p, i, ColumnId::Duration),
        BarField::Start => date(t.calc.start),
        BarField::Finish => date(t.calc.finish),
        BarField::PercentComplete => columns::cell_text(p, i, ColumnId::PercentComplete),
        BarField::TotalSlack => columns::cell_text(p, i, ColumnId::TotalSlack),
        BarField::FreeSlack => columns::cell_text(p, i, ColumnId::FreeSlack),
        BarField::Predecessors => columns::cell_text(p, i, ColumnId::Predecessors),
        BarField::Wbs => t.calc.outline_number.clone(),
        BarField::Notes => columns::cell_text(p, i, ColumnId::Notes),
        BarField::BaselineStart => date(t.baseline.map(|b| b.start)),
        BarField::BaselineFinish => date(t.baseline.map(|b| b.finish)),
        BarField::Status => t.calc.status.label().into(),
        BarField::ResourceNames => p.resource_names_text(i),
        BarField::ResourceInitials => p.resource_initials_text(i),
        BarField::Work => columns::cell_text(p, i, ColumnId::Work),
        BarField::Cost => columns::cell_text(p, i, ColumnId::Cost),
    }
}

/// Draw the five text positions around bar rect `r`.
pub fn paint_texts(app: &ProjectFcgApp, p: &Painter, i: usize, r: Rect, st: &BarStyle, font: &FontId) {
    if !app.project.format.layout.show_bar_text {
        return;
    }
    let tx = &st.text;
    let pad = 4.0;
    let col = pal().text;
    let put = |field: BarField, pos: Pos2, anchor: Align2| {
        if field != BarField::None {
            let s = bar_text(app, i, field);
            if !s.is_empty() {
                p.text(pos, anchor, s, font.clone(), col);
            }
        }
    };
    put(tx.left, pos2(r.left() - pad - 2.0, r.center().y), Align2::RIGHT_CENTER);
    put(tx.right, pos2(r.right() + pad + 2.0, r.center().y), Align2::LEFT_CENTER);
    let small = FontId::proportional((font.size - 1.5).max(7.0));
    if tx.top != BarField::None {
        let s = bar_text(app, i, tx.top);
        p.text(pos2(r.center().x, r.top() - 1.0), Align2::CENTER_BOTTOM, s, small.clone(), col);
    }
    if tx.bottom != BarField::None {
        let s = bar_text(app, i, tx.bottom);
        p.text(pos2(r.center().x, r.bottom() + 1.0), Align2::CENTER_TOP, s, small.clone(), col);
    }
    if tx.inside != BarField::None {
        let s = bar_text(app, i, tx.inside);
        let clip = p.with_clip_rect(r.intersect(p.clip_rect()));
        let tc = on_fill(c32(st.fill));
        clip.text(r.center(), Align2::CENTER_CENTER, s, small, tc);
    }
}

pub fn luminance(c: Rgb) -> f32 {
    (0.2126 * c.0 as f32 + 0.7152 * c.1 as f32 + 0.0722 * c.2 as f32) / 255.0
}

/// A small preview swatch of a style (used in the Bar Styles dialog).
pub fn preview(p: &Painter, r: Rect, cat: BarCategory, st: &BarStyle) {
    let mid = r.center().y;
    let hh = (r.height() * st.height / 2.0).clamp(1.5, r.height() * 0.45);
    match cat {
        BarCategory::Milestone => {
            let shape = if st.start_shape == EndShape::None { EndShape::Diamond } else { st.start_shape };
            let rr = Rect::from_center_size(r.center(), vec2(hh * 2.0, hh * 2.0));
            paint_end(p, shape, r.center().x, rr, c32(st.fill), true);
        }
        BarCategory::Deadline => {
            let rr = Rect::from_center_size(r.center() + vec2(0.0, hh), vec2(2.0, hh));
            paint_end(p, EndShape::Arrow, r.center().x, rr, c32(st.fill), true);
        }
        BarCategory::Progress => {
            // progress is drawn inside task bars, so preview it on one
            let x0 = r.left() + 6.0;
            let x1 = r.right() - 6.0;
            let bar = Rect::from_min_max(pos2(x0, mid - r.height() * 0.3), pos2(x1, mid + r.height() * 0.3));
            p.rect_filled(bar, 1.0, pal().bar_fill);
            p.rect_stroke(bar, 1.0, Stroke::new(1.0, pal().bar_border), StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(pos2(x0, mid - hh), pos2(x0 + (x1 - x0) * 0.6, mid + hh)), 0.0, c32(st.fill));
        }
        _ => {
            let x0 = r.left() + 6.0;
            let x1 = r.right() - 6.0;
            let br = paint_bar(p, x0, x1, mid, hh, st);
            paint_end(p, st.start_shape, x0, br, c32(st.border), true);
            paint_end(p, st.end_shape, x1, br, c32(st.border), false);
        }
    }
}
