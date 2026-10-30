//! Board dialogs: Start Sprint, Complete Sprint, sprint goal, and the
//! board's columns and WIP limits.

use super::theme::*;
use super::ProjectFcgApp;
use crate::model::fmt_date;
use egui::RichText;

#[derive(Clone, Debug, PartialEq)]
pub enum SprintAction {
    Start,
    Complete,
    Goal,
}

#[derive(Clone, Debug)]
pub struct SprintDlg {
    pub uid: u32,
    pub action: SprintAction,
    pub goal: String,
    /// Complete: where unfinished tickets go (`None` = backlog).
    pub dest: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ColumnsDlg {
    /// (name, old column index, WIP limit text)
    pub rows: Vec<(String, Option<usize>, String)>,
    pub error: Option<String>,
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

impl ProjectFcgApp {
    pub fn open_sprint_dialog(&mut self, action: SprintAction) {
        let Some(uid) = self.board_sprint() else { return };
        let goal = self.project.board.sprint_meta(uid).map(|m| m.goal.clone()).unwrap_or_default();
        // the next sprint that isn't complete yet
        let p = &self.project;
        let rows = p.sprint_rows();
        let at = rows.iter().position(|&r| p.tasks[r].uid == uid).unwrap_or(0);
        let dest = rows.iter().skip(at + 1).map(|&r| p.tasks[r].uid).find(|&u| p.board.sprint_meta(u).is_none_or(|m| m.completed.is_none()));
        self.dialogs.sprint = Some(SprintDlg { uid, action, goal, dest });
    }

    pub fn open_columns_dialog(&mut self) {
        let b = &self.project.board;
        let rows = b.columns.iter().enumerate().map(|(i, c)| (c.clone(), Some(i), b.wip_limit(i).map(|n| n.to_string()).unwrap_or_default())).collect();
        self.dialogs.columns = Some(ColumnsDlg { rows, error: None });
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    sprint(app, ctx);
    columns(app, ctx);
    show_recurring(app, ctx);
}

fn sprint(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.sprint.take() else { return };
    let p = &app.project;
    let Some(task) = p.task_by_uid(st.uid).cloned() else { return };
    let stats = p.sprint_stats(Some(st.uid));
    let est = p.board.estimate;
    let open = stats.tickets - stats.done_tickets;
    // Jira's "workload by assignee" when starting a sprint
    let mut workload: Vec<(String, f64, usize)> = vec![];
    for t in p.tickets.iter().filter(|t| t.sprint == Some(st.uid) && t.kind.is_standard()) {
        let who = t.assignee.and_then(|u| p.resources.iter().find(|r| r.uid == u)).map(|r| r.name.clone()).unwrap_or_else(|| "Unassigned".into());
        let u = p.ticket_units(t);
        match workload.iter_mut().find(|w| w.0 == who) {
            Some(w) => {
                w.1 += u;
                w.2 += 1;
            }
            None => workload.push((who, u, 1)),
        }
    }
    workload.sort_by(|a, b| (a.0 == "Unassigned").cmp(&(b.0 == "Unassigned")).then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)));
    let sprints: Vec<(u32, String)> = p
        .sprint_rows()
        .into_iter()
        .map(|r| (p.tasks[r].uid, p.tasks[r].name.clone()))
        .filter(|(u, _)| *u != st.uid && p.board.sprint_meta(*u).is_none_or(|m| m.completed.is_none()))
        .collect();
    let title = match st.action {
        SprintAction::Start => format!("Start {}", task.name),
        SprintAction::Complete => format!("Complete {}", task.name),
        SprintAction::Goal => format!("{} goal", task.name),
    };
    let (mut ok, mut cancel) = (false, false);
    let mut goal_focus = false;
    let keys = super::dialogs::modal(ctx, "sprint", &title, 440.0, |ui| {
        if let (Some(s), Some(f)) = (task.calc.start, task.calc.finish) {
            ui.label(RichText::new(format!("{} – {}", fmt_date(s), fmt_date(f))).color(pal().text_dim));
        }
        match st.action {
            SprintAction::Start | SprintAction::Goal => {
                if st.action == SprintAction::Start {
                    ui.label(format!(
                        "Commits to {} in {} ticket{}. Velocity compares what's done with this commitment, even if scope changes later.",
                        est.unit(stats.total),
                        stats.tickets,
                        if stats.tickets == 1 { "" } else { "s" }
                    ));
                    if !workload.is_empty() {
                        ui.add_space(2.0);
                        egui::Grid::new("sprint_workload").num_columns(3).spacing([16.0, 2.0]).show(ui, |ui| {
                            for (who, u, n) in &workload {
                                ui.label(who);
                                ui.label(est.unit(*u));
                                ui.label(RichText::new(format!("{n} ticket{}", if *n == 1 { "" } else { "s" })).color(pal().text_dim));
                                ui.end_row();
                            }
                        });
                    }
                    if stats.unestimated > 0 {
                        ui.colored_label(pal().warn, format!("{} ticket{} not estimated yet.", stats.unestimated, if stats.unestimated == 1 { " is" } else { "s are" }));
                    }
                }
                ui.add_space(4.0);
                ui.label("Sprint goal:");
                let r = ui.add(egui::TextEdit::multiline(&mut st.goal).desired_rows(2).desired_width(f32::INFINITY).hint_text("What this sprint should achieve"));
                goal_focus = r.has_focus();
            }
            SprintAction::Complete => {
                ui.label(format!("{} of {} done: {} ticket{} complete, {open} unfinished.", crate::duration::fmt_num(stats.done), est.unit(stats.total), stats.done_tickets, if stats.done_tickets == 1 { "" } else { "s" }));
                if open > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Move unfinished tickets to:");
                        let cur = st.dest.and_then(|d| sprints.iter().find(|x| x.0 == d)).map(|x| x.1.clone()).unwrap_or_else(|| "Backlog".into());
                        egui::ComboBox::from_id_salt("sprint_dest").selected_text(cur).show_ui(ui, |ui| {
                            ui.selectable_value(&mut st.dest, None, "Backlog");
                            for (u, n) in &sprints {
                                ui.selectable_value(&mut st.dest, Some(*u), n);
                            }
                        });
                    });
                }
                ui.label(RichText::new("The sprint's velocity is frozen at what's done now. Its bar stays on the schedule; its % complete becomes 100% once only finished tickets remain.").size(11.0).color(pal().text_dim));
            }
        }
        let (o, c) = super::dialogs::ok_cancel(ui);
        ok = o;
        cancel = c;
    });
    if keys.esc || cancel {
        return;
    }
    if !(ok || (keys.enter && !goal_focus)) {
        app.dialogs.sprint = Some(st);
        return;
    }
    let (uid, goal, dest) = (st.uid, st.goal.clone(), st.dest);
    match st.action {
        SprintAction::Start => {
            app.edit_project("Start Sprint", |p| p.start_sprint(uid, &goal, now()));
        }
        SprintAction::Goal => {
            app.edit_project("Sprint Goal", |p| p.set_sprint_goal(uid, &goal));
        }
        SprintAction::Complete => {
            if let Some(n) = app.edit_project("Complete Sprint", |p| p.complete_sprint(uid, dest, now())) {
                if n > 0 {
                    if let Some(d) = dest {
                        app.board.sprint = Some(d);
                    }
                }
            }
        }
    }
}

