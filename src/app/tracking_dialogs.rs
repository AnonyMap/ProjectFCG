//! Tracking dialogs: Update Tasks, Update Project, Project Statistics.

use super::dialogs::{modal, ok_cancel};
use super::theme::*;
use super::ProjectFcgApp;
use crate::model::{fmt_datetime, parse_date};
use crate::tracking::UpdateMode;
use egui::RichText;

#[derive(Clone, Debug)]
pub struct UpdateTasksDlg {
    pub uids: Vec<u32>,
    pub name: String,
    pub duration: String,
    pub percent: f64,
    pub percent0: f64,
    pub actual_dur: String,
    pub actual_dur0: String,
    pub remaining: String,
    pub remaining0: String,
    pub actual_start: String,
    pub actual_start0: String,
    pub actual_finish: String,
    pub actual_finish0: String,
    pub start: String,
    pub finish: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UpdateProjectDlg {
    pub through: String,
    pub mode: UpdateMode,
    pub reschedule: bool,
    pub after: String,
    pub selected_only: bool,
    pub n_selected: usize,
    pub error: Option<String>,
}

fn na(d: Option<chrono::NaiveDateTime>) -> String {
    d.map(fmt_datetime).unwrap_or_else(|| "NA".into())
}

impl ProjectFcgApp {
    pub fn open_update_tasks(&mut self) {
        let rows: Vec<usize> = self.selected_rows_sorted().into_iter().filter(|&r| !self.project.tasks[r].calc.is_summary).collect();
        let Some(&first) = rows.first() else {
            self.error("Select one or more subtasks to update.");
            return;
        };
        let p = &self.project;
        let t = &p.tasks[first];
        let unit = crate::duration::DurUnit::Day;
        let ad = p.format_duration_minutes(p.actual_duration(first), unit);
        let rd = p.format_duration_minutes(p.remaining_duration(first), unit);
        let name = if rows.len() == 1 { t.name.clone() } else { format!("{} tasks", rows.len()) };
        self.dialogs.update_tasks = Some(UpdateTasksDlg {
            uids: rows.iter().map(|&r| p.tasks[r].uid).collect(),
            name,
            duration: t.duration.format(&p.settings.durations),
            percent: t.percent_complete,
            percent0: t.percent_complete,
            actual_dur: ad.clone(),
            actual_dur0: ad,
            remaining: rd.clone(),
            remaining0: rd,
            actual_start: na(t.actual_start),
            actual_start0: na(t.actual_start),
            actual_finish: na(t.actual_finish),
            actual_finish0: na(t.actual_finish),
            start: na(t.calc.start),
            finish: na(t.calc.finish),
            error: None,
        });
    }

