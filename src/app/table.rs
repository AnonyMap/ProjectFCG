//! The Entry table on the left of the Gantt Chart view.

use super::columns::{self, ColumnId, EditKind};
use super::commands::Command;
use super::theme::*;
use super::{CellEdit, DisplayRow, ProjectFcgApp, RowRef};
use crate::model::{fmt_date, TaskMode};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind};


struct Layout {
    header: Rect,
    body: Rect,
    hbar: Rect,
    /// x of each column's left edge (scrolled), plus the right edge at the end.
    col_x: Vec<f32>,
}

fn layout(app: &ProjectFcgApp, rect: Rect) -> Layout {
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), header_h()));
    let body = Rect::from_min_max(
        pos2(rect.left(), rect.top() + header_h()),
        pos2(rect.right(), rect.bottom() - SCROLLBAR),
    );
    let hbar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - SCROLLBAR), rect.max);
    let mut x = rect.left() + row_header_w() - app.table_scroll_x;
    let mut col_x = vec![];
    for c in &app.columns {
        col_x.push(x);
        x += c.width;
    }
    col_x.push(x);
    Layout {
        header,
        body,
        hbar,
        col_x,
    }
}

fn row_y(body: Rect, d: usize, scroll: f32) -> f32 {
    body.top() + d as f32 * row_h() - scroll
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect, rows: &[DisplayRow]) {
    let lay = layout(app, rect);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal().bg);

    // ---------- interactions on the body ----------
    let body_resp = ui.interact(lay.body, ui.scope_id().with("table_body"), Sense::click_and_drag());
    let hover = body_resp.hover_pos();
    // scrolling (before hit-testing so this frame uses the new offset)
    if body_resp.hovered() {
        let (dy, dx, shift) = ui.input(|i| (i.smooth_scroll_delta.y, i.smooth_scroll_delta.x, i.modifiers.shift));
        if shift {
            app.table_scroll_x = (app.table_scroll_x - dy - dx).max(0.0);
        } else {
            app.scroll_y = (app.scroll_y - dy).max(0.0);
            app.table_scroll_x = (app.table_scroll_x - dx).max(0.0);
        }
    }
    let scroll_y = app.scroll_y;
    let hit = |p: Pos2| -> (usize, Option<usize>) {
        let d = ((p.y - lay.body.top() + scroll_y) / row_h()).floor().max(0.0) as usize;
        let col = if p.x < rect.left() + row_header_w() {
            None
        } else {
            (0..app.columns.len()).find(|&k| p.x >= lay.col_x[k] && p.x < lay.col_x[k + 1])
        };
        (d, col)
    };
    let row_ref_of = |d: usize, app: &ProjectFcgApp| -> Option<RowRef> {
        match rows.get(d) {
            Some(DisplayRow::Task(i)) => Some(RowRef::Task(app.project.tasks[*i].uid)),
            Some(DisplayRow::ProjectSummary) => None,
            None => Some(RowRef::Blank),
        }
    };

    let mods = ui.input(|i| i.modifiers);
    if body_resp.double_clicked() {
        if let Some(p) = body_resp.interact_pointer_pos() {
            let (d, _) = hit(p);
            if let Some(RowRef::Task(_)) = row_ref_of(d, app) {
                app.edit = None;
                let ctx = ui.ctx().clone();
                app.execute(Command::TaskInfo, &ctx);
            }
        }
    } else if body_resp.clicked() || body_resp.secondary_clicked() {
        if let Some(p) = body_resp.interact_pointer_pos() {
            let (d, col) = hit(p);
            let secondary = body_resp.secondary_clicked();
            if let Some(r) = row_ref_of(d, app) {
                let was_active = app.active_row == r && col == Some(app.active_col);
                // collapse triangle in the Name column
                let mut toggled = false;
                if let (Some(k), Some(DisplayRow::Task(i))) = (col, rows.get(d)) {
                    if app.columns[k].id == ColumnId::Name && app.project.tasks[*i].calc.is_summary {
                        let lvl = app.project.tasks[*i].outline_level as f32;
                        let tri_x = lay.col_x[k] + 4.0 + (lvl - 1.0) * indent_px() + app.outline_prefix_w(*i);
                        if p.x >= tri_x - 2.0 && p.x <= tri_x + 12.0 {
                            app.commit_edit();
                            app.project.toggle_collapse(*i);
                            toggled = true;
                        }
                    }
                }
                if !toggled {
                    if !app.commit_edit() {
                        return;
                    }
                    if let Some(k) = col {
                        app.active_col = k;
                    }
                    let uid = match r {
                        RowRef::Task(u) => Some(u),
                        RowRef::Blank => None,
                    };
                    if secondary {
                        if let Some(u) = uid {
                            if !app.selected.contains(&u) {
                                app.select_single(r);
                            }
                            app.active_row = r;
                        }
                    } else if mods.ctrl || mods.command {
                        if let Some(u) = uid {
                            if let Some(pos) = app.selected.iter().position(|&x| x == u) {
                                app.selected.remove(pos);
                            } else {
                                app.selected.push(u);
                            }
                            app.active_row = r;
                        }
                    } else if mods.shift {
                        if let (Some(u), Some(a)) = (uid, app.anchor) {
                            select_range(app, rows, a, u);
                            app.active_row = r;
                        }
                    } else {
                        app.select_single(r);
                        if was_active && col.is_some() {
                            app.begin_edit(None);
                        }
                    }
                }
            }
        }
    } else if body_resp.dragged() {
        // drag-select rows
        if let (Some(start), Some(cur)) = (ui.input(|i| i.pointer.press_origin()), body_resp.interact_pointer_pos()) {
            let (d0, _) = hit(start);
            let (d1, _) = hit(cur);
            if let (Some(RowRef::Task(a)), Some(RowRef::Task(b))) = (row_ref_of(d0, app), row_ref_of(d1.min(rows.len().saturating_sub(1)), app)) {
                select_range(app, rows, a, b);
                app.active_row = RowRef::Task(b);
                app.anchor = Some(a);
            }
            // auto-scroll when dragging past the edges
            if cur.y > lay.body.bottom() - 4.0 {
                app.scroll_y += 6.0;
            } else if cur.y < lay.body.top() + 4.0 {
                app.scroll_y = (app.scroll_y - 6.0).max(0.0);
            }
        }
    }

    body_resp.context_menu(|ui| task_context_menu(app, ui));

    // ---------- paint rows ----------
    let first = (app.scroll_y / row_h()).floor() as usize;
    let visible = (lay.body.height() / row_h()).ceil() as usize + 1;
    let body_painter = painter.with_clip_rect(lay.body);
    let cells_clip = Rect::from_min_max(pos2(rect.left() + row_header_w(), lay.body.top()), lay.body.max);
    let cell_painter = painter.with_clip_rect(cells_clip);
    let font = FontId::proportional(text_size());

    let selected_set: std::collections::HashSet<u32> = app.selected.iter().copied().collect();
    let mut indicator_tip: Option<String> = None;

    for d in first..first + visible {
        let y = row_y(lay.body, d, app.scroll_y);
        let row_rect = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h()));
        let disp = rows.get(d).copied();
        let (uid, is_sel) = match disp {
            Some(DisplayRow::Task(i)) => {
                let u = app.project.tasks[i].uid;
                (Some(u), selected_set.contains(&u))
            }
            _ => (None, false),
        };
        if is_sel {
            body_painter.rect_filled(row_rect, 0.0, pal().selected_row);
        } else if let Some(DisplayRow::Task(i)) = disp {
            // View ▸ Highlight
            if app.view.highlight != super::commands::Filter::AllTasks
                && super::filter_matches(&app.project, i, app.view.highlight)
            {
                body_painter.rect_filled(row_rect, 0.0, paper(app.project.format.text.highlighted_fill));
            }
        }
        // grid (Format ▸ Gridlines ▸ Sheet rows / columns)
        let gl = app.project.format.gridlines;
        let grid = paper(gl.line_color);
        if gl.sheet_rows {
            body_painter.line_segment(
                [pos2(rect.left(), y + row_h() - 0.5), pos2(rect.right(), y + row_h() - 0.5)],
                Stroke::new(1.0, grid),
            );
        }
        if gl.sheet_columns {
            for k in 0..app.columns.len() {
                let x = lay.col_x[k + 1] - 0.5;
                cell_painter.line_segment([pos2(x, y), pos2(x, y + row_h())], Stroke::new(1.0, grid));
            }
        }
        // row header
        let rh = Rect::from_min_size(pos2(rect.left(), y), vec2(row_header_w(), row_h()));
        let rh_fill = if is_sel { pal().tint } else { pal().header_bg };
        body_painter.rect_filled(rh, 0.0, rh_fill);
        body_painter.line_segment([rh.right_top(), rh.right_bottom()], Stroke::new(1.0, pal().header_border));
        body_painter.line_segment(
            [pos2(rh.left(), y + row_h() - 0.5), pos2(rh.right(), y + row_h() - 0.5)],
            Stroke::new(1.0, pal().header_border),
        );

        match disp {
            Some(DisplayRow::Task(i)) => {
                body_painter.text(
                    pos2(rh.right() - 5.0, rh.center().y),
                    Align2::RIGHT_CENTER,
                    (i + 1).to_string(),
                    FontId::proportional(small_size()),
                    pal().text,
                );
                for (k, c) in app.columns.iter().enumerate() {
                    let cell = Rect::from_min_max(pos2(lay.col_x[k], y), pos2(lay.col_x[k + 1], y + row_h()));
                    if cell.right() < cells_clip.left() || cell.left() > cells_clip.right() {
                        continue;
                    }
                    let tip = paint_cell(app, &cell_painter, cell, i, c.id, &font);
                    if let (Some(t), Some(h)) = (tip, hover) {
                        if cell.contains(h) {
                            indicator_tip = Some(t);
                        }
                    }
                }
            }
            Some(DisplayRow::ProjectSummary) => {
                body_painter.text(
                    pos2(rh.right() - 5.0, rh.center().y),
                    Align2::RIGHT_CENTER,
                    "0",
                    FontId::proportional(small_size()),
                    pal().text,
                );
                for (k, c) in app.columns.iter().enumerate() {
                    let cell = Rect::from_min_max(pos2(lay.col_x[k], y), pos2(lay.col_x[k + 1], y + row_h()));
                    paint_project_summary_cell(app, &cell_painter, cell, c.id, &font);
                }
            }
            None => {}
        }

        // active cell border
        let is_active_row = match (disp, app.active_row) {
            (Some(DisplayRow::Task(_)), RowRef::Task(u)) => uid == Some(u),
            (None, RowRef::Blank) => d == rows.len(),
            _ => false,
        };
        if is_active_row && app.edit.is_none() {
            if let Some(k) = (app.active_col < app.columns.len()).then_some(app.active_col) {
                let cell = Rect::from_min_max(pos2(lay.col_x[k], y), pos2(lay.col_x[k + 1], y + row_h()));
                cell_painter.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(2.0, pal().active_cell), StrokeKind::Inside);
            }
        }
    }
    if let Some(t) = indicator_tip {
        body_resp.on_hover_text_at_pointer(t);
    }

    // ---------- header ----------
    paint_header(app, ui, &painter, rect, &lay);

    // ---------- inline editor ----------
    if app.edit.is_some() {
        show_editor(app, ui, rows, &lay, rect);
    }

    // ---------- horizontal scrollbar ----------
    let content_w: f32 = row_header_w() + app.columns.iter().map(|c| c.width).sum::<f32>();
    if let Some(v) = super::scrollbar(ui, lay.hbar, false, app.table_scroll_x, rect.width(), content_w.max(rect.width()), "table_hscroll") {
        app.table_scroll_x = v;
    }
    let max_x = (content_w - rect.width()).max(0.0);
    // keep the active column in view when it changes (keyboard navigation,
    // Home/End, a column jump) — otherwise you can edit a cell you can't see
    if app.shown_col != app.active_col {
        app.shown_col = app.active_col;
        if app.active_col < app.columns.len() {
            let left: f32 = app.columns[..app.active_col].iter().map(|c| c.width).sum();
            let w = app.columns[app.active_col].width;
            let view_w = (rect.width() - row_header_w()).max(1.0);
            if left < app.table_scroll_x {
                app.table_scroll_x = left;
            } else if left + w > app.table_scroll_x + view_w {
                app.table_scroll_x = (left + w - view_w).min(left);
            }
        }
    }
    app.table_scroll_x = app.table_scroll_x.clamp(0.0, max_x);
}

