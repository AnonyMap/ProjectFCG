//! Modal dialogs modelled on Project's: Task Information, Project
//! Information, Change Working Time, Options, Task Dependency, Go To, Find.

use super::commands::SHORTCUT_HELP;
use super::theme::*;
use super::{ProjectFcgApp, PendingAction, APP_NAME};
use crate::calendar::{Calendar, CalendarException, Shift};
use crate::duration::{DurUnit, Lag};
use crate::model::{
    fmt_date, fmt_datetime, parse_date, parse_time, ConstraintType, LinkType, Project, TaskMode,
};
use chrono::{Datelike, NaiveDate};
use egui::{Color32, RichText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoTab {
    General,
    Predecessors,
    Resources,
    Advanced,
    Notes,
}

#[derive(Clone, Debug)]
pub struct AssignRow {
    pub res_uid: u32,
    /// "50%" for work, quantity for material, amount for cost resources.
    pub units: String,
    pub units0: String,
}

#[derive(Clone, Debug)]
pub struct PredRow {
    pub id_text: String,
    pub kind: LinkType,
    pub lag_text: String,
}

#[derive(Clone, Debug)]
pub struct TaskInfo {
    pub uid: u32,
    pub tab: InfoTab,
    pub summary: bool,
    pub name: String,
    pub duration: String,
    pub duration0: String,
    pub estimated: bool,
    pub percent: f64,
    pub mode: TaskMode,
    pub start: String,
    pub start0: String,
    pub finish: String,
    pub finish0: String,
    pub on_timeline: bool,
    pub preds: Vec<PredRow>,
    pub add_pred: String,
    pub constraint: ConstraintType,
    pub constraint_date: String,
    pub deadline: String,
    pub milestone: bool,
    pub notes: String,
    pub wbs: String,
    pub task_type: crate::model::TaskType,
    pub effort_driven: bool,
    pub priority: u32,
    pub assigns: Vec<AssignRow>,
    pub is_sprint: bool,
    /// % complete comes from the Board (a sprint with tickets).
    pub board_progress: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectInfo {
    pub title: String,
    pub manager: String,
    pub start: String,
    pub status_date: String,
    pub base: String,
}

#[derive(Clone, Debug)]
pub struct WorkingTime {
    pub cal: Calendar,
    pub month: NaiveDate,
    pub tab: u8,
    pub week_text: [String; 7],
    pub week_working: [bool; 7],
    pub ex_name: String,
    pub ex_start: String,
    pub ex_finish: String,
    pub ex_times: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OptionsDlg {
    pub hours_per_day: f64,
    pub hours_per_week: f64,
    pub days_per_month: f64,
    pub start_time: String,
    pub finish_time: String,
    pub unit: DurUnit,
    pub new_auto: bool,
    pub honor: bool,
    pub crit_days: f64,
    pub task_type: crate::model::TaskType,
    pub effort_driven: bool,
    pub auto_add: bool,
    pub currency: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Dependency {
    pub pred_uid: u32,
    pub succ_uid: u32,
    pub kind: LinkType,
    pub lag: String,
}

#[derive(Default)]
pub struct Dialogs {
    pub message: Option<(String, String)>,
    pub save_prompt: bool,
    pub task_info: Option<TaskInfo>,
    pub project_info: Option<ProjectInfo>,
    pub working_time: Option<WorkingTime>,
    pub options: Option<OptionsDlg>,
    pub dependency: Option<Dependency>,
    pub goto_open: bool,
    pub goto_id: String,
    pub goto_date: String,
    pub find_open: bool,
    pub find_text: String,
    pub shortcuts_open: bool,
    pub about_open: bool,
    pub bar_styles: Option<super::format_dialogs::BarStylesDlg>,
    pub format_bar: Option<super::format_dialogs::FormatBarDlg>,
    pub text_styles: Option<super::format_dialogs::TextStylesDlg>,
    pub gridlines: Option<crate::format::Gridlines>,
    pub layout: Option<crate::format::LayoutOptions>,
    pub timescale: Option<crate::format::TimescaleSpec>,
    pub baseline: Option<super::format_dialogs::BaselineDlg>,
    /// Non-modal Assign Resources window.
    pub assign: Option<super::resource_dialogs::AssignDlg>,
    pub resource_info: Option<super::resource_dialogs::ResInfoDlg>,
    pub leveling: Option<super::resource_dialogs::LevelingDlg>,
    pub ticket: Option<super::board::TicketDlg>,
    pub update_tasks: Option<super::tracking_dialogs::UpdateTasksDlg>,
    pub update_project: Option<super::tracking_dialogs::UpdateProjectDlg>,
    pub statistics: bool,
    pub jira_import: Option<super::interchange::JiraImportDlg>,
    pub sprint: Option<super::sprint_dialogs::SprintDlg>,
    pub columns: Option<super::sprint_dialogs::ColumnsDlg>,
    pub recurring: Option<super::sprint_dialogs::RecurringDlg>,
}

impl Dialogs {
    pub fn any_modal_open(&self) -> bool {
        self.message.is_some()
            || self.save_prompt
            || self.task_info.is_some()
            || self.project_info.is_some()
            || self.working_time.is_some()
            || self.options.is_some()
            || self.dependency.is_some()
            || self.goto_open
            || self.find_open
            || self.shortcuts_open
            || self.about_open
            || self.bar_styles.is_some()
            || self.format_bar.is_some()
            || self.text_styles.is_some()
            || self.gridlines.is_some()
            || self.layout.is_some()
            || self.timescale.is_some()
            || self.baseline.is_some()
            || self.resource_info.is_some()
            || self.leveling.is_some()
            || self.ticket.is_some()
            || self.jira_import.is_some()
            || self.sprint.is_some()
            || self.columns.is_some()
            || self.recurring.is_some()
            || self.update_tasks.is_some()
            || self.update_project.is_some()
            || self.statistics
    }

    pub fn open_task_info(&mut self, p: &Project, row: usize, tab: InfoTab) {
        let t = &p.tasks[row];
        let ds = &p.settings.durations;
        let dur = if t.calc.is_summary {
            super::columns::cell_text(p, row, super::columns::ColumnId::Duration)
        } else {
            let mut d = t.duration;
            d.estimated = false;
            d.format(ds)
        };
        let start = t.calc.start.map(fmt_datetime).unwrap_or_default();
        let finish = t.calc.finish.map(fmt_datetime).unwrap_or_default();
        self.task_info = Some(TaskInfo {
            uid: t.uid,
            tab,
            summary: t.calc.is_summary,
            name: t.name.clone(),
            duration: dur.clone(),
            duration0: dur,
            estimated: t.duration.estimated,
            percent: if t.calc.is_summary { t.calc.percent_complete } else { t.percent_complete },
            mode: t.mode,
            start: start.clone(),
            start0: start,
            finish: finish.clone(),
            finish0: finish,
            on_timeline: t.on_timeline,
            preds: {
                let row_of = |id_text: String, kind: LinkType, lag: &Lag| {
                    let s = lag.format(ds);
                    PredRow { id_text, kind, lag_text: if s.is_empty() { "0d".into() } else { s } }
                };
                t.board_deps
                    .iter()
                    .map(|d| row_of(p.ticket_key(d.ticket), LinkType::FS, &d.lag))
                    .chain(t.predecessors.iter().filter_map(|l| Some(row_of(p.id_of_uid(l.pred)?.to_string(), l.kind, &l.lag))))
                    .collect()
            },
            add_pred: String::new(),
            constraint: t.constraint,
            constraint_date: t.constraint_date.map(fmt_datetime).unwrap_or_else(|| "NA".into()),
            deadline: t.deadline.map(fmt_datetime).unwrap_or_else(|| "NA".into()),
            milestone: t.milestone,
            notes: t.notes.clone(),
            wbs: t.calc.outline_number.clone(),
            task_type: t.task_type,
            effort_driven: t.effort_driven,
            priority: t.priority,
            is_sprint: t.is_sprint,
            board_progress: p.progress_from_board(row),
            assigns: p
                .assignments_of_task(row)
                .into_iter()
                .filter_map(|ai| {
                    let a = p.assignments[ai];
                    let r = &p.resources[p.resource_index(a.resource)?];
                    let u = assign_units_text(p, r.rtype, &a);
                    Some(AssignRow { res_uid: a.resource, units: u.clone(), units0: u })
                })
                .collect(),
        });
    }

    pub fn open_project_info(&mut self, p: &Project) {
        self.project_info = Some(ProjectInfo {
            title: p.title.clone(),
            manager: p.manager.clone(),
            start: fmt_datetime(p.start),
            status_date: p.status_date.map(fmt_datetime).unwrap_or_else(|| "NA".into()),
            base: p.calendar.name.clone(),
        });
    }

    pub fn open_working_time(&mut self, p: &Project) {
        let cal = p.calendar.clone();
        let week_text = std::array::from_fn(|i| shifts_to_text(&cal.week[i]));
        let week_working = std::array::from_fn(|i| !cal.week[i].is_empty());
        let m = p.start.date();
        self.working_time = Some(WorkingTime {
            cal,
            month: NaiveDate::from_ymd_opt(m.year(), m.month(), 1).unwrap(),
            tab: 0,
            week_text,
            week_working,
            ex_name: String::new(),
            ex_start: String::new(),
            ex_finish: String::new(),
            ex_times: String::new(),
            error: None,
        });
    }

    pub fn open_options(&mut self, p: &Project) {
        let s = &p.settings;
        self.options = Some(OptionsDlg {
            hours_per_day: s.durations.minutes_per_day as f64 / 60.0,
            hours_per_week: s.durations.minutes_per_week as f64 / 60.0,
            days_per_month: s.durations.days_per_month as f64,
            start_time: fmt_time(s.default_start_minute),
            finish_time: fmt_time(s.default_finish_minute),
            unit: s.default_duration_unit,
            new_auto: s.new_tasks_auto,
            honor: s.honor_constraints,
            crit_days: s.critical_slack_threshold as f64 / s.durations.minutes_per_day as f64,
            task_type: s.default_task_type,
            effort_driven: s.new_tasks_effort_driven,
            auto_add: s.auto_add_resources,
            currency: s.currency_symbol.clone(),
            error: None,
        });
    }

    pub fn open_dependency(&mut self, p: &Project, pred_uid: u32, succ_uid: u32) {
        let Some(t) = p.task_by_uid(succ_uid) else { return };
        let Some(l) = t.predecessors.iter().find(|l| l.pred == pred_uid) else { return };
        let lag = l.lag.format(&p.settings.durations);
        self.dependency = Some(Dependency {
            pred_uid,
            succ_uid,
            kind: l.kind,
            lag: if lag.is_empty() { "0d".into() } else { lag },
        });
    }
}

fn fmt_time(m: u32) -> String {
    let h = m / 60;
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{}:{:02} {}", h12, m % 60, ap)
}

pub fn shifts_to_text(shifts: &[Shift]) -> String {
    if shifts.is_empty() {
        return "Nonworking".into();
    }
    shifts
        .iter()
        .map(|s| format!("{}-{}", fmt_time(s.start), fmt_time(s.end % 1440)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse "8:00 AM-12:00 PM, 1:00 PM-5:00 PM" (or 24h "8:00-12:00").
pub fn parse_shifts(text: &str) -> Result<Vec<Shift>, String> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("nonworking") {
        return Ok(vec![]);
    }
    let mut out: Vec<Shift> = vec![];
    for part in t.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (a, b) = part
            .split_once('-')
            .ok_or_else(|| format!("\"{part}\" should look like 8:00 AM-12:00 PM"))?;
        let ta = parse_time(a.trim()).ok_or_else(|| format!("\"{}\" is not a time", a.trim()))?;
        let tb = parse_time(b.trim()).ok_or_else(|| format!("\"{}\" is not a time", b.trim()))?;
        use chrono::Timelike;
        let sa = ta.hour() * 60 + ta.minute();
        let mut sb = tb.hour() * 60 + tb.minute();
        if sb == 0 {
            sb = 1440; // "…-12:00 AM" means midnight at the end of the day
        }
        if sb <= sa {
            return Err(format!("In \"{part}\" the end time must be after the start time."));
        }
        if let Some(prev) = out.last() {
            if sa < prev.end {
                return Err("Working times must be in order and must not overlap.".into());
            }
        }
        out.push(Shift::new(sa, sb));
    }
    Ok(out)
}

fn assign_units_text(p: &Project, rtype: crate::model::ResourceType, a: &crate::model::Assignment) -> String {
    match rtype {
        crate::model::ResourceType::Work => crate::resources::fmt_units(a.units),
        crate::model::ResourceType::Material => crate::duration::fmt_num(a.units),
        crate::model::ResourceType::Cost => crate::resources::fmt_money(a.cost, &p.settings.currency_symbol),
    }
}

/// Text field with a calendar drop-down.
pub(crate) fn date_field(ui: &mut egui::Ui, text: &mut String, id: &str, fallback: NaiveDate) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(text).desired_width(170.0));
        ui.menu_button("▾", |ui| {
            let base = parse_date(text, chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap())
                .map(|d| d.date())
                .unwrap_or(fallback);
            if let Some(d) = super::datepicker::calendar(ui, egui::Id::unique(id), base) {
                *text = fmt_date(d.and_hms_opt(0, 0, 0).unwrap());
                ui.close();
            }
        });
    });
}

/// Ids of modals shown in the previous frame. A dialog only reacts to
/// Enter/Esc once it has been on screen for a frame, so the key press that
/// opened it (e.g. Enter committing a bad cell value) can't also close it.
fn modal_sets(ctx: &egui::Context) -> (Vec<String>, Vec<String>) {
    ctx.data(|d| {
        (
            d.get_temp::<Vec<String>>(egui::Id::unique("projectfcg_prev_modals")).unwrap_or_default(),
            d.get_temp::<Vec<String>>(egui::Id::unique("projectfcg_cur_modals")).unwrap_or_default(),
        )
    })
}

pub struct ModalKeys {
    pub esc: bool,
    pub enter: bool,
}

pub(crate) fn modal(ctx: &egui::Context, id: &str, title: &str, width: f32, add: impl FnOnce(&mut egui::Ui)) -> ModalKeys {
    let (prev, mut cur) = modal_sets(ctx);
    cur.push(id.to_string());
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("projectfcg_cur_modals"), cur));
    // Project's dialogs don't close when clicking outside them; only Esc cancels.
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(15.0).strong().color(pal().heading));
        });
        ui.add_space(4.0);
        add(ui);
    });
    let armed = prev.iter().any(|p| p == id);
    let (esc, enter) = ctx.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter)));
    ModalKeys {
        esc: armed && esc,
        enter: armed && enter,
    }
}

