//! Task ▸ Move, as in Project: move the selected tasks forward or back by
//! a number of working days (on each task's own calendar).
//!
//! * A manually scheduled task simply starts later or earlier.
//! * An auto-scheduled task gets a Start No Earlier Than constraint on its
//!   new start (moving it back may not move it at all if its
//!   predecessors hold it, as in Project).
//! * A task that already has a date constraint (Must Start On, Finish No
//!   Later Than, …) keeps it, with the date moved by the same amount.
//! * Summaries move their subtasks. Tasks that have started stay put.

use super::ProjectFcgApp;
use crate::model::{ConstraintType, TaskMode};

#[derive(Clone, Debug)]
pub struct MoveDlg {
    pub days: f64,
    pub back: bool,
}

impl Default for MoveDlg {
    fn default() -> Self {
        MoveDlg { days: 1.0, back: false }
    }
}

impl ProjectFcgApp {
    /// Working days in a week, from Options (5 for 40 h at 8 h a day).
    pub fn days_per_week(&self) -> i32 {
        let d = &self.project.settings.durations;
        if d.minutes_per_day > 0 {
            ((d.minutes_per_week as f64 / d.minutes_per_day as f64).round() as i32).max(1)
        } else {
            5
        }
    }

    /// Move the selected tasks by `days` working days (negative: back).
    pub fn move_tasks(&mut self, days: i32) {
        self.move_tasks_by(days as f64);
    }

    pub fn move_tasks_by(&mut self, days: f64) {
        if days == 0.0 || !days.is_finite() {
            return;
        }
        let mut rows: Vec<usize> = vec![];
        for r in self.selected_rows_sorted() {
            if self.project.is_summary(r) {
                rows.extend(((r + 1)..self.project.subtree_end(r)).filter(|&k| !self.project.is_summary(k)));
            } else {
                rows.push(r);
            }
        }
        rows.sort();
        rows.dedup();
        if rows.is_empty() {
            self.set_status("Select the tasks to move");
            return;
        }
        let mins = (days * self.project.settings.durations.minutes_per_day as f64).round() as i64;
        let label = if days > 0.0 { "Move Task Forward" } else { "Move Task Back" };
        let res = self.edit_project(label, move |p| {
            let (mut moved, mut started) = (0, 0);
            for &r in &rows {
                let t = &p.tasks[r];
                if t.actual_start.is_some() {
                    started += 1;
                    continue;
                }
                let Some(s) = t.calc.start else { continue };
                let cal = p.duration_calendar(r).into_owned();
                // a start moves to the start of a working day…
                let shift = |d: chrono::NaiveDateTime| {
                    if mins >= 0 {
                        cal.next_working_start(cal.add_work(d, mins))
                    } else {
                        cal.sub_work(d, -mins)
                    }
                };
                // …a finish to the end of one
                let shift_finish = |d: chrono::NaiveDateTime| {
                    if mins >= 0 {
                        cal.add_work(d, mins)
                    } else {
                        cal.prev_working_end(cal.sub_work(d, -mins))
                    }
                };
                let ns = shift(s);
                let from_finish = p.finish_date.is_some();
                let t = &mut p.tasks[r];
                match t.mode {
                    TaskMode::Manual => t.manual_start = Some(ns),
                    TaskMode::Auto => match (t.constraint, t.constraint_date) {
                        // scheduled from the finish, tasks sit as late as
                        // they can: moving one earlier needs No Later Than
                        (ConstraintType::ASAP | ConstraintType::ALAP | ConstraintType::SNET | ConstraintType::SNLT, _) | (_, None) => {
                            t.constraint = if from_finish && mins < 0 { ConstraintType::SNLT } else { ConstraintType::SNET };
                            t.constraint_date = Some(ns);
                        }
                        (ConstraintType::FNET | ConstraintType::FNLT | ConstraintType::MFO, Some(cd)) => t.constraint_date = Some(shift_finish(cd)),
                        (_, Some(cd)) => t.constraint_date = Some(shift(cd)),
                    },
                }
                moved += 1;
            }
            if moved == 0 {
                return Err(if started > 0 { "Tasks that have started can't be moved; update their progress instead.".to_string() } else { "Nothing to move.".to_string() });
            }
            Ok((moved, started))
        });
        if let Some((moved, started)) = res {
            let mut msg = format!("{label}: {moved} task(s) by {} working day(s)", crate::duration::fmt_num(days.abs()));
            if started > 0 {
                msg.push_str(&format!("; {started} already started, left in place"));
            }
            self.set_status(msg);
        }
    }

    /// Task ▸ Move ▸ Incomplete Parts to Status Date: the selected tasks'
    /// unfinished work starts after the status date (Update Project's
    /// "Reschedule uncompleted work").
    pub fn move_incomplete_to_status_date(&mut self) {
        let after = self.project.status_date.unwrap_or_else(|| chrono::Local::now().naive_local());
        let rows = self.selected_rows_sorted();
        if let Some(n) = self.edit_project("Move Incomplete Parts", move |p| Ok(p.reschedule_uncompleted(after, Some(&rows)).len())) {
            self.set_status(format!("Moved the unfinished work of {n} task(s) after the status date"));
        }
    }
}

pub fn dialog(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.move_tasks.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = super::dialogs::modal(ctx, "move_tasks", "Move Task", 340.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Move by");
            ui.add(egui::DragValue::new(&mut st.days).range(0.0..=3650.0).speed(0.5).suffix(" working days"));
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.back, false, "Forward (later)");
            ui.radio_value(&mut st.back, true, "Back (earlier)");
        });
        ui.label(egui::RichText::new("Auto-scheduled tasks get a Start No Earlier Than constraint on their new start.").size(11.0).color(super::theme::pal().text_dim));
        ui.separator();
        (ok, cancel) = super::dialogs::ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if ok || keys.enter {
        let d = if st.back { -st.days } else { st.days };
        app.move_tasks_by(d);
        return;
    }
    app.dialogs.move_tasks = Some(st);
}