fn select_range(app: &mut ProjectFcgApp, rows: &[DisplayRow], a: u32, b: u32) {
    let pa = rows.iter().position(|r| matches!(r, DisplayRow::Task(i) if app.project.tasks[*i].uid == a));
    let pb = rows.iter().position(|r| matches!(r, DisplayRow::Task(i) if app.project.tasks[*i].uid == b));
    if let (Some(pa), Some(pb)) = (pa, pb) {
        let (lo, hi) = (pa.min(pb), pa.max(pb));
        app.selected = rows[lo..=hi]
            .iter()
            .filter_map(|r| match r {
                DisplayRow::Task(i) => Some(app.project.tasks[*i].uid),
                _ => None,
            })
            .collect();
    }
}

impl ProjectFcgApp {
    /// Width of the "1.2.3 " prefix drawn before names when outline
    /// numbers are shown (approximate, monospace-free estimate).
    pub fn outline_prefix_w(&self, i: usize) -> f32 {
        if self.view.outline_number {
            self.project.tasks[i].calc.outline_number.len() as f32 * 7.0 + 6.0
        } else {
            0.0
        }
    }
}

fn paint_text(p: &Painter, cell: Rect, text: &str, align: Align, font: &FontId, color: Color32) {
    let clip = p.with_clip_rect(cell.shrink2(vec2(1.0, 0.0)).intersect(p.clip_rect()));
    let (pos, anchor) = match align {
        Align::LEFT => (pos2(cell.left() + 4.0, cell.center().y), Align2::LEFT_CENTER),
        Align::RIGHT => (pos2(cell.right() - 5.0, cell.center().y), Align2::RIGHT_CENTER),
        _ => (cell.center(), Align2::CENTER_CENTER),
    };
    clip.text(pos, anchor, text, font.clone(), color);
}

