//! Task Inspector pane (Task ▸ Inspect): what drives the selected task's
//! dates, its slack and criticality, and fixes for scheduling problems.

use super::columns::{cell_text, ColumnId};
use super::commands::Command;
use super::theme::*;
use super::ProjectFcgApp;
use crate::model::{fmt_datetime, ConstraintType, LinkType, StartDriver, TaskMode};
use egui::RichText;

enum Fix {
    RespectLinks(usize),
    AutoSchedule(usize),
    ClearDeadline(usize),
    Select(usize),
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let mut fix: Option<Fix> = None;
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Task Inspector").size(15.0).strong().color(pal().heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✖").on_hover_text("Close").clicked() {
                close = true;
            }
        });
    });
    ui.separator();
    let Some(i) = app.active_index() else {
        ui.label(RichText::new("Select a task to see what drives its dates.").color(pal().text_dim));
        if close {
            app.view.inspector = false;
        }
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let p = &app.project;
        let t = &p.tasks[i];
        let c = &t.calc;
        let name = |k: usize| format!("({}) {}", k + 1, p.tasks[k].name);
        ui.label(RichText::new(name(i)).strong().size(14.0));
        if let Some(par) = p.parent_of(i) {
            ui.label(RichText::new(format!("in {}", name(par))).size(11.5).color(pal().text_dim));
        }
        ui.add_space(4.0);
        egui::Grid::new("insp_dates").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
            ui.label("Start:");
            ui.label(c.start.map(fmt_datetime).unwrap_or_default());
            ui.end_row();
            ui.label("Finish:");
            ui.label(c.finish.map(fmt_datetime).unwrap_or_default());
            ui.end_row();
            ui.label("Duration:");
            ui.label(cell_text(p, i, ColumnId::Duration));
            ui.end_row();
            ui.label("Mode:");
            ui.label(if t.mode == TaskMode::Manual { "Manually Scheduled" } else { "Auto Scheduled" });
            ui.end_row();
        });

        // ---- factors ----
        ui.add_space(6.0);
        ui.label(RichText::new("Factors Affecting Task").strong());
        if c.is_summary {
            ui.label("• Summary task: dates roll up from its subtasks.");
        } else {
            match c.start_driver {
                StartDriver::Predecessors => {
                    for (uid, kind) in &c.driving_links {
                        if let Some(k) = p.index_of_uid(*uid) {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("• Predecessor:");
                                if ui.link(name(k)).on_hover_text("Select this task").clicked() {
                                    fix = Some(Fix::Select(k));
                                }
                                let lag = t
                                    .predecessors
                                    .iter()
                                    .chain(p.ancestors_of(i).iter().flat_map(|&a| p.tasks[a].predecessors.iter()))
                                    .find(|l| l.pred == *uid)
                                    .map(|l| l.lag.format(&p.settings.durations))
                                    .unwrap_or_default();
                                ui.label(RichText::new(format!("{}{}", kind_name(*kind), if lag.is_empty() { String::new() } else { format!(" {lag}") })).color(pal().text_dim));
                            });
                        }
                    }
                    for tk in &c.board_driving {
                        let sprint = p.ticket(*tk).and_then(|x| x.sprint).and_then(|u| p.task_by_uid(u)).map(|x| x.name.clone()).unwrap_or_default();
                        ui.label(format!("• Waits on {} {} (until {sprint} ends)", p.ticket_key(*tk), p.ticket(*tk).map(|x| x.title.clone()).unwrap_or_default()));
                    }
                    if c.board_driving.is_empty() && !t.predecessors.iter().any(|l| c.driving_links.iter().any(|(u, _)| *u == l.pred)) {
                        ui.label(RichText::new("  (link on a summary task)").size(11.0).color(pal().text_dim));
                    }
                }
                StartDriver::Constraint => {
                    ui.label(format!(
                        "• Constraint: {}{}",
                        t.constraint.label(),
                        t.constraint_date.map(|d| format!(" {}", fmt_datetime(d))).unwrap_or_default()
                    ));
                }
                StartDriver::ParentConstraint => {
                    for a in p.ancestors_of(i) {
                        let at = &p.tasks[a];
                        if matches!(at.constraint, ConstraintType::SNET | ConstraintType::MSO) {
                            ui.label(format!(
                                "• Summary task {} constraint: {}{}",
                                name(a),
                                at.constraint.label(),
                                at.constraint_date.map(|d| format!(" {}", fmt_datetime(d))).unwrap_or_default()
                            ));
                        }
                    }
                }
                StartDriver::Leveling => {
                    ui.label(format!(
                        "• Resource leveling delay: {}",
                        p.format_duration_minutes(t.leveling_delay, crate::duration::DurUnit::Day)
                    ));
                }
                StartDriver::Actual => {
                    ui.label(format!("• Started: actual start {} (a fact, so links and constraints no longer move it)", t.actual_start.map(fmt_datetime).unwrap_or_default()));
                }
                StartDriver::BoardItem => {
                    for d in &t.board_deps {
                        if let Some(x) = p.ticket(d.ticket).filter(|x| p.is_done(x)) {
                            ui.label(format!("• Waits on {} {}, done {}", p.ticket_key(x.id), x.title, x.done_at.map(fmt_datetime).unwrap_or_default()));
                        }
                    }
                }
                StartDriver::ProjectStart => {
                    ui.label(format!("• Project start date: {}", fmt_datetime(p.start)));
                }
                StartDriver::ManualDates => {
                    ui.label("• Manually scheduled: the dates you entered.");
                }
                StartDriver::AsLateAsPossible => {
                    ui.label("• As Late As Possible: scheduled at its late dates.");
                }
                StartDriver::Circular => {
                    ui.colored_label(pal().warn, "• Part of a circular dependency; links ignored.");
                }
            }
            for n in &c.board_notes {
                ui.colored_label(pal().warn, format!("• {n}"));
            }
            if t.constraint != ConstraintType::ASAP && c.start_driver != StartDriver::Constraint {
                ui.label(RichText::new(format!("• Constraint (not driving): {}", t.constraint.label())).color(pal().text_dim));
            }
        }
        ui.label(format!("• Calendar: {}", p.calendar.name));

        // ---- slack ----
        ui.add_space(6.0);
        ui.label(RichText::new("Schedule Analysis").strong());
        egui::Grid::new("insp_slack").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
            ui.label("Critical:");
            if c.critical {
                ui.colored_label(pal().warn, "Yes");
            } else {
                ui.label("No");
            }
            ui.end_row();
            ui.label("Total slack:");
            ui.label(cell_text(p, i, ColumnId::TotalSlack));
            ui.end_row();
            ui.label("Free slack:");
            ui.label(cell_text(p, i, ColumnId::FreeSlack));
            ui.end_row();
            ui.label("Late start:");
            ui.label(c.late_start.map(fmt_datetime).unwrap_or_default());
            ui.end_row();
            ui.label("Late finish:");
            ui.label(c.late_finish.map(fmt_datetime).unwrap_or_default());
            ui.end_row();
            ui.label("Status:");
            ui.label(c.status.label());
            ui.end_row();
            if t.baseline.is_some() {
                ui.label("Finish variance:");
                ui.label(cell_text(p, i, ColumnId::FinishVariance));
                ui.end_row();
            }
        });

        // ---- driving chain ----
        if !c.is_summary && c.start_driver == StartDriver::Predecessors {
            let tp = crate::analysis::task_path(p, &[i]);
            if !tp.driving_predecessors.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Driving Path").strong());
                for &k in tp.driving_predecessors.iter() {
                    if ui.link(name(k)).clicked() {
                        fix = Some(Fix::Select(k));
                    }
                }
            }
        }

        // ---- warnings & fixes ----
        let mut warned = false;
        if let Some(cf) = &c.conflict {
            warn(ui, &mut warned, cf);
            if t.constraint.needs_date()
                && ui
                    .button("Respect Links")
                    .on_hover_text("Change the constraint to As Soon As Possible so links decide the dates")
                    .clicked()
            {
                fix = Some(Fix::RespectLinks(i));
            }
        }
        if t.mode == TaskMode::Manual && !c.is_summary {
            if let Some(s) = c.start {
                let early: Vec<usize> = t
                    .predecessors
                    .iter()
                    .filter_map(|l| {
                        let k = p.index_of_uid(l.pred)?;
                        let pt = &p.tasks[k].calc;
                        let bad = match l.kind {
                            LinkType::FS => pt.finish.is_some_and(|f| f > s),
                            LinkType::SS => pt.start.is_some_and(|ps| ps > s),
                            _ => false,
                        };
                        bad.then_some(k)
                    })
                    .collect();
                for k in early {
                    warn(ui, &mut warned, &format!("Starts before its predecessor {} allows.", name(k)));
                }
                if warned
                    && ui
                        .button("Auto Schedule")
                        .on_hover_text("Let ProjectFCG schedule this task from its links")
                        .clicked()
                {
                    fix = Some(Fix::AutoSchedule(i));
                }
            }
        }
        if c.deadline_missed {
            warn(ui, &mut warned, "Finishes after its deadline.");
            if ui.button("Clear Deadline").clicked() {
                fix = Some(Fix::ClearDeadline(i));
            }
        }
    });
    if close {
        app.view.inspector = false;
    }
    let ctx = ui.ctx().clone();
    match fix {
        Some(Fix::RespectLinks(i)) => {
            app.edit_project("Respect Links", |p| p.set_constraint(i, ConstraintType::ASAP, None));
        }
        Some(Fix::AutoSchedule(i)) => {
            app.select_index(i);
            app.execute(Command::SetAuto, &ctx);
        }
        Some(Fix::ClearDeadline(i)) => {
            app.edit_project("Clear Deadline", |p| {
                p.tasks[i].deadline = None;
                Ok(())
            });
        }
        Some(Fix::Select(k)) => {
            for a in app.project.ancestors_of(k) {
                app.project.tasks[a].collapsed = false;
            }
            app.select_index(k);
            if let Some(s) = app.project.tasks[k].calc.start {
                app.scroll_gantt_to(s);
            }
        }
        None => {}
    }
}

fn warn(ui: &mut egui::Ui, warned: &mut bool, text: &str) {
    if !*warned {
        ui.add_space(6.0);
        ui.label(RichText::new("Warnings").strong().color(pal().warn));
        *warned = true;
    }
    ui.colored_label(pal().warn, text);
}

fn kind_name(k: LinkType) -> &'static str {
    match k {
        LinkType::FS => "Finish-to-Start",
        LinkType::SS => "Start-to-Start",
        LinkType::FF => "Finish-to-Finish",
        LinkType::SF => "Start-to-Finish",
    }
}
