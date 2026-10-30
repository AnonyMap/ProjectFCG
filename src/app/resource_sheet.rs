//! Resource Sheet view (View ▸ Resource Sheet).

use super::commands::Command;
use super::theme::*;
use super::ProjectFcgApp;
use crate::model::{Accrue, Project, ResourceType};
use crate::resources::{fmt_money, fmt_rate, fmt_units, parse_money, parse_rate, parse_units};
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Stroke, StrokeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResCol {
    Indicators,
    Name,
    Type,
    Material,
    Initials,
    Group,
    MaxUnits,
    StdRate,
    OvtRate,
    CostPerUse,
    Accrue,
    Calendar,
    Work,
    Cost,
}

impl ResCol {
    pub const DEFAULT: [(ResCol, f32); 14] = [
        (ResCol::Indicators, 26.0),
        (ResCol::Name, 170.0),
        (ResCol::Type, 70.0),
        (ResCol::Material, 70.0),
        (ResCol::Initials, 54.0),
        (ResCol::Group, 90.0),
        (ResCol::MaxUnits, 60.0),
        (ResCol::StdRate, 90.0),
        (ResCol::OvtRate, 90.0),
        (ResCol::CostPerUse, 76.0),
        (ResCol::Accrue, 72.0),
        (ResCol::Calendar, 90.0),
        (ResCol::Work, 70.0),
        (ResCol::Cost, 90.0),
    ];
    pub fn title(&self) -> &'static str {
        match self {
            ResCol::Indicators => "ℹ",
            ResCol::Name => "Resource Name",
            ResCol::Type => "Type",
            ResCol::Material => "Material",
            ResCol::Initials => "Initials",
            ResCol::Group => "Group",
            ResCol::MaxUnits => "Max.",
            ResCol::StdRate => "Std. Rate",
            ResCol::OvtRate => "Ovt. Rate",
            ResCol::CostPerUse => "Cost/Use",
            ResCol::Accrue => "Accrue At",
            ResCol::Calendar => "Base Calendar",
            ResCol::Work => "Work",
            ResCol::Cost => "Cost",
        }
    }
    pub fn editable(&self) -> bool {
        !matches!(self, ResCol::Indicators | ResCol::Calendar | ResCol::Work | ResCol::Cost)
    }
    pub fn choices(&self) -> Vec<&'static str> {
        match self {
            ResCol::Type => ResourceType::ALL.iter().map(|t| t.label()).collect(),
            ResCol::Accrue => Accrue::ALL.iter().map(|a| a.label()).collect(),
            _ => vec![],
        }
    }
    fn right(&self) -> bool {
        matches!(
            self,
            ResCol::MaxUnits | ResCol::StdRate | ResCol::OvtRate | ResCol::CostPerUse | ResCol::Work | ResCol::Cost
        )
    }
}

pub fn res_text(p: &Project, i: usize, c: ResCol) -> String {
    let r = &p.resources[i];
    let sym = &p.settings.currency_symbol;
    let material = r.rtype == ResourceType::Material;
    let costonly = r.rtype == ResourceType::Cost;
    match c {
        ResCol::Indicators => String::new(),
        ResCol::Name => r.name.clone(),
        ResCol::Type => r.rtype.label().into(),
        ResCol::Material => r.material_label.clone(),
        ResCol::Initials => r.initials.clone(),
        ResCol::Group => r.group.clone(),
        ResCol::MaxUnits if r.rtype == ResourceType::Work => fmt_units(r.max_units),
        ResCol::MaxUnits => String::new(),
        ResCol::StdRate if costonly => String::new(),
        ResCol::StdRate => fmt_rate(r.std_rate, sym, material),
        ResCol::OvtRate if r.rtype == ResourceType::Work => fmt_rate(r.ovt_rate, sym, false),
        ResCol::OvtRate => String::new(),
        ResCol::CostPerUse if costonly => String::new(),
        ResCol::CostPerUse => fmt_money(r.cost_per_use, sym),
        ResCol::Accrue => r.accrue.label().into(),
        ResCol::Calendar if r.rtype == ResourceType::Work => {
            if r.has_own_calendar() {
                format!("{}*", p.calendar.name)
            } else {
                p.calendar.name.clone()
            }
        }
        ResCol::Calendar => String::new(),
        ResCol::Work => match r.rtype {
            ResourceType::Work => p.format_duration_minutes(p.calc.resources.get(i).map_or(0, |c| c.work), crate::duration::DurUnit::Hour),
            ResourceType::Material => {
                let q: f64 = p.assignments.iter().filter(|a| a.resource == r.uid).map(|a| a.units).sum();
                format!("{} {}", crate::duration::fmt_num(q), r.material_label).trim().to_string()
            }
            ResourceType::Cost => String::new(),
        },
        ResCol::Cost => fmt_money(p.calc.resources.get(i).map_or(0.0, |c| c.cost), sym),
    }
}