fn columns(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.columns.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = super::dialogs::modal(ctx, "board_columns", "Board Columns", 460.0, |ui| {
        ui.label(RichText::new("Left to right. The last column means Done. A WIP limit flags a column holding more cards than that in a sprint.").size(11.0).color(pal().text_dim));
        ui.add_space(4.0);
        let n = st.rows.len();
        let mut swap: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;
        egui::Grid::new("cols_grid").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.strong("Column");
            ui.strong("WIP limit");
            ui.label("");
            ui.label("");
            ui.end_row();
            for (i, r) in st.rows.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(200.0));
                ui.add(egui::TextEdit::singleline(&mut r.2).desired_width(50.0).hint_text("none"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("◀").small()).on_hover_text("Move left").clicked() {
                        swap = Some((i - 1, i));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("▶").small()).on_hover_text("Move right").clicked() {
                        swap = Some((i, i + 1));
                    }
                });
                if ui.add_enabled(n > 2, egui::Button::new("✖").small()).on_hover_text("Remove (its cards move to the column before it)").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some((a, b)) = swap {
            st.rows.swap(a, b);
        }
        if let Some(i) = remove {
            st.rows.remove(i);
        }
        if ui.button("✚ Add Column").clicked() {
            let at = st.rows.len().saturating_sub(1);
            st.rows.insert(at, ("New column".into(), None, String::new()));
        }
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        let (o, c) = super::dialogs::ok_cancel(ui);
        ok = o;
        cancel = c;
    });
    if keys.esc || cancel {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.columns = Some(st);
        return;
    }
    let mut cols = vec![];
    for (name, old, wip) in &st.rows {
        let w = wip.trim();
        let limit = if w.is_empty() {
            None
        } else {
            match w.parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => {
                    st.error = Some(format!("\"{w}\" isn't a whole number of cards."));
                    app.dialogs.columns = Some(st);
                    return;
                }
            }
        };
        cols.push((name.clone(), *old, limit));
    }
    if app.edit_project("Board Columns", |p| p.set_board_columns(&cols, now())).is_none() {
        st.error = Some(app.status.clone());
        app.dialogs.columns = Some(st);
    }
}