/// A vertical divider of fixed height. (`ui.separator()` in a horizontal
/// layout stretches to the full available height, which makes a modal grow
/// off-screen.)
pub(crate) fn vsep(ui: &mut egui::Ui, height: f32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, height), egui::Sense::hover());
    ui.painter().line_segment(
        [egui::pos2(r.center().x, r.top()), egui::pos2(r.center().x, r.bottom())],
        egui::Stroke::new(1.0, pal().grid),
    );
}

pub(crate) fn ok_cancel(ui: &mut egui::Ui) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(6.0);
    ui.separator();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("  Cancel  ").clicked() {
            cancel = true;
        }
        if ui.button(RichText::new("    OK    ").strong()).clicked() {
            ok = true;
        }
    });
    (ok, cancel)
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let (_, cur) = modal_sets(ctx);
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("projectfcg_prev_modals"), cur);
        d.insert_temp(egui::Id::unique("projectfcg_cur_modals"), Vec::<String>::new());
    });
    task_info(app, ctx);
    project_info(app, ctx);
    working_time(app, ctx);
    options(app, ctx);
    dependency(app, ctx);
    goto(app, ctx);
    find(app, ctx);
    shortcuts(app, ctx);
    about(app, ctx);
    save_prompt(app, ctx);
    message(app, ctx);
}

