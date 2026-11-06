//! Risk Register view (View ▸ Risk Register): the register as a table,
//! the selected risk's details, and the 5×5 risk matrix.
//!
//! The details are edited in a draft and saved to the plan (one undo step)
//! whenever you stop typing: picking a value, leaving a field, selecting
//! another risk or leaving the view. Undo and redo reload the draft.

use super::ProjectFcgApp;
use crate::risks::{response_help, responses, Dimension, Rating, Risk, RiskKind, RiskLevel, RiskLink, RiskStandard, RiskStatus, RiskStep};
use chrono::NaiveDate;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    Active,
    All,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    Id,
    #[default]
    Level,
    Title,
    Status,
    Due,
    Owner,
}

#[derive(Default)]
pub struct RiskViewState {
    pub selected: Option<u32>,
    /// The selected risk being edited, and the version of it in the plan
    /// the edits started from.
    pub draft: Option<Risk>,
    base: Option<Risk>,
    /// Date fields as typed (identified, due, closed), and per step.
    dates: [String; 3],
    step_dates: Vec<String>,
    pub status: StatusFilter,
    pub kind: Option<RiskKind>,
    pub search: String,
    /// Clicked matrix cell: (opportunities?, likelihood, consequence).
    pub cell: Option<(bool, u8, u8)>,
    pub sort: SortBy,
    pub sort_desc: bool,
    /// Show where handling should take each risk.
    pub show_target: bool,
    task_search: String,
}

fn fmt_d(d: Option<NaiveDate>) -> String {
    d.map(|d| d.format("%-m/%-d/%y").to_string()).unwrap_or_default()
}

fn parse_d(s: &str) -> Option<Option<NaiveDate>> {
    let t = s.trim();
    if t.is_empty() {
        return Some(None);
    }
    crate::model::parse_date(t, chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap()).ok().map(|d| Some(d.date()))
}

/// Colours for Low / Moderate / High (threats, issues) and for
/// opportunities (blues), in the current theme.
pub fn level_colors(level: RiskLevel, opportunity: bool) -> (Color32, Color32) {
    let dark = super::theme::pal().dark;
    let (fill, text) = match (level, opportunity, dark) {
        (RiskLevel::None, _, false) => (Color32::from_gray(225), Color32::from_gray(80)),
        (RiskLevel::None, _, true) => (Color32::from_gray(60), Color32::from_gray(190)),
        (RiskLevel::Low, false, false) => (Color32::from_rgb(120, 190, 110), Color32::from_gray(15)),
        (RiskLevel::Moderate, false, false) => (Color32::from_rgb(246, 210, 90), Color32::from_gray(15)),
        (RiskLevel::High, false, false) => (Color32::from_rgb(222, 82, 70), Color32::WHITE),
        (RiskLevel::Low, false, true) => (Color32::from_rgb(54, 110, 52), Color32::from_gray(235)),
        (RiskLevel::Moderate, false, true) => (Color32::from_rgb(140, 116, 30), Color32::from_gray(245)),
        (RiskLevel::High, false, true) => (Color32::from_rgb(150, 44, 40), Color32::WHITE),
        (RiskLevel::Low, true, false) => (Color32::from_rgb(200, 224, 246), Color32::from_gray(15)),
        (RiskLevel::Moderate, true, false) => (Color32::from_rgb(120, 176, 232), Color32::from_gray(15)),
        (RiskLevel::High, true, false) => (Color32::from_rgb(40, 110, 196), Color32::WHITE),
        (RiskLevel::Low, true, true) => (Color32::from_rgb(40, 62, 90), Color32::from_gray(230)),
        (RiskLevel::Moderate, true, true) => (Color32::from_rgb(40, 90, 150), Color32::WHITE),
        (RiskLevel::High, true, true) => (Color32::from_rgb(52, 120, 210), Color32::WHITE),
    };
    (fill, text)
}

fn level_chip(ui: &mut egui::Ui, level: RiskLevel, std: RiskStandard, opportunity: bool) {
    let (fill, text) = level_colors(level, opportunity);
    egui::Frame::new().fill(fill).corner_radius(3.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
        ui.label(RichText::new(level.label(std)).size(11.5).color(text));
    });
}

impl ProjectFcgApp {
    /// The risks the table shows, in its order.
    pub fn risk_rows(&self) -> Vec<u32> {
        let st = &self.risk_view;
        let p = &self.project;
        let needle = st.search.trim().to_lowercase();
        let mut v: Vec<&Risk> = p
            .risks
            .iter()
            .filter(|r| match st.status {
                StatusFilter::Active => r.status.is_active(),
                StatusFilter::Closed => !r.status.is_active(),
                StatusFilter::All => true,
            })
            .filter(|r| st.kind.is_none_or(|k| r.kind == k))
            .filter(|r| match st.cell {
                Some((opp, l, c)) => (r.kind == RiskKind::Opportunity) == opp && r.kind != RiskKind::Issue && r.rating.likelihood == l && r.rating.worst() == c,
                None => true,
            })
            .filter(|r| {
                needle.is_empty()
                    || [&r.title, &r.event, &r.consequence, &r.cause, &r.description, &r.owner, &r.category].iter().any(|s| s.to_lowercase().contains(&needle))
                    || r.key().to_lowercase().contains(&needle)
                    || p.risk_links_text(r).to_lowercase().contains(&needle)
            })
            .collect();
        let key = |r: &Risk| (p.risk_level(r), (p.risk_score(r) * 1000.0) as i64);
        v.sort_by(|a, b| {
            let o = match st.sort {
                SortBy::Id => a.id.cmp(&b.id),
                SortBy::Level => key(a).cmp(&key(b)).reverse(),
                SortBy::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                SortBy::Status => (a.status as u8).cmp(&(b.status as u8)),
                SortBy::Due => a.due.unwrap_or(NaiveDate::MAX).cmp(&b.due.unwrap_or(NaiveDate::MAX)),
                SortBy::Owner => a.owner.to_lowercase().cmp(&b.owner.to_lowercase()),
            };
            let o = if st.sort_desc { o.reverse() } else { o };
            o.then(a.id.cmp(&b.id))
        });
        v.into_iter().map(|r| r.id).collect()
    }

