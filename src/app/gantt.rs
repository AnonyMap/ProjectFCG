//! The Gantt chart: timescale, bars, links, and bar dragging.

use super::commands::{Command, Timescale};
use super::theme::*;
use super::{from_days, to_days, DisplayRow, ProjectFcgApp, RowRef};
use crate::format::EndShape;
use crate::model::{fmt_date, fmt_datetime, LinkType, TaskMode};
use chrono::{Datelike, Duration as ChronoDur, NaiveDate, NaiveDateTime, Timelike};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Pending { on_right_edge: bool },
    Move,
    Resize,
    Link,
}

#[derive(Clone, Debug)]
pub struct BarDrag {
    pub uid: u32,
    pub kind: DragKind,
    pub origin: Pos2,
    pub cur: Pos2,
    pub orig_start: NaiveDateTime,
    pub orig_finish: NaiveDateTime,
}

#[derive(Clone, Debug)]
pub struct LinkPath {
    pub succ_uid: u32,
    pub pred_uid: u32,
    pub pts: Vec<Pos2>,
    /// A board dependency: the ticket waited on (pred is its sprint).
    pub board: Option<u32>,
}

#[derive(Default)]
pub struct GanttState {
    pub drag: Option<BarDrag>,
    pub link_paths: Vec<LinkPath>,
}

/// Mapping between time and x for the current frame.
#[derive(Clone, Copy)]
pub struct Xform {
    pub left: f32,
    pub left_days: f64,
    pub ppd: f32,
}

impl Xform {
    pub fn x(&self, dt: NaiveDateTime) -> f32 {
        self.left + ((to_days(dt) - self.left_days) * self.ppd as f64) as f32
    }
    pub fn dt(&self, x: f32) -> NaiveDateTime {
        from_days(self.left_days + ((x - self.left) / self.ppd) as f64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Hours(u32),
    Day,
    Week,
    Month,
    Third,
    Quarter,
    Half,
    Year,
}

fn tiers(ts: Timescale, ppd: f32) -> (Unit, Unit) {
    let px_per_hour = ppd / 24.0;
    match ts {
        Timescale::Hours => {
            let step = if px_per_hour >= 30.0 {
                1
            } else if px_per_hour >= 18.0 {
                2
            } else {
                3
            };
            (Unit::Day, Unit::Hours(step))
        }
        Timescale::QuarterDays => (Unit::Day, Unit::Hours(if px_per_hour >= 12.0 { 3 } else { 6 })),
        Timescale::Days => (Unit::Week, Unit::Day),
        Timescale::Weeks => (Unit::Month, Unit::Week),
        Timescale::ThirdsOfMonths => (Unit::Month, Unit::Third),
        Timescale::Months => (Unit::Quarter, Unit::Month),
        Timescale::Quarters => (Unit::Year, Unit::Quarter),
        Timescale::HalfYears | Timescale::Years => (Unit::Year, Unit::Half),
    }
}

fn floor_unit(dt: NaiveDateTime, u: Unit) -> NaiveDateTime {
    let d = dt.date();
    let mid = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap();
    match u {
        Unit::Hours(n) => {
            let h = dt.hour() / n * n;
            d.and_hms_opt(h, 0, 0).unwrap()
        }
        Unit::Day => mid(d),
        Unit::Week => mid(d - ChronoDur::days(d.weekday().num_days_from_sunday() as i64)),
        Unit::Month => mid(NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap()),
        Unit::Third => {
            let day = if d.day() >= 21 {
                21
            } else if d.day() >= 11 {
                11
            } else {
                1
            };
            mid(NaiveDate::from_ymd_opt(d.year(), d.month(), day).unwrap())
        }
        Unit::Quarter => {
            let m = (d.month() - 1) / 3 * 3 + 1;
            mid(NaiveDate::from_ymd_opt(d.year(), m, 1).unwrap())
        }
        Unit::Half => {
            let m = if d.month() >= 7 { 7 } else { 1 };
            mid(NaiveDate::from_ymd_opt(d.year(), m, 1).unwrap())
        }
        Unit::Year => mid(NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap()),
    }
}

fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + n as i32;
    NaiveDate::from_ymd_opt(total / 12, (total % 12) as u32 + 1, 1).unwrap()
}

fn next_unit(dt: NaiveDateTime, u: Unit) -> NaiveDateTime {
    let mid = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap();
    match u {
        Unit::Hours(n) => dt + ChronoDur::hours(n as i64),
        Unit::Day => dt + ChronoDur::days(1),
        Unit::Week => dt + ChronoDur::days(7),
        Unit::Month => mid(add_months(dt.date(), 1)),
        Unit::Third => {
            let d = dt.date();
            match d.day() {
                1 => mid(NaiveDate::from_ymd_opt(d.year(), d.month(), 11).unwrap()),
                11 => mid(NaiveDate::from_ymd_opt(d.year(), d.month(), 21).unwrap()),
                _ => mid(add_months(d, 1)),
            }
        }
        Unit::Quarter => mid(add_months(dt.date(), 3)),
        Unit::Half => mid(add_months(dt.date(), 6)),
        Unit::Year => mid(add_months(dt.date(), 12)),
    }
}

fn label(dt: NaiveDateTime, u: Unit, width: f32, top: bool) -> String {
    let d = dt.date();
    let yy = d.year() % 100;
    match u {
        Unit::Hours(_) => {
            let h = dt.hour();
            let (h12, ap) = match h {
                0 => (12, "AM"),
                1..=11 => (h, "AM"),
                12 => (12, "PM"),
                _ => (h - 12, "PM"),
            };
            if width >= 44.0 {
                format!("{h12} {ap}")
            } else {
                format!("{h12}")
            }
        }
        Unit::Day => {
            if top {
                format!("{}", dt.format("%a %b %-d, '%y"))
            } else if width >= 46.0 {
                format!("{}", dt.format("%a %-d"))
            } else {
                dt.format("%a").to_string().chars().next().unwrap().to_string()
            }
        }
        Unit::Week => {
            if top {
                format!("{} {}, '{:02}", dt.format("%b"), d.day(), yy)
            } else {
                format!("{}/{}", d.month(), d.day())
            }
        }
        Unit::Month => {
            if top {
                format!("{}", dt.format("%B %Y"))
            } else if width >= 28.0 {
                dt.format("%b").to_string()
            } else {
                dt.format("%b").to_string().chars().next().unwrap().to_string()
            }
        }
        Unit::Third => match d.day() {
            1 => "B".into(),
            11 => "M".into(),
            _ => "E".into(),
        },
        Unit::Quarter => {
            let q = (d.month() - 1) / 3 + 1;
            if top {
                format!("Qtr {q}, {}", d.year())
            } else {
                format!("Q{q}")
            }
        }
        Unit::Half => {
            let h = if d.month() >= 7 { 2 } else { 1 };
            if top {
                format!("H{h} {}", d.year())
            } else {
                format!("H{h}")
            }
        }
        Unit::Year => format!("{}", d.year()),
    }
}

pub(crate) fn paint_tier(p: &Painter, rect: Rect, xf: &Xform, u: Unit, count: u32, top: bool) {
    p.rect_filled(rect, 0.0, pal().header_bg);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, pal().header_border));
    let start = floor_unit(xf.dt(rect.left()), u);
    let mut cur = start;
    let font = FontId::proportional(small_size());
    let mut guard = 0;
    while xf.x(cur) < rect.right() && guard < 2000 {
        let mut next = cur;
        for _ in 0..count.max(1) {
            next = next_unit(next, u);
        }
        let x0 = xf.x(cur);
        let x1 = xf.x(next);
        p.line_segment([pos2(x0, rect.top()), pos2(x0, rect.bottom())], Stroke::new(1.0, pal().header_border));
        let w = x1 - x0;
        if w > 8.0 {
            let text = label(cur, u, w, top);
            let cell = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
            let pc = p.with_clip_rect(cell.intersect(rect));
            if top {
                pc.text(pos2(x0.max(rect.left()) + 4.0, cell.center().y), Align2::LEFT_CENTER, text, font.clone(), pal().text);
            } else {
                pc.text(cell.center(), Align2::CENTER_CENTER, text, font.clone(), pal().text);
            }
        }
        cur = next;
        guard += 1;
    }
}