fn task_info(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.task_info.take() else { return };
    let Some(row) = app.project.index_of_uid(st.uid) else { return };
    let fallback = app.project.start.date();
    let mut ok = false;
    let mut cancel = false;
    let tasks_for_combo: Vec<(usize, String)> = app
        .project
        .tasks
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != row)
        .map(|(i, t)| (i + 1, t.name.clone()))
        .collect();
    // board items a task can wait on: stories, tasks, bugs and epics
    let tickets_for_combo: Vec<(String, String)> = app
        .project
        .tickets
        .iter()
        .filter(|t| t.kind != crate::board::IssueType::Subtask)
        .map(|t| (app.project.ticket_key(t.id), format!("{} · {}", t.title, t.kind.label())))
        .collect();
    let keys = modal(ctx, "task_info", "Task Information", 600.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(330.0));
            ui.label("Duration:");
            ui.add_enabled(!st.summary, egui::TextEdit::singleline(&mut st.duration).desired_width(80.0));
            ui.add_enabled(!st.summary, egui::Checkbox::new(&mut st.estimated, "Estimated"));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (t, l) in [
                (InfoTab::General, "General"),
                (InfoTab::Predecessors, "Predecessors"),
                (InfoTab::Resources, "Resources"),
                (InfoTab::Advanced, "Advanced"),
                (InfoTab::Notes, "Notes"),
            ] {
                ui.selectable_value(&mut st.tab, t, l);
            }
        });
        ui.separator();
        ui.set_min_height(230.0);
        match st.tab {
            InfoTab::General => {
                egui::Grid::new("ti_general").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Percent complete:");
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            !st.summary && !st.board_progress,
                            egui::DragValue::new(&mut st.percent).range(0.0..=100.0).suffix("%").speed(1.0),
                        );
                        if st.board_progress {
                            ui.label(RichText::new("from the Board").size(11.0).color(pal().text_dim));
                        }
                    });
                    ui.end_row();
                    ui.label("Schedule Mode:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut st.mode, TaskMode::Manual, "Manually Scheduled");
                        ui.radio_value(&mut st.mode, TaskMode::Auto, "Auto Scheduled");
                    });
                    ui.end_row();
                    ui.label("Start:");
                    date_field(ui, &mut st.start, "ti_start", fallback);
                    ui.end_row();
                    ui.label("Finish:");
                    if st.summary {
                        ui.label(&st.finish);
                    } else {
                        date_field(ui, &mut st.finish, "ti_finish", fallback);
                    }
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut st.on_timeline, "Display on Timeline");
                    ui.end_row();
                    ui.label("Priority:");
                    ui.add(egui::DragValue::new(&mut st.priority).range(0..=1000).speed(5.0))
                        .on_hover_text("0–1000; higher is levelled first; 1000 = never level");
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Typing a Start or Finish on an auto-scheduled task sets a Start/Finish No Earlier Than constraint, as in Project.",
                    )
                    .size(11.0)
                    .color(pal().text_dim),
                );
            }
            InfoTab::Predecessors => {
                let mut remove = None;
                egui::Grid::new("ti_preds").num_columns(5).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.strong("ID");
                    ui.strong("Task Name");
                    ui.strong("Type");
                    ui.strong("Lag");
                    ui.label("");
                    ui.end_row();
                    for (k, r) in st.preds.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut r.id_text).desired_width(40.0));
                        let id = r.id_text.trim();
                        let name = id
                            .parse::<usize>()
                            .ok()
                            .and_then(|id| tasks_for_combo.iter().find(|(i, _)| *i == id))
                            .map(|(_, n)| n.clone())
                            .or_else(|| tickets_for_combo.iter().find(|(k, _)| k.eq_ignore_ascii_case(id)).map(|(_, n)| format!("▣ {n}")))
                            .unwrap_or_default();
                        ui.add_sized([220.0, 18.0], egui::Label::new(name).truncate());
                        egui::ComboBox::from_id_salt(("pt", k))
                            .selected_text(r.kind.long_name())
                            .width(170.0)
                            .show_ui(ui, |ui| {
                                for kind in LinkType::ALL {
                                    ui.selectable_value(&mut r.kind, kind, kind.long_name());
                                }
                            });
                        ui.add(egui::TextEdit::singleline(&mut r.lag_text).desired_width(70.0));
                        if ui.small_button("✖").on_hover_text("Remove link").clicked() {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                });
                if let Some(k) = remove {
                    st.preds.remove(k);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Add predecessor:");
                    egui::ComboBox::from_id_salt("add_pred")
                        .selected_text(if st.add_pred.is_empty() { "Choose task…".to_string() } else { st.add_pred.clone() })
                        .width(300.0)
                        .show_ui(ui, |ui| {
                            for (id, n) in &tasks_for_combo {
                                if ui.selectable_label(false, format!("{id}  {n}")).clicked() {
                                    st.preds.push(PredRow {
                                        id_text: id.to_string(),
                                        kind: LinkType::FS,
                                        lag_text: "0d".into(),
                                    });
                                }
                            }
                        });
                });
                if !tickets_for_combo.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Wait on board item:");
                        egui::ComboBox::from_id_salt("add_board_pred")
                            .selected_text("Choose story or epic…")
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for (k, n) in &tickets_for_combo {
                                    if ui.selectable_label(false, format!("{k}  {n}")).clicked() {
                                        st.preds.push(PredRow { id_text: k.clone(), kind: LinkType::FS, lag_text: "0d".into() });
                                    }
                                }
                            });
                    });
                    ui.label(
                        RichText::new("A task waiting on a story follows the end of the story's sprint until the story is done, then the time it was done. You can also type keys like APP-12 in the Predecessors column.")
                            .size(11.0)
                            .color(pal().text_dim),
                    );
                }
            }
            InfoTab::Advanced => {
                egui::Grid::new("ti_adv").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Deadline:");
                    date_field(ui, &mut st.deadline, "ti_deadline", fallback);
                    ui.end_row();
                    ui.label("Constraint type:");
                    egui::ComboBox::from_id_salt("ct")
                        .selected_text(st.constraint.label())
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for c in ConstraintType::ALL {
                                ui.selectable_value(&mut st.constraint, c, c.label());
                            }
                        });
                    ui.end_row();
                    ui.label("Constraint date:");
                    ui.add_enabled_ui(st.constraint.needs_date(), |ui| {
                        date_field(ui, &mut st.constraint_date, "ti_cd", fallback);
                    });
                    ui.end_row();
                    ui.label("Task calendar:");
                    ui.label(format!("{} (project calendar)", app.project.calendar.name));
                    ui.end_row();
                    ui.label("WBS code:");
                    ui.label(&st.wbs);
                    ui.end_row();
                    ui.label("Task type:");
                    egui::ComboBox::from_id_salt("ti_tt").selected_text(st.task_type.label()).show_ui(ui, |ui| {
                        for t in crate::model::TaskType::ALL {
                            ui.selectable_value(&mut st.task_type, t, t.label());
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut st.effort_driven, "Effort driven");
                    ui.end_row();
                    ui.label("");
                    ui.add_enabled(!st.summary, egui::Checkbox::new(&mut st.milestone, "Mark task as milestone"));
                    ui.end_row();
                    ui.label("");
                    ui.add_enabled(!st.summary, egui::Checkbox::new(&mut st.is_sprint, "Sprint (holds tickets on the Board)"))
                        .on_hover_text("A sprint is a fixed-duration timebox; its % complete comes from its tickets.");
                    ui.end_row();
                });
            }
            InfoTab::Resources => {
                if st.summary {
                    ui.label("Resources are assigned to subtasks, not summary tasks.");
                } else {
                    let mut remove = None;
                    egui::Grid::new("ti_res").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        ui.strong("Resource Name");
                        ui.strong("Units");
                        ui.strong("Type");
                        ui.label("");
                        ui.end_row();
                        for (k, a) in st.assigns.iter_mut().enumerate() {
                            let r = app.project.resource_index(a.res_uid).map(|i| &app.project.resources[i]);
                            ui.label(r.map(|r| r.name.clone()).unwrap_or_default());
                            ui.add(egui::TextEdit::singleline(&mut a.units).desired_width(80.0));
                            ui.label(r.map(|r| r.rtype.label()).unwrap_or(""));
                            if ui.small_button("✖").on_hover_text("Remove assignment").clicked() {
                                remove = Some(k);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(k) = remove {
                        st.assigns.remove(k);
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Add resource:");
                        egui::ComboBox::from_id_salt("ti_addres")
                            .selected_text("Choose resource…")
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for r in &app.project.resources {
                                    if st.assigns.iter().any(|a| a.res_uid == r.uid) {
                                        continue;
                                    }
                                    if ui.selectable_label(false, &r.name).clicked() {
                                        let u = match r.rtype {
                                            crate::model::ResourceType::Work => "100%".to_string(),
                                            crate::model::ResourceType::Material => "1".to_string(),
                                            crate::model::ResourceType::Cost => "0".to_string(),
                                        };
                                        st.assigns.push(AssignRow { res_uid: r.uid, units: u, units0: String::new() });
                                    }
                                }
                            });
                    });
                    ui.label(
                        RichText::new(format!(
                            "{} task: Work = Duration × Units. {}",
                            st.task_type.label(),
                            if st.effort_driven { "Effort driven: total work stays the same when resources change." } else { "" }
                        ))
                        .size(11.0)
                        .color(pal().text_dim),
                    );
                }
            }
            InfoTab::Notes => {
                ui.add(egui::TextEdit::multiline(&mut st.notes).desired_rows(10).desired_width(f32::INFINITY));
            }
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if keys.enter && st.tab != InfoTab::Notes {
        ok = true;
    }
    if cancel || keys.esc {
        return;
    }
    if !ok {
        app.dialogs.task_info = Some(st);
        return;
    }
    let s2 = st.clone();
    let res = app.edit_project("Task Information", move |p| {
        let st = s2;
        p.tasks[row].name = st.name.clone();
        p.set_mode(row, st.mode);
        p.tasks[row].task_type = st.task_type;
        p.tasks[row].effort_driven = st.effort_driven;
        p.tasks[row].priority = st.priority.min(1000);
        if st.is_sprint != p.tasks[row].is_sprint {
            p.set_sprint(row, st.is_sprint)?;
        }
        if !st.summary {
            // assignments: removals, then additions and unit changes
            let tuid = p.tasks[row].uid;
            let current: Vec<u32> = p.assignments.iter().filter(|a| a.task == tuid).map(|a| a.resource).collect();
            for r in current {
                if !st.assigns.iter().any(|a| a.res_uid == r) {
                    p.unassign(row, r);
                }
            }
            let sym = p.settings.currency_symbol.clone();
            for a in &st.assigns {
                if a.units.trim() == a.units0.trim() {
                    continue;
                }
                let Some(ri) = p.resource_index(a.res_uid) else { continue };
                let rtype = p.resources[ri].rtype;
                let v = match rtype {
                    crate::model::ResourceType::Work => crate::resources::parse_units(&a.units)?,
                    crate::model::ResourceType::Material => a.units.trim().parse::<f64>().map_err(|_| format!("\"{}\" is not a quantity", a.units))?,
                    crate::model::ResourceType::Cost => crate::resources::parse_money(&a.units, &sym)?,
                };
                let ai = p.assign(row, a.res_uid, v)?;
                if rtype == crate::model::ResourceType::Cost {
                    p.assignments[ai].cost = v;
                }
            }
            if st.duration.trim() != st.duration0.trim() {
                let d = p.parse_duration(&st.duration)?;
                p.set_duration(row, d)?;
            }
            if p.work_assignments(row).is_empty() {
                p.tasks[row].duration.estimated = st.estimated;
            }
            // % complete is applied last (below), once links and dates are in
            p.tasks[row].milestone = st.milestone;
        }
        p.tasks[row].on_timeline = st.on_timeline;
        p.tasks[row].notes = st.notes.clone();
        // predecessors
        let text: Vec<String> = st
            .preds
            .iter()
            .filter(|r| !r.id_text.trim().is_empty())
            .map(|r| {
                let lag = Lag::parse(&r.lag_text, &p.settings.durations).unwrap_or_default();
                let lag_s = lag.format(&p.settings.durations);
                format!("{}{}{}", r.id_text.trim(), r.kind.code(), lag_s)
            })
            .collect();
        for r in &st.preds {
            Lag::parse(&r.lag_text, &p.settings.durations)?;
        }
        p.set_predecessors_text(row, &text.join(","))?;
        // advanced
        let cd = if st.constraint.needs_date() {
            Some(parse_date(&st.constraint_date, p.default_start_time()).map_err(|_| {
                format!("{} needs a valid constraint date.", st.constraint.label())
            })?)
        } else {
            None
        };
        p.set_constraint(row, st.constraint, cd)?;
        let dl = st.deadline.trim();
        p.tasks[row].deadline = if dl.is_empty() || dl.eq_ignore_ascii_case("na") {
            None
        } else {
            Some(parse_date(dl, p.default_finish_time())?)
        };
        // dates last, so typed dates win (Project sets constraints from them)
        if st.start.trim() != st.start0.trim() {
            let d = parse_date(&st.start, p.default_start_time())?;
            p.set_start(row, d)?;
        }
        if !st.summary && st.finish.trim() != st.finish0.trim() {
            let d = parse_date(&st.finish, p.default_finish_time())?;
            p.set_finish(row, d)?;
        }
        // progress last: starting the task stamps its actual start from the
        // schedule, which must already reflect this dialog's other changes
        if !st.summary && !st.board_progress && (p.tasks[row].percent_complete - st.percent).abs() > 1e-9 {
            crate::schedule::schedule(p);
            p.set_percent_complete(row, st.percent.clamp(0.0, 100.0))?;
        }
        Ok(())
    });
    if res.is_none() {
        // keep the dialog open so the user can correct the value
        app.dialogs.task_info = Some(st);
    }
}