    /// Add a risk (tied to `links`) and select it.
    pub fn new_risk(&mut self, kind: RiskKind, links: Vec<RiskLink>) -> Option<u32> {
        self.commit_risk_draft();
        let today = chrono::Local::now().date_naive();
        let title = match kind {
            RiskKind::Threat => "New risk",
            RiskKind::Opportunity => "New opportunity",
            RiskKind::Issue => "New issue",
        };
        let r = Risk { kind, title: title.into(), identified: Some(today), links, ..Default::default() };
        let id = self.edit_project("New Risk", move |p| Ok(p.add_risk(r)))?;
        self.select_risk(Some(id));
        Some(id)
    }

    /// Task context menu ▸ Add Risk for Task: a risk tied to the active
    /// task, opened in the Risk Register.
    pub fn add_risk_for_active_task(&mut self) {
        let Some(i) = self.active_index() else { return };
        let uid = self.project.tasks[i].uid;
        self.commit_edit();
        self.view.main = super::MainView::Risks;
        self.risk_view.cell = None;
        self.risk_view.kind = None;
        self.risk_view.search.clear();
        self.risk_view.status = StatusFilter::Active;
        self.new_risk(RiskKind::Threat, vec![RiskLink::Task(uid)]);
    }

    pub fn select_risk(&mut self, id: Option<u32>) {
        if self.risk_view.selected != id {
            self.commit_risk_draft();
            self.risk_view.selected = id;
            self.risk_view.draft = None;
            self.risk_view.base = None;
        }
    }

    /// Save the draft to the plan if it changed (one undo step).
    pub fn commit_risk_draft(&mut self) {
        let (Some(d), Some(b)) = (self.risk_view.draft.clone(), self.risk_view.base.clone()) else { return };
        if d == b {
            return;
        }
        if self.project.risk(d.id) != Some(&b) {
            // the plan changed underneath (undo): the draft is stale
            self.risk_view.draft = None;
            self.risk_view.base = None;
            return;
        }
        let label = format!("Edit {}", d.key());
        let d2 = d.clone();
        if self.edit_project(&label, move |p| p.update_risk(d2)).is_some() {
            self.risk_view.base = Some(d);
        }
    }

    pub fn delete_selected_risk(&mut self) {
        let Some(id) = self.risk_view.selected else { return };
        self.risk_view.draft = None;
        self.risk_view.base = None;
        let key = self.project.risk(id).map(|r| r.key()).unwrap_or_default();
        if self.edit_project(&format!("Delete {key}"), move |p| p.delete_risk(id)).is_some() {
            self.risk_view.selected = None;
        }
    }

    /// Bring the draft in line with the plan: load it when the selection
    /// or the plan's copy changed.
    fn sync_risk_draft(&mut self) {
        let Some(id) = self.risk_view.selected else { return };
        let Some(cur) = self.project.risk(id).cloned() else {
            self.risk_view.selected = None;
            self.risk_view.draft = None;
            self.risk_view.base = None;
            return;
        };
        if self.risk_view.base.as_ref() != Some(&cur) || self.risk_view.draft.is_none() {
            let st = &mut self.risk_view;
            st.dates = [fmt_d(cur.identified), fmt_d(cur.due), fmt_d(cur.closed)];
            st.step_dates = cur.steps.iter().map(|s| fmt_d(s.date)).collect();
            st.draft = Some(cur.clone());
            st.base = Some(cur);
        }
    }
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let pal = super::theme::pal();
    ui.painter().rect_filled(rect, 0.0, pal.bg);
    app.sync_risk_draft();
    let rows = app.risk_rows();
    let detail_w = (rect.width() * 0.42).clamp(360.0, 560.0);
    let left = Rect::from_min_max(rect.min, pos2(rect.right() - detail_w, rect.bottom()));
    let right = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
    let toolbar_h = 36.0;
    let matrix_h = (left.height() * 0.48).clamp(240.0, 380.0);
    let bar = Rect::from_min_size(left.min, vec2(left.width(), toolbar_h));
    let table = Rect::from_min_max(pos2(left.left(), bar.bottom()), pos2(left.right(), left.bottom() - matrix_h));
    let matrix = Rect::from_min_max(pos2(left.left(), table.bottom()), left.max);

    ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 4.0))), |ui| toolbar(app, ui));
    ui.painter().hline(left.x_range(), bar.bottom(), Stroke::new(1.0, pal.grid));
    ui.scope_builder(egui::UiBuilder::new().max_rect(table.shrink2(vec2(6.0, 4.0))), |ui| risk_table(app, ui, &rows));
    ui.painter().hline(left.x_range(), matrix.top(), Stroke::new(1.0, pal.grid));
    matrices(app, ui, matrix.shrink(8.0));
    ui.painter().vline(right.left(), right.y_range(), Stroke::new(1.0, pal.grid));
    ui.scope_builder(egui::UiBuilder::new().max_rect(right.shrink2(vec2(10.0, 6.0))), |ui| {
        egui::ScrollArea::vertical().id_salt("risk_detail").auto_shrink([false, false]).show(ui, |ui| details(app, ui));
    });
    // save edits once nobody is typing
    if !ui.ctx().egui_wants_keyboard_input() {
        app.commit_risk_draft();
    }
}