#[derive(Clone, Copy)]
struct BarGeom {
    x0: f32,
    x1: f32,
    mid: f32,
    half_h: f32,
    milestone: bool,
}

fn geom_for(app: &ProjectFcgApp, i: usize, d: usize, xf: &Xform, body_top: f32) -> Option<BarGeom> {
    let t = &app.project.tasks[i];
    let (s, f) = (t.calc.start?, t.calc.finish?);
    let row_mid = body_top + d as f32 * row_h() - app.scroll_y + row_h() / 2.0;
    // Tracking Gantt: actual bar in the upper part of the row, baseline below.
    let mid = if app.project.format.baseline_on() { row_mid - row_h() * 0.14 } else { row_mid };
    let (_, st) = super::bars::style_for(app, i, None);
    let hh = super::bars::half_h(app, &st);
    let milestone = t.is_milestone();
    let mut x0 = xf.x(s);
    let mut x1 = xf.x(f);
    if milestone {
        // a "milestone" with duration (Mark as Milestone) shows at its finish
        let c = if t.duration.is_zero() { x0 } else { x1 };
        x0 = c - hh;
        x1 = c + hh;
    }
    if x1 < x0 + 2.0 {
        x1 = x0 + 2.0;
    }
    Some(BarGeom { x0, x1, mid, half_h: hh, milestone })
}

/// Timescale tiers (top first) with their unit counts.
pub(crate) fn tier_list(app: &ProjectFcgApp) -> Vec<(Unit, u32)> {
    let spec = &app.project.format.timescale;
    if spec.auto {
        let (t, b) = tiers(Timescale::for_ppd(app.gantt.ppd), app.gantt.ppd);
        return vec![(t, 1), (b, 1)];
    }
    let conv = |tier: crate::format::Tier| -> (Unit, u32) {
        use crate::format::TsUnit::*;
        let n = tier.count.max(1);
        match tier.unit {
            Hours => (Unit::Hours(n.min(24)), 1),
            Days => (Unit::Day, n),
            Weeks => (Unit::Week, n),
            ThirdsOfMonths => (Unit::Third, n),
            Months => (Unit::Month, n),
            Quarters => (Unit::Quarter, n),
            HalfYears => (Unit::Half, n),
            Years => (Unit::Year, n),
        }
    };
    match spec.tiers {
        1 => vec![conv(spec.bottom)],
        3 => vec![conv(spec.top), conv(spec.middle), conv(spec.bottom)],
        _ => vec![conv(spec.middle), conv(spec.bottom)],
    }
}

