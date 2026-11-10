//! System Diagram view (experimental; File ▸ Options ▸ Experimental): a
//! freeform map of what the project is building, with the work, risks
//! and performance measures tied to each part. See `crate::system`.
//!
//! * Double-click the background to add an element; drag one to move it
//!   (dropped inside a system or group, it belongs to it); drag the
//!   corner to resize.
//! * Drag the round handle on a selected element's right edge onto
//!   another to connect them, or Ctrl+click two and press Connect.
//! * Drag the background to pan, scroll to move, Ctrl+scroll to zoom.
//! * The panel on the right edits the selection: name, kind, state,
//!   measures, and the tasks, tickets and risks it is tied to.

use super::item_search::Item;
use super::ProjectFcgApp;
use crate::system::{Element, ElementKind, ElementState, Flow, Health, Interface, InterfaceKind, Measure, MeasureStatus, Rollup, WorkLink};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

/// What is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sel {
    #[default]
    None,
    Element(u32),
    Interface(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorBy {
    #[default]
    Kind,
    Health,
    State,
}

impl ColorBy {
    pub const ALL: [ColorBy; 3] = [ColorBy::Kind, ColorBy::Health, ColorBy::State];
    pub fn label(self) -> &'static str {
        match self {
            ColorBy::Kind => "Kind",
            ColorBy::Health => "Health",
            ColorBy::State => "State",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Pan,
    /// Moving an element (and what's inside it) by `d` (canvas units).
    Move { id: u32, d: Vec2 },
    Resize { id: u32, d: Vec2 },
    /// From an element to the pointer.
    Connect { from: u32, to: Pos2 },
}

/// The selection's details being edited, and what they were.
#[derive(Clone, Debug, PartialEq)]
pub enum Draft {
    Element(Element),
    Interface(Interface),
}

struct Cache {
    gen: u64,
    elements: HashMap<u32, Rollup>,
    interfaces: HashMap<u32, Rollup>,
}

pub struct SystemViewState {
    pub offset: Vec2,
    pub zoom: f32,
    pub sel: Sel,
    /// Elements Ctrl+clicked besides the selection (to connect two).
    pub also: Vec<u32>,
    pub new_kind: ElementKind,
    pub color_by: ColorBy,
    /// The panel's "link work" search.
    pub link_query: String,
    drag: Option<Drag>,
    fitted: bool,
    focus_name: bool,
    /// (what it was, what it is now) for the selection.
    draft: Option<(Draft, Draft)>,
    cache: Option<Cache>,
}

impl Default for SystemViewState {
    fn default() -> Self {
        SystemViewState {
            offset: vec2(40.0, 40.0),
            zoom: 1.0,
            sel: Sel::None,
            also: vec![],
            new_kind: ElementKind::Component,
            color_by: ColorBy::Kind,
            link_query: String::new(),
            drag: None,
            fitted: false,
            focus_name: false,
            draft: None,
            cache: None,
        }
    }
}

const PANEL_W: f32 = 340.0;
const HANDLE_R: f32 = 6.0;

// ------------------------------------------------------------- colours

fn kind_color(k: ElementKind) -> Color32 {
    let dark = super::theme::pal().dark;
    let (l, d) = match k {
        ElementKind::System => ((90, 110, 140), (140, 160, 195)),
        ElementKind::Subsystem => ((45, 105, 185), (110, 160, 235)),
        ElementKind::Component => ((25, 130, 125), (90, 195, 185)),
        ElementKind::Software => ((120, 80, 175), (175, 140, 230)),
        ElementKind::External => ((110, 110, 110), (160, 160, 160)),
        ElementKind::Function => ((195, 110, 30), (235, 160, 80)),
        ElementKind::Group => ((120, 120, 120), (150, 150, 150)),
        ElementKind::Note => ((175, 145, 20), (220, 195, 90)),
    };
    let (r, g, b) = if dark { d } else { l };
    Color32::from_rgb(r, g, b)
}

pub fn health_color(h: Health) -> Color32 {
    let dark = super::theme::pal().dark;
    match (h, dark) {
        (Health::Good, false) => Color32::from_rgb(40, 140, 70),
        (Health::Good, true) => Color32::from_rgb(100, 200, 120),
        (Health::Watch, false) => Color32::from_rgb(200, 140, 0),
        (Health::Watch, true) => Color32::from_rgb(235, 185, 60),
        (Health::Trouble, false) => Color32::from_rgb(200, 50, 40),
        (Health::Trouble, true) => Color32::from_rgb(240, 110, 95),
        (Health::Unknown, _) => Color32::from_rgb(140, 140, 140),
    }
}

fn measure_color(m: MeasureStatus) -> Color32 {
    health_color(match m {
        MeasureStatus::Met => Health::Good,
        MeasureStatus::AtRisk => Health::Watch,
        MeasureStatus::Breach => Health::Trouble,
        MeasureStatus::Unknown => Health::Unknown,
    })
}

fn state_color(s: ElementState) -> Color32 {
    // grey (concept) through blue to green (fielded)
    let t = s as usize as f32 / 5.0;
    let a = (130.0, 130.0, 140.0);
    let b = (40.0, 150.0, 80.0);
    let lerp = |x: f32, y: f32| (x + (y - x) * t) as u8;
    Color32::from_rgb(lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn interface_color(k: InterfaceKind) -> Color32 {
    let dark = super::theme::pal().dark;
    let c = match k {
        InterfaceKind::Data => (45, 110, 200),
        InterfaceKind::Signal => (130, 80, 190),
        InterfaceKind::Power => (205, 70, 50),
        InterfaceKind::Mechanical => (110, 110, 110),
        InterfaceKind::Fluid => (20, 150, 175),
        InterfaceKind::Thermal => (215, 120, 30),
        InterfaceKind::Human => (40, 140, 70),
        InterfaceKind::Other => (120, 120, 120),
    };
    let c = Color32::from_rgb(c.0, c.1, c.2);
    if dark {
        c.lerp_to_gamma(Color32::WHITE, 0.25)
    } else {
        c
    }
}

// ------------------------------------------------------------- geometry

fn rect_of(e: &Element) -> Rect {
    Rect::from_min_size(pos2(e.pos[0], e.pos[1]), vec2(e.size[0], e.size[1]))
}

/// Where the line from `r`'s centre towards `p` leaves `r`.
fn edge_point(r: Rect, p: Pos2) -> Pos2 {
    let c = r.center();
    let d = p - c;
    if d.x.abs() < 1e-3 && d.y.abs() < 1e-3 {
        return c;
    }
    let sx = if d.x.abs() > 1e-6 { (r.width() / 2.0) / d.x.abs() } else { f32::INFINITY };
    let sy = if d.y.abs() > 1e-6 { (r.height() / 2.0) / d.y.abs() } else { f32::INFINITY };
    c + d * sx.min(sy)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

fn arrow_head(painter: &egui::Painter, at: Pos2, from: Pos2, color: Color32, size: f32) {
    let d = (at - from).normalized() * size;
    let n = vec2(-d.y, d.x) * 0.55;
    painter.add(Shape::convex_polygon(vec![at, at - d + n, at - d - n], color, Stroke::NONE));
}

// ------------------------------------------------------------- app side

impl ProjectFcgApp {
    /// Save the panel's edits (as one undo step), before any command.
    pub fn commit_system_draft(&mut self) {
        let Some((base, edit)) = self.system_view.draft.clone() else { return };
        if base == edit {
            return;
        }
        self.system_view.draft = Some((edit.clone(), edit.clone()));
        match edit {
            Draft::Element(e) => {
                self.edit_project("Edit Element", move |p| {
                    let x = p.system.element_mut(e.id).ok_or("That element no longer exists.")?;
                    *x = e;
                    Ok(())
                });
            }
            Draft::Interface(f) => {
                self.edit_project("Edit Interface", move |p| {
                    let x = p.system.interface_mut(f.id).ok_or("That interface no longer exists.")?;
                    *x = f;
                    Ok(())
                });
            }
        }
    }

    /// Delete the selected element or interface.
    pub fn delete_system_selection(&mut self) {
        self.commit_system_draft();
        match self.system_view.sel {
            Sel::Element(id) => {
                let name = self.project.system.element(id).map(|e| e.name.clone()).unwrap_or_default();
                if self.edit_project("Delete Element", move |p| {
                    p.system.delete_element(id);
                    Ok(())
                })
                .is_some()
                {
                    self.set_status(format!("Deleted {name}. Undo (Ctrl+Z) brings it back."));
                }
            }
            Sel::Interface(id) => {
                self.edit_project("Delete Interface", move |p| {
                    p.system.delete_interface(id);
                    Ok(())
                });
            }
            Sel::None => return,
        }
        self.system_view.sel = Sel::None;
        self.system_view.also.clear();
        self.system_view.draft = None;
    }

    /// Add an element of the chosen kind with its top-left at `at` (canvas
    /// units), and select it.
    pub fn add_system_element(&mut self, kind: ElementKind, at: [f32; 2]) -> Option<u32> {
        self.commit_system_draft();
        let id = self.edit_project("New Element", move |p| Ok(p.system.add_element(Element { kind, pos: at, ..Default::default() })))?;
        self.system_view.sel = Sel::Element(id);
        self.system_view.also.clear();
        self.system_view.focus_name = true;
        Some(id)
    }

    /// Connect two elements and select the interface.
    pub fn connect_system(&mut self, from: u32, to: u32) -> Option<u32> {
        self.commit_system_draft();
        let id = self.edit_project("New Interface", move |p| p.system.connect(from, to, InterfaceKind::Data))?;
        self.system_view.sel = Sel::Interface(id);
        self.system_view.also.clear();
        Some(id)
    }

    /// A starter layout for an empty diagram: a system holding three
    /// subsystems, a user outside it, and the interfaces between them.
    pub fn system_starter(&mut self) {
        self.commit_system_draft();
        self.edit_project("Starter Layout", |p| {
            let s = &mut p.system;
            let sys = s.add_element(Element { kind: ElementKind::System, name: p.title.clone(), pos: [0.0, 0.0], size: [700.0, 300.0], ..Default::default() });
            let a = s.add_element(Element { kind: ElementKind::Subsystem, name: "Subsystem A".into(), pos: [40.0, 70.0], size: [180.0, 90.0], ..Default::default() });
            let b = s.add_element(Element { kind: ElementKind::Subsystem, name: "Subsystem B".into(), pos: [260.0, 70.0], size: [180.0, 90.0], ..Default::default() });
            let c = s.add_element(Element { kind: ElementKind::Software, name: "Software".into(), pos: [480.0, 70.0], size: [180.0, 90.0], ..Default::default() });
            let u = s.add_element(Element { kind: ElementKind::External, name: "User".into(), pos: [260.0, 380.0], size: [180.0, 70.0], ..Default::default() });
            s.connect(a, b, InterfaceKind::Mechanical)?;
            s.connect(b, c, InterfaceKind::Data)?;
            let f = s.connect(u, b, InterfaceKind::Human)?;
            if let Some(x) = s.interface_mut(f) {
                x.flow = Flow::Both;
                x.label = "Operates".into();
            }
            let _ = sys;
            Ok(())
        });
        self.system_view.fitted = false;
    }

    fn system_cache(&mut self) -> &Cache {
        let gen = self.history.generation();
        if self.system_view.cache.as_ref().is_none_or(|c| c.gen != gen) {
            let p = &self.project;
            let elements = p.system.elements.iter().map(|e| (e.id, p.system_rollup(e.id))).collect();
            let interfaces = p.system.interfaces.iter().map(|f| (f.id, p.interface_rollup(f.id))).collect();
            self.system_view.cache = Some(Cache { gen, elements, interfaces });
        }
        self.system_view.cache.as_ref().unwrap()
    }
}

fn rollup_text(p: &crate::model::Project, r: &Rollup) -> String {
    let mut lines = vec![format!("Health: {}", r.health().label())];
    if r.tasks > 0 {
        let mut s = format!("{} task{}, {:.0}% complete", r.tasks, if r.tasks == 1 { "" } else { "s" }, r.percent_complete.unwrap_or(0.0));
        if r.late > 0 {
            s += &format!(", {} late", r.late);
        }
        if r.critical > 0 {
            s += &format!(", {} critical", r.critical);
        }
        lines.push(s);
    }
    if r.tickets > 0 {
        let mut s = format!("{} of {} tickets done", r.tickets_done, r.tickets);
        if r.points > 0.0 {
            s += &format!(" ({} of {} points)", crate::duration::fmt_num(r.points_done), crate::duration::fmt_num(r.points));
        }
        lines.push(s);
    }
    if r.risks > 0 {
        let std = p.risk_settings.standard;
        lines.push(format!("{} active risk{}, worst {}", r.risks, if r.risks == 1 { "" } else { "s" }, r.worst_risk.map(|l| l.label(std)).unwrap_or("—")));
    }
    if r.measures > 0 {
        lines.push(format!("{} measure{}: worst {}", r.measures, if r.measures == 1 { "" } else { "s" }, r.worst_measure.label().to_lowercase()));
    }
    lines.join("\n")
}

// ------------------------------------------------------------- the view

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let pal = super::theme::pal();
    ui.painter().rect_filled(rect, 0.0, pal.bg);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 36.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 4.0))), |ui| toolbar(app, ui, rect));
    ui.painter().hline(rect.x_range(), bar.bottom(), Stroke::new(1.0, pal.grid));
    let body = Rect::from_min_max(pos2(rect.left(), bar.bottom() + 1.0), rect.max);
    let panel_w = PANEL_W.min(body.width() * 0.45);
    let canvas = Rect::from_min_max(body.min, pos2(body.right() - panel_w, body.bottom()));
    let panel_r = Rect::from_min_max(pos2(canvas.right(), body.top()), body.max);
    // keep the selection valid (an undo may have removed it)
    match app.system_view.sel {
        Sel::Element(id) if app.project.system.element(id).is_none() => app.system_view.sel = Sel::None,
        Sel::Interface(id) if app.project.system.interface(id).is_none() => app.system_view.sel = Sel::None,
        _ => {}
    }
    app.system_view.also.retain(|id| app.project.system.element(*id).is_some());
    if !app.system_view.fitted && canvas.width() > 100.0 {
        fit(app, canvas);
    }
    canvas_ui(app, ui, canvas);
    ui.painter().vline(panel_r.left(), panel_r.y_range(), Stroke::new(1.0, pal.grid));
    ui.painter().rect_filled(panel_r.shrink2(vec2(0.5, 0.0)).translate(vec2(0.5, 0.0)), 0.0, pal.panel);
    ui.scope_builder(egui::UiBuilder::new().max_rect(panel_r.shrink2(vec2(12.0, 8.0))), |ui| panel(app, ui));
}