fn toolbar(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        ui.menu_button(RichText::new("+ New").strong(), |ui| {
            for (k, label) in [(RiskKind::Threat, "Risk"), (RiskKind::Opportunity, "Opportunity"), (RiskKind::Issue, "Issue (it has happened)")] {
                if ui.button(label).clicked() {
                    app.new_risk(k, vec![]);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Add a risk, opportunity or issue to the register");
        let has = app.risk_view.selected.is_some();
        if ui.add_enabled(has, egui::Button::new("Delete")).clicked() {
            app.delete_selected_risk();
        }
        ui.separator();
        ui.label("Standard:");
        let mut std = app.project.risk_settings.standard;
        egui::ComboBox::from_id_salt("risk_std").selected_text(std.label()).show_ui(ui, |ui| {
            for s in RiskStandard::ALL {
                ui.selectable_value(&mut std, s, s.label());
            }
        })
        .response
        .on_hover_text("How risks are rated and handled, unless a risk names its own standard");
        if std != app.project.risk_settings.standard {
            app.commit_risk_draft();
            let had_custom = app.project.risk_settings.matrix.is_some();
            // a recoloured matrix belongs to the standard it was made for
            if app.edit_project("Risk Standard", move |p| {
                p.risk_settings.standard = std;
                p.risk_settings.matrix = None;
                Ok(())
            })
            .is_some()
                && had_custom
            {
                app.set_status(format!("Risk register now follows {}; the matrix's custom colours were reset", std.label()));
            }
        }
        ui.separator();
        let st = &mut app.risk_view;
        egui::ComboBox::from_id_salt("risk_status_f")
            .selected_text(match st.status {
                StatusFilter::Active => "Open",
                StatusFilter::All => "All",
                StatusFilter::Closed => "Closed",
            })
            .width(70.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.status, StatusFilter::Active, "Open (not closed)");
                ui.selectable_value(&mut st.status, StatusFilter::Closed, "Closed");
                ui.selectable_value(&mut st.status, StatusFilter::All, "All");
            });
        egui::ComboBox::from_id_salt("risk_kind_f").selected_text(st.kind.map_or("All types", |k| k.label())).width(96.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut st.kind, None, "All types");
            for k in RiskKind::ALL {
                ui.selectable_value(&mut st.kind, Some(k), k.label());
            }
        });
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("Search").desired_width(120.0));
        if let Some((opp, l, c)) = st.cell {
            if ui.button(format!("{}L{l} × C{c}  ✖", if opp { "Opp. " } else { "" })).on_hover_text("Showing one matrix cell; click to show all").clicked() {
                st.cell = None;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Export CSV…").on_hover_text("The register as a spreadsheet").clicked() {
                app.commit_risk_draft();
                app.pick_save(super::filebrowser::Mode::ExportRisksCsv);
            }
        });
    });
}

fn risk_table(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rows: &[u32]) {
    let pal = super::theme::pal();
    if app.project.risks.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No risks yet").size(15.0).color(pal.text_dim));
            ui.label(RichText::new("+ New adds one; right-click a task ▸ Add Risk ties one to it.").color(pal.text_dim));
        });
        return;
    }
    let header = |ui: &mut egui::Ui, app: &mut ProjectFcgApp, label: &str, by: Option<SortBy>| {
        let st = &mut app.risk_view;
        let arrow = match by {
            // the level sorts highest first: that's "down"
            Some(b) if st.sort == b => {
                if st.sort_desc != (b == SortBy::Level) {
                    " ▾"
                } else {
                    " ▴"
                }
            }
            _ => "",
        };
        let r = ui.add(egui::Label::new(RichText::new(format!("{label}{arrow}")).strong().size(12.0)).sense(Sense::click()));
        if let (Some(b), true) = (by, r.clicked()) {
            if st.sort == b {
                st.sort_desc = !st.sort_desc;
            } else {
                st.sort = b;
                st.sort_desc = false;
            }
        }
    };
    egui::ScrollArea::both().id_salt("risk_table").auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("risk_grid").striped(true).num_columns(9).spacing([12.0, 5.0]).min_row_height(22.0).show(ui, |ui| {
            header(ui, app, "ID", Some(SortBy::Id));
            header(ui, app, "Title", Some(SortBy::Title));
            header(ui, app, "Level", Some(SortBy::Level));
            header(ui, app, "L × C", None);
            header(ui, app, "Status", Some(SortBy::Status));
            header(ui, app, "Owner", Some(SortBy::Owner));
            header(ui, app, "Response", None);
            header(ui, app, "Due", Some(SortBy::Due));
            header(ui, app, "Linked to", None);
            ui.end_row();
            let mut clicked = None;
            for &id in rows {
                let p = &app.project;
                let Some(r) = p.risk(id) else { continue };
                let std = p.risk_standard(r);
                let sel = app.risk_view.selected == Some(id);
                let key = ui.selectable_label(sel, RichText::new(r.key()).monospace());
                let title = if r.title.trim().is_empty() { "(untitled)".to_string() } else { r.title.clone() };
                let t = ui.selectable_label(sel, title);
                level_chip(ui, p.risk_level(r), std, r.kind == RiskKind::Opportunity);
                let lc = if r.kind == RiskKind::Issue {
                    format!("— × {}", r.rating.worst())
                } else if r.rating.is_rated(r.kind) {
                    format!("{} × {}", r.rating.likelihood, r.rating.worst())
                } else {
                    String::new()
                };
                ui.label(lc);
                ui.label(r.status.label());
                ui.label(&r.owner);
                ui.label(&r.response);
                let overdue = r.due.is_some_and(|d| d < chrono::Local::now().date_naive()) && r.status.is_active();
                ui.label(RichText::new(fmt_d(r.due)).color(if overdue { Color32::from_rgb(214, 66, 52) } else { pal.text }));
                ui.label(RichText::new(p.risk_links_text(r)).color(pal.text_dim));
                ui.end_row();
                if key.clicked() || t.clicked() {
                    clicked = Some(id);
                }
            }
            if let Some(id) = clicked {
                app.select_risk(Some(id));
            }
        });
        if rows.is_empty() {
            ui.label(RichText::new("No risks match the filters.").color(pal.text_dim));
        }
    });
}