/// Boundaries of a tier unit across [x0, x1].
pub(crate) fn tier_marks(xf: &Xform, u: Unit, count: u32, x0: f32, x1: f32) -> Vec<NaiveDateTime> {
    let mut out = vec![];
    let mut cur = floor_unit(xf.dt(x0), u);
    let mut guard = 0;
    while xf.x(cur) <= x1 && guard < 3000 {
        out.push(cur);
        for _ in 0..count.max(1) {
            cur = next_unit(cur, u);
        }
        guard += 1;
    }
    out.push(cur);
    out
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect, rows: &[DisplayRow]) {
    let inner = Rect::from_min_max(rect.min, pos2(rect.right() - SCROLLBAR, rect.bottom()));
    let header = Rect::from_min_size(inner.min, vec2(inner.width(), header_h()));
    let body = Rect::from_min_max(pos2(inner.left(), header.bottom()), pos2(inner.right(), inner.bottom() - SCROLLBAR));
    let hbar = Rect::from_min_max(pos2(inner.left(), inner.bottom() - SCROLLBAR), pos2(inner.right(), inner.bottom()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal().bg);

    // ---------- input: scrolling & zoom ----------
    let resp = ui.interact(body, ui.scope_id().with("gantt_body"), Sense::click_and_drag());
    if resp.hovered() {
        let (delta, zoom, shift, pointer) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.shift, i.pointer.hover_pos()));
        if zoom != 1.0 {
            let frac = pointer.map(|p| ((p.x - body.left()) / body.width()).clamp(0.0, 1.0)).unwrap_or(0.5);
            app.zoom_to(app.gantt.ppd * zoom, frac);
        } else if shift {
            app.gantt.left_days -= ((delta.y + delta.x) / app.gantt.ppd) as f64;
        } else {
            app.scroll_y = (app.scroll_y - delta.y).max(0.0);
            app.gantt.left_days -= (delta.x / app.gantt.ppd) as f64;
        }
    }
    // middle-button pan
    if resp.dragged_by(egui::PointerButton::Middle) {
        let dd = resp.drag_delta();
        app.gantt.left_days -= (dd.x / app.gantt.ppd) as f64;
        app.scroll_y = (app.scroll_y - dd.y).max(0.0);
    }

    let xf = Xform {
        left: body.left(),
        left_days: app.gantt.left_days,
        ppd: app.gantt.ppd,
    };
    let tier_units = tier_list(app);
    let fmt = app.project.format.clone();
    let gl = fmt.gridlines;

    // ---------- background: nonworking time ----------
    let bp = painter.with_clip_rect(body);
    let nonworking_rects = nonworking(app, &xf, body);
    let nw_color = paper(fmt.timescale.nonworking_color);
    if fmt.timescale.nonworking == crate::format::NonworkingDraw::Behind {
        for r in &nonworking_rects {
            bp.rect_filled(*r, 0.0, nw_color);
        }
    }
    // vertical gridlines at timescale boundaries
    let line_col = paper(gl.line_color);
    if gl.major_columns || gl.minor_columns {
        let n = tier_units.len();
        let mut sets = vec![];
        if gl.major_columns && n > 1 {
            sets.push((tier_units[0], 1.0f32));
        }
        if gl.minor_columns {
            sets.push((tier_units[n - 1], 0.6));
        }
        for ((u, c), alpha) in sets {
            for m in tier_marks(&xf, u, c, body.left(), body.right()) {
                let x = xf.x(m).round() + 0.5;
                bp.line_segment(
                    [pos2(x, body.top()), pos2(x, body.bottom())],
                    Stroke::new(1.0, line_col.gamma_multiply(alpha)),
                );
            }
        }
    }
    // selected rows band and highlight filter
    let selected: std::collections::HashSet<u32> = app.selected.iter().copied().collect();
    let first = (app.scroll_y / row_h()).floor() as usize;
    let visible = (body.height() / row_h()).ceil() as usize + 1;
    for d in first..(first + visible).min(rows.len()) {
        if let DisplayRow::Task(i) = rows[d] {
            if selected.contains(&app.project.tasks[i].uid) {
                let y = body.top() + d as f32 * row_h() - app.scroll_y;
                bp.rect_filled(
                    Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), row_h())),
                    0.0,
                    {
                        let c = pal().selected_row;
                        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 150)
                    },
                );
            }
        }
    }
    if gl.gantt_rows {
        for d in first..first + visible {
            let y = (body.top() + (d + 1) as f32 * row_h() - app.scroll_y).round() - 0.5;
            bp.line_segment([pos2(body.left(), y), pos2(body.right(), y)], Stroke::new(1.0, line_col));
        }
    }
    // date lines
    let vline = |dt: NaiveDateTime, col: Color32, dash: (f32, f32), w: f32| {
        let x = xf.x(dt);
        if x >= body.left() && x <= body.right() {
            bp.add(Shape::dashed_line(&[pos2(x, body.top()), pos2(x, body.bottom())], Stroke::new(w, col), dash.0, dash.1));
        }
    };
    if gl.current_date {
        vline(chrono::Local::now().naive_local(), ink(gl.current_date_color), (4.0, 3.0), 1.5);
    }
    if gl.status_date {
        if let Some(sd) = app.project.status_date {
            vline(sd, ink(gl.status_date_color), (8.0, 3.0), 1.5);
        }
    }
    if gl.project_start {
        vline(app.project.start, pal().project_line, (2.0, 4.0), 1.0);
    }
    if gl.project_finish {
        if let Some(f) = app.project.calc.finish {
            vline(f, pal().project_line, (2.0, 4.0), 1.0);
        }
    }

    // ---------- geometry ----------
    let mut pos_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (d, r) in rows.iter().enumerate() {
        if let DisplayRow::Task(i) = r {
            pos_of.insert(*i, d);
        }
    }
    let row_visible = |d: usize| d + 1 >= first && d <= first + visible;

    // ---------- links ----------
    app.gantt_state.link_paths.clear();
    let crit = fmt.critical_on();
    let link_normal = bars_c32(super::bars::style(&fmt, crate::format::BarCategory::Task).border);
    let link_crit = bars_c32(super::bars::style(&fmt, crate::format::BarCategory::CriticalTask).border);
    let row_of: std::collections::HashMap<u32, usize> = app.project.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    for (&si, &sd) in &pos_of {
        let st = &app.project.tasks[si];
        for l in &st.predecessors {
            let Some(&pi) = row_of.get(&l.pred) else { continue };
            let Some(&pd) = pos_of.get(&pi) else { continue };
            let lo = sd.min(pd);
            let hi = sd.max(pd);
            if hi + 1 < first || lo > first + visible {
                continue;
            }
            let (Some(pg), Some(sg)) = (geom_for(app, pi, pd, &xf, body.top()), geom_for(app, si, sd, &xf, body.top())) else {
                continue;
            };
            let pts = match fmt.layout.links {
                crate::format::LinkStyle::Elbow | crate::format::LinkStyle::Hidden => link_route(l.kind, &pg, &sg),
                crate::format::LinkStyle::Straight => straight_route(l.kind, &pg, &sg),
            };
            let critical = crit && app.project.tasks[pi].calc.critical && st.calc.critical;
            let color = if critical { link_crit } else { link_normal };
            if fmt.layout.links != crate::format::LinkStyle::Hidden {
                draw_polyline_arrow(&bp, &pts, color);
            }
            app.gantt_state.link_paths.push(LinkPath {
                succ_uid: st.uid,
                pred_uid: l.pred,
                pts,
                board: None,
            });
        }
        // board dependencies: dashed, from the sprint the story is planned in
        for d in &st.board_deps {
            let Some(target) = app.project.resolve_board_dep(d.ticket) else { continue };
            for sp in target.sprints {
                let Some(&pi) = row_of.get(&sp) else { continue };
                let Some(&pd) = pos_of.get(&pi) else { continue };
                if sd.max(pd) + 1 < first || sd.min(pd) > first + visible || pi == si {
                    continue;
                }
                let (Some(pg), Some(sg)) = (geom_for(app, pi, pd, &xf, body.top()), geom_for(app, si, sd, &xf, body.top())) else {
                    continue;
                };
                let pts = link_route(LinkType::FS, &pg, &sg);
                if fmt.layout.links != crate::format::LinkStyle::Hidden {
                    let c = super::board::type_color(crate::board::IssueType::Epic);
                    for w in pts.windows(2) {
                        bp.add(Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.2, c), 4.0, 3.0));
                    }
                    if let (Some(a), Some(b)) = (pts.get(pts.len().wrapping_sub(2)), pts.last()) {
                        draw_polyline_arrow(&bp, &[*a, *b], c);
                    }
                }
                app.gantt_state.link_paths.push(LinkPath { succ_uid: st.uid, pred_uid: sp, pts, board: Some(d.ticket) });
            }
        }
    }

    // ---------- bars ----------
    let font = FontId::proportional(small_size());
    let roles = app.path_roles();
    for d in first..(first + visible).min(rows.len()) {
        let y = body.top() + d as f32 * row_h() - app.scroll_y;
        match rows[d] {
            DisplayRow::ProjectSummary => {
                let (s, f) = app.project_span();
                let st = super::bars::style(&fmt, crate::format::BarCategory::ProjectSummary);
                if st.show {
                    let hh = super::bars::half_h(app, &st);
                    let (x0, x1) = (xf.x(s), xf.x(f).max(xf.x(s) + 2.0));
                    let r = super::bars::paint_bar(&bp, x0, x1, y + row_h() / 2.0, hh, &st);
                    super::bars::paint_end(&bp, st.start_shape, x0, r, bars_c32(st.fill), true);
                    super::bars::paint_end(&bp, st.end_shape, x1, r, bars_c32(st.fill), false);
                }
            }
            DisplayRow::Task(i) => {
                if !row_visible(d) {
                    continue;
                }
                let Some(g) = geom_for(app, i, d, &xf, body.top()) else { continue };
                paint_task(app, &bp, i, d, &g, &xf, &font, roles.get(&i).copied(), body.top());
            }
        }
    }
    // ---------- progress lines (Format ▸ Gridlines ▸ Progress Lines) ----------
    if fmt.gridlines.progress_lines {
        let at = app.project.status_or_now();
        let x_at = xf.x(at);
        let mut pts: Vec<Pos2> = vec![];
        for d in first..(first + visible).min(rows.len()) {
            let DisplayRow::Task(i) = rows[d] else { continue };
            if app.project.tasks[i].calc.is_summary {
                continue;
            }
            let Some(pt) = crate::tracking::progress_point(&app.project, i, at) else { continue };
            let y = body.top() + d as f32 * row_h() - app.scroll_y;
            // a peak per task: left of the line = behind, right = ahead
            pts.push(pos2(x_at, y + 1.0));
            pts.push(pos2(xf.x(pt), y + row_h() / 2.0));
            pts.push(pos2(x_at, y + row_h() - 1.0));
        }
        if pts.len() >= 2 {
            bp.add(Shape::line(pts, Stroke::new(1.6, ink(fmt.gridlines.status_date_color))));
        }
    }
    if fmt.timescale.nonworking == crate::format::NonworkingDraw::InFront {
        for r in &nonworking_rects {
            bp.rect_filled(*r, 0.0, nw_color.gamma_multiply(0.75));
        }
    }

    // ---------- hover / drag ----------
    let hover = resp.hover_pos();
    let bar_at = |p: Pos2, app: &ProjectFcgApp| -> Option<(usize, BarGeom, bool)> {
        let d = ((p.y - body.top() + app.scroll_y) / row_h()).floor();
        if d < 0.0 {
            return None;
        }
        let d = d as usize;
        if let Some(DisplayRow::Task(i)) = rows.get(d) {
            let g = geom_for(app, *i, d, &xf, body.top())?;
            if p.x >= g.x0 - 3.0 && p.x <= g.x1 + 3.0 && (p.y - g.mid).abs() <= g.half_h + 4.0 {
                let right_edge = !g.milestone
                    && !app.project.tasks[*i].calc.is_summary
                    && (p.x - g.x1).abs() <= 4.0;
                return Some((*i, g, right_edge));
            }
        }
        None
    };

    if app.gantt_state.drag.is_none() {
        if let Some(h) = hover {
            if let Some((i, _, right)) = bar_at(h, app) {
                ui.ctx().set_cursor_icon(if right { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Move });
                if !resp.dragged() {
                    let t = &app.project.tasks[i];
                    let mut tip = format!("{}\n", if t.name.is_empty() { "(unnamed task)" } else { &t.name });
                    if t.calc.is_summary {
                        tip.push_str("Summary task\n");
                    } else if t.is_milestone() {
                        tip.push_str("Milestone\n");
                    }
                    tip.push_str(&format!(
                        "Start:  {}\nFinish: {}\nDuration: {}",
                        t.calc.start.map(fmt_datetime).unwrap_or_default(),
                        t.calc.finish.map(fmt_datetime).unwrap_or_default(),
                        super::columns::cell_text(&app.project, i, super::columns::ColumnId::Duration)
                    ));
                    if t.calc.total_slack != 0 {
                        tip.push_str(&format!(
                            "\nTotal slack: {}",
                            super::columns::cell_text(&app.project, i, super::columns::ColumnId::TotalSlack)
                        ));
                    }
                    if t.mode == TaskMode::Manual {
                        tip.push_str("\nManually scheduled");
                    }
                    paint_tooltip(ui, h + vec2(14.0, 14.0), &tip);
                }
            } else if let Some(lp) = link_at(&app.gantt_state.link_paths, h) {
                let pi = app.project.index_of_uid(lp.pred_uid);
                let si = app.project.index_of_uid(lp.succ_uid);
                if let (Some(tk), Some(pi), Some(si)) = (lp.board, pi, si) {
                    let p = &app.project;
                    let t = p.ticket(tk).map(|x| x.title.clone()).unwrap_or_default();
                    paint_tooltip(
                        ui,
                        h + vec2(14.0, 14.0),
                        &format!(
                            "Board dependency\n({}) {} waits on {} {}\nuntil {} ends (or the item is done)",
                            si + 1,
                            p.tasks[si].name,
                            p.ticket_key(tk),
                            t,
                            p.tasks[pi].name
                        ),
                    );
                } else if let (Some(pi), Some(si)) = (pi, si) {
                    let l = app.project.tasks[si].predecessors.iter().find(|l| l.pred == lp.pred_uid).copied();
                    if let Some(l) = l {
                        let lag = l.lag.format(&app.project.settings.durations);
                        paint_tooltip(
                            ui,
                            h + vec2(14.0, 14.0),
                            &format!(
                                "Link: {}{}\nFrom: ({}) {}\nTo: ({}) {}",
                                l.kind.long_name(),
                                if lag.is_empty() { String::new() } else { format!(" {lag}") },
                                pi + 1,
                                app.project.tasks[pi].name,
                                si + 1,
                                app.project.tasks[si].name
                            ),
                        );
                    }
                }
            }
        }
    }

    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            if let Some((i, _, right)) = bar_at(p, app) {
                let t = &app.project.tasks[i];
                let u = t.uid;
                if let (Some(s), Some(f)) = (t.calc.start, t.calc.finish) {
                    app.commit_edit();
                    app.select_single(RowRef::Task(u));
                    app.gantt_state.drag = Some(BarDrag {
                        uid: u,
                        kind: DragKind::Pending { on_right_edge: right },
                        origin: p,
                        cur: p,
                        orig_start: s,
                        orig_finish: f,
                    });
                }
            }
        }
    }
    if let Some(drag) = app.gantt_state.drag.as_mut() {
        if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
            drag.cur = p;
        }
        let delta = drag.cur - drag.origin;
        if let DragKind::Pending { on_right_edge } = drag.kind {
            if delta.y.abs() > row_h() * 0.6 && delta.y.abs() > delta.x.abs() {
                drag.kind = DragKind::Link;
            } else if delta.x.abs() > 3.0 {
                drag.kind = if on_right_edge { DragKind::Resize } else { DragKind::Move };
            }
        }
        // auto-scroll horizontally near the edges
        if matches!(drag.kind, DragKind::Move | DragKind::Resize) {
            if drag.cur.x > body.right() - 10.0 {
                app.gantt.left_days += (6.0 / app.gantt.ppd) as f64;
                drag.origin.x -= 6.0;
            } else if drag.cur.x < body.left() + 10.0 {
                app.gantt.left_days -= (6.0 / app.gantt.ppd) as f64;
                drag.origin.x += 6.0;
            }
        }
    }
    if let Some(drag) = app.gantt_state.drag.clone() {
        paint_drag_preview(app, ui, &bp, &drag, &xf, body, rows);
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            app.gantt_state.drag = None;
            finish_drag(app, &drag, &xf, body, rows);
        }
    }

    // clicks
    if resp.double_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            if let Some((i, _, _)) = bar_at(p, app) {
                app.select_index(i);
                let ctx = ui.ctx().clone();
                app.execute(Command::TaskInfo, &ctx);
            } else if let Some(lp) = link_at(&app.gantt_state.link_paths, p).cloned() {
                if lp.board.is_some() {
                    // board dependencies are edited with the task's predecessors
                    if let Some(si) = app.project.index_of_uid(lp.succ_uid) {
                        app.select_index(si);
                        app.dialogs.open_task_info(&app.project, si, super::dialogs::InfoTab::Predecessors);
                    }
                } else {
                    app.dialogs.open_dependency(&app.project, lp.pred_uid, lp.succ_uid);
                }
            }
        }
    } else if resp.clicked() || resp.secondary_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            app.commit_edit();
            let d = ((p.y - body.top() + app.scroll_y) / row_h()).floor().max(0.0) as usize;
            match rows.get(d) {
                Some(DisplayRow::Task(i)) => {
                    let u = app.project.tasks[*i].uid;
                    if !(resp.secondary_clicked() && app.selected.contains(&u)) {
                        app.select_single(RowRef::Task(u));
                    }
                }
                Some(DisplayRow::ProjectSummary) => {}
                None => app.select_single(RowRef::Blank),
            }
        }
    }
    resp.context_menu(|ui| super::table::task_context_menu(app, ui));

    // ---------- header (timescale) ----------
    let hp = painter.with_clip_rect(header);
    let nt = tier_units.len().max(1);
    let th = header_h() / nt as f32;
    for (k, (u, c)) in tier_units.iter().enumerate() {
        let r = Rect::from_min_max(
            pos2(header.left(), header.top() + th * k as f32),
            pos2(header.right(), header.top() + th * (k + 1) as f32),
        );
        paint_tier(&hp, r, &xf, *u, *c, k == 0 && nt > 1);
    }
    // header interactions: double-click → Timescale dialog equivalent (cycle presets)
    let hresp = ui.interact(header, ui.scope_id().with("timescale_hdr"), Sense::click());
    hresp.context_menu(|ui| {
        ui.label(egui::RichText::new("Timescale").strong());
        for t in Timescale::ALL {
            if ui.selectable_label(Timescale::for_ppd(app.gantt.ppd) == t, t.label()).clicked() {
                let ctx = ui.ctx().clone();
                app.execute(Command::SetTimescale(t), &ctx);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Zoom In").clicked() {
            app.zoom_to(app.gantt.ppd * 1.5, 0.5);
            ui.close();
        }
        if ui.button("Zoom Out").clicked() {
            app.zoom_to(app.gantt.ppd / 1.5, 0.5);
            ui.close();
        }
    });

    // ---------- horizontal scrollbar ----------
    let (ps, pf) = app.project_span();
    let view_days = (body.width() / app.gantt.ppd) as f64;
    let lo = (to_days(ps) - 30.0).min(app.gantt.left_days);
    let hi = (to_days(pf) + 60.0).max(app.gantt.left_days + view_days);
    let scale = app.gantt.ppd; // px per day for content units
    let off = ((app.gantt.left_days - lo) as f32) * scale;
    let content = ((hi - lo) as f32) * scale;
    if let Some(v) = super::scrollbar(ui, hbar, false, off, body.width(), content, "gantt_hscroll") {
        app.gantt.left_days = lo + (v / scale) as f64;
    }
    // corner
    painter.rect_filled(
        Rect::from_min_max(pos2(inner.right(), inner.bottom() - SCROLLBAR), rect.max),
        0.0,
        pal().scroll_track,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(inner.right(), rect.top()), pos2(rect.right(), header.bottom())),
        0.0,
        pal().header_bg,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_task(
    app: &ProjectFcgApp,
    p: &Painter,
    i: usize,
    d: usize,
    g: &BarGeom,
    xf: &Xform,
    font: &FontId,
    role: Option<super::bars::PathRole>,
    body_top: f32,
) {
    use super::bars::{c32, paint_bar, paint_end, paint_texts, style_for};
    use crate::format::BarCategory as C;
    let t = &app.project.tasks[i];
    let fmt = &app.project.format;
    let (cat, mut st) = style_for(app, i, role);
    // inactive tasks are hollow, as in Project
    if app.inactive_rows.get(i).copied().unwrap_or(false) {
        st.fill = super::theme::to_rgb(pal().bg);
    }
    let row_mid = body_top + d as f32 * row_h() - app.scroll_y + row_h() / 2.0;

    // Baseline (Tracking Gantt / Format ▸ Baseline) below the actual bar.
    if fmt.baseline_on() {
        if let Some(b) = t.baseline_slot(fmt.baseline_index as usize) {
            let bs = super::bars::style(fmt, C::Baseline);
            if bs.show {
                let hh = super::bars::half_h(app, &bs);
                let by = row_mid + row_h() * 0.22;
                if b.start == b.finish {
                    let r = Rect::from_center_size(pos2(xf.x(b.start), by), vec2(hh * 2.0, hh * 2.0));
                    paint_end(p, EndShape::Diamond, xf.x(b.start), r, c32(bs.border), true);
                } else {
                    paint_bar(p, xf.x(b.start), xf.x(b.finish).max(xf.x(b.start) + 2.0), by, hh, &bs);
                }
            }
        }
    }
    if !st.show {
        return;
    }
    let r = if cat == C::Milestone {
        let c = (g.x0 + g.x1) / 2.0;
        let rr = Rect::from_center_size(pos2(c, g.mid), vec2(g.half_h * 2.0, g.half_h * 2.0));
        let shape = if st.start_shape == EndShape::None { EndShape::Diamond } else { st.start_shape };
        paint_end(p, shape, c, rr, c32(st.fill), true);
        rr
    } else {
        let r = paint_bar(p, g.x0, g.x1, g.mid, g.half_h, &st);
        let end_col = if cat == C::Summary { c32(st.fill) } else { c32(st.border) };
        paint_end(p, st.start_shape, g.x0, r, end_col, true);
        paint_end(p, st.end_shape, g.x1, r, end_col, false);
        // progress inside the bar
        let pct = (t.calc.percent_complete / 100.0).clamp(0.0, 1.0) as f32;
        let ps = super::bars::style(fmt, C::Progress);
        if pct > 0.0 && ps.show {
            let ph = super::bars::half_h(app, &ps).min(g.half_h);
            let pr = Rect::from_min_max(pos2(r.left(), g.mid - ph), pos2(r.left() + r.width() * pct, g.mid + ph));
            p.rect_filled(pr, 0.0, c32(ps.fill));
        }
        r
    };
    paint_texts(app, p, i, r, &st, font);

    // Leveling Gantt: the delay leveling added, drawn before the bar
    if fmt.leveling_on() && t.leveling_delay > 0 && !t.calc.is_summary {
        let ls = super::bars::style(fmt, C::LevelingDelay);
        if let (true, Some(s)) = (ls.show, t.calc.start) {
            let before = app.project.calendar.sub_work(s, t.leveling_delay);
            let hh = super::bars::half_h(app, &ls);
            let (x0, x1) = (xf.x(before), xf.x(s));
            let lr = Rect::from_min_max(pos2(x0, g.mid - hh), pos2(x1, g.mid + hh));
            p.rect_filled(lr, 0.0, c32(ls.fill));
            paint_end(p, ls.start_shape, x0, lr.expand2(vec2(0.0, 2.0)), c32(ls.fill), true);
        }
    }
    // slack bar: finish → late finish
    if fmt.slack_on() && !t.calc.is_summary && t.calc.total_slack > 0 {
        let ss = super::bars::style(fmt, C::Slack);
        if let (true, Some(f), Some(lf)) = (ss.show, t.calc.finish, t.calc.late_finish) {
            let hh = super::bars::half_h(app, &ss);
            let y = r.bottom() + hh + 1.5;
            let (x0, x1) = (xf.x(f), xf.x(lf));
            p.rect_filled(Rect::from_min_max(pos2(x0, y - hh), pos2(x1, y + hh)), 0.0, c32(ss.fill));
        }
    }
    // deadline marker
    if let Some(dl) = t.deadline {
        let ds = super::bars::style(fmt, C::Deadline);
        if ds.show {
            let hh = super::bars::half_h(app, &ds);
            let x = xf.x(dl);
            let rr = Rect::from_min_max(pos2(x - 1.0, g.mid - hh * 0.2), pos2(x + 1.0, g.mid + hh));
            let shape = if ds.start_shape == EndShape::None { EndShape::Arrow } else { ds.start_shape };
            paint_end(p, shape, x, rr, c32(ds.fill), true);
        }
    }
}

fn bars_c32(c: crate::format::Rgb) -> Color32 {
    super::bars::c32(c)
}

/// Nonworking-time rectangles across the visible chart.
fn nonworking(app: &ProjectFcgApp, xf: &Xform, body: Rect) -> Vec<Rect> {
    let mut out = vec![];
    if app.project.format.timescale.nonworking == crate::format::NonworkingDraw::Hidden || app.gantt.ppd < 2.0 {
        return out;
    }
    let cal = &app.project.calendar;
    let mut d = xf.dt(body.left()).date();
    let end = xf.dt(body.right()).date();
    while d <= end {
        let day_start = d.and_hms_opt(0, 0, 0).unwrap();
        let shifts = cal.shifts_on(d);
        if shifts.is_empty() {
            let x0 = xf.x(day_start);
            let x1 = xf.x(day_start + ChronoDur::days(1));
            out.push(Rect::from_min_max(pos2(x0, body.top()), pos2(x1, body.bottom())));
        } else if app.gantt.ppd >= 110.0 {
            let mut prev = 0u32;
            for s in shifts.iter().chain(std::iter::once(&crate::calendar::Shift::new(1440, 1440))) {
                if s.start > prev {
                    let x0 = xf.x(day_start + ChronoDur::minutes(prev as i64));
                    let x1 = xf.x(day_start + ChronoDur::minutes(s.start as i64));
                    out.push(Rect::from_min_max(pos2(x0, body.top()), pos2(x1, body.bottom())));
                }
                prev = s.end;
            }
        }
        d += ChronoDur::days(1);
    }
    out
}

/// "Straight" link style: a direct line from the predecessor's end point to
/// the successor's, with an arrowhead.
fn straight_route(kind: LinkType, pg: &BarGeom, sg: &BarGeom) -> Vec<Pos2> {
    let from = match kind {
        LinkType::FS | LinkType::FF => pos2(pg.x1, pg.mid),
        LinkType::SS | LinkType::SF => pos2(pg.x0, pg.mid),
    };
    let to = match kind {
        LinkType::FS | LinkType::SS => pos2(sg.x0, sg.mid),
        LinkType::FF | LinkType::SF => pos2(sg.x1, sg.mid),
    };
    vec![from, to]
}

fn link_route(kind: LinkType, pg: &BarGeom, sg: &BarGeom) -> Vec<Pos2> {
    let py = pg.mid;
    let sy = sg.mid;
    let down = sy > py;
    let between = if down { py + row_h() / 2.0 } else { py - row_h() / 2.0 };
    match kind {
        LinkType::FS => {
            let x_turn = sg.x0 + 4.0;
            if x_turn >= pg.x1 + 3.0 {
                let ty = if down { sy - sg.half_h - 1.0 } else { sy + sg.half_h + 1.0 };
                vec![pos2(pg.x1, py), pos2(x_turn, py), pos2(x_turn, ty)]
            } else {
                vec![
                    pos2(pg.x1, py),
                    pos2(pg.x1 + 8.0, py),
                    pos2(pg.x1 + 8.0, between),
                    pos2(sg.x0 - 8.0, between),
                    pos2(sg.x0 - 8.0, sy),
                    pos2(sg.x0, sy),
                ]
            }
        }
        LinkType::SS => {
            let x = pg.x0.min(sg.x0) - 8.0;
            vec![pos2(pg.x0, py), pos2(x, py), pos2(x, sy), pos2(sg.x0, sy)]
        }
        LinkType::FF => {
            let x = pg.x1.max(sg.x1) + 8.0;
            vec![pos2(pg.x1, py), pos2(x, py), pos2(x, sy), pos2(sg.x1, sy)]
        }
        LinkType::SF => vec![
            pos2(pg.x0, py),
            pos2(pg.x0 - 8.0, py),
            pos2(pg.x0 - 8.0, between),
            pos2(sg.x1 + 8.0, between),
            pos2(sg.x1 + 8.0, sy),
            pos2(sg.x1, sy),
        ],
    }
}

fn draw_polyline_arrow(p: &Painter, pts: &[Pos2], color: Color32) {
    if pts.len() < 2 {
        return;
    }
    p.add(Shape::line(pts.to_vec(), Stroke::new(1.0, color)));
    let a = pts[pts.len() - 2];
    let b = pts[pts.len() - 1];
    let dir = (b - a).normalized();
    if dir.length() < 0.5 {
        return;
    }
    let n = vec2(-dir.y, dir.x);
    let tri = vec![b, b - dir * 6.0 + n * 3.5, b - dir * 6.0 - n * 3.5];
    p.add(Shape::convex_polygon(tri, color, Stroke::NONE));
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn link_at(paths: &[LinkPath], p: Pos2) -> Option<&LinkPath> {
    paths.iter().find(|lp| lp.pts.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) <= 3.5))
}