/// Simulated bold (egui's default font set has no bold face).
fn paint_bold(p: &Painter, pos: Pos2, anchor: Align2, text: &str, font: &FontId, color: Color32) {
    p.text(pos, anchor, text, font.clone(), color);
    p.text(pos + vec2(0.6, 0.0), anchor, text, font.clone(), color);
}

/// Paint one cell; returns tooltip text for indicator cells.
fn paint_cell(app: &ProjectFcgApp, p: &Painter, cell: Rect, i: usize, col: ColumnId, font: &FontId) -> Option<String> {
    let proj = &app.project;
    let t = &proj.tasks[i];
    let clip = p.with_clip_rect(cell.intersect(p.clip_rect()));
    match col {
        ColumnId::Indicators => {
            let mut tips = vec![];
            let mut x = cell.left() + 4.0;
            let cy = cell.center().y;
            if t.percent_complete >= 100.0 && !t.calc.is_summary {
                icon_check(&clip, pos2(x + 5.0, cy));
                tips.push("This task was completed.".to_string());
                x += 13.0;
            }
            if t.constraint != crate::model::ConstraintType::ASAP {
                icon_calendar(&clip, pos2(x + 5.0, cy), t.constraint.is_inflexible());
                tips.push(format!(
                    "This task has a '{}' constraint{}.",
                    t.constraint.label(),
                    t.constraint_date.map(|d| format!(" on {}", fmt_date(d))).unwrap_or_default()
                ));
                x += 13.0;
            }
            if let Some(c) = &t.calc.conflict {
                icon_warning(&clip, pos2(x + 5.0, cy));
                tips.push(c.clone());
                x += 13.0;
            }
            if t.calc.deadline_missed {
                icon_warning(&clip, pos2(x + 5.0, cy));
                tips.push("This task finishes after its deadline.".to_string());
                x += 13.0;
            }
            if t.calc.overallocated {
                icon_overalloc(&clip, pos2(x + 5.0, cy));
                tips.push("This task has resources assigned that are overallocated. Resource ▸ Level All can resolve it.".to_string());
                x += 13.0;
            }
            if t.is_sprint {
                icon_sprint(&clip, pos2(x + 5.0, cy));
                let st = proj.sprint_stats(Some(t.uid));
                let est = proj.board.estimate;
                tips.push(if st.tickets == 0 {
                    "Sprint with no tickets yet. Right-click ▸ Open on Board to plan it.".to_string()
                } else {
                    format!(
                        "Sprint: {} of {} done ({} tickets). Its % complete comes from the Board.",
                        crate::duration::fmt_num(st.done),
                        est.unit(st.total),
                        st.tickets
                    )
                });
                x += 13.0;
            }
            if let Some(r) = &t.recurrence {
                icon_recurring(&clip, pos2(x + 5.0, cy));
                tips.push(format!("Recurring task: {}. Task Information edits the series.", r.describe()));
                x += 13.0;
            }
            if !t.board_deps.is_empty() {
                let waiting = &t.calc.board_waiting;
                icon_board_dep(&clip, pos2(x + 5.0, cy), !waiting.is_empty(), !t.calc.board_notes.is_empty());
                let mut tip = String::from("Waits on board items: ");
                tip += &t.board_deps.iter().map(|d| {
                    let k = proj.ticket_key(d.ticket);
                    match proj.ticket(d.ticket) {
                        Some(x) if proj.is_done(x) => format!("{k} (done)"),
                        Some(x) => format!("{k} {}", x.sprint.and_then(|u| proj.task_by_uid(u)).map(|s| format!("(in {})", s.name)).unwrap_or_else(|| "(not planned)".into())),
                        None => k,
                    }
                }).collect::<Vec<_>>().join(", ");
                for n in &t.calc.board_notes {
                    tip += &format!("\n{n}");
                }
                tips.push(tip);
                x += 13.0;
            }
            if !t.notes.is_empty() {
                icon_note(&clip, pos2(x + 5.0, cy));
                tips.push(format!("Notes: '{}'", t.notes.lines().next().unwrap_or("")));
            }
            if tips.is_empty() {
                None
            } else {
                Some(tips.join("\n"))
            }
        }
        ColumnId::Mode => {
            let c = cell.center();
            match t.mode {
                TaskMode::Manual => icon_pin(&clip, c),
                TaskMode::Auto => icon_auto(&clip, c),
            }
            Some(match t.mode {
                TaskMode::Manual => "Manually Scheduled".into(),
                TaskMode::Auto => "Auto Scheduled".into(),
            })
            .filter(|_| false)
        }
        ColumnId::Name => {
            let lvl = t.outline_level as f32;
            let mut x = cell.left() + 4.0 + (lvl - 1.0) * indent_px();
            if app.view.outline_number {
                clip.text(pos2(x, cell.center().y), Align2::LEFT_CENTER, &t.calc.outline_number, font.clone(), pal().text_dim);
                x += app.outline_prefix_w(i);
            }
            let (color, bold) = row_style(app, i);
            if t.calc.is_summary {
                let c = pos2(x + 5.0, cell.center().y);
                let pts = if t.collapsed {
                    vec![c + vec2(-2.5, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.5, 4.0)]
                } else {
                    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
                };
                clip.add(egui::Shape::convex_polygon(pts, pal().glyph, Stroke::NONE));
                x += 14.0;
            }
            if bold {
                paint_bold(&clip, pos2(x, cell.center().y), Align2::LEFT_CENTER, &t.name, font, color);
            } else {
                let r = clip.text(pos2(x, cell.center().y), Align2::LEFT_CENTER, &t.name, font.clone(), color);
                if app.inactive_rows.get(i).copied().unwrap_or(false) {
                    clip.line_segment([pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, color));
                }
            }
            if app.inactive_rows.get(i).copied().unwrap_or(false) {
                return Some("Inactive: kept in the plan, but it doesn't move other tasks, roll up, or book resources.".into());
            }
            None
        }
        _ => {
            let text = columns::cell_text(proj, i, col);
            let (mut color, bold) = row_style(app, i);
            if col == ColumnId::TotalSlack && t.calc.total_slack < 0 {
                color = pal().warn;
            }
            if col == ColumnId::Status && t.calc.late {
                color = pal().warn;
            }
            if bold {
                let (pos, anchor) = match col.align() {
                    Align::RIGHT => (pos2(cell.right() - 5.0, cell.center().y), Align2::RIGHT_CENTER),
                    _ => (pos2(cell.left() + 4.0, cell.center().y), Align2::LEFT_CENTER),
                };
                paint_bold(&clip, pos, anchor, &text, font, color);
            } else {
                paint_text(&clip, cell, &text, col.align(), font, color);
            }
            None
        }
    }
}