/// Zoom and scroll to show everything.
fn fit(app: &mut ProjectFcgApp, canvas: Rect) {
    app.system_view.fitted = true;
    let Some((lo, hi)) = app.project.system.bounds() else {
        app.system_view.zoom = 1.0;
        app.system_view.offset = vec2(40.0, 40.0);
        return;
    };
    let size = vec2(hi[0] - lo[0], hi[1] - lo[1]) + vec2(80.0, 80.0);
    let z = (canvas.width() / size.x).min(canvas.height() / size.y).clamp(0.3, 1.25);
    let centre = vec2((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    app.system_view.zoom = z;
    app.system_view.offset = canvas.size() / 2.0 - centre * z;
}

fn toolbar(app: &mut ProjectFcgApp, ui: &mut egui::Ui, whole: Rect) {
    ui.horizontal_centered(|ui| {
        ui.label(RichText::new("System Diagram").strong().size(super::theme::text_size() + 2.0));
        ui.label(RichText::new("experimental").small().color(super::theme::pal().text_dim));
        ui.separator();
        egui::ComboBox::from_id_salt("sys_kind").selected_text(app.system_view.new_kind.label()).width(100.0).show_ui(ui, |ui| {
            for k in ElementKind::ALL {
                ui.selectable_value(&mut app.system_view.new_kind, k, k.label());
            }
        });
        if ui.button("+ Add").on_hover_text("Add an element (or double-click the background)").clicked() {
            let st = &app.system_view;
            let c = (whole.size() / 2.0 - st.offset) / st.zoom;
            let s = st.new_kind.default_size();
            app.add_system_element(st.new_kind, [c.x - s[0] / 2.0, c.y - s[1] / 2.0]);
        }
        let pair: Option<(u32, u32)> = match (app.system_view.sel, app.system_view.also.as_slice()) {
            (Sel::Element(a), [b]) if a != *b => Some((*b, a)),
            _ => None,
        };
        if ui.add_enabled(pair.is_some(), egui::Button::new("Connect")).on_hover_text("Ctrl+click two elements, then Connect. Or drag the round handle of a selected element onto another.").clicked() {
            if let Some((a, b)) = pair {
                app.connect_system(a, b);
            }
        }
        ui.separator();
        ui.label("Color by:");
        egui::ComboBox::from_id_salt("sys_color").selected_text(app.system_view.color_by.label()).width(70.0).show_ui(ui, |ui| {
            for c in ColorBy::ALL {
                ui.selectable_value(&mut app.system_view.color_by, c, c.label());
            }
        });
        ui.separator();
        if ui.button("Fit").on_hover_text("Show the whole diagram").clicked() {
            app.system_view.fitted = false;
        }
        if ui.button("−").clicked() {
            app.system_view.zoom = (app.system_view.zoom / 1.2).max(0.25);
        }
        ui.label(format!("{:.0}%", app.system_view.zoom * 100.0));
        if ui.button("+").clicked() {
            app.system_view.zoom = (app.system_view.zoom * 1.2).min(2.5);
        }
    });
}

/// The element under `p` (canvas units): the topmost, preferring ones that
/// aren't containers (a container is only hit near its edges or title).
fn element_at(sys: &crate::system::SystemModel, p: Pos2) -> Option<u32> {
    let hit = |e: &&Element| rect_of(e).contains(p);
    if let Some(e) = sys.elements.iter().rev().filter(|e| !e.kind.is_container()).find(hit) {
        return Some(e.id);
    }
    sys.elements.iter().rev().filter(|e| e.kind.is_container()).find(hit).map(|e| e.id)
}

/// The interface near `p` (screen), given each interface's screen segment.
fn interface_at(segs: &[(u32, Pos2, Pos2)], p: Pos2) -> Option<u32> {
    segs.iter().map(|(id, a, b)| (*id, dist_to_segment(p, *a, *b))).filter(|(_, d)| *d < 6.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
}

fn canvas_ui(app: &mut ProjectFcgApp, ui: &mut egui::Ui, canvas: Rect) {
    let pal = super::theme::pal();
    let painter = ui.painter_at(canvas);
    let resp = ui.interact(canvas, ui.scope_id().with("system_canvas"), Sense::click_and_drag());
    let z = app.system_view.zoom;
    let origin = canvas.min + app.system_view.offset;
    let to_screen = |p: Pos2, z: f32, origin: Pos2| origin + p.to_vec2() * z;
    let to_world = |s: Pos2, z: f32, origin: Pos2| ((s - origin) / z).to_pos2();

    // ---- scroll and zoom
    if resp.hovered() {
        let (scroll, ctrl, hover) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.command, i.pointer.hover_pos()));
        if ctrl && scroll.y != 0.0 {
            let nz = (z * (1.0 + scroll.y * 0.002)).clamp(0.25, 2.5);
            if let Some(hp) = hover {
                let w = to_world(hp, z, origin);
                app.system_view.offset = hp - canvas.min - w.to_vec2() * nz;
            }
            app.system_view.zoom = nz;
        } else if scroll != Vec2::ZERO {
            app.system_view.offset += scroll;
        }
    }
    let z = app.system_view.zoom;
    let origin = canvas.min + app.system_view.offset;

    // ---- the drag in progress, applied to a copy for drawing
    let mut sys = app.project.system.clone();
    match app.system_view.drag {
        Some(Drag::Move { id, d }) => sys.move_by(id, [d.x, d.y]),
        Some(Drag::Resize { id, d }) => {
            if let Some(e) = sys.element_mut(id) {
                e.size = [(e.size[0] + d.x).max(60.0), (e.size[1] + d.y).max(36.0)];
            }
        }
        _ => {}
    }

    // ---- interface segments (screen), parallel ones spread apart
    let screen_rect = |e: &Element| Rect::from_min_max(to_screen(rect_of(e).min, z, origin), to_screen(rect_of(e).max, z, origin));
    let mut pairs: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for f in &sys.interfaces {
        pairs.entry((f.from.min(f.to), f.from.max(f.to))).or_default().push(f.id);
    }
    let mut segs: Vec<(u32, Pos2, Pos2)> = vec![];
    for f in &sys.interfaces {
        let (Some(a), Some(b)) = (sys.element(f.from), sys.element(f.to)) else { continue };
        let (ra, rb) = (screen_rect(a), screen_rect(b));
        let group = &pairs[&(f.from.min(f.to), f.from.max(f.to))];
        let k = group.iter().position(|x| *x == f.id).unwrap_or(0) as f32 - (group.len() as f32 - 1.0) / 2.0;
        let dir = (rb.center() - ra.center()).normalized();
        let mut n = vec2(-dir.y, dir.x) * k * 12.0 * z.max(0.6);
        if f.from > f.to {
            n = -n;
        }
        let pa = edge_point(ra, rb.center() + n);
        let pb = edge_point(rb, ra.center() + n);
        segs.push((f.id, pa + n, pb + n));
    }

    // ---- pointer
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let selected_el = match app.system_view.sel {
        Sel::Element(id) => sys.element(id).cloned(),
        _ => None,
    };
    let handles = selected_el.as_ref().map(|e| {
        let r = screen_rect(e);
        (pos2(r.right(), r.center().y), r.right_bottom())
    });
    if resp.drag_started() {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            let w = to_world(p, z, origin);
            app.system_view.drag = Some(match (handles, &selected_el) {
                (Some((c, _)), Some(e)) if (p - c).length() <= HANDLE_R + 4.0 => Drag::Connect { from: e.id, to: p },
                (Some((_, rz)), Some(e)) if (p - rz).length() <= 10.0 => Drag::Resize { id: e.id, d: Vec2::ZERO },
                _ => match element_at(&app.project.system, w) {
                    Some(id) => {
                        app.commit_system_draft();
                        app.system_view.sel = Sel::Element(id);
                        app.system_view.also.clear();
                        Drag::Move { id, d: Vec2::ZERO }
                    }
                    None => Drag::Pan,
                },
            });
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        match &mut app.system_view.drag {
            Some(Drag::Pan) => app.system_view.offset += d,
            Some(Drag::Move { d: m, .. }) | Some(Drag::Resize { d: m, .. }) => *m += d / z,
            Some(Drag::Connect { to, .. }) => {
                if let Some(p) = pointer {
                    *to = p;
                }
            }
            None => {}
        }
    }
    if resp.drag_stopped() {
        match app.system_view.drag.take() {
            Some(Drag::Move { id, d }) if d.length() > 0.5 => {
                app.edit_project("Move Element", move |p| {
                    p.system.move_by(id, [d.x, d.y]);
                    p.system.reparent(id);
                    Ok(())
                });
            }
            Some(Drag::Resize { id, d }) if d.length() > 0.5 => {
                app.edit_project("Resize Element", move |p| {
                    if let Some(e) = p.system.element_mut(id) {
                        e.size = [(e.size[0] + d.x).max(60.0), (e.size[1] + d.y).max(36.0)];
                    }
                    // what is inside a container now changes
                    let ids: Vec<u32> = p.system.elements.iter().map(|e| e.id).collect();
                    for k in ids {
                        p.system.reparent(k);
                    }
                    Ok(())
                });
            }
            Some(Drag::Connect { from, to }) => {
                if let Some(target) = element_at(&app.project.system, to_world(to, z, origin)).filter(|t| *t != from) {
                    app.connect_system(from, target);
                }
            }
            _ => {}
        }
    }
    if resp.clicked() {
        if let Some(p) = pointer {
            let ctrl = ui.input(|i| i.modifiers.command);
            let w = to_world(p, z, origin);
            app.commit_system_draft();
            let hit_el = element_at(&app.project.system, w);
            // an interface is hit before the container under it
            let hit_if = interface_at(&segs, p);
            let container = hit_el.and_then(|id| app.project.system.element(id)).is_some_and(|e| e.kind.is_container());
            match (hit_el, hit_if) {
                (_, Some(f)) if hit_el.is_none() || container => {
                    app.system_view.sel = Sel::Interface(f);
                    app.system_view.also.clear();
                }
                (Some(id), _) => {
                    if ctrl {
                        if let Sel::Element(prev) = app.system_view.sel {
                            if prev != id {
                                app.system_view.also = vec![prev];
                            }
                        }
                    } else {
                        app.system_view.also.clear();
                    }
                    app.system_view.sel = Sel::Element(id);
                }
                _ => {
                    app.system_view.sel = Sel::None;
                    app.system_view.also.clear();
                }
            }
        }
    }
    if resp.double_clicked() {
        if let Some(p) = pointer {
            let w = to_world(p, z, origin);
            match element_at(&app.project.system, w) {
                Some(id) if !app.project.system.element(id).is_some_and(|e| e.kind.is_container()) => app.system_view.focus_name = true,
                _ => {
                    let k = app.system_view.new_kind;
                    let s = k.default_size();
                    app.add_system_element(k, [w.x - s[0] / 2.0, w.y - s[1] / 2.0]);
                }
            }
        }
    }
    if resp.hovered() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && ui.memory(|m| m.focused().is_none()) {
        app.system_view.sel = Sel::None;
        app.system_view.also.clear();
    }

    // ---- draw
    let cache = app.system_cache();
    let rollups = cache.elements.clone();
    let if_rollups = cache.interfaces.clone();
    let st = &app.system_view;
    // dots
    let step = 40.0 * z;
    if step >= 12.0 {
        let dot = pal.grid;
        let x0 = canvas.left() + (origin.x - canvas.left()).rem_euclid(step);
        let y0 = canvas.top() + (origin.y - canvas.top()).rem_euclid(step);
        let mut y = y0;
        while y < canvas.bottom() {
            let mut x = x0;
            while x < canvas.right() {
                painter.circle_filled(pos2(x, y), 1.0, dot);
                x += step;
            }
            y += step;
        }
    }
    if sys.elements.is_empty() {
        painter.text(canvas.center() - vec2(0.0, 20.0), Align2::CENTER_CENTER, "Map what the project is building: its parts and how they connect.", FontId::proportional(super::theme::text_size() + 2.0), pal.text);
        painter.text(canvas.center() + vec2(0.0, 6.0), Align2::CENTER_CENTER, "Double-click anywhere to add an element, or start from a layout:", FontId::proportional(super::theme::text_size()), pal.text_dim);
    }
    let base_font = super::theme::text_size() * z;
    let color_of = |e: &Element| -> Color32 {
        if let Some(c) = e.color {
            return Color32::from_rgb(c[0], c[1], c[2]);
        }
        match st.color_by {
            ColorBy::Kind => kind_color(e.kind),
            ColorBy::Health => health_color(rollups.get(&e.id).map(|r| r.health()).unwrap_or(Health::Unknown)),
            ColorBy::State => state_color(e.state),
        }
    };
    let is_sel = |id: u32| st.sel == Sel::Element(id) || st.also.contains(&id);
    let progress_bar = |r: Rect, pct: f64, color: Color32| {
        let bar = Rect::from_min_max(pos2(r.left() + 6.0 * z, r.bottom() - 7.0 * z), pos2(r.right() - 6.0 * z, r.bottom() - 4.0 * z));
        painter.rect_filled(bar, 1.0, pal.grid);
        let w = bar.width() * (pct as f32 / 100.0).clamp(0.0, 1.0);
        painter.rect_filled(Rect::from_min_size(bar.min, vec2(w, bar.height())), 1.0, color);
    };
    let health_dot = |at: Pos2, r: &Rollup| {
        let h = r.health();
        if h != Health::Unknown {
            painter.circle_filled(at, 4.5 * z.max(0.6), health_color(h));
        }
    };
    // containers first, behind everything
    for e in sys.elements.iter().filter(|e| e.kind.is_container()) {
        let r = screen_rect(e);
        if !r.intersects(canvas) {
            continue;
        }
        let c = color_of(e);
        painter.rect_filled(r, 6.0 * z, c.gamma_multiply(0.08));
        let stroke = if is_sel(e.id) { Stroke::new(2.5, pal.accent) } else { Stroke::new(1.5, c) };
        if e.kind == ElementKind::Group {
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.add(Shape::dashed_line(&pts, stroke, 6.0, 4.0));
        } else {
            painter.rect_stroke(r, 6.0 * z, stroke, StrokeKind::Inside);
        }
        if base_font >= 6.0 {
            let title = format!("{}  ·  {}", e.name, e.kind.label());
            painter.text(r.left_top() + vec2(10.0, 8.0) * z, Align2::LEFT_TOP, title, FontId::proportional(base_font * 1.05), pal.heading);
        }
        if let Some(ru) = rollups.get(&e.id) {
            health_dot(r.right_top() + vec2(-12.0, 12.0) * z, ru);
            if let Some(pct) = ru.progress() {
                if base_font >= 6.0 {
                    painter.text(r.right_top() + vec2(-24.0, 6.0) * z, Align2::RIGHT_TOP, format!("{pct:.0}%"), FontId::proportional(base_font * 0.85), pal.text_dim);
                }
            }
        }
    }
    // interfaces
    for (id, a, b) in &segs {
        let Some(f) = sys.interface(*id) else { continue };
        let sel = st.sel == Sel::Interface(*id);
        let c = interface_color(f.kind);
        let w = if sel { 3.0 } else { 1.8 };
        painter.line_segment([*a, *b], Stroke::new(w, if sel { pal.accent } else { c }));
        let head = 9.0 * z.max(0.7);
        match f.flow {
            Flow::Forward => arrow_head(&painter, *b, *a, c, head),
            Flow::Both => {
                arrow_head(&painter, *b, *a, c, head);
                arrow_head(&painter, *a, *b, c, head);
            }
            Flow::None => {}
        }
        let mid = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        if let Some(ru) = if_rollups.get(id).filter(|r| r.tasks + r.tickets + r.risks > 0) {
            painter.circle_filled(mid, 5.0 * z.max(0.6), health_color(ru.health()));
        }
        if !f.label.is_empty() && base_font >= 6.0 {
            let g = painter.layout_no_wrap(f.label.clone(), FontId::proportional(base_font * 0.85), pal.text);
            let lr = Rect::from_center_size(mid - vec2(0.0, 12.0 * z), g.size() + vec2(8.0, 2.0));
            painter.rect_filled(lr, 3.0, pal.bg);
            painter.galley(lr.min + vec2(4.0, 1.0), g, pal.text);
        }
    }
    // everything else
    for e in sys.elements.iter().filter(|e| !e.kind.is_container()) {
        let r = screen_rect(e);
        if !r.intersects(canvas) {
            continue;
        }
        let c = color_of(e);
        let sel = is_sel(e.id);
        let stroke = if sel { Stroke::new(2.5, pal.accent) } else { Stroke::new(1.5, c) };
        let round = match e.kind {
            ElementKind::Function => r.height().min(r.width()) / 2.0,
            ElementKind::Note => 2.0,
            _ => 6.0 * z,
        };
        let fill = if e.kind == ElementKind::Note { c.gamma_multiply(0.30) } else { pal.panel.lerp_to_gamma(c, 0.12) };
        painter.rect_filled(r, round, fill);
        if e.kind == ElementKind::External {
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.add(Shape::dashed_line(&pts, stroke, 6.0, 4.0));
        } else {
            painter.rect_stroke(r, round, stroke, StrokeKind::Inside);
        }
        if e.kind == ElementKind::Software && base_font >= 6.0 {
            // a code bracket in the corner
            painter.text(r.left_top() + vec2(8.0, 5.0) * z, Align2::LEFT_TOP, "</>", FontId::monospace(base_font * 0.7), c);
        }
        if base_font >= 5.0 {
            let inner = r.shrink2(vec2(10.0, 8.0) * z);
            if e.kind == ElementKind::Note {
                let g = painter.layout(e.name.clone(), FontId::proportional(base_font * 0.95), pal.text, inner.width().max(10.0));
                painter.with_clip_rect(r).galley(inner.min, g, pal.text);
            } else {
                let g = painter.layout(e.name.clone(), FontId::proportional(base_font).clone(), pal.text, inner.width().max(10.0));
                let caption = format!("{} · {}", e.kind.label(), e.state.label());
                let gc = painter.layout_no_wrap(caption, FontId::proportional(base_font * 0.75), pal.text_dim);
                let h = g.size().y + gc.size().y;
                let top = r.center().y - h / 2.0 - 2.0 * z;
                let clip = painter.with_clip_rect(r.shrink(2.0));
                clip.galley(pos2(r.center().x - g.size().x / 2.0, top), g.clone(), pal.text);
                clip.galley(pos2(r.center().x - gc.size().x / 2.0, top + g.size().y), gc, pal.text_dim);
            }
        }
        if let Some(ru) = rollups.get(&e.id) {
            if e.kind != ElementKind::Note {
                health_dot(r.right_top() + vec2(-9.0, 9.0) * z, ru);
                if let Some(pct) = ru.progress() {
                    progress_bar(r, pct, health_color(Health::Good));
                }
                if ru.measures > 0 {
                    painter.circle_stroke(r.right_top() + vec2(-21.0, 9.0) * z, 4.0 * z.max(0.6), Stroke::new(1.5, measure_color(ru.worst_measure)));
                }
            }
        }
    }
    // the selection's handles
    if let Some(e) = selected_el {
        let r = screen_rect(&e);
        let (c, rz) = (pos2(r.right(), r.center().y), r.right_bottom());
        painter.circle_filled(c, HANDLE_R, pal.accent);
        painter.circle_stroke(c, HANDLE_R, Stroke::new(1.5, pal.bg));
        painter.rect_filled(Rect::from_center_size(rz, vec2(8.0, 8.0)), 1.0, pal.accent);
    }
    if let Some(Drag::Connect { from, to }) = st.drag {
        if let Some(e) = sys.element(from) {
            let r = screen_rect(e);
            let a = edge_point(r, to);
            painter.line_segment([a, to], Stroke::new(2.0, pal.accent));
            arrow_head(&painter, to, a, pal.accent, 9.0);
        }
    }
    // hover: the rollup
    if st.drag.is_none() {
        if let Some(hp) = resp.hover_pos() {
            let w = to_world(hp, z, origin);
            let tip = if let Some(f) = interface_at(&segs, hp).filter(|_| element_at(&sys, w).is_none_or(|id| sys.element(id).is_some_and(|e| e.kind.is_container()))) {
                sys.interface(f).map(|f| {
                    let n = |id| sys.element(id).map(|e| e.name.clone()).unwrap_or_default();
                    let mut s = format!("{} interface: {} {} {}", f.kind.label(), n(f.from), if f.flow == Flow::Both { "↔" } else { "→" }, n(f.to));
                    if !f.label.is_empty() {
                        s = format!("{}\n{s}", f.label);
                    }
                    if let Some(r) = if_rollups.get(&f.id).filter(|r| r.tasks + r.tickets + r.risks > 0) {
                        s += &format!("\n{}", rollup_text(&app.project, r));
                    }
                    s
                })
            } else {
                element_at(&sys, w).and_then(|id| sys.element(id)).filter(|e| e.kind != ElementKind::Note).map(|e| {
                    let r = rollups.get(&e.id).cloned().unwrap_or_default();
                    format!("{}\n{} · {}\n{}", e.name, e.kind.label(), e.state.label(), rollup_text(&app.project, &r))
                })
            };
            if let Some(t) = tip {
                resp.clone().on_hover_text_at_pointer(t);
            }
        }
    }
    if sys.elements.is_empty() {
        let b = Rect::from_center_size(canvas.center() + vec2(0.0, 40.0), vec2(160.0, 26.0));
        if ui.put(b, egui::Button::new("Starter layout")).clicked() {
            app.system_starter();
        }
    }
}

// ------------------------------------------------------------- the panel

fn work_label(app: &ProjectFcgApp, w: WorkLink) -> Option<(Item, String, String)> {
    let p = &app.project;
    let item = match w {
        WorkLink::Task(uid) => Item::Task(p.index_of_uid(uid)?),
        WorkLink::Ticket(id) => {
            p.ticket_index(id)?;
            Item::Ticket(id)
        }
        WorkLink::Risk(id) => {
            p.risk(id)?;
            Item::Risk(id)
        }
    };
    let (icon, title, sub) = app.describe(item);
    Some((item, format!("{icon} {title}"), sub))
}

fn panel(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let pal = super::theme::pal();
    let ctx = ui.ctx().clone();
    // load the draft for the selection
    let current = match app.system_view.sel {
        Sel::Element(id) => app.project.system.element(id).cloned().map(Draft::Element),
        Sel::Interface(id) => app.project.system.interface(id).cloned().map(Draft::Interface),
        Sel::None => None,
    };
    let same = |a: &Draft, b: &Draft| match (a, b) {
        (Draft::Element(x), Draft::Element(y)) => x.id == y.id,
        (Draft::Interface(x), Draft::Interface(y)) => x.id == y.id,
        _ => false,
    };
    let focused = ctx.memory(|m| m.focused().is_some());
    match (&app.system_view.draft, &current) {
        (Some((_, edit)), Some(c)) if same(edit, c) => {
            // changed elsewhere (undo, a drag): take it, unless mid-typing
            let (base, _) = app.system_view.draft.as_ref().unwrap();
            if base != c && !focused {
                app.system_view.draft = Some((c.clone(), c.clone()));
            }
        }
        (Some(_), _) => {
            app.commit_system_draft();
            app.system_view.draft = current.clone().map(|c| (c.clone(), c));
            app.system_view.link_query.clear();
        }
        (None, Some(c)) => app.system_view.draft = Some((c.clone(), c.clone())),
        (None, None) => {}
    }
    egui::ScrollArea::vertical().id_salt("sys_panel").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some((_, mut edit)) = app.system_view.draft.clone() else {
            overview(app, ui);
            return;
        };
        let mut link_to: Option<WorkLink> = None;
        let mut unlink: Option<WorkLink> = None;
        let mut go: Option<Item> = None;
        let mut delete = false;
        let work: Vec<WorkLink>;
        match &mut edit {
            Draft::Element(e) => {
                ui.label(RichText::new(e.kind.label()).color(pal.text_dim));
                let name = if e.kind == ElementKind::Note {
                    ui.add(egui::TextEdit::multiline(&mut e.name).desired_rows(4).desired_width(f32::INFINITY))
                } else {
                    ui.add(egui::TextEdit::singleline(&mut e.name).font(egui::TextStyle::Heading).desired_width(f32::INFINITY))
                };
                if std::mem::take(&mut app.system_view.focus_name) {
                    name.request_focus();
                }
                egui::Grid::new("sys_el").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Kind:");
                    egui::ComboBox::from_id_salt("sys_el_kind").selected_text(e.kind.label()).show_ui(ui, |ui| {
                        for k in ElementKind::ALL {
                            ui.selectable_value(&mut e.kind, k, k.label());
                        }
                    });
                    ui.end_row();
                    if e.kind != ElementKind::Note {
                        ui.label("State:");
                        egui::ComboBox::from_id_salt("sys_el_state").selected_text(e.state.label()).show_ui(ui, |ui| {
                            for s in ElementState::ALL {
                                ui.selectable_value(&mut e.state, s, s.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Owner:");
                        ui.add(egui::TextEdit::singleline(&mut e.owner).desired_width(160.0));
                        ui.end_row();
                    }
                    ui.label("Color:");
                    ui.horizontal(|ui| {
                        let mut own = e.color.is_some();
                        if ui.checkbox(&mut own, "Own color").changed() {
                            e.color = if own {
                                let c = kind_color(e.kind);
                                Some([c.r(), c.g(), c.b()])
                            } else {
                                None
                            };
                        }
                        if let Some(c) = e.color.as_mut() {
                            ui.color_edit_button_srgb(c);
                        }
                    });
                    ui.end_row();
                });
                if e.kind != ElementKind::Note {
                    ui.label("Notes:");
                    ui.add(egui::TextEdit::multiline(&mut e.notes).desired_rows(2).desired_width(f32::INFINITY));
                    let ru = app.system_cache().elements.get(&e.id).cloned().unwrap_or_default();
                    ui.add_space(4.0);
                    egui::Frame::new().fill(pal.bg).corner_radius(4.0).inner_margin(6).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                            ui.painter().circle_filled(dot.center(), 5.0, health_color(ru.health()));
                            ui.label(RichText::new(rollup_text(&app.project, &ru).lines().next().unwrap_or("").to_string()).strong());
                        });
                        for l in rollup_text(&app.project, &ru).lines().skip(1) {
                            ui.label(l);
                        }
                        if !app.project.system.descendants(e.id).is_empty() {
                            ui.weak("Includes the elements inside it.");
                        }
                    });
                    ui.add_space(6.0);
                    measures_ui(ui, &mut e.measures);
                }
                work = e.work.clone();
            }
            Draft::Interface(f) => {
                let n = |id| app.project.system.element(id).map(|e| e.name.clone()).unwrap_or_default();
                ui.label(RichText::new("Interface").color(pal.text_dim));
                ui.add(egui::TextEdit::singleline(&mut f.label).hint_text("Name (e.g. telemetry link)").font(egui::TextStyle::Heading).desired_width(f32::INFINITY));
                egui::Grid::new("sys_if").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("From:");
                    ui.label(n(f.from));
                    ui.end_row();
                    ui.label("To:");
                    ui.horizontal(|ui| {
                        ui.label(n(f.to));
                        if ui.small_button("⇄").on_hover_text("Swap the ends").clicked() {
                            std::mem::swap(&mut f.from, &mut f.to);
                        }
                    });
                    ui.end_row();
                    ui.label("Kind:");
                    egui::ComboBox::from_id_salt("sys_if_kind").selected_text(f.kind.label()).show_ui(ui, |ui| {
                        for k in InterfaceKind::ALL {
                            ui.selectable_value(&mut f.kind, k, k.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Flow:");
                    egui::ComboBox::from_id_salt("sys_if_flow").selected_text(f.flow.label()).show_ui(ui, |ui| {
                        for k in Flow::ALL {
                            ui.selectable_value(&mut f.flow, k, k.label());
                        }
                    });
                    ui.end_row();
                });
                ui.label("Notes:");
                ui.add(egui::TextEdit::multiline(&mut f.notes).desired_rows(2).desired_width(f32::INFINITY));
                let ru = app.system_cache().interfaces.get(&f.id).cloned().unwrap_or_default();
                if ru.tasks + ru.tickets + ru.risks > 0 {
                    ui.label(rollup_text(&app.project, &ru));
                }
                work = f.work.clone();
            }
        }
        // ---- linked work
        let is_note = matches!(&edit, Draft::Element(e) if e.kind == ElementKind::Note);
        if !is_note {
            ui.add_space(8.0);
            ui.label(RichText::new("Work tied to it").strong()).on_hover_text("Tasks, tickets (stories, epics…) and risks. Their progress, lateness and ratings roll up into the element and anything it sits in.");
            for w in &work {
                match work_label(app, *w) {
                    Some((item, title, sub)) => {
                        ui.horizontal(|ui| {
                            if ui.small_button("✖").on_hover_text("Untie").clicked() {
                                unlink = Some(*w);
                            }
                            if ui.add(egui::Label::new(title).sense(Sense::click()).truncate()).on_hover_text(format!("{sub}\nClick: go to it")).clicked() {
                                go = Some(item);
                            }
                        });
                    }
                    None => {
                        ui.horizontal(|ui| {
                            if ui.small_button("✖").clicked() {
                                unlink = Some(*w);
                            }
                            ui.weak("(deleted)");
                        });
                    }
                }
            }
            if work.is_empty() {
                ui.weak("Nothing yet.");
            }
            let picked: Vec<u32> = app.selected_rows().into_iter().map(|i| app.project.tasks[i].uid).filter(|u| !work.contains(&WorkLink::Task(*u))).collect();
            if !picked.is_empty() && ui.button(format!("Tie the {} selected task{}", picked.len(), if picked.len() == 1 { "" } else { "s" })).on_hover_text("The tasks selected in the Gantt chart").clicked() {
                for u in picked {
                    match &mut edit {
                        Draft::Element(e) => e.work.push(WorkLink::Task(u)),
                        Draft::Interface(f) => f.work.push(WorkLink::Task(u)),
                    }
                }
            }
            ui.add(egui::TextEdit::singleline(&mut app.system_view.link_query).hint_text("Find a task, ticket or risk to tie…").desired_width(f32::INFINITY));
            if !app.system_view.link_query.trim().is_empty() {
                let hits = super::item_search::search(&app.project, &app.system_view.link_query);
                let mut shown = 0;
                for (item, _) in hits {
                    let w = match item {
                        Item::Task(i) => WorkLink::Task(app.project.tasks[i].uid),
                        Item::Ticket(id) => WorkLink::Ticket(id),
                        Item::Risk(id) => WorkLink::Risk(id),
                        Item::Resource(_) => continue,
                    };
                    if work.contains(&w) {
                        continue;
                    }
                    let (icon, title, sub) = app.describe(item);
                    if ui.add(egui::Button::new(format!("+ {icon} {title}")).truncate()).on_hover_text(sub).clicked() {
                        link_to = Some(w);
                    }
                    shown += 1;
                    if shown >= 8 {
                        break;
                    }
                }
                if shown == 0 {
                    ui.weak("Nothing found.");
                }
            }
        }
        ui.add_space(10.0);
        if ui.button("Delete").on_hover_text("Delete key").clicked() {
            delete = true;
        }
        // ---- apply
        let list = match &mut edit {
            Draft::Element(e) => &mut e.work,
            Draft::Interface(f) => &mut f.work,
        };
        if let Some(w) = link_to {
            list.push(w);
            app.system_view.link_query.clear();
        }
        if let Some(w) = unlink {
            list.retain(|x| *x != w);
        }
        if let Some((base, _)) = app.system_view.draft.clone() {
            app.system_view.draft = Some((base, edit));
        }
        if delete {
            app.delete_system_selection();
            return;
        }
        if let Some(item) = go {
            app.commit_system_draft();
            app.go_to_item(item, false, &ctx);
            return;
        }
    });
    // save edits once nothing is being typed
    if !ctx.memory(|m| m.focused().is_some()) {
        app.commit_system_draft();
    }
}