fn project_info(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.project_info.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let finish = app.project.calc.finish;
    let n_tasks = app.project.tasks.len();
    let n_crit = app.project.tasks.iter().filter(|t| !t.calc.is_summary && t.calc.critical).count();
    let n_ms = app.project.tasks.iter().filter(|t| t.is_milestone()).count();
    let dur = finish.map(|f| app.project.format_duration_minutes(app.project.calendar.work_between(app.project.start, f), DurUnit::Day));
    let fallback = app.project.start.date();
    let keys = modal(ctx, "project_info", "Project Information", 520.0, |ui| {
        egui::Grid::new("pi").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Title:");
            ui.add(egui::TextEdit::singleline(&mut st.title).desired_width(300.0));
            ui.end_row();
            ui.label("Manager:");
            ui.add(egui::TextEdit::singleline(&mut st.manager).desired_width(300.0));
            ui.end_row();
            ui.label("Start date:");
            date_field(ui, &mut st.start, "pi_start", fallback);
            ui.end_row();
            ui.label("Finish date:");
            ui.label(finish.map(fmt_datetime).unwrap_or_default());
            ui.end_row();
            ui.label("Schedule from:");
            ui.label("Project Start Date");
            ui.end_row();
            ui.label("Current date:");
            ui.label(fmt_date(chrono::Local::now().naive_local()));
            ui.end_row();
            ui.label("Status date:");
            date_field(ui, &mut st.status_date, "pi_status", fallback);
            ui.end_row();
            ui.label("Calendar:");
            egui::ComboBox::from_id_salt("pi_cal").selected_text(&st.base).show_ui(ui, |ui| {
                for b in ["Standard", "24 Hours", "Night Shift"] {
                    ui.selectable_value(&mut st.base, b.to_string(), b);
                }
            });
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Statistics").strong());
            ui.label(format!(
                "Duration: {}    Tasks: {}    Critical: {}    Milestones: {}",
                dur.clone().unwrap_or_default(),
                n_tasks,
                n_crit,
                n_ms
            ));
        });
        (ok, cancel) = ok_cancel(ui);
    });
    if keys.enter {
        ok = true;
    }
    if cancel || keys.esc {
        return;
    }
    if !ok {
        app.dialogs.project_info = Some(st);
        return;
    }
    let s2 = st.clone();
    if app
        .edit_project("Project Information", move |p| {
            p.title = s2.title.clone();
            p.manager = s2.manager.clone();
            p.start = parse_date(&s2.start, p.default_start_time())?;
            let sd = s2.status_date.trim();
            p.status_date = if sd.is_empty() || sd.eq_ignore_ascii_case("na") {
                None
            } else {
                Some(parse_date(sd, p.default_finish_time())?)
            };
            if s2.base != p.calendar.name {
                let mut c = match s2.base.as_str() {
                    "24 Hours" => Calendar::twenty_four_hours(),
                    "Night Shift" => Calendar::night_shift(),
                    _ => Calendar::standard(),
                };
                c.exceptions = p.calendar.exceptions.clone();
                p.calendar = c;
            }
            Ok(())
        })
        .is_none()
    {
        app.dialogs.project_info = Some(st);
    }
}