/// Text colour and weight for a row (Format ▸ Text Styles).
pub fn row_style(app: &ProjectFcgApp, i: usize) -> (Color32, bool) {
    let t = &app.project.tasks[i];
    let ts = &app.project.format.text;
    let style = if t.calc.is_summary {
        ts.summary
    } else if t.is_milestone() {
        ts.milestone
    } else if t.calc.critical && (ts.critical.color.is_some() || ts.critical.bold) {
        ts.critical
    } else {
        ts.normal
    };
    let default_col = if t.mode == TaskMode::Manual { pal().manual_text } else { pal().text };
    let col = style.color.or(ts.normal.color).map(ink_text).unwrap_or(default_col);
    if app.inactive_rows.get(i).copied().unwrap_or(false) {
        return (pal().text_disabled, false);
    }
    (col, style.bold || (ts.normal.bold && !t.calc.is_summary))
}

fn paint_project_summary_cell(app: &ProjectFcgApp, p: &Painter, cell: Rect, col: ColumnId, font: &FontId) {
    let proj = &app.project;
    let (s, f) = app.project_span();
    let text = match col {
        ColumnId::Name => proj.title.clone(),
        ColumnId::Duration => proj.format_duration_minutes(proj.calendar.work_between(s, f), crate::duration::DurUnit::Day),
        ColumnId::Start | ColumnId::EarlyStart => fmt_date(s),
        ColumnId::Finish | ColumnId::EarlyFinish => fmt_date(f),
        ColumnId::Mode => {
            icon_auto(p, cell.center());
            String::new()
        }
        _ => String::new(),
    };
    let clip = p.with_clip_rect(cell.intersect(p.clip_rect()));
    let (pos, anchor) = match col.align() {
        Align::RIGHT => (pos2(cell.right() - 5.0, cell.center().y), Align2::RIGHT_CENTER),
        _ => (pos2(cell.left() + 4.0, cell.center().y), Align2::LEFT_CENTER),
    };
    paint_bold(&clip, pos, anchor, &text, font, pal().text);
}

