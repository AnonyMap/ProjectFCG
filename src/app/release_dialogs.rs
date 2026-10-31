//! Release dialogs: edit a release (name, dates, milestone) and release it.

use super::dialogs::{date_field, modal, ok_cancel};
use super::theme::*;
use super::ProjectFcgApp;
use crate::model::fmt_date;
use egui::RichText;

/// What the release's milestone should be after OK.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MilestoneChoice {
    None,
    Task(u32),
    /// Add a new milestone at the end of the plan.
    New,
}

#[derive(Clone, Debug)]
pub struct ReleaseDlg {
    /// `None`: a new release.
    pub id: Option<u32>,
    pub name: String,
    pub description: String,
    pub start: String,
    pub date: String,
    pub milestone: MilestoneChoice,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ReleaseVersionDlg {
    pub id: u32,
    pub date: String,
    /// Where unfinished stories go (`None`: out of any release).
    pub move_to: Option<u32>,
}

fn day_text(d: Option<chrono::NaiveDate>) -> String {
    d.map(|d| fmt_date(d.and_time(chrono::NaiveTime::MIN))).unwrap_or_default()
}

impl ProjectFcgApp {
    pub fn open_release_dialog(&mut self, id: Option<u32>) {
        let d = match id.and_then(|r| self.project.release(r)) {
            Some(r) => ReleaseDlg {
                id: Some(r.id),
                name: r.name.clone(),
                description: r.description.clone(),
                start: day_text(r.start),
                date: day_text(r.date),
                milestone: r.task.map_or(MilestoneChoice::None, MilestoneChoice::Task),
                error: None,
            },
            None => ReleaseDlg { id: None, name: String::new(), description: String::new(), start: String::new(), date: String::new(), milestone: MilestoneChoice::None, error: None },
        };
        self.dialogs.release = Some(d);
    }