fn working_time(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.working_time.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "working_time", "Change Working Time", 700.0, |ui| {
        ui.label(format!("For calendar: {} (Project Calendar)", st.cal.name));
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            // month view
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("◀").clicked() {
                        st.month = super::datepicker::shift_month(st.month, -1);
                    }
                    ui.add_sized([120.0, 18.0], egui::Label::new(RichText::new(st.month.format("%B %Y").to_string()).strong()));
                    if ui.small_button("▶").clicked() {
                        st.month = super::datepicker::shift_month(st.month, 1);
                    }
                });
                let first = st.month - chrono::Duration::days(st.month.weekday().num_days_from_sunday() as i64);
                egui::Grid::new("wt_cal").spacing([2.0, 2.0]).show(ui, |ui| {
                    for w in ["S", "M", "T", "W", "T", "F", "S"] {
                        ui.label(RichText::new(w).size(11.0).color(pal().text_dim));
                    }
                    ui.end_row();
                    for wk in 0..6 {
                        for dw in 0..7 {
                            let d = first + chrono::Duration::days(wk * 7 + dw);
                            let is_ex = st.cal.exceptions.iter().any(|e| d >= e.start && d <= e.end);
                            let working = st.cal.is_working_day(d);
                            let fill = if is_ex {
                                pal().cal_exception
                            } else if working {
                                pal().cal_working
                            } else {
                                pal().cal_nonworking
                            };
                            let mut txt = RichText::new(d.day().to_string()).size(11.5);
                            if d.month() != st.month.month() {
                                txt = txt.color(pal().text_disabled);
                            }
                            let b = egui::Button::new(txt).fill(fill).min_size(egui::vec2(26.0, 20.0));
                            let r = ui.add(b);
                            if r.clicked() {
                                st.ex_start = fmt_date(d.and_hms_opt(0, 0, 0).unwrap());
                                st.ex_finish = st.ex_start.clone();
                            }
                            r.on_hover_text(format!(
                                "{}: {}",
                                fmt_date(d.and_hms_opt(0, 0, 0).unwrap()),
                                shifts_to_text(st.cal.shifts_on(d))
                            ));
                        }
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    legend(ui, pal().cal_working, "Working");
                    legend(ui, pal().cal_nonworking, "Nonworking");
                    legend(ui, pal().cal_exception, "Exception");
                });
            });
            vsep(ui, 300.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut st.tab, 0, "Exceptions");
                    ui.selectable_value(&mut st.tab, 1, "Work Weeks");
                });
                ui.separator();
                if st.tab == 0 {
                    let mut remove = None;
                    egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                        egui::Grid::new("wt_ex").striped(true).num_columns(4).show(ui, |ui| {
                            ui.strong("Name");
                            ui.strong("Start");
                            ui.strong("Finish");
                            ui.strong("");
                            ui.end_row();
                            for (k, e) in st.cal.exceptions.iter().enumerate() {
                                ui.label(&e.name).on_hover_text(shifts_to_text(&e.shifts));
                                ui.label(fmt_date(e.start.and_hms_opt(0, 0, 0).unwrap()));
                                ui.label(fmt_date(e.end.and_hms_opt(0, 0, 0).unwrap()));
                                if ui.small_button("Delete").clicked() {
                                    remove = Some(k);
                                }
                                ui.end_row();
                            }
                        });
                    });
                    if let Some(k) = remove {
                        st.cal.exceptions.remove(k);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("New exception").strong());
                    egui::Grid::new("wt_new").num_columns(2).show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut st.ex_name);
                        ui.end_row();
                        ui.label("Start:");
                        ui.text_edit_singleline(&mut st.ex_start);
                        ui.end_row();
                        ui.label("Finish:");
                        ui.text_edit_singleline(&mut st.ex_finish);
                        ui.end_row();
                        ui.label("Working times:");
                        ui.add(egui::TextEdit::singleline(&mut st.ex_times).hint_text("Nonworking"));
                        ui.end_row();
                    });
                    if ui.button("Add Exception").clicked() {
                        let t8 = chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap();
                        let s = parse_date(&st.ex_start, t8);
                        let f = if st.ex_finish.trim().is_empty() { s.clone() } else { parse_date(&st.ex_finish, t8) };
                        match (s, f, parse_shifts(&st.ex_times)) {
                            (Ok(s), Ok(f), Ok(sh)) if f.date() >= s.date() => {
                                st.cal.exceptions.push(CalendarException {
                                    name: if st.ex_name.trim().is_empty() { "Exception".into() } else { st.ex_name.trim().into() },
                                    start: s.date(),
                                    end: f.date(),
                                    shifts: sh,
                                });
                                st.ex_name.clear();
                                st.ex_times.clear();
                                st.error = None;
                            }
                            (_, _, Err(e)) => st.error = Some(e),
                            (Ok(_), Ok(_), Ok(_)) => st.error = Some("The finish must be on or after the start.".into()),
                            _ => st.error = Some("Enter valid start and finish dates.".into()),
                        }
                    }
                } else {
                    egui::Grid::new("wt_week").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                        for (k, day) in ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"].iter().enumerate() {
                            ui.label(*day);
                            ui.checkbox(&mut st.week_working[k], "Working");
                            ui.add_enabled(
                                st.week_working[k],
                                egui::TextEdit::singleline(&mut st.week_text[k]).desired_width(250.0),
                            );
                            ui.end_row();
                        }
                    });
                    ui.label(RichText::new("Times like: 8:00 AM-12:00 PM, 1:00 PM-5:00 PM").size(11.0).color(pal().text_dim));
                }
                if let Some(e) = &st.error {
                    ui.colored_label(pal().warn, e);
                }
            });
        });
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !ok {
        app.dialogs.working_time = Some(st);
        return;
    }
    // build week pattern
    let mut cal = st.cal.clone();
    for k in 0..7 {
        if !st.week_working[k] {
            cal.week[k] = vec![];
        } else {
            match parse_shifts(&st.week_text[k]) {
                Ok(s) if !s.is_empty() => cal.week[k] = s,
                Ok(_) => cal.week[k] = Calendar::standard().week[0].clone(),
                Err(e) => {
                    st.error = Some(e);
                    app.dialogs.working_time = Some(st);
                    return;
                }
            }
        }
    }
    if !cal.has_any_working_time() {
        st.error = Some("The calendar must have some working time.".into());
        app.dialogs.working_time = Some(st);
        return;
    }
    app.edit_project("Change Working Time", move |p| {
        p.calendar = cal;
        Ok(())
    });
}

