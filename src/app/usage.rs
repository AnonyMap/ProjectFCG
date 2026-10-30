//! Resource Usage and Task Usage views: time-phased work per period, with
//! overallocated periods highlighted (View ▸ Resource Usage / Task Usage).

use super::commands::Command;
use super::gantt::{paint_tier, tier_list, tier_marks, Xform};
use super::theme::*;
use std::collections::HashMap;
use super::{ProjectFcgApp, RowRef};
use crate::model::ResourceType;
use chrono::NaiveDateTime;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Shape, Stroke, StrokeKind};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageKind {
    Resource,
    Task,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum URow {
    Resource(usize),
    Task(usize),
    Assignment(usize),
}

pub struct UsageState {
    pub scroll_y: f32,
    pub active: Option<URow>,
    pub collapsed: HashSet<u32>,
    pub left_w: f32,
}

impl Default for UsageState {
    fn default() -> Self {
        UsageState {
            scroll_y: 0.0,
            active: None,
            collapsed: HashSet::new(),
            left_w: 420.0,
        }
    }
}

/// Lookups the view needs per row and per cell, built once per frame
/// (the model's helpers scan every assignment, which is quadratic over a
/// large project).
pub(crate) struct UIndex {
    by_task: HashMap<u32, Vec<usize>>,
    by_res: HashMap<u32, Vec<usize>>,
    row_of: HashMap<u32, usize>,
    /// Assignment index → is a work resource.
    work: Vec<bool>,
}

impl UIndex {
    pub(crate) fn build(p: &crate::model::Project) -> UIndex {
        let mut by_task: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut by_res: HashMap<u32, Vec<usize>> = HashMap::new();
        let rtype: HashMap<u32, ResourceType> = p.resources.iter().map(|r| (r.uid, r.rtype)).collect();
        let mut work = Vec::with_capacity(p.assignments.len());
        for (ai, a) in p.assignments.iter().enumerate() {
            by_task.entry(a.task).or_default().push(ai);
            by_res.entry(a.resource).or_default().push(ai);
            work.push(rtype.get(&a.resource) == Some(&ResourceType::Work));
        }
        let row_of = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        UIndex { by_task, by_res, row_of, work }
    }
    fn of_task(&self, uid: u32) -> &[usize] {
        self.by_task.get(&uid).map_or(&[], |v| v.as_slice())
    }
    fn of_res(&self, uid: u32) -> &[usize] {
        self.by_res.get(&uid).map_or(&[], |v| v.as_slice())
    }
}

/// Rows of the view in display order.
pub fn rows(app: &ProjectFcgApp, kind: UsageKind) -> Vec<URow> {
    rows_with(app, kind, &UIndex::build(&app.project))
}

fn rows_with(app: &ProjectFcgApp, kind: UsageKind, ix: &UIndex) -> Vec<URow> {
    let p = &app.project;
    let mut out = vec![];
    match kind {
        UsageKind::Resource => {
            for (ri, r) in p.resources.iter().enumerate() {
                out.push(URow::Resource(ri));
                if app.usage.collapsed.contains(&r.uid) {
                    continue;
                }
                let mut asg = ix.of_res(r.uid).to_vec();
                asg.sort_by_key(|&ai| ix.row_of.get(&p.assignments[ai].task).copied());
                out.extend(asg.into_iter().map(URow::Assignment));
            }
        }
        UsageKind::Task => {
            for i in p.visible_rows() {
                out.push(URow::Task(i));
                let t = &p.tasks[i];
                if t.calc.is_summary || app.usage.collapsed.contains(&(t.uid | 0x8000_0000)) {
                    continue;
                }
                out.extend(ix.of_task(t.uid).iter().map(|&ai| URow::Assignment(ai)));
            }
        }
    }
    out
}

fn hours(m: f64) -> String {
    if m.abs() < 0.5 {
        String::new()
    } else {
        format!("{}h", crate::duration::fmt_num(m / 60.0))
    }
}

/// Work of a row in [a, b), and whether it is overallocated there.
fn cell_value(app: &ProjectFcgApp, ix: &UIndex, r: URow, a: NaiveDateTime, b: NaiveDateTime) -> (f64, bool) {
    let p = &app.project;
    let over_in = |ri: usize| {
        p.calc.resources.get(ri).is_some_and(|c| c.over_intervals.iter().any(|(s, f)| *s < b && a < *f))
    };
    match r {
        URow::Resource(ri) => {
            let uid = p.resources[ri].uid;
            if p.resources[ri].rtype != ResourceType::Work {
                return (0.0, false);
            }
            let w: f64 = ix.of_res(uid).iter().map(|&ai| p.assignment_work_between(ai, a, b)).sum();
            (w, over_in(ri))
        }
        URow::Task(i) => {
            let t = &p.tasks[i];
            let end = if t.calc.is_summary { p.subtree_end(i) } else { i + 1 };
            let mut w = 0.0;
            for j in i..end {
                for &ai in ix.of_task(p.tasks[j].uid) {
                    if ix.work[ai] {
                        w += p.assignment_work_between(ai, a, b);
                    }
                }
            }
            (w, false)
        }
        URow::Assignment(ai) => {
            let asg = &p.assignments[ai];
            let w = p.assignment_work_between(ai, a, b);
            let over = p.calc.assignments.get(ai).is_some_and(|c| c.overallocated)
                && p.resource_index(asg.resource).is_some_and(over_in);
            (w, over && w > 0.0)
        }
    }
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect, kind: UsageKind) {
    let ix = UIndex::build(&app.project);
    let rows = rows_with(app, kind, &ix);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal().bg);
    let lw = app.usage.left_w.clamp(200.0, (rect.width() - 200.0).max(200.0));
    app.usage.left_w = lw;
    let left = Rect::from_min_max(rect.min, pos2(rect.left() + lw, rect.bottom()));
    let split = Rect::from_min_max(pos2(left.right(), rect.top()), pos2(left.right() + SPLITTER_W, rect.bottom()));
    let right = Rect::from_min_max(pos2(split.right(), rect.top()), pos2(rect.right() - SCROLLBAR, rect.bottom() - SCROLLBAR));
    let body_top = rect.top() + header_h();
    let body_l = Rect::from_min_max(pos2(left.left(), body_top), pos2(left.right(), rect.bottom() - SCROLLBAR));
    let body_r = Rect::from_min_max(pos2(right.left(), body_top), right.max);
    app.rows_view_h = body_l.height();
    app.gantt_view_w = right.width();
    let content_h = (rows.len() as f32 + 2.0) * row_h();
    app.usage.scroll_y = app.usage.scroll_y.clamp(0.0, (content_h - body_l.height()).max(0.0));

    // ---- input ----
    let resp_l = ui.interact(body_l, ui.scope_id().with("usage_l"), Sense::click());
    let resp_r = ui.interact(body_r, ui.scope_id().with("usage_r"), Sense::click_and_drag());
    for resp in [&resp_l, &resp_r] {
        if resp.hovered() {
            let (dy, dx, shift, zoom, ptr) =
                ui.input(|i| (i.smooth_scroll_delta.y, i.smooth_scroll_delta.x, i.modifiers.shift, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom != 1.0 && resp.rect == body_r {
                let frac = ptr.map(|p| ((p.x - body_r.left()) / body_r.width()).clamp(0.0, 1.0)).unwrap_or(0.5);
                app.zoom_to(app.gantt.ppd * zoom, frac);
            } else if shift {
                app.gantt.left_days -= ((dy + dx) / app.gantt.ppd) as f64;
            } else {
                app.usage.scroll_y = (app.usage.scroll_y - dy).max(0.0);
                app.gantt.left_days -= (dx / app.gantt.ppd) as f64;
            }
        }
    }
    let sy = app.usage.scroll_y;
    let row_at = |p: egui::Pos2| -> Option<URow> {
        let d = ((p.y - body_top + sy) / row_h()).floor();
        (d >= 0.0).then(|| rows.get(d as usize).copied()).flatten()
    };
    let ctx = ui.ctx().clone();
    for resp in [&resp_l, &resp_r] {
        if resp.clicked() || resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some(r) = row_at(p) {
                    // collapse triangle on resource/task rows
                    if resp.clicked() && p.x < left.left() + 22.0 {
                        let key = match r {
                            URow::Resource(ri) => Some(app.project.resources[ri].uid),
                            URow::Task(i) => Some(app.project.tasks[i].uid | 0x8000_0000),
                            _ => None,
                        };
                        if let Some(k) = key {
                            if !app.usage.collapsed.remove(&k) {
                                app.usage.collapsed.insert(k);
                            }
                            continue;
                        }
                    }
                    select_row(app, r);
                    if resp.double_clicked() {
                        match r {
                            URow::Resource(_) => app.execute(Command::ResourceInfo, &ctx),
                            URow::Task(_) => app.execute(Command::TaskInfo, &ctx),
                            URow::Assignment(_) => app.execute(Command::TaskInfo, &ctx),
                        }
                    }
                }
            }
        }
    }
    let sresp = ui.interact(split, ui.scope_id().with("usage_split"), Sense::drag());
    if sresp.hovered() || sresp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if sresp.dragged() {
        app.usage.left_w += sresp.drag_delta().x;
    }

    let p = &app.project;
    let first = (app.usage.scroll_y / row_h()).floor() as usize;
    let vis = (body_l.height() / row_h()).ceil() as usize + 1;
    let font = FontId::proportional(text_size());
    let sfont = FontId::proportional(small_size());
    let grid = paper(p.format.gridlines.line_color);

    // ---- left table: name | work | details ----
    let lp = painter.with_clip_rect(body_l);
    let work_w = (text_size() * 6.0).round();
    let det_w = (text_size() * 4.5).round();
    let name_r = left.right() - work_w - det_w;
    for d in first..(first + vis).min(rows.len()) {
        let y = body_top + d as f32 * row_h() - app.usage.scroll_y;
        let rr = Rect::from_min_size(pos2(left.left(), y), vec2(lw, row_h()));
        let r = rows[d];
        if app.usage.active == Some(r) {
            lp.rect_filled(rr, 0.0, pal().selected_row);
        }
        lp.line_segment([pos2(left.left(), y + row_h() - 0.5), pos2(left.right(), y + row_h() - 0.5)], Stroke::new(1.0, grid));
        for x in [name_r, name_r + work_w] {
            lp.line_segment([pos2(x, y), pos2(x, y + row_h())], Stroke::new(1.0, grid));
        }
        let (name, work, bold, indent, over, collapsible, collapsed) = match r {
            URow::Resource(ri) => {
                let res = &p.resources[ri];
                let w = match res.rtype {
                    ResourceType::Work => p.format_duration_minutes(p.calc.resources.get(ri).map_or(0, |c| c.work), crate::duration::DurUnit::Hour),
                    // material work is a quantity, in the resource's own units
                    ResourceType::Material => {
                        let q: f64 = ix.of_res(res.uid).iter().map(|&ai| p.assignments[ai].units).sum();
                        material_qty(q, &res.material_label)
                    }
                    ResourceType::Cost => String::new(),
                };
                let over = p.calc.resources.get(ri).is_some_and(|c| c.overallocated);
                (res.name.clone(), w, true, 0.0, over, true, app.usage.collapsed.contains(&res.uid))
            }
            URow::Task(i) => {
                let t = &p.tasks[i];
                let w = p.format_duration_minutes(t.calc.work, crate::duration::DurUnit::Hour);
                let ind = (t.outline_level as f32 - 1.0) * indent_px();
                let has = !t.calc.is_summary && !ix.of_task(t.uid).is_empty();
                (t.name.clone(), w, t.calc.is_summary, ind, t.calc.overallocated, has, app.usage.collapsed.contains(&(t.uid | 0x8000_0000)))
            }
            URow::Assignment(ai) => {
                let a = &p.assignments[ai];
                let label = match kind {
                    UsageKind::Resource => ix.row_of.get(&a.task).map(|&i| p.tasks[i].name.clone()).unwrap_or_default(),
                    UsageKind::Task => p.resource_index(a.resource).map(|i| p.resources[i].name.clone()).unwrap_or_default(),
                };
                let w = match p.resource_index(a.resource).map(|i| (&p.resources[i].rtype, &p.resources[i].material_label)) {
                    Some((ResourceType::Work, _)) => p.format_duration_minutes(a.work, crate::duration::DurUnit::Hour),
                    Some((ResourceType::Material, label)) => material_qty(a.units, label),
                    _ => String::new(),
                };
                let base = match kind {
                    UsageKind::Task => ix.row_of.get(&a.task).map(|&i| (p.tasks[i].outline_level as f32) * indent_px()).unwrap_or(0.0),
                    UsageKind::Resource => indent_px() * 1.5,
                };
                let over = p.calc.assignments.get(ai).is_some_and(|c| c.overallocated);
                (label, w, false, base, over, false, false)
            }
        };
        let cy = y + row_h() / 2.0;
        let mut x = left.left() + 6.0 + indent;
        if collapsible {
            let c = pos2(x + 4.0, cy);
            let pts = if collapsed {
                vec![c + vec2(-2.5, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.5, 4.0)]
            } else {
                vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
            };
            lp.add(Shape::convex_polygon(pts, pal().glyph, Stroke::NONE));
        }
        x += 14.0;
        if over && matches!(r, URow::Resource(_)) {
            super::table::icon_overalloc(&lp, pos2(x + 4.0, cy));
            x += 13.0;
        }
        let col = if over { pal().warn } else { pal().text };
        let clip = lp.with_clip_rect(Rect::from_min_max(pos2(left.left(), y), pos2(name_r - 2.0, y + row_h())));
        clip.text(pos2(x, cy), Align2::LEFT_CENTER, &name, font.clone(), col);
        if bold {
            clip.text(pos2(x + 0.6, cy), Align2::LEFT_CENTER, &name, font.clone(), col);
        }
        lp.text(pos2(name_r + work_w - 5.0, cy), Align2::RIGHT_CENTER, &work, font.clone(), col);
        lp.text(pos2(name_r + work_w + 5.0, cy), Align2::LEFT_CENTER, "Work", sfont.clone(), pal().text_dim);
    }
    // left header
    let lh = Rect::from_min_max(left.min, pos2(left.right(), body_top));
    painter.rect_filled(lh, 0.0, pal().header_bg);
    painter.line_segment([lh.left_bottom(), lh.right_bottom()], Stroke::new(1.0, pal().header_border));
    let title = match kind {
        UsageKind::Resource => "Resource Name",
        UsageKind::Task => "Task Name",
    };
    painter.text(pos2(left.left() + 8.0, lh.center().y), Align2::LEFT_CENTER, title, FontId::proportional(text_size() - 0.5), pal().text);
    painter.text(pos2(name_r + work_w - 5.0, lh.center().y), Align2::RIGHT_CENTER, "Work", FontId::proportional(text_size() - 0.5), pal().text);
    painter.text(pos2(name_r + work_w + 5.0, lh.center().y), Align2::LEFT_CENTER, "Details", FontId::proportional(text_size() - 0.5), pal().text);
    for x in [name_r, name_r + work_w] {
        painter.line_segment([pos2(x, lh.top()), pos2(x, lh.bottom())], Stroke::new(1.0, pal().header_border));
    }
    painter.rect_filled(split, 0.0, pal().splitter);

    // ---- right: timescaled grid ----
    let xf = Xform { left: body_r.left(), left_days: app.gantt.left_days, ppd: app.gantt.ppd };
    let tiers = tier_list(app);
    let (bu, bc) = *tiers.last().unwrap();
    let marks = tier_marks(&xf, bu, bc, body_r.left(), body_r.right());
    let rp = painter.with_clip_rect(body_r);
    for d in first..(first + vis).min(rows.len()) {
        let y = body_top + d as f32 * row_h() - app.usage.scroll_y;
        let r = rows[d];
        let row_bg = match r {
            URow::Resource(_) | URow::Task(_) => Some(pal().group_row),
            _ => None,
        };
        if let Some(bg) = row_bg {
            rp.rect_filled(Rect::from_min_size(pos2(body_r.left(), y), vec2(body_r.width(), row_h())), 0.0, bg);
        }
        if app.usage.active == Some(r) {
            rp.rect_filled(Rect::from_min_size(pos2(body_r.left(), y), vec2(body_r.width(), row_h())), 0.0, pal().selected_row.gamma_multiply(0.6));
        }
        for w in marks.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (x0, x1) = (xf.x(a), xf.x(b));
            if x1 < body_r.left() || x0 > body_r.right() {
                continue;
            }
            let (v, over) = cell_value(app, &ix, r, a, b);
            let cell = Rect::from_min_max(pos2(x0, y), pos2(x1, y + row_h()));
            if over {
                rp.rect_filled(cell.shrink(0.5), 0.0, pal().over_bg);
            }
            let s = hours(v);
            if !s.is_empty() && cell.width() > 14.0 {
                let c = rp.with_clip_rect(cell.intersect(body_r));
                c.text(pos2(cell.right() - 4.0, cell.center().y), Align2::RIGHT_CENTER, s, sfont.clone(), if over { pal().warn } else { pal().text });
            }
            rp.line_segment([pos2(x1 - 0.5, y), pos2(x1 - 0.5, y + row_h())], Stroke::new(1.0, grid));
        }
        rp.line_segment([pos2(body_r.left(), y + row_h() - 0.5), pos2(body_r.right(), y + row_h() - 0.5)], Stroke::new(1.0, grid));
    }
    // timescale header
    let header = Rect::from_min_max(pos2(right.left(), rect.top()), pos2(right.right(), body_top));
    let hp = painter.with_clip_rect(header);
    let nt = tiers.len().max(1);
    let th = header_h() / nt as f32;
    for (k, (u, c)) in tiers.iter().enumerate() {
        let r = Rect::from_min_max(pos2(header.left(), header.top() + th * k as f32), pos2(header.right(), header.top() + th * (k + 1) as f32));
        paint_tier(&hp, r, &xf, *u, *c, k == 0 && nt > 1);
    }
    painter.rect_stroke(right, 0.0, Stroke::new(1.0, pal().header_border), StrokeKind::Inside);

    // scrollbars
    let vbar = Rect::from_min_max(pos2(rect.right() - SCROLLBAR, body_top), pos2(rect.right(), rect.bottom() - SCROLLBAR));
    if let Some(v) = super::scrollbar(ui, vbar, true, app.usage.scroll_y, body_l.height(), content_h.max(body_l.height()), "usage_v") {
        app.usage.scroll_y = v;
    }
    let hbar = Rect::from_min_max(pos2(right.left(), rect.bottom() - SCROLLBAR), pos2(right.right(), rect.bottom()));
    let (ps, pf) = app.project_span();
    let view_days = (right.width() / app.gantt.ppd) as f64;
    let lo = (super::to_days(ps) - 30.0).min(app.gantt.left_days);
    let hi = (super::to_days(pf) + 60.0).max(app.gantt.left_days + view_days);
    let off = ((app.gantt.left_days - lo) as f32) * app.gantt.ppd;
    if let Some(v) = super::scrollbar(ui, hbar, false, off, right.width(), ((hi - lo) as f32) * app.gantt.ppd, "usage_h") {
        app.gantt.left_days = lo + (v / app.gantt.ppd) as f64;
    }
}