// ---------------------------------------------------------------- recurring tasks

use crate::recurring::{RecurEnd, RecurPattern, Recurrence};

#[derive(Clone, Debug)]
pub struct RecurringDlg {
    /// Editing an existing series (summary uid), or a new one.
    pub uid: Option<u32>,
    pub name: String,
    pub duration: String,
    pub pattern: RecurPattern,
    pub every: u32,
    pub weekdays: [bool; 7],
    pub working_days_only: bool,
    pub start: String,
    pub by_count: bool,
    pub count: u32,
    pub end: String,
    pub error: Option<String>,
}

impl ProjectFcgApp {
    /// Task ▸ Recurring Task (or Task Information on a recurring summary).
    pub fn open_recurring_dialog(&mut self, row: Option<usize>) {
        let p = &self.project;
        let today = chrono::Local::now().date_naive().max(p.start.date());
        let d = match row.and_then(|r| p.tasks[r].recurrence.clone().map(|rc| (r, rc))) {
            Some((r, rc)) => RecurringDlg {
                uid: Some(p.tasks[r].uid),
                name: p.tasks[r].name.clone(),
                duration: rc.duration.format(&p.settings.durations),
                pattern: rc.pattern,
                every: rc.every.max(1),
                weekdays: rc.weekdays,
                working_days_only: rc.working_days_only,
                start: crate::model::fmt_date(rc.start.and_hms_opt(0, 0, 0).unwrap()),
                by_count: matches!(rc.end, RecurEnd::After(_)),
                count: match rc.end {
                    RecurEnd::After(n) => n,
                    _ => 10,
                },
                end: match rc.end {
                    RecurEnd::By(d) => crate::model::fmt_date(d.and_hms_opt(0, 0, 0).unwrap()),
                    _ => String::new(),
                },
                error: None,
            },
            None => {
                let mut wd = [false; 7];
                use chrono::Datelike;
                wd[today.weekday().num_days_from_monday() as usize] = true;
                RecurringDlg {
                    uid: None,
                    name: String::new(),
                    duration: "1h".into(),
                    pattern: RecurPattern::Weekly,
                    every: 1,
                    weekdays: wd,
                    working_days_only: true,
                    start: crate::model::fmt_date(today.and_hms_opt(0, 0, 0).unwrap()),
                    by_count: true,
                    count: 10,
                    end: String::new(),
                    error: None,
                }
            }
        };
        self.dialogs.recurring = Some(d);
    }
}