fn legend(ui: &mut egui::Ui, c: Color32, label: &str) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 1.0, c);
    ui.painter().rect_stroke(r, 1.0, egui::Stroke::new(1.0, pal().text_dim), egui::StrokeKind::Inside);
    ui.label(RichText::new(label).size(11.0));
}

fn options(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.options.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "options", "Project Options — Schedule", 480.0, |ui| {
        egui::Grid::new("opt").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("New tasks created:");
            egui::ComboBox::from_id_salt("opt_new")
                .selected_text(if st.new_auto { "Auto Scheduled" } else { "Manually Scheduled" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.new_auto, true, "Auto Scheduled");
                    ui.selectable_value(&mut st.new_auto, false, "Manually Scheduled");
                });
            ui.end_row();
            ui.label("Duration is entered in:");
            egui::ComboBox::from_id_salt("opt_unit").selected_text(st.unit.to_string()).show_ui(ui, |ui| {
                for u in [DurUnit::Minute, DurUnit::Hour, DurUnit::Day, DurUnit::Week, DurUnit::Month] {
                    ui.selectable_value(&mut st.unit, u, u.to_string());
                }
            });
            ui.end_row();
            ui.label("Default start time:");
            ui.text_edit_singleline(&mut st.start_time);
            ui.end_row();
            ui.label("Default end time:");
            ui.text_edit_singleline(&mut st.finish_time);
            ui.end_row();
            ui.label("Hours per day:");
            ui.add(egui::DragValue::new(&mut st.hours_per_day).range(0.5..=24.0).speed(0.25));
            ui.end_row();
            ui.label("Hours per week:");
            ui.add(egui::DragValue::new(&mut st.hours_per_week).range(1.0..=168.0).speed(0.5));
            ui.end_row();
            ui.label("Days per month:");
            ui.add(egui::DragValue::new(&mut st.days_per_month).range(1.0..=31.0).speed(0.5));
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.honor, "Tasks will always honor their constraint dates");
            ui.end_row();
            ui.label("Critical if slack ≤ (days):");
            ui.add(egui::DragValue::new(&mut st.crit_days).range(0.0..=100.0).speed(0.25));
            ui.end_row();
            ui.label("Default task type:");
            egui::ComboBox::from_id_salt("opt_ttype").selected_text(st.task_type.label()).show_ui(ui, |ui| {
                for t in crate::model::TaskType::ALL {
                    ui.selectable_value(&mut st.task_type, t, t.label());
                }
            });
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.effort_driven, "New tasks are effort driven");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.auto_add, "Automatically add new resources");
            ui.end_row();
            ui.label("Currency symbol:");
            ui.add(egui::TextEdit::singleline(&mut st.currency).desired_width(60.0).char_limit(4));
            ui.end_row();
        });
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if keys.enter {
        ok = true;
    }
    if cancel || keys.esc {
        return;
    }
    if !ok {
        app.dialogs.options = Some(st);
        return;
    }
    use chrono::Timelike;
    let (Some(ts), Some(tf)) = (parse_time(&st.start_time), parse_time(&st.finish_time)) else {
        st.error = Some("Enter valid default start and end times.".into());
        app.dialogs.options = Some(st);
        return;
    };
    let s2 = st.clone();
    app.edit_project("Options", move |p| {
        let s = &mut p.settings;
        s.new_tasks_auto = s2.new_auto;
        s.default_duration_unit = s2.unit;
        s.default_start_minute = ts.hour() * 60 + ts.minute();
        s.default_finish_minute = tf.hour() * 60 + tf.minute();
        s.durations.minutes_per_day = (s2.hours_per_day * 60.0).round() as i64;
        s.durations.minutes_per_week = (s2.hours_per_week * 60.0).round() as i64;
        s.durations.days_per_month = s2.days_per_month.round() as i64;
        s.honor_constraints = s2.honor;
        s.critical_slack_threshold = (s2.crit_days * s.durations.minutes_per_day as f64).round() as i64;
        s.default_task_type = s2.task_type;
        s.new_tasks_effort_driven = s2.effort_driven;
        s.auto_add_resources = s2.auto_add;
        s.currency_symbol = s2.currency.trim().to_string();
        Ok(())
    });
}