/// The risk matrix (and, when there are any, the opportunity matrix and
/// the issues by consequence).
fn matrices(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect) {
    let p = &app.project;
    let has_opp = p.risks.iter().any(|r| r.kind == RiskKind::Opportunity && r.status.is_active());
    let has_issue = p.risks.iter().any(|r| r.kind == RiskKind::Issue && r.status.is_active());
    let head = Rect::from_min_size(area.min, vec2(area.width(), 22.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(head), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Risk matrix").strong());
            ui.checkbox(&mut app.risk_view.show_target, "Show targets").on_hover_text("Arrows from each risk's rating to where its handling should take it");
            let std = app.project.risk_settings.standard;
            ui.weak(match std {
                RiskStandard::Dod => "DoD risk reporting matrix (worst of cost, schedule, performance)",
                RiskStandard::Pmi => "PMI probability × impact (High ≥ 0.18, Moderate ≥ 0.05)",
                RiskStandard::None => "Likelihood × impact (High ≥ 15, Medium ≥ 5)",
            });
            if app.project.risk_settings.matrix.is_some() {
                ui.weak("· custom colours");
            }
        });
    });
    let body = Rect::from_min_max(pos2(area.left(), head.bottom() + 4.0), area.max);
    let issues_w = if has_issue { 120.0 } else { 0.0 };
    let n = if has_opp { 2.0 } else { 1.0 };
    let w = (body.width() - issues_w - 16.0 * n) / n;
    let mut x = body.left();
    matrix(app, ui, Rect::from_min_size(pos2(x, body.top()), vec2(w, body.height())), false);
    x += w + 16.0;
    if has_opp {
        matrix(app, ui, Rect::from_min_size(pos2(x, body.top()), vec2(w, body.height())), true);
        x += w + 16.0;
    }
    if has_issue {
        issues_strip(app, ui, Rect::from_min_size(pos2(x, body.top()), vec2(issues_w - 8.0, body.height())));
    }
}

fn matrix(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect, opportunities: bool) {
    let pal = super::theme::pal();
    let p = &app.project;
    let std = p.risk_settings.standard;
    let painter = ui.painter_at(area);
    let label_w = 86.0;
    let foot = 44.0;
    let title_h = 16.0;
    painter.text(pos2(area.left() + label_w, area.top()), Align2::LEFT_TOP, if opportunities { "Opportunities (likelihood × benefit)" } else { "Risks (likelihood × consequence)" }, FontId::proportional(11.5), pal.text_dim);
    let grid = Rect::from_min_max(pos2(area.left() + label_w, area.top() + title_h), pos2(area.right(), area.bottom() - foot));
    let (cw, ch) = (grid.width() / 5.0, grid.height() / 5.0);
    let cell = |l: u8, c: u8| Rect::from_min_size(pos2(grid.left() + (c - 1) as f32 * cw, grid.bottom() - l as f32 * ch), vec2(cw, ch));
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let lnames = std.likelihood_levels();
    let cnames = std.consequence_levels();
    // risks in each cell
    let mut in_cell: std::collections::HashMap<(u8, u8), Vec<&Risk>> = Default::default();
    for r in p.risks.iter().filter(|r| r.status.is_active() && r.kind != RiskKind::Issue && (r.kind == RiskKind::Opportunity) == opportunities) {
        if r.rating.is_rated(r.kind) {
            in_cell.entry((r.rating.likelihood, r.rating.worst())).or_default().push(r);
        }
    }
    let mut hover_tip: Option<String> = None;
    let mut clicked: Option<(u8, u8)> = None;
    // right-click: Some(Some(cell, level)) recolours a cell, Some(None) resets
    let mut recolour: Option<Option<(u8, u8, u8)>> = None;
    let custom = p.risk_settings.matrix.is_some();
    for l in 1..=5u8 {
        let y = cell(l, 1).center().y;
        painter.text(pos2(grid.left() - 6.0, y), Align2::RIGHT_CENTER, format!("{l} {}", lnames[l as usize - 1].0), FontId::proportional(10.5), pal.text);
        for c in 1..=5u8 {
            let r = cell(l, c);
            let (fill, text) = level_colors(p.risk_settings.cell_level(std, l, c), opportunities);
            let picked = app.risk_view.cell == Some((opportunities, l, c));
            painter.rect_filled(r.shrink(1.0), 2.0, fill);
            if picked {
                painter.rect_stroke(r.shrink(1.5), 2.0, Stroke::new(2.5, pal.accent), egui::StrokeKind::Inside);
            }
            if let Some(rs) = in_cell.get(&(l, c)) {
                let keys: Vec<String> = rs.iter().map(|r| r.key()).collect();
                let fits = ((r.width() - 6.0) / 44.0).floor().max(1.0) as usize * ((r.height() - 6.0) / 14.0).floor().max(1.0) as usize;
                let shown = if keys.len() > fits { fits.saturating_sub(1) } else { keys.len() };
                let per_row = ((r.width() - 6.0) / 44.0).floor().max(1.0) as usize;
                for (k, key) in keys.iter().take(shown).enumerate() {
                    let at = pos2(r.left() + 4.0 + (k % per_row) as f32 * 44.0, r.top() + 4.0 + (k / per_row) as f32 * 14.0);
                    let sel = rs[k].id == app.risk_view.selected.unwrap_or(0);
                    let g = painter.text(at, Align2::LEFT_TOP, key, FontId::monospace(10.5), text);
                    if sel {
                        painter.rect_stroke(g.expand(1.5), 2.0, Stroke::new(1.5, text), egui::StrokeKind::Outside);
                    }
                }
                if keys.len() > shown {
                    painter.text(r.right_bottom() - vec2(4.0, 3.0), Align2::RIGHT_BOTTOM, format!("+{}", keys.len() - shown), FontId::proportional(10.5), text);
                }
                if pointer.is_some_and(|pp| r.contains(pp)) {
                    hover_tip = Some(rs.iter().map(|r| format!("{}  {}", r.key(), r.title)).collect::<Vec<_>>().join("\n"));
                }
            }
            let resp = ui.interact(r, ui.scope_id().with(("cell", opportunities, l, c)), Sense::click());
            if resp.clicked() {
                clicked = Some((l, c));
            }
            resp.context_menu(|ui| {
                ui.weak(format!("Likelihood {l}, consequence {c}"));
                for (v, name) in [(0u8, RiskLevel::Low), (1, RiskLevel::Moderate), (2, RiskLevel::High)] {
                    if ui.button(format!("Colour as {}", name.label(std))).clicked() {
                        recolour = Some(Some((l, c, v)));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.add_enabled(custom, egui::Button::new(format!("Reset to the {} matrix", std.short()))).clicked() {
                    recolour = Some(None);
                    ui.close();
                }
            });
        }
    }
    for c in 1..=5u8 {
        let r = cell(1, c);
        let name = cnames[c as usize - 1].split(" (").next().unwrap_or("").replace(" Impact", "");
        // number above name: five names don't fit on one line
        painter.text(pos2(r.center().x, grid.bottom() + 2.0), Align2::CENTER_TOP, c.to_string(), FontId::proportional(10.5), pal.text);
        if cw > 40.0 {
            painter.text(pos2(r.center().x, grid.bottom() + 14.0), Align2::CENTER_TOP, name, FontId::proportional(if cw > 70.0 { 10.5 } else { 9.5 }), pal.text_dim);
        }
    }
    painter.text(pos2(grid.center().x, area.bottom() - 2.0), Align2::CENTER_BOTTOM, if opportunities { "Benefit →" } else { "Consequence →" }, FontId::proportional(10.5), pal.text_dim);
    painter.text(pos2(area.left() + 2.0, grid.top() - 2.0), Align2::LEFT_BOTTOM, "Likelihood ↑", FontId::proportional(10.5), pal.text_dim);
    // targets: from each rating to where handling takes it
    if app.risk_view.show_target {
        for rs in in_cell.values() {
            for r in rs {
                if !r.target.is_rated(r.kind) {
                    continue;
                }
                let a = cell(r.rating.likelihood, r.rating.worst()).center();
                let b = cell(r.target.likelihood, r.target.worst()).center();
                if a == b {
                    continue;
                }
                let sel = app.risk_view.selected == Some(r.id);
                let stroke = Stroke::new(if sel { 2.4 } else { 1.3 }, if sel { pal.accent } else { pal.text.gamma_multiply(0.7) });
                painter.line_segment([a, b], stroke);
                let d = (b - a).normalized() * 8.0;
                let nrm = vec2(-d.y, d.x) * 0.5;
                painter.add(egui::Shape::convex_polygon(vec![b, b - d + nrm, b - d - nrm], stroke.color, Stroke::NONE));
                painter.circle_stroke(b, 4.0, stroke);
            }
        }
    }
    if let Some(change) = recolour {
        app.commit_risk_draft();
        app.edit_project("Risk Matrix", move |p| {
            match change {
                None => p.risk_settings.matrix = None,
                Some((l, c, v)) => {
                    let s = &p.risk_settings;
                    let mut m = s.matrix.unwrap_or_else(|| {
                        let mut m = [[0u8; 5]; 5];
                        for (li, row) in m.iter_mut().enumerate() {
                            for (ci, x) in row.iter_mut().enumerate() {
                                *x = s.cell_level(std, li as u8 + 1, ci as u8 + 1) as u8 - 1;
                            }
                        }
                        m
                    });
                    m[l as usize - 1][c as usize - 1] = v;
                    p.risk_settings.matrix = Some(m);
                }
            }
            Ok(())
        });
    }
    if let Some((l, c)) = clicked {
        let st = &mut app.risk_view;
        st.cell = if st.cell == Some((opportunities, l, c)) { None } else { Some((opportunities, l, c)) };
    }
    if let Some(t) = hover_tip {
        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), ui.scope_id().with("cell_tip"), egui::PopupAnchor::Pointer).show(|ui| {
            ui.label(t);
        });
    }
}

