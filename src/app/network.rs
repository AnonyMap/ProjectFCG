//! Network Diagram view (Task ▸ Network Diagram), as in Project: each task
//! is a box, links are lines between them, and the boxes are arranged in
//! columns by the order the work happens.
//!
//! * Box: name; start and ID; finish and duration; resources. Critical
//!   tasks are red. A finished task is crossed out (X), one under way
//!   has a single diagonal. Milestones are hexagons, summaries
//!   parallelograms, manually scheduled tasks dashed.
//! * Layout: a task's column is one past its latest predecessor
//!   (summaries sit one column before their subtasks), then each column
//!   is ordered to keep lines from crossing (two barycenter sweeps).
//! * Follows the view's filter, outline (collapsed summaries) and sort.
//! * Drag the background to pan, scroll to move, Ctrl+scroll (or View ▸
//!   Zoom) to zoom. Click a box to select it, double-click for Task
//!   Information.

use super::{DisplayRow, ProjectFcgApp, RowRef};
use crate::model::TaskMode;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke};
use std::collections::HashMap;

pub const BOX_W: f32 = 212.0;
pub const BOX_H: f32 = 84.0;
const GAP_X: f32 = 64.0;
const GAP_Y: f32 = 26.0;
const MARGIN: f32 = 24.0;

#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Task row → top-left of its box (zoom 1).
    pub pos: HashMap<usize, Pos2>,
    /// (pred row, succ row, critical) for every link between boxes shown.
    pub links: Vec<(usize, usize, bool)>,
    pub size: egui::Vec2,
}

pub struct NetworkState {
    pub pan: egui::Vec2,
    pub zoom: f32,
    /// (rows shown, history generation) the layout was made for.
    key: Option<(Vec<usize>, u64)>,
    pub layout: Layout,
    /// The task last scrolled into view (to follow selection changes).
    shown: Option<u32>,
}

impl Default for NetworkState {
    fn default() -> Self {
        NetworkState { pan: vec2(0.0, 0.0), zoom: 1.0, key: None, layout: Layout::default(), shown: None }
    }
}