fn measures_ui(ui: &mut egui::Ui, ms: &mut Vec<Measure>) {
    ui.label(RichText::new("Measures").strong()).on_hover_text(
        "Technical performance measures: what the element must achieve (range, weight, latency…).\nCurrent: the latest estimate or test result. Target: the goal. Threshold: the least that's acceptable.",
    );
    let mut remove = None;
    if !ms.is_empty() {
        egui::Grid::new("sys_measures").num_columns(6).spacing([4.0, 4.0]).show(ui, |ui| {
            for h in ["", "Measure", "Current", "Target", "Threshold", ""] {
                ui.label(RichText::new(h).small());
            }
            ui.end_row();
            for (k, m) in ms.iter_mut().enumerate() {
                let (dot, resp) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, measure_color(m.status()));
                resp.on_hover_text(m.status().label());
                ui.add(egui::TextEdit::singleline(&mut m.name).hint_text("Name").desired_width(78.0));
                for v in [&mut m.current, &mut m.target, &mut m.threshold] {
                    num_field(ui, v);
                }
                ui.menu_button("⋯", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Unit:");
                        ui.add(egui::TextEdit::singleline(&mut m.unit).desired_width(60.0));
                    });
                    ui.checkbox(&mut m.lower_is_better, "Lower is better");
                    if ui.button("Remove").clicked() {
                        remove = Some(k);
                        ui.close();
                    }
                });
                ui.end_row();
            }
        });
    }
    if let Some(k) = remove {
        ms.remove(k);
    }
    if ui.small_button("+ Measure").clicked() {
        ms.push(Measure::default());
    }
}