fn dependency(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.dependency.take() else { return };
    let (Some(pi), Some(si)) = (app.project.index_of_uid(st.pred_uid), app.project.index_of_uid(st.succ_uid)) else {
        return;
    };
    let from = format!("({}) {}", pi + 1, app.project.tasks[pi].name);
    let to = format!("({}) {}", si + 1, app.project.tasks[si].name);
    let (mut ok, mut cancel, mut delete) = (false, false, false);
    let keys = modal(ctx, "dependency", "Task Dependency", 420.0, |ui| {
        egui::Grid::new("dep").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("From:");
            ui.label(&from);
            ui.end_row();
            ui.label("To:");
            ui.label(&to);
            ui.end_row();
            ui.label("Type:");
            egui::ComboBox::from_id_salt("dep_type").selected_text(st.kind.long_name()).width(200.0).show_ui(ui, |ui| {
                for k in LinkType::ALL {
                    ui.selectable_value(&mut st.kind, k, k.long_name());
                }
            });
            ui.end_row();
            ui.label("Lag:");
            ui.add(egui::TextEdit::singleline(&mut st.lag).desired_width(90.0));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
                delete = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("  Cancel  ").clicked() {
                    cancel = true;
                }
                if ui.button(RichText::new("    OK    ").strong()).clicked() {
                    ok = true;
                }
            });
        });
    });
    if keys.enter {
        ok = true;
    }
    if cancel || keys.esc {
        return;
    }
    if delete {
        app.edit_project("Delete Link", |p| {
            p.remove_link(pi, si);
            Ok(())
        });
        return;
    }
    if !ok {
        app.dialogs.dependency = Some(st);
        return;
    }
    let s2 = st.clone();
    if app
        .edit_project("Edit Link", move |p| {
            let lag = Lag::parse(&s2.lag, &p.settings.durations)?;
            p.add_link(pi, si, s2.kind, lag)
        })
        .is_none()
    {
        app.dialogs.dependency = Some(st);
    }
}