pub fn paint_tooltip(ui: &egui::Ui, at: Pos2, text: &str) {
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("gantt_tip"));
    let p = ui.ctx().layer_painter(layer);
    let font = FontId::proportional(small_size());
    let galley = p.layout(text.to_string(), font, pal().text, 360.0);
    let screen = ui.ctx().content_rect();
    let mut pos = at;
    if pos.x + galley.size().x + 12.0 > screen.right() {
        pos.x = screen.right() - galley.size().x - 14.0;
    }
    if pos.y + galley.size().y + 12.0 > screen.bottom() {
        pos.y = at.y - galley.size().y - 30.0;
    }
    let r = Rect::from_min_size(pos, galley.size() + vec2(12.0, 8.0));
    p.rect_filled(r.translate(vec2(2.0, 2.0)), 3.0, Color32::from_black_alpha(30));
    p.rect_filled(r, 3.0, pal().tooltip_bg);
    p.rect_stroke(r, 3.0, Stroke::new(1.0, pal().tooltip_border), StrokeKind::Inside);
    p.galley(pos + vec2(6.0, 4.0), galley, pal().text);
}

/// Snap a dragged instant to the timescale (whole days unless zoomed in).
fn snap(app: &ProjectFcgApp, dt: NaiveDateTime, is_finish: bool) -> NaiveDateTime {
    let ppd = app.gantt.ppd;
    if ppd >= 110.0 {
        let h = (dt + ChronoDur::minutes(30)).hour();
        dt.date().and_hms_opt(h, 0, 0).unwrap()
    } else {
        let cal = &app.project.calendar;
        let date = (dt + ChronoDur::hours(12)).date();
        if is_finish {
            let d = date - ChronoDur::days(1);
            let m = cal.day_end_minute(d).unwrap_or(app.project.settings.default_finish_minute);
            crate::calendar::at_minute(d, m)
        } else {
            let m = cal.day_start_minute(date).unwrap_or(app.project.settings.default_start_minute);
            crate::calendar::at_minute(date, m)
        }
    }
}