    pub fn open_update_project(&mut self) {
        let at = self.project.status_or_now();
        let n = self.selected_rows().len();
        self.dialogs.update_project = Some(UpdateProjectDlg {
            through: fmt_datetime(at),
            mode: UpdateMode::Proportional,
            reschedule: false,
            after: fmt_datetime(at),
            selected_only: false,
            n_selected: n,
            error: None,
        });
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    update_tasks(app, ctx);
    update_project(app, ctx);
    statistics(app, ctx);
}

fn date_or_na(s: &str, default_time: chrono::NaiveTime) -> Result<Option<chrono::NaiveDateTime>, String> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("na") {
        Ok(None)
    } else {
        parse_date(s, default_time).map(Some)
    }
}

fn update_tasks(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.update_tasks.take() else { return };
    let single = st.uids.len() == 1;
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "update_tasks", "Update Tasks", 520.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.label(RichText::new(&st.name).strong());
            ui.add_space(12.0);
            ui.label(RichText::new(format!("Duration: {}", st.duration)).color(pal().text_dim));
        });
        ui.add_space(4.0);
        egui::Grid::new("ut").num_columns(4).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("% Complete:");
            ui.add(egui::DragValue::new(&mut st.percent).range(0.0..=100.0).suffix("%").speed(1.0));
            ui.label("Actual duration:");
            ui.add_enabled(single, egui::TextEdit::singleline(&mut st.actual_dur).desired_width(90.0));
            ui.end_row();
            ui.label("");
            ui.label("");
            ui.label("Remaining duration:");
            ui.add_enabled(single, egui::TextEdit::singleline(&mut st.remaining).desired_width(90.0));
            ui.end_row();
        });
        ui.separator();
        egui::Grid::new("ut2").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Actual").strong());
            ui.label(RichText::new("Current").strong());
            ui.end_row();
            ui.label("Start:");
            ui.add(egui::TextEdit::singleline(&mut st.actual_start).desired_width(170.0));
            ui.label(RichText::new(&st.start).color(pal().text_dim));
            ui.end_row();
            ui.label("Finish:");
            ui.add(egui::TextEdit::singleline(&mut st.actual_finish).desired_width(170.0));
            ui.label(RichText::new(&st.finish).color(pal().text_dim));
            ui.end_row();
        });
        ui.label(
            RichText::new("An actual start pins the task (links and constraints no longer move it). An actual finish completes it and sets its duration. NA clears.")
                .size(11.0)
                .color(pal().text_dim),
        );
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.update_tasks = Some(st);
        return;
    }
    let s2 = st.clone();
    let res = app.edit_project("Update Tasks", move |p| {
        let start_t = p.default_start_time();
        let finish_t = p.default_finish_time();
        for uid in &s2.uids {
            let row = p.index_of_uid(*uid).ok_or("A task was deleted.")?;
            if s2.actual_start.trim() != s2.actual_start0.trim() {
                p.set_actual_start(row, date_or_na(&s2.actual_start, start_t)?)?;
            }
            if s2.uids.len() == 1 && s2.actual_dur.trim() != s2.actual_dur0.trim() {
                let d = p.parse_duration(&s2.actual_dur)?;
                p.set_actual_duration(row, d.minutes)?;
            } else if s2.uids.len() == 1 && s2.remaining.trim() != s2.remaining0.trim() {
                let d = p.parse_duration(&s2.remaining)?;
                p.set_remaining_duration(row, d.minutes)?;
            } else if (s2.percent - s2.percent0).abs() > 1e-9 {
                p.set_percent_complete(row, s2.percent)?;
            }
            if s2.actual_finish.trim() != s2.actual_finish0.trim() {
                p.set_actual_finish(row, date_or_na(&s2.actual_finish, finish_t)?)?;
            }
        }
        Ok(())
    });
    if res.is_none() {
        st.error = Some(app.status.clone());
        app.dialogs.message = None;
        app.dialogs.update_tasks = Some(st);
    }
}

fn update_project(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.update_project.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "update_project", "Update Project", 470.0, |ui| {
        ui.radio_value(&mut st.reschedule, false, "Update work as complete through:");
        ui.add_enabled_ui(!st.reschedule, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add(egui::TextEdit::singleline(&mut st.through).desired_width(170.0));
            });
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.radio_value(&mut st.mode, UpdateMode::Proportional, "Set 0% – 100% complete");
                ui.radio_value(&mut st.mode, UpdateMode::ZeroOrHundred, "Set 0% or 100% complete only");
            });
        });
        ui.radio_value(&mut st.reschedule, true, "Reschedule uncompleted work to start after:");
        ui.add_enabled_ui(st.reschedule, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add(egui::TextEdit::singleline(&mut st.after).desired_width(170.0));
            });
            ui.label(RichText::new("Tasks that haven't started get a Start No Earlier Than this date.").size(11.0).color(pal().text_dim));
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("For:");
            ui.radio_value(&mut st.selected_only, false, "Entire project");
            ui.add_enabled_ui(st.n_selected > 0, |ui| {
                ui.radio_value(&mut st.selected_only, true, format!("Selected tasks ({})", st.n_selected));
            });
        });
        ui.label(RichText::new("Progress is only ever raised; work already reported isn't reduced.").size(11.0).color(pal().text_dim));
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.update_project = Some(st);
        return;
    }
    let rows: Vec<usize> = app.selected_rows_sorted();
    let s2 = st.clone();
    let res = app.edit_project("Update Project", move |p| {
        let sel = s2.selected_only.then_some(rows.as_slice());
        let changed = if s2.reschedule {
            let after = parse_date(&s2.after, p.default_start_time())?;
            p.update_project(after, UpdateMode::Proportional, Some(&[]), Some(after))
        } else {
            let through = parse_date(&s2.through, p.default_finish_time())?;
            p.update_project(through, s2.mode, sel, None)
        };
        Ok(changed.len())
    });
    match res {
        Some(n) => app.set_status(format!("Update Project: {n} task{} updated", if n == 1 { "" } else { "s" })),
        None => {
            st.error = Some(app.status.clone());
            app.dialogs.message = None;
            app.dialogs.update_project = Some(st);
        }
    }
}