/// A number that may be blank.
fn num_field(ui: &mut egui::Ui, v: &mut Option<f64>) {
    let id = ui.next_auto_id();
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| v.map(crate::duration::fmt_num).unwrap_or_default());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(46.0));
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        let t = text.trim().replace(',', "");
        if t.is_empty() {
            *v = None;
        } else if let Ok(x) = t.parse::<f64>() {
            if x.is_finite() {
                *v = Some(x);
            }
        }
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

/// With nothing selected: what's on the diagram, and how to use it.
fn overview(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let pal = super::theme::pal();
    let sys = &app.project.system;
    ui.label(RichText::new("System Diagram").strong().size(super::theme::text_size() + 1.0));
    ui.weak("Experimental: try it, and say what's missing.");
    ui.add_space(6.0);
    let n = sys.elements.iter().filter(|e| e.kind != ElementKind::Note).count();
    ui.label(format!("{n} element{}, {} interface{}", if n == 1 { "" } else { "s" }, sys.interfaces.len(), if sys.interfaces.len() == 1 { "" } else { "s" }));
    let rollups = app.system_cache().elements.clone();
    let mut trouble: Vec<(String, Health)> = app
        .project
        .system
        .elements
        .iter()
        .filter_map(|e| rollups.get(&e.id).map(|r| (e.name.clone(), r.health())))
        .filter(|(_, h)| *h >= Health::Watch)
        .collect();
    trouble.sort_by(|a, b| b.1.cmp(&a.1));
    if !trouble.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new("Needs attention").strong());
        for (name, h) in trouble.iter().take(10) {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, health_color(*h));
                ui.label(name);
            });
        }
    }
    ui.add_space(10.0);
    ui.label(RichText::new("How it works").strong());
    for line in [
        "Double-click the background to add an element of the kind chosen above.",
        "Drag an element to move it. Dropped inside a system, subsystem or group, it belongs to it and moves with it.",
        "Select an element and drag its round handle onto another to connect them.",
        "Tie tasks, tickets and risks to an element: their progress, lateness and ratings roll up into it.",
        "Add measures (range, weight, latency…) with a target and a threshold, so the diagram shows whether it works, not only whether it's on time.",
        "Drag the background to pan; Ctrl+scroll to zoom. Delete removes the selection; Ctrl+Z undoes.",
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("•").color(pal.text_dim));
            ui.label(line);
        });
    }
}