pub fn res_edit_text(p: &Project, i: usize, c: ResCol) -> String {
    let r = &p.resources[i];
    match c {
        ResCol::StdRate | ResCol::OvtRate | ResCol::CostPerUse => {
            let v = match c {
                ResCol::StdRate => r.std_rate,
                ResCol::OvtRate => r.ovt_rate,
                _ => r.cost_per_use,
            };
            crate::duration::fmt_num(v)
        }
        _ => res_text(p, i, c),
    }
}

pub fn set_res(p: &mut Project, i: usize, c: ResCol, text: &str) -> Result<(), String> {
    let t = text.trim();
    let sym = p.settings.currency_symbol.clone();
    let (mpd, mpw) = (p.settings.durations.minutes_per_day, p.settings.durations.minutes_per_week);
    match c {
        ResCol::Name => {
            if t.is_empty() {
                return Err("A resource needs a name.".into());
            }
            if p.resource_by_name(t).is_some_and(|j| j != i) {
                return Err(format!("There is already a resource named \"{t}\"."));
            }
            p.resources[i].name = t.to_string();
        }
        ResCol::Type => {
            let ty = ResourceType::ALL
                .into_iter()
                .find(|x| x.label().eq_ignore_ascii_case(t))
                .ok_or_else(|| format!("\"{t}\" is not a resource type (Work, Material, Cost)"))?;
            if ty != p.resources[i].rtype && !p.assignments_of_resource(p.resources[i].uid).is_empty() {
                return Err("Remove this resource's assignments before changing its type.".into());
            }
            p.resources[i].rtype = ty;
            if ty == ResourceType::Work && p.resources[i].max_units <= 0.0 {
                p.resources[i].max_units = 1.0;
            }
        }
        ResCol::Material => p.resources[i].material_label = t.to_string(),
        ResCol::Initials => p.resources[i].initials = t.to_string(),
        ResCol::Group => p.resources[i].group = t.to_string(),
        ResCol::MaxUnits => p.resources[i].max_units = parse_units(t)?,
        ResCol::StdRate => p.resources[i].std_rate = parse_rate(t, &sym, mpd, mpw)?,
        ResCol::OvtRate => p.resources[i].ovt_rate = parse_rate(t, &sym, mpd, mpw)?,
        ResCol::CostPerUse => p.resources[i].cost_per_use = parse_money(t, &sym)?,
        ResCol::Accrue => {
            p.resources[i].accrue = Accrue::ALL
                .into_iter()
                .find(|x| x.label().eq_ignore_ascii_case(t))
                .ok_or_else(|| format!("\"{t}\" is not Start, Prorated or End"))?;
        }
        _ => return Err(format!("{} is calculated.", c.title())),
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ResEdit {
    /// Resource uid, or None for the blank row (creates a resource).
    pub uid: Option<u32>,
    pub col: usize,
    pub text: String,
    pub focus_requested: bool,
    pub select_all: bool,
    pub picker_open: bool,
}

pub struct SheetState {
    pub columns: Vec<(ResCol, f32)>,
    /// Active row: resource uid, None = the blank row.
    pub active: Option<u32>,
    pub col: usize,
    pub selected: Vec<u32>,
    pub edit: Option<ResEdit>,
    pub scroll_y: f32,
    pub scroll_x: f32,
}

impl Default for SheetState {
    fn default() -> Self {
        SheetState {
            columns: ResCol::DEFAULT.to_vec(),
            active: None,
            col: 1,
            selected: vec![],
            edit: None,
            scroll_y: 0.0,
            scroll_x: 0.0,
        }
    }
}

impl ProjectFcgApp {
    pub fn res_active_index(&self) -> Option<usize> {
        self.sheet.active.and_then(|u| self.project.resource_index(u))
    }

    pub fn res_selected_rows(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.sheet.selected.iter().filter_map(|&u| self.project.resource_index(u)).collect();
        if v.is_empty() {
            v.extend(self.res_active_index());
        }
        v.sort();
        v.dedup();
        v
    }

    pub fn res_select(&mut self, row: Option<usize>) {
        let uid = row.and_then(|r| self.project.resources.get(r)).map(|r| r.uid);
        self.sheet.active = uid;
        self.sheet.selected = uid.into_iter().collect();
        let d = row.unwrap_or(self.project.resources.len());
        let y = d as f32 * row_h();
        if y < self.sheet.scroll_y {
            self.sheet.scroll_y = y;
        } else if y + row_h() > self.sheet.scroll_y + self.rows_view_h {
            self.sheet.scroll_y = y + row_h() - self.rows_view_h;
        }
    }

    pub fn res_begin_edit(&mut self, initial: Option<String>) {
        let Some(&(col, _)) = self.sheet.columns.get(self.sheet.col) else { return };
        if !col.editable() {
            self.set_status(format!("{} is a calculated field", col.title()));
            return;
        }
        let existing = match self.res_active_index() {
            Some(i) => res_edit_text(&self.project, i, col),
            None => String::new(),
        };
        let (text, select_all) = match initial {
            Some(t) => (t, false),
            None => (existing, true),
        };
        self.sheet.edit = Some(ResEdit {
            uid: self.sheet.active,
            col: self.sheet.col,
            text,
            focus_requested: false,
            select_all,
            picker_open: false,
        });
    }

    pub fn res_commit_edit(&mut self) -> bool {
        let Some(e) = self.sheet.edit.take() else { return true };
        let Some(&(col, _)) = self.sheet.columns.get(e.col) else { return true };
        match e.uid.and_then(|u| self.project.resource_index(u)) {
            Some(i) => {
                if res_edit_text(&self.project, i, col) == e.text {
                    return true;
                }
                let text = e.text.clone();
                self.edit_project(&format!("Edit {}", col.title()), |p| set_res(p, i, col, &text)).is_some()
            }
            None => {
                if e.text.trim().is_empty() {
                    return true;
                }
                let text = e.text.clone();
                let made = self.edit_project("New Resource", |p| {
                    let name = if col == ResCol::Name { text.trim().to_string() } else { format!("Resource {}", p.resources.len() + 1) };
                    if p.resource_by_name(&name).is_some() {
                        return Err(format!("There is already a resource named \"{name}\"."));
                    }
                    let i = p.add_resource(&name, ResourceType::Work);
                    if col != ResCol::Name {
                        set_res(p, i, col, &text)?;
                    }
                    Ok(i)
                });
                if let Some(i) = made {
                    self.sheet.active = Some(self.project.resources[i].uid);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn res_move(&mut self, drow: i32, dcol: i32) {
        let n = self.project.resources.len() as i32;
        let cur = self.res_active_index().map(|i| i as i32).unwrap_or(n);
        let nr = (cur + drow).clamp(0, n);
        self.sheet.col = (self.sheet.col as i32 + dcol).clamp(0, self.sheet.columns.len() as i32 - 1) as usize;
        self.res_select(if nr < n { Some(nr as usize) } else { None });
    }

    /// Keyboard for the Resource Sheet. Returns true if handled.
    pub fn res_key(&mut self, key: egui::Key, m: egui::Modifiers) -> bool {
        use egui::Key::*;
        let ctrl = m.ctrl || m.command;
        match key {
            ArrowDown if ctrl => self.res_move(10_000, 0),
            ArrowUp if ctrl => self.res_move(-10_000, 0),
            ArrowDown | Enter if !m.shift => self.res_move(1, 0),
            ArrowUp => self.res_move(-1, 0),
            Enter => self.res_move(-1, 0),
            ArrowLeft => self.res_move(0, -1),
            ArrowRight => self.res_move(0, 1),
            Tab => self.res_move(0, if m.shift { -1 } else { 1 }),
            Home => self.sheet.col = 0,
            End => self.sheet.col = self.sheet.columns.len() - 1,
            Backspace => self.res_begin_edit(Some(String::new())),
            _ => return false,
        }
        true
    }
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal().bg);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), header_h()));
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), pos2(rect.right(), rect.bottom() - SCROLLBAR));
    let hbar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - SCROLLBAR), rect.max);
    let n = app.project.resources.len();
    let mut xs = vec![];
    let mut x = rect.left() + row_header_w() - app.sheet.scroll_x;
    for (_, w) in &app.sheet.columns {
        xs.push(x);
        x += w;
    }
    xs.push(x);
    let content_h = (n as f32 + 3.0) * row_h();
    app.rows_view_h = body.height();
    app.sheet.scroll_y = app.sheet.scroll_y.clamp(0.0, (content_h - body.height()).max(0.0));

    // ---- interaction ----
    let resp = ui.interact(body, ui.scope_id().with("res_body"), Sense::click_and_drag());
    if resp.hovered() {
        let (dy, dx, shift) = ui.input(|i| (i.smooth_scroll_delta.y, i.smooth_scroll_delta.x, i.modifiers.shift));
        if shift {
            app.sheet.scroll_x = (app.sheet.scroll_x - dy - dx).max(0.0);
        } else {
            app.sheet.scroll_y = (app.sheet.scroll_y - dy).max(0.0);
        }
    }
    let scroll_y = app.sheet.scroll_y;
    let hit = |p: egui::Pos2| -> (usize, Option<usize>) {
        let d = ((p.y - body.top() + scroll_y) / row_h()).floor().max(0.0) as usize;
        let c = (0..xs.len() - 1).find(|&k| p.x >= xs[k] && p.x < xs[k + 1]);
        (d, if p.x < rect.left() + row_header_w() { None } else { c })
    };
    let mods = ui.input(|i| i.modifiers);
    if resp.double_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let (d, _) = hit(p);
            if d < n {
                app.sheet.edit = None;
                app.res_select(Some(d));
                let ctx = ui.ctx().clone();
                app.execute(Command::ResourceInfo, &ctx);
            }
        }
    } else if resp.clicked() || resp.secondary_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let (d, c) = hit(p);
            if app.res_commit_edit() {
                let row = (d < n).then_some(d);
                let uid = row.map(|r| app.project.resources[r].uid);
                let was = app.sheet.active == uid && c == Some(app.sheet.col);
                if let Some(c) = c {
                    app.sheet.col = c;
                }
                if (mods.ctrl || mods.command) && uid.is_some() {
                    let u = uid.unwrap();
                    if let Some(k) = app.sheet.selected.iter().position(|&x| x == u) {
                        app.sheet.selected.remove(k);
                    } else {
                        app.sheet.selected.push(u);
                    }
                    app.sheet.active = uid;
                } else if mods.shift && uid.is_some() {
                    let a = app.res_active_index().unwrap_or(0);
                    let (lo, hi) = (a.min(d), a.max(d));
                    app.sheet.selected = (lo..=hi.min(n - 1)).map(|k| app.project.resources[k].uid).collect();
                    app.sheet.active = uid;
                } else if !(resp.secondary_clicked() && uid.is_some_and(|u| app.sheet.selected.contains(&u))) {
                    app.res_select(row);
                    if was && !resp.secondary_clicked() {
                        app.res_begin_edit(None);
                    }
                }
            }
        }
    }
    resp.context_menu(|ui| {
        let ctx = ui.ctx().clone();
        for (label, cmd) in [
            ("Insert Resource", Command::InsertTask),
            ("Delete Resource", Command::DeleteTasks),
            ("Information...", Command::ResourceInfo),
            ("Level Resource", Command::LevelAll),
        ] {
            if ui.button(label).clicked() {
                app.execute(cmd, &ctx);
                ui.close();
            }
        }
    });

    // ---- rows ----
    let bp = painter.with_clip_rect(body);
    let cells = Rect::from_min_max(pos2(rect.left() + row_header_w(), body.top()), body.max);
    let cp = painter.with_clip_rect(cells);
    let font = FontId::proportional(text_size());
    let first = (app.sheet.scroll_y / row_h()).floor() as usize;
    let vis = (body.height() / row_h()).ceil() as usize + 1;
    let sel: std::collections::HashSet<u32> = app.sheet.selected.iter().copied().collect();
    let grid = paper(app.project.format.gridlines.line_color);
    for d in first..first + vis {
        let y = body.top() + d as f32 * row_h() - app.sheet.scroll_y;
        let rr = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h()));
        let res = app.project.resources.get(d);
        let is_sel = res.is_some_and(|r| sel.contains(&r.uid));
        if is_sel {
            bp.rect_filled(rr, 0.0, pal().selected_row);
        }
        bp.line_segment([pos2(rect.left(), y + row_h() - 0.5), pos2(rect.right(), y + row_h() - 0.5)], Stroke::new(1.0, grid));
        for k in 1..xs.len() {
            cp.line_segment([pos2(xs[k] - 0.5, y), pos2(xs[k] - 0.5, y + row_h())], Stroke::new(1.0, grid));
        }
        let rh = Rect::from_min_size(pos2(rect.left(), y), vec2(row_header_w(), row_h()));
        bp.rect_filled(rh, 0.0, if is_sel { pal().tint } else { pal().header_bg });
        bp.line_segment([rh.right_top(), rh.right_bottom()], Stroke::new(1.0, pal().header_border));
        if let Some(r) = res {
            bp.text(pos2(rh.right() - 5.0, rh.center().y), Align2::RIGHT_CENTER, (d + 1).to_string(), FontId::proportional(small_size()), pal().text);
            let over = app.project.calc.resources.get(d).is_some_and(|c| c.overallocated);
            let col = if over { pal().warn } else { pal().text };
            for (k, (c, _)) in app.sheet.columns.iter().enumerate() {
                let cell = Rect::from_min_max(pos2(xs[k], y), pos2(xs[k + 1], y + row_h()));
                let clip = cp.with_clip_rect(cell.intersect(cells));
                if *c == ResCol::Indicators {
                    if over {
                        super::table::icon_overalloc(&clip, cell.center());
                    }
                    continue;
                }
                let text = res_text(&app.project, d, *c);
                let (pos, anchor) = if c.right() {
                    (pos2(cell.right() - 5.0, cell.center().y), Align2::RIGHT_CENTER)
                } else {
                    (pos2(cell.left() + 4.0, cell.center().y), Align2::LEFT_CENTER)
                };
                clip.text(pos, anchor, &text, font.clone(), col);
                if over && *c == ResCol::Name {
                    clip.text(pos + vec2(0.6, 0.0), anchor, &text, font.clone(), col);
                }
            }
            let _ = r;
        }
        let active = match app.sheet.active {
            Some(u) => res.is_some_and(|r| r.uid == u),
            None => d == n,
        };
        if active && app.sheet.edit.is_none() && app.sheet.col + 1 < xs.len() {
            let k = app.sheet.col;
            let cell = Rect::from_min_max(pos2(xs[k], y), pos2(xs[k + 1], y + row_h()));
            cp.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(2.0, pal().active_cell), StrokeKind::Inside);
        }
    }

    // ---- header ----
    painter.rect_filled(header, 0.0, pal().header_bg);
    painter.line_segment([header.left_bottom(), header.right_bottom()], Stroke::new(1.0, pal().header_border));
    let hp = painter.with_clip_rect(Rect::from_min_max(pos2(rect.left() + row_header_w(), header.top()), header.max));
    let mut resize = None;
    for (k, (c, _)) in app.sheet.columns.iter().enumerate() {
        let cell = Rect::from_min_max(pos2(xs[k], header.top()), pos2(xs[k + 1], header.bottom()));
        if k == app.sheet.col {
            hp.rect_filled(cell, 0.0, pal().header_hover);
        }
        let g = hp.layout(c.title().to_string(), FontId::proportional(text_size() - 0.5), pal().text, (cell.width() - 8.0).max(10.0));
        hp.with_clip_rect(cell.intersect(hp.clip_rect())).galley(pos2(cell.left() + 4.0, cell.center().y - g.size().y / 2.0), g, pal().text);
        hp.line_segment([pos2(cell.right() - 0.5, cell.top()), pos2(cell.right() - 0.5, cell.bottom())], Stroke::new(1.0, pal().header_border));
        let h = Rect::from_center_size(pos2(cell.right(), cell.center().y), vec2(8.0, cell.height()));
        let r = ui.interact(h, ui.scope_id().with(("rescol", k)), Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        }
        if r.dragged() {
            resize = Some((k, r.drag_delta().x));
        }
    }
    if let Some((k, dx)) = resize {
        app.sheet.columns[k].1 = (app.sheet.columns[k].1 + dx).max(20.0);
    }

    // ---- editor ----
    if let Some(e) = app.sheet.edit.clone() {
        let d = match e.uid {
            Some(u) => app.project.resource_index(u),
            None => Some(n),
        };
        if let (Some(d), true) = (d, e.col + 1 < xs.len()) {
            let y = body.top() + d as f32 * row_h() - app.sheet.scroll_y;
            let cell = Rect::from_min_max(pos2(xs[e.col], y), pos2(xs[e.col + 1], y + row_h())).intersect(cells);
            if cell.height() > 6.0 {
                show_editor(app, ui, cell, &e);
            }
        } else {
            app.sheet.edit = None;
        }
    }

    let content_w: f32 = row_header_w() + app.sheet.columns.iter().map(|c| c.1).sum::<f32>();
    if let Some(v) = super::scrollbar(ui, hbar, false, app.sheet.scroll_x, rect.width(), content_w.max(rect.width()), "res_hscroll") {
        app.sheet.scroll_x = v;
    }
    app.sheet.scroll_x = app.sheet.scroll_x.clamp(0.0, (content_w - rect.width()).max(0.0));
}