fn issues_strip(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect) {
    let pal = super::theme::pal();
    let p = &app.project;
    let painter = ui.painter_at(area);
    painter.text(area.left_top(), Align2::LEFT_TOP, "Issues", FontId::proportional(11.5), pal.text_dim);
    let grid = Rect::from_min_max(pos2(area.left(), area.top() + 16.0), pos2(area.right(), area.bottom() - 44.0));
    let h = grid.height() / 5.0;
    for c in (1..=5u8).rev() {
        let r = Rect::from_min_size(pos2(grid.left(), grid.top() + (5 - c) as f32 * h), vec2(grid.width(), h));
        let probe = Risk { kind: RiskKind::Issue, rating: Rating::new(5, c), ..Default::default() };
        let (fill, text) = level_colors(p.risk_level(&probe), false);
        painter.rect_filled(r.shrink(1.0), 2.0, fill);
        let keys: Vec<String> = p.risks.iter().filter(|r| r.kind == RiskKind::Issue && r.status.is_active() && r.rating.worst() == c).map(|r| r.key()).collect();
        painter.text(r.left_top() + vec2(4.0, 3.0), Align2::LEFT_TOP, format!("C{c}"), FontId::proportional(10.0), text);
        painter.text(r.left_top() + vec2(26.0, 3.0), Align2::LEFT_TOP, keys.join(" "), FontId::monospace(10.5), text);
    }
}

// ---------------------------------------------------------------- details

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(13.0));
    ui.separator();
}

fn level_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut u8, names: &[String], help: &[String]) {
    let shown = match names.get((*value as usize).wrapping_sub(1)) {
        Some(n) => format!("{value} {n}"),
        None => "Not rated".to_string(),
    };
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(210.0).show_ui(ui, |ui| {
        ui.selectable_value(value, 0, "Not rated");
        for (k, n) in names.iter().enumerate() {
            let r = ui.selectable_value(value, k as u8 + 1, format!("{} {n}", k + 1));
            if let Some(h) = help.get(k).filter(|h| !h.is_empty()) {
                r.on_hover_text(h.as_str());
            }
        }
    });
}

fn multiline(ui: &mut egui::Ui, text: &mut String, hint: &str) {
    ui.add(egui::TextEdit::multiline(text).desired_rows(2).desired_width(f32::INFINITY).hint_text(hint));
}

