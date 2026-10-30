//! Assign Resources window, Resource Information and Leveling Options.

use super::dialogs::{modal, ok_cancel, parse_shifts, shifts_to_text, vsep};
use super::theme::*;
use super::ProjectFcgApp;
use crate::calendar::{Calendar, CalendarException};
use crate::model::{fmt_date, parse_date, Accrue, LevelingOrder, ResourceType};
use crate::resources::{fmt_money, fmt_units, parse_money, parse_rate, parse_units};
use egui::RichText;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct AssignDlg {
    pub selected: Option<u32>,
    pub units: HashMap<u32, String>,
    pub new_name: String,
}

#[derive(Clone, Debug)]
pub struct ResInfoDlg {
    pub uid: u32,
    pub tab: u8,
    pub name: String,
    pub initials: String,
    pub group: String,
    pub rtype: ResourceType,
    pub material: String,
    pub max_units: String,
    pub std_rate: String,
    pub ovt_rate: String,
    pub cost_per_use: String,
    pub accrue: Accrue,
    pub notes: String,
    pub custom_week: bool,
    pub week_working: [bool; 7],
    pub week_text: [String; 7],
    pub exceptions: Vec<CalendarException>,
    pub ex_name: String,
    pub ex_start: String,
    pub ex_finish: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct LevelingDlg {
    pub order: LevelingOrder,
    pub within_slack: bool,
}

impl super::dialogs::Dialogs {
    pub fn open_resource_info(&mut self, p: &crate::model::Project, row: usize) {
        let r = &p.resources[row];
        let sym = &p.settings.currency_symbol;
        let week = r.week.clone().unwrap_or_else(|| p.calendar.week.clone());
        self.resource_info = Some(ResInfoDlg {
            uid: r.uid,
            tab: 0,
            name: r.name.clone(),
            initials: r.initials.clone(),
            group: r.group.clone(),
            rtype: r.rtype,
            material: r.material_label.clone(),
            max_units: fmt_units(r.max_units),
            std_rate: crate::resources::fmt_rate(r.std_rate, sym, r.rtype == ResourceType::Material),
            ovt_rate: crate::resources::fmt_rate(r.ovt_rate, sym, false),
            cost_per_use: fmt_money(r.cost_per_use, sym),
            accrue: r.accrue,
            notes: r.notes.clone(),
            custom_week: r.week.is_some(),
            week_working: std::array::from_fn(|i| !week[i].is_empty()),
            week_text: std::array::from_fn(|i| shifts_to_text(&week[i])),
            exceptions: r.exceptions.clone(),
            ex_name: String::new(),
            ex_start: String::new(),
            ex_finish: String::new(),
            error: None,
        });
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    assign_window(app, ctx);
    resource_info(app, ctx);
    leveling(app, ctx);
}

/// Project's Assign Resources dialog: non-modal so the user can keep
/// selecting tasks while it is open.
fn assign_window(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.assign.take() else { return };
    let rows = app.selected_rows();
    let mut open = true;
    let mut act: Option<(&'static str, u32)> = None;
    let mut add_new = false;
    let sym = app.project.settings.currency_symbol.clone();
    egui::Window::new("Assign Resources")
        .open(&mut open)
        .default_width(430.0)
        .default_pos(ctx.content_rect().right_top() + egui::vec2(-460.0, 140.0))
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            let label = match rows.as_slice() {
                [] => "No task selected".to_string(),
                [r] => format!("Task: {}", app.project.tasks[*r].name),
                rs => format!("{} tasks selected", rs.len()),
            };
            ui.label(RichText::new(label).strong());
            ui.add_space(4.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().max_height(260.0).min_scrolled_height(160.0).show(ui, |ui| {
                        egui::Grid::new("assign_grid").striped(true).num_columns(4).spacing([8.0, 3.0]).show(ui, |ui| {
                            ui.strong("");
                            ui.strong("Resource Name");
                            ui.strong("Units");
                            ui.strong("Cost");
                            ui.end_row();
                            for (ri, r) in app.project.resources.iter().enumerate() {
                                let assigned: Vec<usize> = rows
                                    .iter()
                                    .filter_map(|&t| {
                                        let tu = app.project.tasks[t].uid;
                                        app.project.assignments.iter().position(|a| a.task == tu && a.resource == r.uid)
                                    })
                                    .collect();
                                let all = !rows.is_empty() && assigned.len() == rows.len();
                                ui.label(if all { "✔" } else if !assigned.is_empty() { "–" } else { "" });
                                let sel = st.selected == Some(r.uid);
                                let over = app.project.calc.resources.get(ri).is_some_and(|c| c.overallocated);
                                let mut name = RichText::new(&r.name);
                                if over {
                                    name = name.color(pal().warn);
                                }
                                let resp = ui.selectable_label(sel, name);
                                if resp.clicked() {
                                    st.selected = Some(r.uid);
                                }
                                if resp.double_clicked() {
                                    act = Some((if all { "remove" } else { "assign" }, r.uid));
                                }
                                let default = match (r.rtype, assigned.first()) {
                                    (ResourceType::Cost, Some(&ai)) => fmt_money(app.project.assignments[ai].cost, &sym),
                                    (ResourceType::Material, Some(&ai)) => crate::duration::fmt_num(app.project.assignments[ai].units),
                                    (ResourceType::Work, Some(&ai)) => fmt_units(app.project.assignments[ai].units),
                                    (ResourceType::Cost, None) => fmt_money(0.0, &sym),
                                    (ResourceType::Material, None) => "1".into(),
                                    (ResourceType::Work, None) => "100%".into(),
                                };
                                let txt = st.units.entry(r.uid).or_insert(default);
                                let te = ui.add_sized([72.0, 18.0], egui::TextEdit::singleline(txt));
                                if te.lost_focus() && !assigned.is_empty() {
                                    act = Some(("units", r.uid));
                                }
                                let cost: f64 = assigned
                                    .iter()
                                    .map(|&ai| app.project.calc.assignments.get(ai).map_or(0.0, |c| c.cost))
                                    .sum();
                                ui.label(if assigned.is_empty() { String::new() } else { fmt_money(cost, &sym) });
                                ui.end_row();
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut st.new_name).hint_text("New resource name").desired_width(170.0));
                        if ui.button("Add Resource").clicked() && !st.new_name.trim().is_empty() {
                            add_new = true;
                        }
                    });
                });
                vsep(ui, 200.0);
                ui.vertical(|ui| {
                    let enabled = st.selected.is_some() && !rows.is_empty();
                    if ui.add_enabled(enabled, egui::Button::new("  Assign  ")).clicked() {
                        act = st.selected.map(|u| ("assign", u));
                    }
                    if ui.add_enabled(enabled, egui::Button::new("  Remove  ")).clicked() {
                        act = st.selected.map(|u| ("remove", u));
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Units: 50% for half time.\nMaterial: quantity.\nCost: amount.").size(11.0).color(pal().text_dim));
                });
            });
        });
    if add_new {
        let name = st.new_name.trim().to_string();
        if app.project.resource_by_name(&name).is_some() {
            app.error(format!("There is already a resource named \"{name}\"."));
        } else if let Some(i) = app.edit_project("Add Resource", |p| Ok(p.add_resource(&name, ResourceType::Work))) {
            st.selected = Some(app.project.resources[i].uid);
            st.new_name.clear();
        }
    }
    if let Some((what, uid)) = act {
        let ri = app.project.resource_index(uid);
        let rtype = ri.map(|i| app.project.resources[i].rtype).unwrap_or(ResourceType::Work);
        let text = st.units.get(&uid).cloned().unwrap_or_default();
        let val = match rtype {
            ResourceType::Work => parse_units(&text),
            ResourceType::Material => text.trim().parse::<f64>().map_err(|_| format!("\"{text}\" is not a quantity")),
            ResourceType::Cost => parse_money(&text, &sym),
        };
        match (what, val) {
            ("remove", _) => {
                app.edit_project("Remove Assignment", |p| {
                    for &t in &rows {
                        p.unassign(t, uid);
                    }
                    Ok(())
                });
            }
            (_, Err(e)) => app.error(e),
            (_, Ok(v)) => {
                let label = if what == "units" { "Change Units" } else { "Assign Resource" };
                app.edit_project(label, |p| {
                    for &t in &rows {
                        let ai = p.assign(t, uid, v)?;
                        if rtype == ResourceType::Cost {
                            p.assignments[ai].cost = v;
                        }
                    }
                    Ok(())
                });
                st.units.remove(&uid);
            }
        }
    }
    if open {
        app.dialogs.assign = Some(st);
    }
}