fn select_row(app: &mut ProjectFcgApp, r: URow) {
    app.usage.active = Some(r);
    match r {
        URow::Resource(ri) => {
            app.sheet.active = Some(app.project.resources[ri].uid);
            app.sheet.selected = vec![app.project.resources[ri].uid];
        }
        URow::Task(i) => app.select_single(RowRef::Task(app.project.tasks[i].uid)),
        URow::Assignment(ai) => {
            let a = app.project.assignments[ai];
            app.select_single(RowRef::Task(a.task));
            app.sheet.active = Some(a.resource);
            app.sheet.selected = vec![a.resource];
            app.usage.active = Some(r);
        }
    }
}

/// Move the usage selection up/down (arrow keys).
pub fn move_selection(app: &mut ProjectFcgApp, kind: UsageKind, delta: i32) {
    let rows = rows(app, kind);
    if rows.is_empty() {
        return;
    }
    let cur = app.usage.active.and_then(|a| rows.iter().position(|r| *r == a)).map(|x| x as i32).unwrap_or(-1);
    let n = (cur + delta).clamp(0, rows.len() as i32 - 1) as usize;
    select_row(app, rows[n]);
    let y = n as f32 * row_h();
    if y < app.usage.scroll_y {
        app.usage.scroll_y = y;
    } else if y + row_h() > app.usage.scroll_y + app.rows_view_h {
        app.usage.scroll_y = y + row_h() - app.rows_view_h;
    }
}

/// "5 devices", or a bare "5" for a material resource without a label.
fn material_qty(q: f64, label: &str) -> String {
    let n = crate::duration::fmt_num(q);
    if label.is_empty() {
        n
    } else {
        format!("{n} {label}")
    }
}