fn details(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let pal = super::theme::pal();
    let Some(mut d) = app.risk_view.draft.clone() else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Select a risk to see and edit it.").color(pal.text_dim)));
        return;
    };
    let p = &app.project;
    let std = p.risk_standard(&d);
    let mut dates = app.risk_view.dates.clone();
    let mut step_dates = app.risk_view.step_dates.clone();
    step_dates.resize(d.steps.len(), String::new());
    let mut go_to: Option<u32> = None;

    // ---- header ----
    ui.horizontal(|ui| {
        ui.label(RichText::new(d.key()).monospace().size(15.0).strong());
        level_chip(ui, p.risk_level(&d), std, d.kind == RiskKind::Opportunity);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut s = d.standard;
            let reg = p.risk_settings.standard;
            egui::ComboBox::from_id_salt("rd_std").selected_text(s.map_or(format!("Register ({})", reg.short()), |s| s.label().to_string())).show_ui(ui, |ui| {
                ui.selectable_value(&mut s, None, format!("Register's ({})", reg.label()));
                for x in RiskStandard::ALL {
                    ui.selectable_value(&mut s, Some(x), x.label());
                }
            });
            d.standard = s;
            ui.label("Standard:");
        });
    });
    ui.horizontal(|ui| {
        for k in RiskKind::ALL {
            ui.selectable_value(&mut d.kind, k, k.label());
        }
    });
    ui.add(egui::TextEdit::singleline(&mut d.title).hint_text("Title").desired_width(f32::INFINITY).font(FontId::proportional(14.0)));

    // ---- statement ----
    section(ui, "Statement");
    let std = p.risk_standard(&d);
    match std {
        RiskStandard::Dod => {
            if d.kind == RiskKind::Issue {
                ui.label("Issue (what has happened):");
                multiline(ui, &mut d.event, "the event or condition");
                ui.label("Consequence:");
                multiline(ui, &mut d.consequence, "its effect on cost, schedule or performance");
            } else {
                ui.label("If (the event or condition):");
                multiline(ui, &mut d.event, if d.kind == RiskKind::Opportunity { "an action or condition that could be taken advantage of" } else { "something that may happen" });
                ui.label("then (the consequence):");
                multiline(ui, &mut d.consequence, if d.kind == RiskKind::Opportunity { "the benefit to cost, schedule or performance" } else { "the effect on cost, schedule or performance" });
            }
            ui.label("Root cause (if known):");
            multiline(ui, &mut d.cause, "why it may happen (no mitigation here)");
        }
        RiskStandard::Pmi => {
            ui.label("Because of (cause):");
            multiline(ui, &mut d.cause, "a fact, condition or requirement");
            ui.label(if d.kind == RiskKind::Opportunity { "this opportunity may occur:" } else { "this risk may occur:" });
            multiline(ui, &mut d.event, "the uncertain event");
            ui.label("which would lead to (effect):");
            multiline(ui, &mut d.effect_or_consequence(), "the effect on the project's objectives");
        }
        RiskStandard::None => multiline(ui, &mut d.description, "What could happen, and what it would do"),
    }
    let preview = d.statement(std);
    if std != RiskStandard::None && !preview.is_empty() {
        ui.label(RichText::new(preview).italics().color(pal.text_dim));
    }
    if std != RiskStandard::None {
        ui.label("Notes:");
        multiline(ui, &mut d.description, "background, assumptions");
    }

    // ---- what it's tied to ----
    section(ui, "Tied to");
    let mut remove = None;
    if d.links.is_empty() {
        ui.label(RichText::new("The whole project").color(pal.text_dim));
    }
    for (k, &l) in d.links.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button("✖").on_hover_text("Untie").clicked() {
                remove = Some(k);
            }
            let label = p.risk_link_label(l);
            if let RiskLink::Task(u) = l {
                if ui.link(label).on_hover_text("Show the task in the Gantt Chart").clicked() {
                    go_to = Some(u);
                }
            } else {
                ui.label(label);
            }
        });
    }
    if let Some(k) = remove {
        d.links.remove(k);
    }
    ui.horizontal(|ui| {
        let ts = &mut app.risk_view.task_search;
        ui.menu_button("+ Task", |ui| {
            ui.add(egui::TextEdit::singleline(ts).hint_text("ID or name").desired_width(200.0));
            let needle = ts.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for (i, t) in p.tasks.iter().enumerate().filter(|(i, t)| needle.is_empty() || t.name.to_lowercase().contains(&needle) || (i + 1).to_string() == needle).take(200) {
                    let link = RiskLink::Task(t.uid);
                    if !d.links.contains(&link) && ui.button(format!("{}  {}", i + 1, t.name)).clicked() {
                        d.links.push(link);
                        ui.close();
                    }
                }
            });
        });
        ui.menu_button("+ Resource", |ui| {
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for r in &p.resources {
                    let link = RiskLink::Resource(r.uid);
                    if !d.links.contains(&link) && ui.button(&r.name).clicked() {
                        d.links.push(link);
                        ui.close();
                    }
                }
                if p.resources.is_empty() {
                    ui.weak("No resources yet");
                }
            });
        });
        ui.menu_button("+ Release", |ui| {
            for r in &p.board.releases {
                let link = RiskLink::Release(r.id);
                if !d.links.contains(&link) && ui.button(&r.name).clicked() {
                    d.links.push(link);
                    ui.close();
                }
            }
            if p.board.releases.is_empty() {
                ui.weak("No releases yet (Board ▸ Releases)");
            }
        });
    });

    // ---- who, when ----
    section(ui, "Ownership");
    egui::Grid::new("rd_own").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Category:");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut d.category).desired_width(170.0));
            ui.menu_button("▾", |ui| {
                for c in std.categories() {
                    if ui.button(*c).clicked() {
                        d.category = c.to_string();
                        ui.close();
                    }
                }
            });
        });
        ui.end_row();
        ui.label("Owner:");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut d.owner).desired_width(170.0));
            if !p.resources.is_empty() {
                ui.menu_button("▾", |ui| {
                    for r in p.resources.iter().filter(|r| r.rtype == crate::model::ResourceType::Work) {
                        if ui.button(&r.name).clicked() {
                            d.owner = r.name.clone();
                            ui.close();
                        }
                    }
                });
            }
        });
        ui.end_row();
        ui.label("Status:");
        egui::ComboBox::from_id_salt("rd_status").selected_text(d.status.label()).show_ui(ui, |ui| {
            for s in RiskStatus::ALL {
                ui.selectable_value(&mut d.status, s, s.label());
            }
        });
        ui.end_row();
        for (k, label) in ["Identified:", if std == RiskStandard::Dod { "Expected closure:" } else { "Due:" }, "Closed:"].iter().enumerate() {
            ui.label(*label);
            ui.add(egui::TextEdit::singleline(&mut dates[k]).desired_width(110.0).hint_text("m/d/yy"));
            ui.end_row();
        }
        if std == RiskStandard::Pmi {
            ui.label("Urgency:");
            ui.add(egui::TextEdit::singleline(&mut d.urgency).desired_width(170.0).hint_text("when it could happen"));
            ui.end_row();
        }
    });
    for (k, slot) in [&mut d.identified, &mut d.due, &mut d.closed].into_iter().enumerate() {
        if let Some(v) = parse_d(&dates[k]) {
            *slot = v;
        }
    }
    let was_closed = app.risk_view.base.as_ref().is_some_and(|b| b.status == RiskStatus::Closed);
    if d.status == RiskStatus::Closed && !was_closed && d.closed.is_none() {
        d.closed = Some(chrono::Local::now().date_naive());
        dates[2] = fmt_d(d.closed);
    }

    // ---- assessment ----
    section(ui, "Assessment");
    let lnames: Vec<String> = std.likelihood_levels().iter().map(|(n, pct)| if pct.is_empty() { n.to_string() } else { format!("{n} ({pct})") }).collect();
    let cnames: Vec<String> = std.consequence_levels().iter().map(|s| s.to_string()).collect();
    rating_editor(ui, "now", &mut d.rating, d.kind, std, &lnames, &cnames, true);
    if std == RiskStandard::None && d.kind != RiskKind::Issue {
        ui.horizontal(|ui| {
            let mut on = d.probability_pct.is_some();
            ui.checkbox(&mut on, "Probability:");
            if on {
                let mut v = d.probability_pct.unwrap_or(50.0);
                ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0).suffix("%"));
                d.probability_pct = Some(v);
            } else {
                d.probability_pct = None;
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("Cost impact:");
        ui.add(egui::DragValue::new(&mut d.cost_impact).speed(100.0).range(0.0..=1e12).prefix(p.settings.currency_symbol.clone()));
        ui.label("Schedule:");
        ui.add(egui::DragValue::new(&mut d.schedule_days).speed(0.5).range(0.0..=36600.0).suffix(" days"));
    });
    // what it adds up to
    let level = p.risk_level(&d);
    ui.horizontal(|ui| {
        ui.label("Level:");
        level_chip(ui, level, std, d.kind == RiskKind::Opportunity);
        let score = p.risk_score(&d);
        if d.rating.is_rated(d.kind) {
            ui.label(match std {
                RiskStandard::Pmi => format!("P × I = {:.2}", score),
                _ => format!("score {}", score as i64),
            });
        }
        if d.cost_impact > 0.0 {
            let emv = p.risk_emv(&d);
            ui.label(format!("EMV {}", crate::resources::fmt_money(emv, &p.settings.currency_symbol))).on_hover_text("Expected monetary value: probability × cost impact (negative for threats)");
        }
    });

    // ---- response ----
    section(ui, if std == RiskStandard::Dod { "Handling" } else { "Response" });
    ui.horizontal(|ui| {
        ui.label("Strategy:");
        egui::ComboBox::from_id_salt("rd_resp").selected_text(if d.response.is_empty() { "—" } else { d.response.as_str() }).show_ui(ui, |ui| {
            ui.selectable_value(&mut d.response, String::new(), "—");
            for r in responses(std, d.kind) {
                ui.selectable_value(&mut d.response, r.to_string(), *r).on_hover_text(response_help(r));
            }
        });
        let help = response_help(&d.response);
        if !help.is_empty() {
            ui.weak(help);
        }
    });
    multiline(ui, &mut d.response_plan, "the plan: what will be done, by whom");
    ui.label(RichText::new(if std == RiskStandard::Dod { "Burn-down steps" } else { "Actions" }).strong());
    let mut del_step = None;
    for (k, s) in d.steps.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.done, "");
            ui.add(egui::TextEdit::singleline(&mut s.activity).desired_width(170.0).hint_text("activity"));
            ui.add(egui::TextEdit::singleline(&mut step_dates[k]).desired_width(64.0).hint_text("date"));
            if let Some(v) = parse_d(&step_dates[k]) {
                s.date = v;
            }
            if d.kind != RiskKind::Issue {
                let mut l = s.after.likelihood;
                ui.add(egui::DragValue::new(&mut l).range(0..=5).prefix("L"));
                s.after.likelihood = l;
            }
            let mut c = s.after.worst();
            ui.add(egui::DragValue::new(&mut c).range(0..=5).prefix("C"));
            if c != s.after.worst() {
                s.after.consequence = vec![(Dimension::Overall, c)];
            }
            if ui.small_button("✖").clicked() {
                del_step = Some(k);
            }
        });
    }
    if let Some(k) = del_step {
        d.steps.remove(k);
        step_dates.remove(k);
    }
    if ui.button("+ Step").on_hover_text("A step of the plan, the date it's due, and the rating expected once it's done").clicked() {
        let after = d.steps.last().map(|s| s.after.clone()).unwrap_or_else(|| d.rating.clone());
        d.steps.push(RiskStep { activity: String::new(), date: None, done: false, after });
        step_dates.push(String::new());
    }
    if !d.steps.is_empty() {
        burn_down(ui, p, &d);
    }
    ui.label(RichText::new(if std == RiskStandard::Pmi { "Residual (after the response)" } else { "Target (after handling)" }).strong());
    rating_editor(ui, "target", &mut d.target, d.kind, std, &lnames, &cnames, false);
    if std == RiskStandard::Pmi {
        egui::Grid::new("rd_pmi").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            for (label, text, hint) in [
                ("Triggers:", &mut d.triggers, "warning signs that it's about to happen"),
                ("Contingency plan:", &mut d.contingency, "what to do if it happens"),
                ("Fallback plan:", &mut d.fallback, "if the contingency plan doesn't work"),
                ("Secondary risks:", &mut d.secondary, "risks the response itself creates"),
            ] {
                ui.label(label);
                ui.add(egui::TextEdit::multiline(text).desired_rows(1).desired_width(260.0).hint_text(hint));
                ui.end_row();
            }
        });
    }

    // ---- store the draft ----
    let st = &mut app.risk_view;
    st.dates = dates;
    st.step_dates = step_dates;
    st.draft = Some(d);
    if let Some(u) = go_to {
        app.commit_risk_draft();
        app.view.main = super::MainView::Gantt;
        if let Some(i) = app.project.index_of_uid(u) {
            for a in app.project.ancestors_of(i) {
                app.project.tasks[a].collapsed = false;
            }
        }
        app.select_single(super::RowRef::Task(u));
    }
}