fn goto(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.goto_open {
        return;
    }
    let (mut ok, mut cancel) = (false, false);
    let fallback = app.project.start.date();
    let mut id = std::mem::take(&mut app.dialogs.goto_id);
    let mut date = std::mem::take(&mut app.dialogs.goto_date);
    let keys = modal(ctx, "goto", "Go To", 360.0, |ui| {
        egui::Grid::new("goto").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("ID:");
            let r = ui.add(egui::TextEdit::singleline(&mut id).desired_width(80.0));
            r.request_focus();
            ui.end_row();
            ui.label("Date:");
            date_field(ui, &mut date, "goto_date", fallback);
            ui.end_row();
        });
        (ok, cancel) = {
            let (a, b) = ok_cancel(ui);
            (ok || a, b)
        };
    });
    app.dialogs.goto_id = id.clone();
    app.dialogs.goto_date = date.clone();
    ok = ok || keys.enter;
    if cancel || keys.esc {
        app.dialogs.goto_open = false;
        return;
    }
    if ok {
        app.dialogs.goto_open = false;
        if let Ok(n) = id.trim().parse::<usize>() {
            if n >= 1 && n <= app.project.tasks.len() {
                for a in app.project.ancestors_of(n - 1) {
                    app.project.tasks[a].collapsed = false;
                }
                app.select_index(n - 1);
                if let Some(s) = app.project.tasks[n - 1].calc.start {
                    app.scroll_gantt_to(s);
                }
            } else {
                app.error(format!("There is no task with ID {n}."));
            }
        }
        if let Ok(d) = parse_date(&date, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()) {
            app.scroll_gantt_to(d);
        }
    }
}

fn find(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.find_open {
        return;
    }
    let mut text = std::mem::take(&mut app.dialogs.find_text);
    let (mut next, mut close) = (false, false);
    let keys = modal(ctx, "find", "Find", 380.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Find what:");
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(220.0));
            r.request_focus();
        });
        ui.label(RichText::new("Look in field: Task Name    Test: contains").size(11.0).color(pal().text_dim));
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                close = true;
            }
            if ui.button(RichText::new("Find Next").strong()).clicked() {
                next = true;
            }
        });
    });
    app.dialogs.find_text = text;
    next = next || keys.enter;
    if close || keys.esc {
        app.dialogs.find_open = false;
        return;
    }
    if next {
        app.find_next();
    }
}

fn shortcuts(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.shortcuts_open {
        return;
    }
    let mut close = false;
    let keys = modal(ctx, "shortcuts", "Keyboard Shortcuts", 560.0, |ui| {
        egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
            egui::Grid::new("sc").striped(true).num_columns(2).spacing([20.0, 3.0]).show(ui, |ui| {
                for (k, a) in SHORTCUT_HELP {
                    ui.label(RichText::new(*k).monospace().strong());
                    ui.label(*a);
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if close || keys.esc || keys.enter {
        app.dialogs.shortcuts_open = false;
    }
}

fn about(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.about_open {
        return;
    }
    let mut close = false;
    let keys = modal(ctx, "about", APP_NAME, 420.0, |ui| {
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.add_space(4.0);
        ui.label("A Gantt-chart project scheduler whose scheduling engine follows Microsoft Project's rules: working-time calendars, FS/SS/FF/SF links with lag, constraints, summary rollups, manual scheduling, and critical path analysis.");
        ui.add_space(4.0);
        ui.label(RichText::new("Built in Rust with egui.").color(pal().text_dim));
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if close || keys.esc || keys.enter {
        app.dialogs.about_open = false;
    }
}

fn save_prompt(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    if !app.dialogs.save_prompt {
        return;
    }
    let name = app.file_name();
    let (mut yes, mut no, mut cancel) = (false, false, false);
    let keys = modal(ctx, "save_prompt", APP_NAME, 380.0, |ui| {
        ui.label(format!("Do you want to save changes to '{name}'?"));
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("  Cancel  ").clicked() {
                cancel = true;
            }
            if ui.button("  No  ").clicked() {
                no = true;
            }
            if ui.button(RichText::new("  Yes  ").strong()).clicked() {
                yes = true;
            }
        });
    });
    yes = yes || keys.enter;
    if cancel || keys.esc {
        app.dialogs.save_prompt = false;
        app.pending = None;
        return;
    }
    if no {
        app.dialogs.save_prompt = false;
        app.dirty = false;
        if let Some(a) = app.pending.take() {
            app.run_pending(a, ctx);
        }
        return;
    }
    if yes {
        app.dialogs.save_prompt = false;
        if let Some(p) = app.path.clone() {
            if app.save_to(p) {
                if let Some(a) = app.pending.take() {
                    app.run_pending(a, ctx);
                }
            }
        } else {
            // Save As first; the pending action runs after a successful save.
            app.pick_save(super::filebrowser::Mode::SaveAs);
            if !app.dirty {
                if let Some(a) = app.pending.take() {
                    app.run_pending(a, ctx);
                }
            }
        }
    }
}

fn message(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some((title, text)) = app.dialogs.message.clone() else { return };
    let mut close = false;
    let keys = modal(ctx, "message", &title, 400.0, |ui| {
        // errors come titled with the app's name; reports (imports) are informational
        let is_error = title == super::APP_NAME;
        ui.horizontal_top(|ui| {
            let at = ui.cursor().min + egui::vec2(10.0, 10.0);
            if is_error {
                super::table::icon_warning(ui.painter(), at);
            } else {
                ui.painter().circle_filled(at, 7.5, pal().accent);
                ui.painter().text(at, egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(11.0), pal().on_band);
            }
            ui.add_space(24.0);
            ui.label(&text);
        });
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = ui.button(RichText::new("    OK    ").strong());
            if b.clicked() {
                close = true;
            }
        });
    });
    if close || keys.esc || keys.enter {
        app.dialogs.message = None;
    }
}

/// Used by `PendingAction` handling in tests.
pub fn _pending_debug(a: &PendingAction) -> String {
    format!("{a:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shift_text_roundtrip() {
        let s = parse_shifts("8:00 AM-12:00 PM, 1:00 PM-5:00 PM").unwrap();
        assert_eq!(s, vec![Shift::new(480, 720), Shift::new(780, 1020)]);
        assert_eq!(parse_shifts(&shifts_to_text(&s)).unwrap(), s);
        assert_eq!(parse_shifts("8:00-17:00").unwrap(), vec![Shift::new(480, 1020)]);
        assert_eq!(parse_shifts("Nonworking").unwrap(), vec![]);
        assert_eq!(parse_shifts("10:00 PM-12:00 AM").unwrap(), vec![Shift::new(1320, 1440)]);
        assert!(parse_shifts("5:00 PM-8:00 AM").is_err());
        assert!(parse_shifts("8:00-12:00, 11:00-13:00").is_err());
    }
}