fn drag_result(app: &ProjectFcgApp, drag: &BarDrag, xf: &Xform) -> (NaiveDateTime, NaiveDateTime) {
    let dx = drag.cur.x - drag.origin.x;
    let ddays = (dx / xf.ppd) as f64;
    let shift = ChronoDur::seconds((ddays * 86400.0) as i64);
    match drag.kind {
        DragKind::Move => {
            let s = snap(app, drag.orig_start + shift, false);
            let s = app.project.calendar.snap_start(s, drag.orig_start == drag.orig_finish);
            let i = app.project.index_of_uid(drag.uid).unwrap_or(0);
            let t = &app.project.tasks[i];
            let f = if t.calc.is_summary {
                s + (drag.orig_finish - drag.orig_start)
            } else if t.duration.elapsed {
                s + ChronoDur::minutes(t.duration.minutes)
            } else {
                app.project.calendar.add_work(s, t.duration.minutes)
            };
            (s, f)
        }
        DragKind::Resize => {
            let f = snap(app, drag.orig_finish + shift, true).max(drag.orig_start);
            (drag.orig_start, f)
        }
        _ => (drag.orig_start, drag.orig_finish),
    }
}

fn target_row(app: &ProjectFcgApp, p: Pos2, body: Rect, rows: &[DisplayRow]) -> Option<usize> {
    let d = ((p.y - body.top() + app.scroll_y) / row_h()).floor();
    if d < 0.0 {
        return None;
    }
    match rows.get(d as usize) {
        Some(DisplayRow::Task(i)) => Some(*i),
        _ => None,
    }
}