fn paint_header(app: &mut ProjectFcgApp, ui: &mut egui::Ui, painter: &Painter, rect: Rect, lay: &Layout) {
    painter.rect_filled(lay.header, 0.0, pal().header_bg);
    painter.line_segment(
        [pos2(rect.left(), lay.header.bottom() - 0.5), pos2(rect.right(), lay.header.bottom() - 0.5)],
        Stroke::new(1.0, pal().header_border),
    );
    let clip = painter.with_clip_rect(Rect::from_min_max(
        pos2(rect.left() + row_header_w(), lay.header.top()),
        lay.header.max,
    ));
    // corner cell (select all)
    let corner = Rect::from_min_size(lay.header.min, vec2(row_header_w(), header_h()));
    painter.line_segment([corner.right_top(), corner.right_bottom()], Stroke::new(1.0, pal().header_border));
    let corner_resp = ui.interact(corner, ui.scope_id().with("corner"), Sense::click());
    if corner_resp.clicked() {
        let ctx = ui.ctx().clone();
        app.execute(Command::SelectAll, &ctx);
    }
    let font = FontId::proportional(text_size() - 0.5);
    let mut resize: Option<(usize, f32)> = None;
    let mut menu_col: Option<usize> = None;
    for k in 0..app.columns.len() {
        let cell = Rect::from_min_max(pos2(lay.col_x[k], lay.header.top()), pos2(lay.col_x[k + 1], lay.header.bottom()));
        let id = app.columns[k].id;
        let title = id.title();
        let active = k == app.active_col;
        if active {
            clip.rect_filled(cell, 0.0, pal().header_hover);
        }
        let galley = clip.layout(title.to_string(), font.clone(), pal().text, (cell.width() - 8.0).max(10.0));
        let pos = match id.align() {
            Align::Center => cell.center() - galley.size() / 2.0,
            _ => pos2(cell.left() + 4.0, cell.center().y - galley.size().y / 2.0),
        };
        clip.with_clip_rect(cell.intersect(clip.clip_rect())).galley(pos, galley, pal().text);
        clip.line_segment(
            [pos2(cell.right() - 0.5, cell.top()), pos2(cell.right() - 0.5, cell.bottom())],
            Stroke::new(1.0, pal().header_border),
        );
        if cell.right() < rect.left() + row_header_w() || cell.left() > rect.right() {
            continue;
        }
        // right-click: column menu
        let hresp = ui.interact(cell.intersect(lay.header), ui.scope_id().with(("hdr", k)), Sense::click());
        if hresp.secondary_clicked() || hresp.clicked() {
            app.active_col = k;
        }
        if hresp.context_menu_opened() || hresp.secondary_clicked() {
            menu_col = Some(k);
        }
        hresp.context_menu(|ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.separator();
            ui.menu_button("Insert Column", |ui| {
                let present: Vec<_> = app.columns.iter().map(|s| s.id).collect();
                if let Some(c) = super::columns::insert_column_menu(ui, &present) {
                    app.columns.insert(k, super::ColumnState { id: c, width: c.default_width() });
                    ui.close();
                }
            });
            if ui
                .add_enabled(app.columns.len() > 1 && id != ColumnId::Name, egui::Button::new("Hide Column"))
                .clicked()
            {
                app.columns.remove(k);
                app.active_col = app.active_col.min(app.columns.len() - 1);
                ui.close();
            }
            if ui.button("Best Fit (reset width)").clicked() {
                app.columns[k].width = id.default_width();
                ui.close();
            }
        });
        // resize handle
        let handle = Rect::from_center_size(pos2(cell.right(), cell.center().y), vec2(8.0, cell.height()));
        let r = ui.interact(handle, ui.scope_id().with(("colresize", k)), Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        }
        if r.dragged() {
            resize = Some((k, r.drag_delta().x));
        }
    }
    let _ = menu_col;
    if let Some((k, dx)) = resize {
        app.columns[k].width = (app.columns[k].width + dx).max(18.0);
    }
}