fn statistics(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.statistics {
        return;
    }
    let p = &app.project;
    let money = |v: f64| crate::resources::fmt_money(v, &p.settings.currency_symbol);
    let days = |m: i64| p.format_duration_minutes(m, crate::duration::DurUnit::Day);
    let hours = |m: i64| p.format_duration_minutes(m, crate::duration::DurUnit::Hour);
    let tops: Vec<usize> = (0..p.tasks.len()).filter(|&i| p.parent_of(i).is_none()).collect();
    let (s, f) = app.project_span();
    let base_s = p.tasks.iter().filter_map(|t| t.baseline.map(|b| b.start)).min();
    let base_f = p.tasks.iter().filter_map(|t| t.baseline.map(|b| b.finish)).max();
    let act_s = p.tasks.iter().filter_map(|t| t.actual_start).min();
    let all_done = p.tasks.iter().filter(|t| !t.calc.is_summary).all(|t| t.actual_finish.is_some());
    let act_f = if all_done { p.tasks.iter().filter_map(|t| t.actual_finish).max() } else { None };
    let dur = p.calendar.work_between(s, f);
    let base_dur = match (base_s, base_f) {
        (Some(a), Some(b)) => Some(p.calendar.work_between(a, b)),
        _ => None,
    };
    let work: i64 = tops.iter().map(|&i| p.tasks[i].calc.work).sum();
    let act_work: i64 = tops.iter().map(|&i| p.tasks[i].calc.actual_work).sum();
    let base_work: i64 = tops.iter().filter_map(|&i| p.tasks[i].baseline.map(|b| b.work)).sum();
    let cost = p.calc.total_cost;
    let act_cost: f64 = tops.iter().map(|&i| p.tasks[i].calc.actual_cost).sum();
    let ev = p.project_earned_value();
    let pct_dur = if dur > 0 {
        let done: i64 = (0..p.tasks.len()).filter(|&i| !p.tasks[i].calc.is_summary).map(|i| p.actual_duration(i)).sum();
        let all: i64 = p.tasks.iter().filter(|t| !t.calc.is_summary).map(|t| t.duration.minutes).sum();
        if all > 0 { done as f64 / all as f64 * 100.0 } else { 0.0 }
    } else {
        0.0
    };
    let date = |d: Option<chrono::NaiveDateTime>| d.map(crate::model::fmt_date).unwrap_or_else(|| "NA".into());
    let status = p.status_or_now();
    let mut open = true;
    let mut close = false;
    let keys = modal(ctx, "statistics", "Project Statistics", 600.0, |ui| {
        egui::Grid::new("stats").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
            for h in ["", "Start", "Finish"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            ui.label("Current");
            ui.label(date(Some(s)));
            ui.label(date(Some(f)));
            ui.end_row();
            ui.label("Baseline");
            ui.label(date(base_s));
            ui.label(date(base_f));
            ui.end_row();
            ui.label("Actual");
            ui.label(date(act_s));
            ui.label(date(act_f));
            ui.end_row();
            ui.label("Variance");
            ui.label(base_s.map(|b| days(p.calendar.work_between(b, s))).unwrap_or_else(|| "0 days".into()));
            ui.label(base_f.map(|b| days(p.calendar.work_between(b, f))).unwrap_or_else(|| "0 days".into()));
            ui.end_row();
        });
        ui.add_space(8.0);
        egui::Grid::new("stats2").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
            for h in ["", "Duration", "Work", "Cost"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            ui.label("Current");
            ui.label(days(dur));
            ui.label(hours(work));
            ui.label(money(cost));
            ui.end_row();
            ui.label("Baseline");
            ui.label(base_dur.map(days).unwrap_or_else(|| "0 days".into()));
            ui.label(hours(base_work));
            ui.label(money(ev.bac));
            ui.end_row();
            ui.label("Actual");
            ui.label(days((dur as f64 * pct_dur / 100.0) as i64));
            ui.label(hours(act_work));
            ui.label(money(act_cost));
            ui.end_row();
            ui.label("Remaining");
            ui.label(days(dur - (dur as f64 * pct_dur / 100.0) as i64));
            ui.label(hours((work - act_work).max(0)));
            ui.label(money((cost - act_cost).max(0.0)));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.label(format!("Percent complete: Duration {}%   Work {}%", pct_dur.round(), if work > 0 { (act_work as f64 / work as f64 * 100.0).round() } else { 0.0 }));
        ui.separator();
        ui.label(RichText::new(format!("Earned value at {}", crate::model::fmt_date(status))).strong());
        egui::Grid::new("ev").num_columns(4).spacing([18.0, 6.0]).show(ui, |ui| {
            ui.label("Planned value (BCWS)");
            ui.label(money(ev.bcws));
            ui.label("Schedule variance (SV)");
            ui.label(money(ev.sv()));
            ui.end_row();
            ui.label("Earned value (BCWP)");
            ui.label(money(ev.bcwp));
            ui.label("Cost variance (CV)");
            ui.label(money(ev.cv()));
            ui.end_row();
            ui.label("Actual cost (ACWP)");
            ui.label(money(ev.acwp));
            ui.label("SPI / CPI");
            ui.label(format!("{} / {}", ev.spi().map(|v| format!("{v:.2}")).unwrap_or_else(|| "–".into()), ev.cpi().map(|v| format!("{v:.2}")).unwrap_or_else(|| "–".into())));
            ui.end_row();
            ui.label("Budget at completion (BAC)");
            ui.label(money(ev.bac));
            ui.label("Estimate at completion (EAC)");
            ui.label(money(ev.eac()));
            ui.end_row();
        });
        let verdict = if ev.bac <= 0.0 {
            "Set a baseline (Project ▸ Set Baseline) to measure earned value.".to_string()
        } else {
            let sched = match ev.spi() {
                Some(v) if v < 0.95 => format!("behind schedule (SPI {v:.2}: {:.0}% of the planned work is done)", v * 100.0),
                Some(v) if v > 1.05 => format!("ahead of schedule (SPI {v:.2})"),
                Some(_) => "on schedule".into(),
                None => "not started per the baseline yet".into(),
            };
            let cost = match ev.cpi() {
                Some(v) if v < 0.95 => format!("over budget (every {} of work is costing {})", money(1.0), money(1.0 / v)),
                Some(v) if v > 1.05 => format!("under budget (CPI {v:.2})"),
                Some(_) => "on budget".into(),
                None => "no actual cost yet".into(),
            };
            format!("The project is {sched}, and {cost}.")
        };
        ui.label(RichText::new(verdict).color(pal().text_dim));
        ui.add_space(6.0);
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("  Close  ").clicked() {
                close = true;
            }
        });
    });
    if close || keys.esc || keys.enter {
        open = false;
    }
    app.dialogs.statistics = open;
}