fn paint_drag_preview(app: &ProjectFcgApp, ui: &egui::Ui, p: &Painter, drag: &BarDrag, xf: &Xform, body: Rect, rows: &[DisplayRow]) {
    let Some(i) = app.project.index_of_uid(drag.uid) else { return };
    let t = &app.project.tasks[i];
    match drag.kind {
        DragKind::Move | DragKind::Resize => {
            let (s, f) = drag_result(app, drag, xf);
            let d = rows.iter().position(|r| *r == DisplayRow::Task(i)).unwrap_or(0);
            let mid = body.top() + d as f32 * row_h() - app.scroll_y + row_h() / 2.0;
            let (x0, x1) = (xf.x(s), xf.x(f).max(xf.x(s) + 2.0));
            let r = Rect::from_min_max(pos2(x0, mid - 6.0), pos2(x1, mid + 6.0));
            p.rect_stroke(r, 0.0, Stroke::new(1.5, pal().drag), StrokeKind::Outside);
            let dur_min = if t.duration.elapsed { (f - s).num_minutes() } else { app.project.calendar.work_between(s, f) };
            let title = if drag.kind == DragKind::Move { "Task" } else { "Task (resize)" };
            paint_tooltip(
                ui,
                drag.cur + vec2(14.0, 16.0),
                &format!(
                    "{title}: {}\nStart:  {}\nFinish: {}\nDuration: {}",
                    t.name,
                    fmt_datetime(s),
                    fmt_datetime(f),
                    app.project.format_duration_minutes(dur_min, t.duration.unit)
                ),
            );
        }
        DragKind::Link => {
            let d = rows.iter().position(|r| *r == DisplayRow::Task(i)).unwrap_or(0);
            let from = pos2(
                (xf.x(drag.orig_start) + xf.x(drag.orig_finish)) / 2.0,
                body.top() + d as f32 * row_h() - app.scroll_y + row_h() / 2.0,
            );
            p.add(Shape::dashed_line(&[from, drag.cur], Stroke::new(1.5, pal().drag), 4.0, 3.0));
            let target = target_row(app, drag.cur, body, rows).filter(|&j| j != i);
            let tip = match target {
                Some(j) => format!(
                    "Finish-to-Start Link\nFrom Task: ({}) {}\nTo Task: ({}) {}",
                    i + 1,
                    t.name,
                    j + 1,
                    app.project.tasks[j].name
                ),
                None => format!("Finish-to-Start Link\nFrom Task: ({}) {}\nTo Task:", i + 1, t.name),
            };
            paint_tooltip(ui, drag.cur + vec2(14.0, 16.0), &tip);
        }
        DragKind::Pending { .. } => {}
    }
}