fn show_editor(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rows: &[DisplayRow], lay: &Layout, rect: Rect) {
    let Some(edit) = app.edit.clone() else { return };
    let d = match edit.row {
        RowRef::Task(u) => rows
            .iter()
            .position(|r| matches!(r, DisplayRow::Task(i) if app.project.tasks[*i].uid == u)),
        RowRef::Blank => Some(rows.len()),
    };
    let Some(d) = d else {
        app.edit = None;
        return;
    };
    if edit.col >= app.columns.len() {
        app.edit = None;
        return;
    }
    let y = row_y(lay.body, d, app.scroll_y);
    let mut cell = Rect::from_min_max(pos2(lay.col_x[edit.col], y), pos2(lay.col_x[edit.col + 1], y + row_h()));
    cell = cell.intersect(Rect::from_min_max(pos2(rect.left() + row_header_w(), lay.body.top()), lay.body.max));
    if cell.width() < 8.0 || cell.height() < 8.0 {
        // scrolled out of view: keep the edit but don't draw
        return;
    }
    let col = app.columns[edit.col].id;
    let kind = col.edit_kind();
    let id = ui.scope_id().with("cell_editor");
    let mut text = edit.text.clone();
    let picker_w = if matches!(kind, EditKind::Date | EditKind::Choice) { 18.0 } else { 0.0 };
    let text_rect = Rect::from_min_max(cell.min, pos2(cell.right() - picker_w, cell.bottom()));
    let te = egui::TextEdit::singleline(&mut text)
        .id(id)
        .font(FontId::proportional(text_size()))
        .margin(vec2(3.0, 2.0))
        .desired_width(text_rect.width());
    let resp = ui.put(text_rect, te);
    if !edit.focus_requested {
        resp.request_focus();
        let len = text.chars().count();
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
            let range = if edit.select_all {
                egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))
            } else {
                egui::text::CCursorRange::one(egui::text::CCursor::new(len))
            };
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), id);
        }
    }
    // drop-down button for dates / choices
    let mut picker_open = edit.picker_open;
    let mut picked: Option<String> = None;
    if picker_w > 0.0 {
        let btn = Rect::from_min_max(pos2(cell.right() - picker_w, cell.top()), cell.max);
        let b = ui.put(btn, egui::Button::new("▾").small());
        if b.clicked() {
            picker_open = !picker_open;
        }
        if picker_open {
            let area = egui::Area::new(id.with("picker"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos2(cell.left(), cell.bottom() + 1.0));
            area.show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| match kind {
                    EditKind::Date => {
                        let base = crate::model::parse_date(&text, app.project.default_start_time())
                            .map(|d| d.date())
                            .unwrap_or_else(|_| app.project.start.date());
                        if let Some(date) = super::datepicker::calendar(ui, id.with("cal"), base) {
                            picked = Some(fmt_date(date.and_hms_opt(0, 0, 0).unwrap()));
                        }
                    }
                    EditKind::Choice => {
                        ui.set_min_width(cell.width().max(140.0));
                        for c in col.choices() {
                            if ui.selectable_label(text == c, c).clicked() {
                                picked = Some(c.to_string());
                            }
                        }
                    }
                    _ => {}
                });
            });
        }
    }

    let (enter, tab, esc, shift, up, down) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Tab),
            i.key_pressed(egui::Key::Escape),
            i.modifiers.shift,
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
        )
    });

    if let Some(e) = app.edit.as_mut() {
        e.text = text.clone();
        e.focus_requested = true;
        e.picker_open = picker_open;
    }
    if let Some(v) = picked {
        if let Some(e) = app.edit.as_mut() {
            e.text = v;
        }
        app.commit_edit();
        return;
    }
    if esc {
        app.cancel_edit();
        return;
    }
    let pointer_on_picker = picker_open
        && ui.ctx().layer_id_at(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()).is_some_and(|l| l.order == egui::Order::Foreground);
    if (resp.has_focus() || resp.lost_focus()) && (up || down) {
        if app.commit_edit() {
            app.move_active(if up { -1 } else { 1 }, 0, false);
        }
        return;
    }
    if resp.lost_focus() && !pointer_on_picker {
        if app.commit_edit() {
            if enter {
                app.move_active(if shift { -1 } else { 1 }, 0, false);
            } else if tab {
                app.move_active(0, if shift { -1 } else { 1 }, false);
            }
        }
    }
}