/// Lay out `rows` (task rows, in view order).
pub fn layout(p: &crate::model::Project, rows: &[usize]) -> Layout {
    let n = rows.len();
    let at: HashMap<usize, usize> = rows.iter().enumerate().map(|(k, &r)| (r, k)).collect();
    let uid_row: HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    // edges between shown boxes: links, and summary → its shown subtasks
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    let mut links = vec![];
    for (k, &r) in rows.iter().enumerate() {
        for l in &p.tasks[r].predecessors {
            if let Some(&pk) = uid_row.get(&l.pred).and_then(|pr| at.get(pr)) {
                out[pk].push(k);
                let crit = p.tasks[rows[pk]].calc.critical && p.tasks[r].calc.critical;
                links.push((rows[pk], r, crit));
            }
        }
        if let Some(par) = p.parent_of(r).and_then(|pp| at.get(&pp)) {
            out[*par].push(k);
        }
    }
    // longest path (Kahn); boxes caught in a loop stay in column 0
    let mut indeg = vec![0usize; n];
    for v in &out {
        for &w in v {
            indeg[w] += 1;
        }
    }
    let mut col = vec![0usize; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&k| indeg[k] == 0).collect();
    while let Some(v) = queue.pop_front() {
        for &w in &out[v] {
            col[w] = col[w].max(col[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    // a summary sits just before its first subtask, not wherever its own
    // links (often none) would put it; deepest summaries first
    let mut by_depth: Vec<usize> = (0..n).filter(|&k| p.tasks[rows[k]].calc.is_summary).collect();
    by_depth.sort_by_key(|&k| std::cmp::Reverse(p.tasks[rows[k]].outline_level));
    for k in by_depth {
        let kids: Vec<usize> = out[k].iter().copied().filter(|&w| p.parent_of(rows[w]) == Some(rows[k])).collect();
        let linked: Vec<usize> = out[k].iter().copied().filter(|w| !kids.contains(w)).collect();
        if let Some(first) = kids.iter().map(|&w| col[w]).min() {
            // as far right as its children and its own successors allow
            let limit = linked.iter().map(|&w| col[w]).min().map_or(usize::MAX, |c| c.saturating_sub(1));
            col[k] = col[k].max(first.saturating_sub(1)).min(limit.max(col[k]));
        }
    }
    let ncols = col.iter().copied().max().map_or(0, |m| m + 1);
    let mut columns: Vec<Vec<usize>> = vec![vec![]; ncols];
    for k in 0..n {
        columns[col[k]].push(k); // view order to start with
    }
    // reduce crossings: order each column by the mean position of the
    // boxes it's linked to in the previous column, then back again
    let mut preds: Vec<Vec<usize>> = vec![vec![]; n];
    for (v, ws) in out.iter().enumerate() {
        for &w in ws {
            preds[w].push(v);
        }
    }
    let mut slot = vec![0f32; n];
    let set_slots = |columns: &Vec<Vec<usize>>, slot: &mut Vec<f32>| {
        for c in columns {
            for (i, &k) in c.iter().enumerate() {
                slot[k] = i as f32;
            }
        }
    };
    set_slots(&columns, &mut slot);
    for sweep in 0..2 {
        let forward = sweep % 2 == 0;
        let order: Vec<usize> = if forward { (1..ncols).collect() } else { (0..ncols.saturating_sub(1)).rev().collect() };
        for c in order {
            let mut keyed: Vec<(f32, usize, usize)> = columns[c]
                .iter()
                .enumerate()
                .map(|(i, &k)| {
                    let nb: &Vec<usize> = if forward { &preds[k] } else { &out[k] };
                    let ys: Vec<f32> = nb.iter().filter(|&&o| col[o] != c).map(|&o| slot[o]).collect();
                    let bary = if ys.is_empty() { i as f32 } else { ys.iter().sum::<f32>() / ys.len() as f32 };
                    (bary, i, k)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
            columns[c] = keyed.into_iter().map(|x| x.2).collect();
            for (i, &k) in columns[c].iter().enumerate() {
                slot[k] = i as f32;
            }
        }
    }
    let mut pos = HashMap::new();
    let mut h = 0f32;
    for (c, ks) in columns.iter().enumerate() {
        for (i, &k) in ks.iter().enumerate() {
            pos.insert(rows[k], pos2(MARGIN + c as f32 * (BOX_W + GAP_X), MARGIN + i as f32 * (BOX_H + GAP_Y)));
        }
        h = h.max(ks.len() as f32 * (BOX_H + GAP_Y));
    }
    Layout { pos, links, size: vec2(MARGIN * 2.0 + ncols as f32 * (BOX_W + GAP_X), MARGIN * 2.0 + h) }
}

fn critical_colors() -> (Color32, Color32) {
    if super::theme::pal().dark {
        (Color32::from_rgb(110, 40, 40), Color32::from_rgb(240, 110, 100))
    } else {
        (Color32::from_rgb(253, 226, 224), Color32::from_rgb(192, 48, 48))
    }
}

/// The box outline for a task (parallelogram, hexagon or rectangle).
fn box_points(r: Rect, summary: bool, milestone: bool) -> Vec<Pos2> {
    let s = r.height() * 0.18;
    if summary {
        vec![pos2(r.left() + s, r.top()), r.right_top(), pos2(r.right() - s, r.bottom()), r.left_bottom()]
    } else if milestone {
        vec![pos2(r.left() + s, r.top()), pos2(r.right() - s, r.top()), pos2(r.right(), r.center().y), pos2(r.right() - s, r.bottom()), pos2(r.left() + s, r.bottom()), pos2(r.left(), r.center().y)]
    } else {
        vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
    }
}

fn arrow(painter: &egui::Painter, pts: &[Pos2], stroke: Stroke) {
    painter.add(Shape::line(pts.to_vec(), stroke));
    if let [.., a, b] = pts {
        let d = (*b - *a).normalized() * 7.0;
        let n = vec2(-d.y, d.x) * 0.55;
        painter.add(Shape::convex_polygon(vec![*b, *b - d + n, *b - d - n], stroke.color, Stroke::NONE));
    }
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let pal = super::theme::pal();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.bg);
    let rows: Vec<usize> = app.display_rows().into_iter().filter_map(|r| if let DisplayRow::Task(i) = r { Some(i) } else { None }).collect();
    let key = (rows.clone(), app.history.generation());
    if app.network.key.as_ref() != Some(&key) {
        app.network.layout = layout(&app.project, &rows);
        app.network.key = Some(key);
    }
    let z = app.network.zoom;

    // ---- interaction: pan, zoom, follow the selection ----
    let resp = ui.interact(rect, ui.scope_id().with("network"), Sense::click_and_drag());
    if resp.hovered() {
        let (scroll, ctrl, hover) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.command, i.pointer.hover_pos()));
        if ctrl && scroll.y != 0.0 {
            let nz = (z * (1.0 + scroll.y * 0.002)).clamp(0.25, 2.5);
            // zoom about the pointer
            if let Some(hp) = hover {
                let world = (hp - rect.min + app.network.pan) / z;
                app.network.pan = world * nz - (hp - rect.min);
            }
            app.network.zoom = nz;
        } else {
            app.network.pan -= scroll;
        }
    }
    if resp.dragged() {
        app.network.pan -= resp.drag_delta();
    }
    let z = app.network.zoom;
    let size = app.network.layout.size * z;
    let maxp = (size - rect.size()).max(vec2(0.0, 0.0));
    app.network.pan = app.network.pan.clamp(vec2(0.0, 0.0), maxp);
    // a task selected elsewhere (Find, Go To) is brought into view
    let active = match app.active_row {
        RowRef::Task(u) => Some(u),
        RowRef::Blank => None,
    };
    if active != app.network.shown {
        app.network.shown = active;
        if let Some(p) = active.and_then(|u| app.project.index_of_uid(u)).and_then(|r| app.network.layout.pos.get(&r)) {
            let b = Rect::from_min_size(*p * z, vec2(BOX_W, BOX_H) * z);
            let view = Rect::from_min_size(app.network.pan.to_pos2(), rect.size());
            if !view.contains_rect(b) {
                app.network.pan = (b.center().to_vec2() - rect.size() / 2.0).clamp(vec2(0.0, 0.0), maxp);
            }
        }
    }
    let origin = rect.min - app.network.pan;
    let to_screen = |p: Pos2| origin + p.to_vec2() * z;

    // ---- links ----
    let lay = &app.network.layout;
    let (crit_fill, crit_line) = critical_colors();
    for &(a, b, crit) in &lay.links {
        let (Some(pa), Some(pb)) = (lay.pos.get(&a), lay.pos.get(&b)) else { continue };
        let s = to_screen(pos2(pa.x + BOX_W, pa.y + BOX_H / 2.0));
        let e = to_screen(pos2(pb.x, pb.y + BOX_H / 2.0));
        let bb = Rect::from_two_pos(s, e).expand(4.0);
        if !bb.intersects(rect) {
            continue;
        }
        let stroke = Stroke::new(if crit { 1.6 } else { 1.1 } * z.max(0.6), if crit { crit_line } else { pal.text_dim });
        if e.x > s.x + 8.0 * z {
            let mx = e.x - GAP_X * z / 2.0;
            arrow(&painter, &[s, pos2(mx, s.y), pos2(mx, e.y), e], stroke);
        } else {
            // backwards (a loop, or a summary's link): go round underneath
            let below = s.y.max(e.y) + (BOX_H / 2.0 + GAP_Y / 2.0) * z;
            arrow(&painter, &[s, pos2(s.x + 10.0 * z, s.y), pos2(s.x + 10.0 * z, below), pos2(e.x - 10.0 * z, below), pos2(e.x - 10.0 * z, e.y), e], stroke);
        }
    }

    // ---- boxes ----
    let p = &app.project;
    let selected: std::collections::HashSet<u32> = app.selected.iter().copied().collect();
    let mut hit: Option<usize> = None;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let small = FontId::proportional(10.5 * z.clamp(0.6, 1.6));
    let bold = FontId::new(12.0 * z.clamp(0.6, 1.6), egui::FontFamily::Proportional);
    for &r in &rows {
        let Some(tl) = lay.pos.get(&r) else { continue };
        let b = Rect::from_min_size(to_screen(*tl), vec2(BOX_W, BOX_H) * z);
        if !b.intersects(rect) {
            continue;
        }
        if pointer.is_some_and(|pp| b.contains(pp)) {
            hit = Some(r);
        }
        let t = &p.tasks[r];
        let c = &t.calc;
        let summary = c.is_summary;
        let milestone = t.is_milestone();
        let (fill, line) = if c.critical { (crit_fill, crit_line) } else { (pal.bg, pal.bar_border) };
        let sel = selected.contains(&t.uid);
        let width = if sel { 2.6 } else if summary { 1.6 } else { 1.2 };
        let stroke = Stroke::new(width, if sel { pal.accent } else { line });
        let pts = box_points(b, summary, milestone);
        painter.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
        if t.mode == TaskMode::Manual {
            let mut closed = pts.clone();
            closed.push(pts[0]);
            painter.extend(Shape::dashed_line(&closed, stroke, 6.0, 4.0));
        } else {
            painter.add(Shape::closed_line(pts, stroke));
        }
        // progress marks: X when done, / when under way
        let pct = c.percent_complete;
        let mark = Stroke::new(1.0, line.gamma_multiply(0.35));
        if pct >= 100.0 {
            painter.line_segment([b.left_top(), b.right_bottom()], mark);
            painter.line_segment([b.left_bottom(), b.right_top()], mark);
        } else if pct > 0.0 {
            painter.line_segment([b.left_bottom(), b.right_top()], mark);
        }
        if z < 0.45 {
            continue; // too small to read: shapes only
        }
        // inside the slanted sides of summaries and milestones
        let slant = if summary || milestone { b.height() * 0.18 } else { 0.0 };
        let inner = b.shrink2(vec2(6.0 * z + slant, 6.0 * z));
        let clip = painter.with_clip_rect(inner.intersect(rect));
        let name = if t.active { t.name.clone() } else { format!("{} (inactive)", t.name) };
        clip.text(pos2(inner.center().x, inner.top()), Align2::CENTER_TOP, name, bold.clone(), pal.text);
        let fmt = |d: Option<chrono::NaiveDateTime>| d.map(|d| p.format.layout.date_format.format(d)).unwrap_or_else(|| "NA".into());
        let dur = if summary { p.format_duration_minutes(c.summary_duration, crate::duration::DurUnit::Day) } else { t.duration.format(&p.settings.durations) };
        let lh = 15.0 * z.clamp(0.6, 1.6);
        let y0 = inner.top() + lh * 1.3;
        let colx = inner.left() + inner.width() * 0.62;
        let rows_txt = [
            (format!("Start: {}", fmt(c.start)), format!("ID: {}", r + 1)),
            (format!("Finish: {}", fmt(c.finish)), format!("Dur: {dur}")),
        ];
        for (k, (l, rt)) in rows_txt.iter().enumerate() {
            let y = y0 + k as f32 * lh;
            let lr = clip.text(pos2(inner.left(), y), Align2::LEFT_TOP, l, small.clone(), pal.text);
            // the right column after the left text, however long the date
            clip.text(pos2(colx.max(lr.right() + 6.0 * z), y), Align2::LEFT_TOP, rt, small.clone(), pal.text);
        }
        let res = p.resource_names_text(r);
        if !res.is_empty() || !summary {
            clip.text(pos2(inner.left(), y0 + 2.0 * lh), Align2::LEFT_TOP, format!("Res: {res}"), small.clone(), pal.text_dim);
        }
    }
    if rows.is_empty() {
        painter.text(rect.center(), Align2::CENTER_CENTER, "No tasks to show", FontId::proportional(14.0), pal.text_dim);
    }

    // ---- clicks ----
    if resp.clicked() || resp.secondary_clicked() {
        match hit {
            Some(r) => {
                let u = app.project.tasks[r].uid;
                if ui.input(|i| i.modifiers.command) {
                    if !app.selected.contains(&u) {
                        app.selected.push(u);
                    }
                    app.active_row = RowRef::Task(u);
                } else {
                    app.select_single(RowRef::Task(u));
                }
                app.row_selected = true;
                app.network.shown = Some(u);
            }
            None => app.select_single(RowRef::Blank),
        }
    }
    if resp.double_clicked() {
        if let Some(r) = hit {
            let u = app.project.tasks[r].uid;
            app.select_single(RowRef::Task(u));
            app.network.shown = Some(u);
            app.execute(super::commands::Command::TaskInfo, &ui.ctx().clone());
        }
    }
    if let Some(r) = hit {
        let t = &app.project.tasks[r];
        resp.on_hover_text(format!("{}: {}\nDouble-click for Task Information", r + 1, t.name));
    }
}
