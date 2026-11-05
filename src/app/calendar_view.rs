//! Calendar view (Task ▸ Calendar), as in Project: a month of weeks with
//! the tasks as bars across the days they run, wrapping from week to week.
//!
//! * Bars are stacked in lanes per week; when a week has more tasks than
//!   fit, "+N more" says so and lists them on hover.
//! * Nonworking days (project calendar) are shaded; today is outlined.
//! * Follows the view's filter. Summary tasks aren't shown (as in
//!   Project's default Calendar), milestones are ◆ markers.
//! * ◀ ▶ (or scrolling) change month; Today goes back to this month.
//!   Click a bar to select its task, double-click for Task Information.

use super::{DisplayRow, ProjectFcgApp, RowRef};
use chrono::{Datelike, Duration, NaiveDate};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke};

#[derive(Default)]
pub struct CalendarState {
    /// First of the month shown (None: the selected task's, or today's).
    pub month: Option<NaiveDate>,
    /// The task last brought into view.
    shown: Option<u32>,
    /// Scroll accumulated toward the next month change.
    wheel: f32,
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

pub fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let m0 = d.year() * 12 + d.month0() as i32 + n;
    NaiveDate::from_ymd_opt(m0.div_euclid(12), m0.rem_euclid(12) as u32 + 1, 1).unwrap_or(d)
}

/// The weeks (each starting Sunday) that show a month.
pub fn weeks_of(month: NaiveDate) -> Vec<NaiveDate> {
    let first = first_of_month(month);
    let start = first - Duration::days(first.weekday().num_days_from_sunday() as i64);
    let next = add_months(first, 1);
    let mut v = vec![];
    let mut w = start;
    while w < next {
        v.push(w);
        w += Duration::days(7);
    }
    v
}

/// A task's bar within one week: (first day 0–6, last day 0–6).
pub fn week_span(start: NaiveDate, finish: NaiveDate, week: NaiveDate) -> Option<(usize, usize)> {
    let end = week + Duration::days(6);
    if finish < week || start > end {
        return None;
    }
    let a = (start.max(week) - week).num_days() as usize;
    let b = (finish.min(end) - week).num_days() as usize;
    Some((a, b))
}