trait EffectField {
    fn effect_or_consequence(&mut self) -> &mut String;
}
impl EffectField for Risk {
    /// PMI's "effect" is the same field as DoD's "then".
    fn effect_or_consequence(&mut self) -> &mut String {
        &mut self.consequence
    }
}

#[allow(clippy::too_many_arguments)]
fn rating_editor(ui: &mut egui::Ui, id: &str, rating: &mut Rating, kind: RiskKind, std: RiskStandard, lnames: &[String], cnames: &[String], per_dimension: bool) {
    egui::Grid::new(("rd_rating", id)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        if kind != RiskKind::Issue {
            ui.label("Likelihood:");
            let mut l = rating.likelihood;
            level_combo(ui, (id, "l"), &mut l, lnames, &[]);
            rating.likelihood = l;
            ui.end_row();
        }
        let std_dims = std.dimensions().to_vec();
        // a rating made under another standard (or overall) is shown as it
        // is, its worst level, until it's split by dimension: just looking
        // at a risk mustn't change it
        let foreign = !rating.consequence.iter().all(|(d, _)| std_dims.contains(d));
        let dims: Vec<Dimension> = if per_dimension && !foreign { std_dims.clone() } else { vec![Dimension::Overall] };
        for dim in dims {
            let what = if kind == RiskKind::Opportunity { "Benefit" } else { "Consequence" };
            ui.label(if dim == Dimension::Overall { format!("{what}:") } else { format!("{what} ({}):", dim.label().to_lowercase()) });
            let help: Vec<String> = if std == RiskStandard::Dod { dim.dod_criteria().iter().map(|s| s.to_string()).collect() } else { vec![] };
            let mut c = if dim == Dimension::Overall { rating.worst() } else { rating.get(dim) };
            level_combo(ui, (id, dim.label()), &mut c, cnames, &help);
            if dim == Dimension::Overall {
                if c != rating.worst() {
                    rating.consequence = if c == 0 { vec![] } else { vec![(Dimension::Overall, c)] };
                }
            } else {
                rating.set(dim, c);
            }
            ui.end_row();
        }
        if per_dimension && foreign && std_dims.len() > 1 {
            ui.label("");
            let names = std_dims.iter().map(|d| d.label().to_lowercase()).collect::<Vec<_>>().join(", ");
            if ui.small_button(format!("Rate {names} separately")).on_hover_text("The worst of them sets the level").clicked() {
                let w = rating.worst();
                rating.consequence = if w == 0 { vec![] } else { vec![(std_dims[0], w)] };
            }
            ui.end_row();
        }
    });
}