pub fn show_recurring(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.recurring.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let fallback = app.project.start.date();
    let keys = super::dialogs::modal(ctx, "recurring", "Recurring Task Information", 500.0, |ui| {
        egui::Grid::new("rec_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Task name:");
            let r = ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(300.0));
            if st.uid.is_none() && st.name.is_empty() && !r.has_focus() {
                r.request_focus();
            }
            ui.end_row();
            ui.label("Duration:");
            ui.add(egui::TextEdit::singleline(&mut st.duration).desired_width(80.0));
            ui.end_row();
            ui.label("Recurrence:");
            ui.horizontal(|ui| {
                for pt in RecurPattern::ALL {
                    ui.radio_value(&mut st.pattern, pt, pt.label());
                }
            });
            ui.end_row();
            ui.label("Every:");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut st.every).range(1..=99));
                ui.label(match st.pattern {
                    RecurPattern::Daily => "day(s)",
                    RecurPattern::Weekly => "week(s) on",
                    RecurPattern::Monthly => "month(s), on the start date's day",
                    RecurPattern::Yearly => "year(s), on the start date",
                });
            });
            ui.end_row();
            if st.pattern == RecurPattern::Weekly {
                ui.label("");
                ui.horizontal(|ui| {
                    for (i, n) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
                        ui.checkbox(&mut st.weekdays[i], *n);
                    }
                });
                ui.end_row();
            }
            if st.pattern == RecurPattern::Daily {
                ui.label("");
                ui.checkbox(&mut st.working_days_only, "Working days only");
                ui.end_row();
            }
            ui.label("Start:");
            super::dialogs::date_field(ui, &mut st.start, "rec_start", fallback);
            ui.end_row();
            ui.label("End:");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut st.by_count, true, "End after");
                    ui.add_enabled(st.by_count, egui::DragValue::new(&mut st.count).range(1..=crate::recurring::MAX_OCCURRENCES as u32));
                    ui.label("occurrences");
                });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut st.by_count, false, "End by");
                    ui.add_enabled_ui(!st.by_count, |ui| super::dialogs::date_field(ui, &mut st.end, "rec_end", fallback));
                });
            });
            ui.end_row();
        });
        ui.label(RichText::new("Occurrences become subtasks held at their dates (Start No Earlier Than). One that falls on a day off moves to the next working day. Editing the series rebuilds them.").size(11.0).color(pal().text_dim));
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        let (o, c) = super::dialogs::ok_cancel(ui);
        ok = o;
        cancel = c;
    });
    if keys.esc || cancel {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.recurring = Some(st);
        return;
    }
    let p = &app.project;
    let parse_day = |s: &str| crate::model::parse_date(s, p.default_start_time()).map(|d| d.date());
    let built = (|| -> Result<Recurrence, String> {
        let duration = p.parse_duration(&st.duration)?;
        let start = parse_day(&st.start).map_err(|_| "The start isn't a valid date.".to_string())?;
        let end = if st.by_count {
            RecurEnd::After(st.count.max(1))
        } else {
            let e = parse_day(&st.end).map_err(|_| "The end isn't a valid date.".to_string())?;
            if e < start {
                return Err("The series ends before it starts.".into());
            }
            RecurEnd::By(e)
        };
        Ok(Recurrence { pattern: st.pattern, every: st.every.max(1), weekdays: st.weekdays, working_days_only: st.working_days_only, start, end, duration, occurrences: vec![] })
    })();
    let rec = match built {
        Ok(r) => r,
        Err(e) => {
            st.error = Some(e);
            app.dialogs.recurring = Some(st);
            return;
        }
    };
    let name = st.name.clone();
    let res = match st.uid.and_then(|u| app.project.index_of_uid(u)) {
        Some(row) => app.edit_project("Edit Recurring Task", |p| p.update_recurring(row, &name, rec).map(|_| row)),
        None => {
            let at = app.active_index().unwrap_or(app.project.tasks.len());
            app.edit_project("Insert Recurring Task", |p| p.insert_recurring(at, &name, rec))
        }
    };
    match res {
        Some(row) => app.select_index(row),
        None => {
            st.error = Some(app.status.clone());
            app.dialogs.recurring = Some(st);
        }
    }
}