pub fn task_context_menu(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let run = |app: &mut ProjectFcgApp, ui: &mut egui::Ui, label: &str, cmd: Command, enabled: bool| {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            app.execute(cmd, &ctx);
            ui.close();
        }
    };
    let has_task = app.active_index().is_some();
    run(app, ui, "Cut Task", Command::Cut, has_task);
    run(app, ui, "Copy Task", Command::Copy, has_task);
    run(app, ui, "Paste", Command::Paste, !app.clipboard.is_empty());
    ui.separator();
    run(app, ui, "Insert Task", Command::InsertTask, true);
    run(app, ui, "Delete Task", Command::DeleteTasks, has_task);
    ui.separator();
    run(app, ui, "Indent", Command::Indent, has_task);
    run(app, ui, "Outdent", Command::Outdent, has_task);
    run(app, ui, "Link Tasks", Command::LinkTasks, app.selected.len() > 1);
    run(app, ui, "Unlink Tasks", Command::UnlinkTasks, has_task);
    ui.separator();
    run(app, ui, "Scroll to Task", Command::ScrollToTask, has_task);
    run(app, ui, "Information...", Command::TaskInfo, has_task);
    run(app, ui, "Notes...", Command::TaskNotes, has_task);
    run(app, ui, "Add to Timeline", Command::AddToTimeline, has_task);
    run(app, ui, "Mark as Milestone", Command::MarkMilestone, has_task);
    run(app, ui, "Format Bar...", Command::FormatBarDlg, has_task);
    run(app, ui, "Inspect Task", Command::ToggleInspector, has_task);
    ui.separator();
    let active = app.active_index().is_none_or(|r| app.project.tasks[r].active);
    run(app, ui, if active { "Inactivate Task" } else { "Activate Task" }, Command::ToggleActive, has_task);
    run(app, ui, "Manually Schedule", Command::SetManual, has_task);
    run(app, ui, "Auto Schedule", Command::SetAuto, has_task);
    ui.separator();
    let is_sprint = app.active_index().is_some_and(|r| app.project.tasks[r].is_sprint);
    if is_sprint {
        run(app, ui, "Open on Board", Command::OpenOnBoard, true);
        run(app, ui, "Unmark Sprint", Command::ToggleSprint, true);
    } else {
        run(app, ui, "Mark as Sprint", Command::ToggleSprint, has_task && !app.active_index().is_some_and(|r| app.project.tasks[r].calc.is_summary));
    }
}

/// Little board: a sprint task.
pub fn icon_sprint(p: &Painter, c: Pos2) {
    let r = Rect::from_center_size(c, vec2(11.0, 9.0));
    p.rect_stroke(r, 1.5, Stroke::new(1.0, pal().accent), StrokeKind::Inside);
    for k in 0..3 {
        let x = r.left() + 1.5 + k as f32 * 3.0;
        let h = [5.0, 3.0, 4.0][k];
        p.rect_filled(Rect::from_min_size(pos2(x, r.top() + 2.0), vec2(2.0, h)), 0.0, pal().accent);
    }
}