/// DoD's burn-down: the rating planned after each step, and where it is
/// now (done steps), against dates.
fn burn_down(ui: &mut egui::Ui, p: &crate::model::Project, d: &Risk) {
    let pal = super::theme::pal();
    let dated: Vec<&RiskStep> = d.steps.iter().filter(|s| s.date.is_some()).collect();
    if dated.is_empty() {
        ui.weak("Give the steps dates to see the burn-down.");
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(460.0), 130.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, pal.grid), egui::StrokeKind::Inside);
    let plot = rect.shrink2(vec2(30.0, 14.0));
    let start = d.identified.or_else(|| dated.iter().filter_map(|s| s.date).min()).unwrap_or_else(|| chrono::Local::now().date_naive());
    let end = dated.iter().filter_map(|s| s.date).max().unwrap_or(start).max(start + chrono::Duration::days(1));
    let span = (end - start).num_days().max(1) as f32;
    let x = |date: NaiveDate| plot.left() + ((date - start).num_days() as f32 / span).clamp(0.0, 1.0) * plot.width();
    let score = |r: &Rating| {
        let probe = Risk { rating: r.clone(), kind: d.kind, standard: Some(p.risk_standard(d)), ..Default::default() };
        let max = if p.risk_standard(d) == RiskStandard::Pmi { 0.72 } else { 25.0 };
        (p.risk_score(&probe) / max) as f32
    };
    let y = |s: f32| plot.bottom() - s.clamp(0.0, 1.0) * plot.height();
    // planned: a staircase from the current rating through each step
    let mut pts = vec![pos2(x(start), y(score(&d.rating)))];
    let mut sorted: Vec<&RiskStep> = dated.clone();
    sorted.sort_by_key(|s| s.date);
    let mut last = score(&d.rating);
    for s in &sorted {
        let px = x(s.date.unwrap());
        pts.push(pos2(px, y(last)));
        last = score(&s.after);
        pts.push(pos2(px, y(last)));
    }
    painter.add(egui::Shape::line(pts.clone(), Stroke::new(1.6, pal.accent)));
    for (k, s) in sorted.iter().enumerate() {
        let at = pts[2 + 2 * k];
        let probe = Risk { rating: s.after.clone(), kind: d.kind, standard: Some(p.risk_standard(d)), ..Default::default() };
        let (fill, _) = level_colors(p.risk_level(&probe), d.kind == RiskKind::Opportunity);
        if s.done {
            painter.circle_filled(at, 4.5, fill);
        }
        painter.circle_stroke(at, 4.5, Stroke::new(1.2, pal.text));
    }
    let today = chrono::Local::now().date_naive();
    if today >= start && today <= end {
        let tx = x(today);
        painter.vline(tx, plot.y_range(), Stroke::new(1.0, pal.today));
        painter.text(pos2(tx + 2.0, plot.top()), Align2::LEFT_TOP, "today", FontId::proportional(9.5), pal.today);
    }
    painter.text(pos2(rect.left() + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, "rating", FontId::proportional(9.5), pal.text_dim);
    painter.text(pos2(plot.left(), rect.bottom() - 1.0), Align2::LEFT_BOTTOM, fmt_d(Some(start)), FontId::proportional(9.5), pal.text_dim);
    painter.text(pos2(plot.right(), rect.bottom() - 1.0), Align2::RIGHT_BOTTOM, fmt_d(Some(end)), FontId::proportional(9.5), pal.text_dim);
}