fn finish_drag(app: &mut ProjectFcgApp, drag: &BarDrag, xf: &Xform, body: Rect, rows: &[DisplayRow]) {
    let Some(i) = app.project.index_of_uid(drag.uid) else { return };
    match drag.kind {
        DragKind::Move => {
            let (s, _) = drag_result(app, drag, xf);
            if s == drag.orig_start {
                return;
            }
            app.edit_project("Move Task", |p| p.set_start(i, s));
        }
        DragKind::Resize => {
            let (s, f) = drag_result(app, drag, xf);
            if f == drag.orig_finish {
                return;
            }
            app.edit_project("Change Duration", |p| {
                let mut d = p.tasks[i].duration;
                d.minutes = if d.elapsed { (f - s).num_minutes() } else { p.calendar.work_between(s, f) };
                d.estimated = false;
                p.set_duration(i, d)
            });
        }
        DragKind::Link => {
            if let Some(j) = target_row(app, drag.cur, body, rows) {
                if j != i {
                    app.edit_project("Link Tasks", |p| {
                        p.add_link(i, j, LinkType::FS, crate::duration::Lag::default())
                    });
                    let u = app.project.tasks[j].uid;
                    app.select_single(RowRef::Task(u));
                }
            }
        }
        DragKind::Pending { .. } => {}
    }
}

/// Exposed for tests: date under an x coordinate.
pub fn date_label(dt: NaiveDateTime) -> String {
    fmt_date(dt)
}