/// Circular arrow: a recurring task.
pub fn icon_recurring(p: &Painter, c: Pos2) {
    let col = pal().accent;
    let r = 4.0;
    let pts: Vec<Pos2> = (0..=12).map(|k| {
        let a = std::f32::consts::PI * (0.15 + 1.6 * k as f32 / 12.0);
        c + vec2(a.cos() * r, a.sin() * r)
    }).collect();
    let end = *pts.last().unwrap();
    p.add(egui::Shape::line(pts, Stroke::new(1.3, col)));
    p.add(egui::Shape::convex_polygon(vec![end + vec2(-1.0, -3.0), end + vec2(3.0, 0.5), end + vec2(-1.5, 2.0)], col, Stroke::NONE));
}

/// A card with an arrow: waits on board items (filled while any is open,
/// warning-coloured when one can't be scheduled against).
pub fn icon_board_dep(p: &Painter, c: Pos2, open: bool, problem: bool) {
    let col = if problem { pal().warn } else { super::board::type_color(crate::board::IssueType::Epic) };
    let r = Rect::from_center_size(c + vec2(-1.5, 0.0), vec2(7.0, 9.0));
    if open {
        p.rect_filled(r, 1.5, col);
    } else {
        p.rect_stroke(r, 1.5, Stroke::new(1.0, col), StrokeKind::Inside);
    }
    let a = pos2(r.right() + 0.5, c.y);
    p.line_segment([a, a + vec2(4.0, 0.0)], Stroke::new(1.2, col));
    p.line_segment([a + vec2(4.0, 0.0), a + vec2(2.0, -2.0)], Stroke::new(1.2, col));
    p.line_segment([a + vec2(4.0, 0.0), a + vec2(2.0, 2.0)], Stroke::new(1.2, col));
}

// ---------------- small vector icons ----------------

pub fn icon_calendar(p: &Painter, c: Pos2, inflexible: bool) {
    let r = Rect::from_center_size(c, vec2(10.0, 9.0));
    p.rect_filled(r, 1.0, Color32::WHITE);
    p.rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_rgb(70, 110, 170)), StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(r.min, vec2(10.0, 3.0)), 0.0, Color32::from_rgb(70, 110, 170));
    if inflexible {
        p.circle_filled(c + vec2(1.0, 1.5), 2.0, pal().warn);
    }
}

/// Red person: overallocated resource.
pub fn icon_overalloc(p: &Painter, c: Pos2) {
    p.circle_filled(c + vec2(0.0, -2.8), 2.4, pal().warn);
    let body = vec![c + vec2(-4.0, 5.0), c + vec2(4.0, 5.0), c + vec2(2.5, 0.5), c + vec2(-2.5, 0.5)];
    p.add(egui::Shape::convex_polygon(body, pal().warn, Stroke::NONE));
}

pub fn icon_note(p: &Painter, c: Pos2) {
    let r = Rect::from_center_size(c, vec2(9.0, 10.0));
    p.rect_filled(r, 0.0, Color32::from_rgb(255, 244, 190));
    p.rect_stroke(r, 0.0, Stroke::new(1.0, pal().note), StrokeKind::Inside);
    for k in 0..3 {
        let y = r.top() + 3.0 + k as f32 * 2.2;
        p.line_segment([pos2(r.left() + 2.0, y), pos2(r.right() - 2.0, y)], Stroke::new(0.8, pal().note));
    }
}

pub fn icon_warning(p: &Painter, c: Pos2) {
    p.circle_filled(c, 5.0, pal().warn);
    p.text(c, Align2::CENTER_CENTER, "!", FontId::proportional(9.0), Color32::WHITE);
}

pub fn icon_check(p: &Painter, c: Pos2) {
    let s = Stroke::new(2.0, Color32::from_rgb(40, 140, 60));
    p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0)], s);
    p.line_segment([c + vec2(-1.0, 3.0), c + vec2(4.5, -3.5)], s);
}

pub fn icon_pin(p: &Painter, c: Pos2) {
    let col = Color32::from_rgb(52, 140, 150);
    p.circle_filled(c + vec2(0.0, -3.0), 3.2, col);
    p.line_segment([c + vec2(0.0, -1.0), c + vec2(0.0, 5.0)], Stroke::new(1.4, col));
}

pub fn icon_auto(p: &Painter, c: Pos2) {
    let r = Rect::from_center_size(c + vec2(-2.0, 0.0), vec2(10.0, 6.0));
    p.rect_filled(r, 1.0, pal().bar_fill);
    p.rect_stroke(r, 1.0, Stroke::new(1.0, pal().bar_border), StrokeKind::Inside);
    p.arrow(c + vec2(3.5, 0.0), vec2(5.0, 0.0), Stroke::new(1.2, pal().bar_border));
}

/// Used by tests to begin editing a specific cell directly.
pub fn start_edit_at(app: &mut ProjectFcgApp, row: RowRef, col: usize, text: &str) {
    app.active_row = row;
    app.active_col = col;
    app.edit = Some(CellEdit {
        row,
        col,
        text: text.to_string(),
        focus_requested: false,
        select_all: false,
        picker_open: false,
    });
}