fn resource_info(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.resource_info.take() else { return };
    let Some(row) = app.project.resource_index(st.uid) else { return };
    let (mut ok, mut cancel) = (false, false);
    let fallback = app.project.start.date();
    let base_week = app.project.calendar.week.clone();
    let keys = modal(ctx, "resource_info", "Resource Information", 620.0, |ui| {
        ui.horizontal(|ui| {
            for (k, l) in ["General", "Costs", "Working Time", "Notes"].iter().enumerate() {
                ui.selectable_value(&mut st.tab, k as u8, *l);
            }
        });
        ui.separator();
        ui.set_min_height(260.0);
        match st.tab {
            0 => {
                egui::Grid::new("ri_gen").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Resource name:");
                    ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(300.0));
                    ui.end_row();
                    ui.label("Initials:");
                    ui.add(egui::TextEdit::singleline(&mut st.initials).desired_width(80.0));
                    ui.end_row();
                    ui.label("Group:");
                    ui.add(egui::TextEdit::singleline(&mut st.group).desired_width(200.0));
                    ui.end_row();
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt("ri_type").selected_text(st.rtype.label()).show_ui(ui, |ui| {
                        for t in ResourceType::ALL {
                            ui.selectable_value(&mut st.rtype, t, t.label());
                        }
                    });
                    ui.end_row();
                    if st.rtype == ResourceType::Material {
                        ui.label("Material label:");
                        ui.add(egui::TextEdit::singleline(&mut st.material).desired_width(120.0));
                        ui.end_row();
                    }
                    if st.rtype == ResourceType::Work {
                        ui.label("Max. units:");
                        ui.add(egui::TextEdit::singleline(&mut st.max_units).desired_width(80.0));
                        ui.end_row();
                    }
                });
                if let Some(c) = app.project.calc.resources.get(row) {
                    ui.add_space(8.0);
                    if c.overallocated {
                        ui.colored_label(
                            pal().warn,
                            format!(
                                "Overallocated: peaks at {} in {} period(s). Resource ▸ Level All can resolve this.",
                                fmt_units(c.peak_units),
                                c.over_intervals.len()
                            ),
                        );
                    }
                }
            }
            1 => {
                egui::Grid::new("ri_cost").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    if st.rtype != ResourceType::Cost {
                        ui.label(if st.rtype == ResourceType::Material { "Standard rate (per unit):" } else { "Standard rate:" });
                        ui.add(egui::TextEdit::singleline(&mut st.std_rate).desired_width(120.0));
                        ui.end_row();
                    }
                    if st.rtype == ResourceType::Work {
                        ui.label("Overtime rate:");
                        ui.add(egui::TextEdit::singleline(&mut st.ovt_rate).desired_width(120.0));
                        ui.end_row();
                    }
                    if st.rtype != ResourceType::Cost {
                        ui.label("Cost per use:");
                        ui.add(egui::TextEdit::singleline(&mut st.cost_per_use).desired_width(120.0));
                        ui.end_row();
                    }
                    ui.label("Cost accrual:");
                    egui::ComboBox::from_id_salt("ri_acc").selected_text(st.accrue.label()).show_ui(ui, |ui| {
                        for a in Accrue::ALL {
                            ui.selectable_value(&mut st.accrue, a, a.label());
                        }
                    });
                    ui.end_row();
                });
                ui.label(RichText::new("Rates accept /hr, /d, /w, e.g. \"$400/d\".").size(11.0).color(pal().text_dim));
            }
            2 => {
                if st.rtype != ResourceType::Work {
                    ui.label("Only work resources have working time.");
                } else {
                    ui.label(format!("Based on the project calendar ({}).", app.project.calendar.name));
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.checkbox(&mut st.custom_week, "Use a custom work week");
                            ui.add_enabled_ui(st.custom_week, |ui| {
                                egui::Grid::new("ri_week").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                                    for (k, day) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
                                        ui.label(*day);
                                        ui.checkbox(&mut st.week_working[k], "");
                                        ui.add_enabled(st.week_working[k], egui::TextEdit::singleline(&mut st.week_text[k]).desired_width(200.0));
                                        ui.end_row();
                                    }
                                });
                            });
                            if !st.custom_week {
                                st.week_working = std::array::from_fn(|i| !base_week[i].is_empty());
                                st.week_text = std::array::from_fn(|i| shifts_to_text(&base_week[i]));
                            }
                        });
                        vsep(ui, 230.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Exceptions (vacation, leave)").strong());
                            let mut rm = None;
                            egui::Grid::new("ri_ex").striped(true).num_columns(3).show(ui, |ui| {
                                for (k, e) in st.exceptions.iter().enumerate() {
                                    ui.label(&e.name);
                                    ui.label(format!(
                                        "{} – {}",
                                        fmt_date(e.start.and_hms_opt(0, 0, 0).unwrap()),
                                        fmt_date(e.end.and_hms_opt(0, 0, 0).unwrap())
                                    ));
                                    if ui.small_button("✖").clicked() {
                                        rm = Some(k);
                                    }
                                    ui.end_row();
                                }
                            });
                            if let Some(k) = rm {
                                st.exceptions.remove(k);
                            }
                            ui.add(egui::TextEdit::singleline(&mut st.ex_name).hint_text("Name (e.g. Vacation)").desired_width(170.0));
                            ui.add(egui::TextEdit::singleline(&mut st.ex_start).hint_text("Start date").desired_width(170.0));
                            ui.add(egui::TextEdit::singleline(&mut st.ex_finish).hint_text("Finish date").desired_width(170.0));
                            if ui.button("Add Exception").clicked() {
                                let t0 = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
                                let s = parse_date(&st.ex_start, t0);
                                let f = if st.ex_finish.trim().is_empty() { s.clone() } else { parse_date(&st.ex_finish, t0) };
                                match (s, f) {
                                    (Ok(s), Ok(f)) if f >= s => {
                                        st.exceptions.push(CalendarException {
                                            name: if st.ex_name.trim().is_empty() { "Nonworking".into() } else { st.ex_name.trim().into() },
                                            start: s.date(),
                                            end: f.date(),
                                            shifts: vec![],
                                        });
                                        st.ex_name.clear();
                                        st.ex_start.clear();
                                        st.ex_finish.clear();
                                        st.error = None;
                                    }
                                    _ => st.error = Some("Enter a valid start and finish date.".into()),
                                }
                            }
                            let _ = fallback;
                        });
                    });
                }
            }
            _ => {
                ui.add(egui::TextEdit::multiline(&mut st.notes).desired_rows(10).desired_width(f32::INFINITY));
            }
        }
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || (keys.enter && st.tab != 3)) {
        app.dialogs.resource_info = Some(st);
        return;
    }
    let s2 = st.clone();
    let res = app.edit_project("Resource Information", move |p| {
        let st = s2;
        let sym = p.settings.currency_symbol.clone();
        let (mpd, mpw) = (p.settings.durations.minutes_per_day, p.settings.durations.minutes_per_week);
        let name = st.name.trim().to_string();
        if name.is_empty() {
            return Err("A resource needs a name.".into());
        }
        if p.resource_by_name(&name).is_some_and(|j| j != row) {
            return Err(format!("There is already a resource named \"{name}\"."));
        }
        if st.rtype != p.resources[row].rtype && !p.assignments_of_resource(st.uid).is_empty() {
            return Err("Remove this resource's assignments before changing its type.".into());
        }
        let mut week = None;
        if st.custom_week {
            let mut w: [Vec<crate::calendar::Shift>; 7] = Default::default();
            for k in 0..7 {
                if st.week_working[k] {
                    w[k] = parse_shifts(&st.week_text[k])?;
                    if w[k].is_empty() {
                        w[k] = Calendar::standard().week[0].clone();
                    }
                }
            }
            if w.iter().all(|d| d.is_empty()) {
                return Err("A work week needs some working time.".into());
            }
            week = Some(w);
        }
        let r = &mut p.resources[row];
        r.name = name;
        r.initials = st.initials.trim().into();
        r.group = st.group.trim().into();
        r.rtype = st.rtype;
        r.material_label = st.material.trim().into();
        if st.rtype == ResourceType::Work {
            r.max_units = parse_units(&st.max_units)?;
            r.ovt_rate = parse_rate(&st.ovt_rate, &sym, mpd, mpw)?;
        }
        if st.rtype != ResourceType::Cost {
            r.std_rate = if st.rtype == ResourceType::Material {
                parse_money(&st.std_rate, &sym)?
            } else {
                parse_rate(&st.std_rate, &sym, mpd, mpw)?
            };
            r.cost_per_use = parse_money(&st.cost_per_use, &sym)?;
        }
        r.accrue = st.accrue;
        r.notes = st.notes.clone();
        r.week = week;
        r.exceptions = st.exceptions.clone();
        Ok(())
    });
    if res.is_none() {
        app.dialogs.resource_info = Some(st);
    }
}