/// Lanes for one week's bars: each (row, a, b) gets the first lane free
/// over its days. Longer, earlier bars go first, as Project draws them.
pub fn lanes(mut bars: Vec<(usize, usize, usize)>) -> Vec<(usize, usize, usize, usize)> {
    bars.sort_by(|x, y| x.1.cmp(&y.1).then((y.2 - y.1).cmp(&(x.2 - x.1))).then(x.0.cmp(&y.0)));
    let mut used: Vec<[bool; 7]> = vec![];
    let mut out = vec![];
    for (r, a, b) in bars {
        let lane = (0..).find(|&l| l >= used.len() || (a..=b).all(|d| !used[l][d])).unwrap();
        if lane >= used.len() {
            used.push([false; 7]);
        }
        for d in a..=b {
            used[lane][d] = true;
        }
        out.push((r, a, b, lane));
    }
    out
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let pal = super::theme::pal();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.bg);
    let p = &app.project;
    let today = chrono::Local::now().date_naive();

    // which month: follow a task selected elsewhere
    let active = match app.active_row {
        RowRef::Task(u) => Some(u),
        RowRef::Blank => None,
    };
    if active != app.calendar.shown {
        app.calendar.shown = active;
        if let Some(s) = active.and_then(|u| p.index_of_uid(u)).and_then(|r| p.tasks[r].calc.start) {
            let m = first_of_month(s.date());
            let shown = app.calendar.month.unwrap_or(m);
            let (s0, f0) = (weeks_of(shown)[0], *weeks_of(shown).last().unwrap() + Duration::days(6));
            if s.date() < s0 || s.date() > f0 {
                app.calendar.month = Some(m);
            }
        }
    }
    let month = app.calendar.month.unwrap_or_else(|| {
        let d = active.and_then(|u| p.index_of_uid(u)).and_then(|r| p.tasks[r].calc.start).map(|d| d.date()).unwrap_or(p.start.date().max(today.min(p.calc.finish.map_or(today, |f| f.date()))));
        first_of_month(d)
    });
    // keep showing this month until moved (clearing the selection doesn't jump)
    app.calendar.month = Some(month);

    // ---- header: ◀ Month Year ▶  Today ----
    let head_h = 34.0;
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), head_h));
    let mut go: i32 = 0;
    let mut to_today = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 4.0))), |ui| {
        ui.horizontal_centered(|ui| {
            if ui.button("◀").on_hover_text("Previous month").clicked() {
                go = -1;
            }
            ui.label(egui::RichText::new(month.format("%B %Y").to_string()).size(17.0).strong());
            if ui.button("▶").on_hover_text("Next month").clicked() {
                go = 1;
            }
            ui.add_space(8.0);
            if ui.button("Today").clicked() {
                to_today = true;
            }
        });
    });
    let days_h = 22.0;
    let grid = Rect::from_min_max(pos2(rect.left(), head.bottom() + days_h), rect.max);
    let weeks = weeks_of(month);
    let cw = grid.width() / 7.0;
    let wh = (grid.height() / weeks.len() as f32).max(60.0);
    for (k, name) in ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].iter().enumerate() {
        let x = grid.left() + k as f32 * cw;
        painter.text(pos2(x + cw / 2.0, head.bottom() + days_h / 2.0), Align2::CENTER_CENTER, if cw < 90.0 { &name[..3] } else { name }, FontId::proportional(12.0), pal.text_dim);
    }

    // ---- day cells ----
    let cell = |w: usize, d: usize| Rect::from_min_size(pos2(grid.left() + d as f32 * cw, grid.top() + w as f32 * wh), vec2(cw, wh));
    for (w, ws) in weeks.iter().enumerate() {
        for d in 0..7 {
            let date = *ws + Duration::days(d as i64);
            let r = cell(w, d);
            let working = p.calendar.is_working_day(date);
            let fill = if !working { pal.cal_nonworking } else { pal.bg };
            painter.rect_filled(r, 0.0, fill);
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, pal.grid), egui::StrokeKind::Inside);
            let in_month = date.month() == month.month();
            let num_col = if in_month { pal.text } else { pal.text_disabled };
            painter.text(r.right_top() + vec2(-5.0, 3.0), Align2::RIGHT_TOP, date.day().to_string(), FontId::proportional(12.0), num_col);
            if date == today {
                painter.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(2.0, pal.today), egui::StrokeKind::Inside);
            }
        }
    }

    // ---- bars ----
    let rows: Vec<usize> = app
        .display_rows()
        .into_iter()
        .filter_map(|r| if let DisplayRow::Task(i) = r { Some(i) } else { None })
        .filter(|&i| !p.tasks[i].calc.is_summary)
        .collect();
    let bar_h = 18.0;
    let top_pad = 20.0;
    let lanes_fit = (((wh - top_pad - 4.0) / (bar_h + 3.0)).floor() as usize).max(1);
    let selected: std::collections::HashSet<u32> = app.selected.iter().copied().collect();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let mut hit: Option<usize> = None;
    let mut more_tip: Option<String> = None;
    let (crit_fill, crit_line) = if pal.dark { (Color32::from_rgb(150, 60, 60), Color32::from_rgb(240, 110, 100)) } else { (Color32::from_rgb(234, 128, 128), Color32::from_rgb(192, 48, 48)) };
    for (w, ws) in weeks.iter().enumerate() {
        let bars: Vec<(usize, usize, usize)> = rows
            .iter()
            .filter_map(|&r| {
                let c = &p.tasks[r].calc;
                let (s, f) = (c.start?.date(), c.finish?.date());
                // a task finishing at midnight ends the day before
                let f = if c.finish?.time() == chrono::NaiveTime::MIN && f > s { f - Duration::days(1) } else { f };
                week_span(s, f, *ws).map(|(a, b)| (r, a, b))
            })
            .collect();
        let placed = lanes(bars);
        let mut hidden: Vec<Vec<usize>> = vec![vec![]; 7];
        for &(r, a, b, lane) in &placed {
            if lane >= lanes_fit {
                for hd in hidden.iter_mut().take(b + 1).skip(a) {
                    hd.push(r);
                }
                continue;
            }
            let t = &p.tasks[r];
            let y = cell(w, 0).top() + top_pad + lane as f32 * (bar_h + 3.0);
            let x0 = grid.left() + a as f32 * cw + 3.0;
            let x1 = grid.left() + (b + 1) as f32 * cw - 3.0;
            let br = Rect::from_min_max(pos2(x0, y), pos2(x1, y + bar_h));
            if pointer.is_some_and(|pp| br.contains(pp)) {
                hit = Some(r);
            }
            let sel = selected.contains(&t.uid);
            let clip = painter.with_clip_rect(br.intersect(rect));
            if t.is_milestone() {
                let c = pos2(x0 + 8.0, br.center().y);
                let s = 6.0;
                clip.add(egui::Shape::convex_polygon(vec![c + vec2(0.0, -s), c + vec2(s, 0.0), c + vec2(0.0, s), c + vec2(-s, 0.0)], pal.milestone, Stroke::NONE));
                clip.text(pos2(x0 + 18.0, br.center().y), Align2::LEFT_CENTER, &t.name, FontId::proportional(11.5), pal.text);
            } else {
                let (fill, line) = if t.calc.critical { (crit_fill, crit_line) } else { (pal.bar_fill, pal.bar_border) };
                clip.rect_filled(br, 3.0, fill);
                // progress inside the bar
                let pct = (t.calc.percent_complete / 100.0).clamp(0.0, 1.0) as f32;
                if pct > 0.0 {
                    let pr = Rect::from_min_max(pos2(br.left(), br.center().y - 2.0), pos2(br.left() + br.width() * pct, br.center().y + 2.0));
                    clip.rect_filled(pr, 0.0, line);
                }
                clip.rect_stroke(br, 3.0, Stroke::new(if sel { 2.4 } else { 1.0 }, if sel { pal.accent } else { line }), egui::StrokeKind::Inside);
                let label = format!("{}, {}", t.name, t.duration.format(&p.settings.durations));
                let text_col = if pal.dark { Color32::WHITE } else { Color32::from_gray(20) };
                clip.text(br.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.5), text_col);
            }
        }
        for (d, hd) in hidden.iter().enumerate() {
            if hd.is_empty() {
                continue;
            }
            let r = cell(w, d);
            let at = pos2(r.left() + 5.0, r.bottom() - 3.0);
            let g = painter.text(at, Align2::LEFT_BOTTOM, format!("+{} more", hd.len()), FontId::proportional(11.0), pal.accent);
            if pointer.is_some_and(|pp| g.expand(3.0).contains(pp)) {
                more_tip = Some(hd.iter().map(|&r| format!("{}: {}", r + 1, p.tasks[r].name)).collect::<Vec<_>>().join("\n"));
            }
        }
    }

    // ---- interaction ----
    let resp = ui.interact(Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max), ui.scope_id().with("calendar"), Sense::click());
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        app.calendar.wheel += dy;
        if app.calendar.wheel.abs() > 120.0 {
            go = if app.calendar.wheel > 0.0 { -1 } else { 1 };
            app.calendar.wheel = 0.0;
        }
    }
    if go != 0 {
        app.calendar.month = Some(add_months(month, go));
    } else if to_today {
        app.calendar.month = Some(first_of_month(today));
    }
    if resp.clicked() {
        match hit {
            Some(r) => {
                let u = app.project.tasks[r].uid;
                app.select_single(RowRef::Task(u));
                app.row_selected = true;
                app.calendar.shown = Some(u);
            }
            None => app.select_single(RowRef::Blank),
        }
    }
    if resp.double_clicked() {
        if let Some(r) = hit {
            let u = app.project.tasks[r].uid;
            app.select_single(RowRef::Task(u));
            app.calendar.shown = Some(u);
            app.execute(super::commands::Command::TaskInfo, &ui.ctx().clone());
        }
    }
    if let Some(tip) = more_tip {
        resp.on_hover_text(tip);
    } else if let Some(r) = hit {
        let t = &app.project.tasks[r];
        let f = |d: Option<chrono::NaiveDateTime>| d.map(super::super::model::fmt_datetime).unwrap_or_default();
        resp.on_hover_text(format!("{}: {}\n{} – {}\nDouble-click for Task Information", r + 1, t.name, f(t.calc.start), f(t.calc.finish)));
    }
}