    pub fn open_release_version_dialog(&mut self, id: u32) {
        // the next unreleased release, as Jira suggests
        let rels = &self.project.board.releases;
        let pos = rels.iter().position(|r| r.id == id).unwrap_or(0);
        let move_to = rels.iter().skip(pos + 1).find(|r| r.released.is_none()).map(|r| r.id);
        let now = chrono::Local::now().naive_local();
        self.dialogs.release_version = Some(ReleaseVersionDlg { id, date: fmt_date(now), move_to });
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    release_dialog(app, ctx);
    release_version(app, ctx);
}

fn release_dialog(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.release.take() else { return };
    let fallback = app.project.start.date();
    // tasks that can stand for a release: not summaries, not sprints, not
    // already standing for another release
    let p = &app.project;
    let tasks: Vec<(u32, String)> = (0..p.tasks.len())
        .filter(|&i| !p.tasks[i].calc.is_summary && !p.tasks[i].is_sprint)
        .filter(|&i| p.release_of_task(p.tasks[i].uid).is_none_or(|r| Some(r.id) == st.id))
        .map(|i| (p.tasks[i].uid, format!("{}  {}{}", i + 1, if p.tasks[i].is_milestone() { "◆ " } else { "" }, p.tasks[i].name)))
        .collect();
    let stats = st.id.map(|r| p.release_stats(r));
    let est = p.board.estimate;
    let (mut ok, mut cancel) = (false, false);
    let title = if st.id.is_some() { "Release" } else { "New Release" };
    let keys = modal(ctx, "release_dlg", title, 520.0, |ui| {
        egui::Grid::new("rel_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(260.0).hint_text("e.g. 1.2 or Spring launch"));
            ui.end_row();
            ui.label("Start date:");
            date_field(ui, &mut st.start, "rel_start", fallback);
            ui.end_row();
            ui.label("Release date:");
            date_field(ui, &mut st.date, "rel_date", fallback);
            ui.end_row();
            ui.label("Milestone:");
            let cur = match st.milestone {
                MilestoneChoice::None => "None".to_string(),
                MilestoneChoice::New => "New milestone at the end of the plan".to_string(),
                MilestoneChoice::Task(u) => tasks.iter().find(|t| t.0 == u).map(|t| t.1.clone()).unwrap_or_default(),
            };
            egui::ComboBox::from_id_salt("rel_milestone").selected_text(cur).width(300.0).height(360.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.milestone, MilestoneChoice::None, "None");
                ui.selectable_value(&mut st.milestone, MilestoneChoice::New, "New milestone at the end of the plan");
                for (u, n) in &tasks {
                    ui.selectable_value(&mut st.milestone, MilestoneChoice::Task(*u), n);
                }
            });
            ui.end_row();
        });
        ui.label(
            RichText::new("The milestone waits on every unfinished story in the release, and the release date becomes its deadline. Link tasks to it on the Gantt to make them follow the release.")
                .size(11.0)
                .color(pal().text_dim),
        );
        ui.add_space(4.0);
        ui.label("Description:");
        ui.add(egui::TextEdit::multiline(&mut st.description).desired_rows(3).desired_width(f32::INFINITY));
        if let Some(s) = &stats {
            ui.add_space(4.0);
            let mut line = format!("{} · {} of {} done ({:.0}%)", s.status.label(), crate::duration::fmt_num(s.done), est.unit(s.total), s.percent());
            if let Some(pj) = s.projected {
                line += &format!(" · projected {}", fmt_date(pj));
            }
            ui.label(RichText::new(line).color(super::board::release_status_color(s.status)));
        }
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.release = Some(st);
        return;
    }
    let t0 = chrono::NaiveTime::MIN;
    let parse = |s: &str| -> Result<Option<chrono::NaiveDate>, String> {
        if s.trim().is_empty() || s.trim().eq_ignore_ascii_case("na") {
            Ok(None)
        } else {
            crate::model::parse_date(s, t0).map(|d| Some(d.date())).map_err(|_| format!("\"{}\" isn't a date.", s.trim()))
        }
    };
    let (start, date) = match (parse(&st.start), parse(&st.date)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            st.error = Some(e);
            app.dialogs.release = Some(st);
            return;
        }
    };
    let s2 = st.clone();
    let label = if st.id.is_some() { "Edit Release" } else { "New Release" };
    let res = app.edit_project(label, |p| {
        let id = match s2.id {
            Some(id) => id,
            None => p.add_release(&s2.name, date)?,
        };
        p.update_release(id, &s2.name, &s2.description, start, date)?;
        match s2.milestone {
            MilestoneChoice::None => p.set_release_task(id, None)?,
            MilestoneChoice::Task(u) => p.set_release_task(id, Some(u))?,
            MilestoneChoice::New => {
                p.add_release_milestone(id)?;
            }
        }
        Ok(id)
    });
    if res.is_none() {
        st.error = Some(app.status.clone());
        app.dialogs.release = Some(st);
    }
}

fn release_version(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.release_version.take() else { return };
    let Some(rel) = app.project.release(st.id).cloned() else { return };
    let fallback = app.project.start.date();
    let unfinished = app.project.release_tickets(st.id).filter(|t| !app.project.is_done(t)).count();
    let others: Vec<(u32, String)> = app.project.board.releases.iter().filter(|r| r.id != st.id && r.released.is_none()).map(|r| (r.id, r.name.clone())).collect();
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "release_version", &format!("Release {}", rel.name), 440.0, |ui| {
        egui::Grid::new("rv_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Release date:");
            date_field(ui, &mut st.date, "rv_date", fallback);
            ui.end_row();
            if unfinished > 0 {
                ui.label(format!("{unfinished} unfinished:"));
                let cur = st.move_to.and_then(|u| others.iter().find(|x| x.0 == u)).map(|x| format!("Move to {}", x.1)).unwrap_or_else(|| "Take out of the release".into());
                egui::ComboBox::from_id_salt("rv_move").selected_text(cur).width(240.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.move_to, None, "Take out of the release");
                    for (u, n) in &others {
                        ui.selectable_value(&mut st.move_to, Some(*u), format!("Move to {n}"));
                    }
                });
                ui.end_row();
            }
        });
        if unfinished == 0 {
            ui.label(RichText::new("Every story in it is done.").color(pal().text_dim));
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.release_version = Some(st);
        return;
    }
    let at = crate::model::parse_date(&st.date, app.project.default_start_time()).unwrap_or_else(|_| chrono::Local::now().naive_local());
    let (id, to) = (st.id, st.move_to);
    app.edit_project(&format!("Release {}", rel.name), |p| p.release_version(id, at, to));
}