fn show_editor(app: &mut ProjectFcgApp, ui: &mut egui::Ui, cell: Rect, e: &ResEdit) {
    let col = app.sheet.columns[e.col].0;
    let id = ui.scope_id().with("res_editor");
    let mut text = e.text.clone();
    let choices = col.choices();
    let pick_w = if choices.is_empty() { 0.0 } else { 18.0 };
    let tr = Rect::from_min_max(cell.min, pos2(cell.right() - pick_w, cell.bottom()));
    let resp = ui.put(tr, egui::TextEdit::singleline(&mut text).id(id).font(FontId::proportional(text_size())).margin(vec2(3.0, 2.0)));
    if !e.focus_requested {
        resp.request_focus();
        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
            let len = text.chars().count();
            let r = if e.select_all {
                egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))
            } else {
                egui::text::CCursorRange::one(egui::text::CCursor::new(len))
            };
            st.cursor.set_char_range(Some(r));
            st.store(ui.ctx(), id);
        }
    }
    let mut picker_open = e.picker_open;
    let mut picked = None;
    if !choices.is_empty() {
        let b = ui.put(Rect::from_min_max(pos2(cell.right() - pick_w, cell.top()), cell.max), egui::Button::new("▾").small());
        if b.clicked() {
            picker_open = !picker_open;
        }
        if picker_open {
            egui::Area::new(id.with("pick")).order(egui::Order::Foreground).fixed_pos(pos2(cell.left(), cell.bottom() + 1.0)).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for c in &choices {
                        if ui.selectable_label(text == *c, *c).clicked() {
                            picked = Some(c.to_string());
                        }
                    }
                });
            });
        }
    }
    if let Some(s) = app.sheet.edit.as_mut() {
        s.text = text;
        s.focus_requested = true;
        s.picker_open = picker_open;
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
    if let Some(v) = picked {
        if let Some(s) = app.sheet.edit.as_mut() {
            s.text = v;
        }
        app.res_commit_edit();
        return;
    }
    if esc {
        app.sheet.edit = None;
        return;
    }
    if (resp.has_focus() || resp.lost_focus()) && (up || down) {
        if app.res_commit_edit() {
            app.res_move(if up { -1 } else { 1 }, 0);
        }
        return;
    }
    let over_picker = picker_open
        && ui.ctx().layer_id_at(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()).is_some_and(|l| l.order == egui::Order::Foreground);
    if resp.lost_focus() && !over_picker && app.res_commit_edit() {
        if enter {
            app.res_move(if shift { -1 } else { 1 }, 0);
        } else if tab {
            app.res_move(0, if shift { -1 } else { 1 });
        }
    }
}