fn leveling(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.leveling.take() else { return };
    let (mut ok, mut cancel, mut level_now) = (false, false, false);
    let keys = modal(ctx, "leveling", "Resource Leveling", 440.0, |ui| {
        egui::Grid::new("lev").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Leveling order:");
            egui::ComboBox::from_id_salt("lev_order").selected_text(st.order.label()).show_ui(ui, |ui| {
                for o in LevelingOrder::ALL {
                    ui.selectable_value(&mut st.order, o, o.label());
                }
            });
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.within_slack, "Level only within available slack");
            ui.end_row();
        });
        ui.label(
            RichText::new(
                "Standard: earlier tasks and tasks with less slack first.\nPriority, Standard: higher Priority first. Priority 1000 tasks are never moved.\nLeveling clears previous leveling delays first.",
            )
            .size(11.0)
            .color(pal().text_dim),
        );
        ui.add_space(4.0);
        if ui.button("Level All").clicked() {
            level_now = true;
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || level_now || keys.enter) {
        app.dialogs.leveling = Some(st);
        return;
    }
    app.edit_project("Leveling Options", |p| {
        p.settings.leveling_order = st.order;
        p.settings.level_within_slack = st.within_slack;
        Ok(())
    });
    if level_now {
        let ctx2 = ctx.clone();
        app.execute(super::commands::Command::LevelAll, &ctx2);
    }
}
