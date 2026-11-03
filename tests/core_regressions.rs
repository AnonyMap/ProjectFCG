//! Regressions for the 0.9 bug hunt in the editing, scheduling and
//! tracking core (one test per finding).

use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::app::columns::{set_cell, ColumnId};
use projectfcg::app::commands::Command;
use projectfcg::app::{ProjectFcgApp, RowRef};
use projectfcg::duration::Lag;
use projectfcg::model::{LinkType, Project, TaskMode};
use projectfcg::ops::Field;
use projectfcg::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}
fn proj() -> Project {
    Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
}
fn add(p: &mut Project, name: &str, dur: &str) -> usize {
    let r = p.append_task(name);
    p.set_field_text(r, Field::Duration, dur).unwrap();
    r
}

// 1. a huge typed duration crashes the scheduler
#[test]
fn huge_duration_panics() {
    let mut p = proj();
    let a = add(&mut p, "A", "1d");
    let res = p.set_field_text(a, Field::Duration, "100000000d");
    assert!(res.unwrap_err().contains("too long"));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| schedule(&mut p)));
    assert!(r.is_ok(), "schedule panicked after typing 100000000d");
}

#[test]
fn inf_percent_lag_panics() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    let b = add(&mut p, "B", "1d");
    assert!(p.set_field_text(b, Field::Predecessors, "1FS+inf%").is_err());
    // and a file with a giant lag can't overflow the arithmetic either
    let ua = p.tasks[0].uid;
    p.tasks[b].predecessors.push(projectfcg::model::Link { pred: ua, kind: LinkType::FS, lag: Lag::Time { minutes: i64::MAX / 4, unit: projectfcg::duration::DurUnit::Day, elapsed: false } });
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| schedule(&mut p)));
    assert!(r.is_ok(), "schedule panicked after 1FS+inf%");
}

// 2. NaN accepted by numeric cells -> saved file can't be reopened
#[test]
fn nan_fixed_cost_makes_file_unloadable() {
    let mut p = proj();
    let a = add(&mut p, "A", "1d");
    assert!(set_cell(&mut p, a, ColumnId::FixedCost, "NaN").is_err());
    assert!(set_cell(&mut p, a, ColumnId::FixedCost, "inf").is_err());
    schedule(&mut p);
    let json = projectfcg::io::to_json(&p).unwrap();
    let back = projectfcg::io::from_json(&json);
    assert!(back.is_ok(), "reload failed: {:?}", back.err());
}

#[test]
fn nan_lag_makes_file_unloadable() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    let b = add(&mut p, "B", "1d");
    assert!(p.set_field_text(b, Field::Predecessors, "1+NaN%").is_err());
    schedule(&mut p);
    let json = projectfcg::io::to_json(&p).unwrap();
    let back = projectfcg::io::from_json(&json);
    assert!(back.is_ok(), "reload failed: {:?}", back.err());
}

// 3. leveling delay typed in elapsed days
#[test]
fn leveling_delay_elapsed_days() {
    let mut p = proj();
    let a = add(&mut p, "A", "1d");
    // working time only (decision 18): elapsed input is refused, not misread
    assert!(set_cell(&mut p, a, ColumnId::LevelingDelay, "3ed").unwrap_err().contains("working time"));
    set_cell(&mut p, a, ColumnId::LevelingDelay, "3d").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 8, 8)));
    // actuals follow the task's kind of time
    assert!(set_cell(&mut p, a, ColumnId::ActualDuration, "1ed").is_err());
}

// 4. manual task with a resource: typing Finish doesn't move the finish
#[test]
fn manual_task_finish_with_resource() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    p.set_field_text(a, Field::ResourceNames, "Alice").unwrap();
    p.set_mode(a, TaskMode::Manual);
    p.tasks[a].manual_start = Some(dt(1, 5, 8));
    schedule(&mut p);
    p.set_field_text(a, Field::Finish, "1/6/2026 5:00 PM").unwrap();
    schedule(&mut p);
    let _ = format!(
        "dur {} finish {:?} work {}",
        p.tasks[a].duration.format(&p.settings.durations),
        p.tasks[a].calc.finish,
        p.tasks[a].calc.work
    );
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 17)));
    assert_eq!(p.tasks[a].calc.work, 16 * 60, "Work = Duration × Units");
}

fn app_with(p: Project) -> (ProjectFcgApp, egui::Context) {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(p);
    app.prefs_path = None;
    (app, ctx)
}

// 5. cut + paste loses resource assignments
#[test]
fn cut_paste_loses_assignments() {
    let mut p = proj();
    add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    add(&mut p, "C", "2d");
    p.set_field_text(b, Field::ResourceNames, "Alice").unwrap();
    schedule(&mut p);
    let (mut app, ctx) = app_with(p);
    let ub = app.project.tasks[1].uid;
    app.select_single(RowRef::Task(ub));
    app.execute(Command::Cut, &ctx);
    let uc = app.project.tasks[1].uid; // C
    app.select_single(RowRef::Task(uc));
    app.execute(Command::Paste, &ctx);
    let names: Vec<_> = app.project.tasks.iter().map(|t| t.name.clone()).collect();
    let _ = format!("{names:?} res={:?} work={}", app.project.resource_names_text(1), app.project.tasks[1].calc.work);
    assert_eq!(app.project.resource_names_text(1), "Alice");
    assert_eq!(app.project.tasks[1].calc.work, 16 * 60);
    // and a copy keeps the original's
    app.execute(Command::Copy, &ctx);
    app.select_single(RowRef::Blank);
    app.execute(Command::Paste, &ctx);
    let n = app.project.tasks.len();
    assert_eq!(app.project.resource_names_text(n - 1), "Alice");
}

// 6. paste of a non-contiguous selection creates a summary->subtask link
#[test]
fn paste_creates_invalid_link() {
    let mut p = proj();
    let b = add(&mut p, "B", "1d"); // row 0, level 1
    add(&mut p, "S", "1d"); // row 1, summary
    let a = add(&mut p, "A", "1d"); // row 2
    p.indent(&[a]).unwrap();
    p.add_link(b, a, LinkType::FS, Lag::default()).unwrap();
    schedule(&mut p);
    assert!(p.calc.errors.is_empty());
    let (mut app, ctx) = app_with(p);
    let (u0, u2) = (app.project.tasks[0].uid, app.project.tasks[2].uid);
    app.selected = vec![u0, u2];
    app.active_row = RowRef::Task(u2);
    app.execute(Command::Copy, &ctx);
    app.select_single(RowRef::Blank);
    app.execute(Command::Paste, &ctx);
    let lv: Vec<_> = app.project.tasks.iter().map(|t| (t.name.clone(), t.outline_level)).collect();
    let _ = format!("{lv:?} errors={:?}", app.project.calc.errors);
    assert!(app.project.calc.errors.is_empty(), "{:?}", app.project.calc.errors);
}

// 7. Delete key in the Duration column deletes the whole task (README decision 5 says reset)
#[test]
fn delete_key_in_duration_column() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    add(&mut p, "B", "3d");
    let (mut app, ctx) = app_with(p);
    let ub = app.project.tasks[1].uid;
    app.select_single(RowRef::Task(ub));
    app.active_col = app.columns.iter().position(|c| c.id == ColumnId::Duration).unwrap();
    // what the Delete key maps to
    let cmd = projectfcg::app::commands::shortcut(egui::Key::Delete, egui::Modifiers::NONE).unwrap();
    app.execute(cmd, &ctx);
    let _ = format!("{} tasks left", app.project.tasks.len());
    assert_eq!(app.project.tasks.len(), 2, "Delete in Duration column deleted the task");
    assert_eq!(app.project.tasks[1].duration.minutes, 480, "it reset the field instead");
    // from the Task Name cell, or with the whole row selected, it deletes
    app.row_selected = true;
    app.execute(cmd, &ctx);
    assert_eq!(app.project.tasks.len(), 1);
}

// 8. Update Project > reschedule, "Selected tasks" ignores the selection
#[test]
fn reschedule_selected_only() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    add(&mut p, "B", "1d");
    let (mut app, ctx) = app_with(p);
    let ua = app.project.tasks[0].uid;
    app.select_single(RowRef::Task(ua));
    app.open_update_project();
    {
        let d = app.dialogs.update_project.as_mut().unwrap();
        d.reschedule = true;
        d.after = "1/12/2026 8:00 AM".into();
        d.selected_only = true;
    }
    // press Enter on the dialog
    let ev = |pressed| egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    for evs in [vec![], vec![ev(true)], vec![ev(false)], vec![]] {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1360.0, 820.0))), events: evs, ..Default::default() };
        let out = ctx.run_ui(raw, |ui| app.frame(ui));
        out.drop_without_applying_deltas();
    }
    let c: Vec<_> = app.project.tasks.iter().map(|t| (t.name.clone(), t.constraint)).collect();
    let _ = format!("{c:?} status={}", app.status);
    assert_eq!(app.project.tasks[1].constraint, projectfcg::model::ConstraintType::ASAP, "unselected task B was rescheduled");
}

#[test]
fn milestone_actual_duration_zero() {
    let mut p = proj();
    let m = add(&mut p, "M", "0d");
    schedule(&mut p);
    set_cell(&mut p, m, ColumnId::ActualDuration, "0").unwrap();
    let _ = format!("pct {}", p.tasks[m].percent_complete);
    assert_eq!(p.tasks[m].percent_complete, 0.0);
}

#[test]
fn fixed_work_zero_duration_wipes_work() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    p.tasks[a].task_type = projectfcg::model::TaskType::FixedWork;
    p.set_field_text(a, Field::ResourceNames, "Alice").unwrap();
    schedule(&mut p);
    assert!(p.set_field_text(a, Field::Duration, "0").is_err(), "a Fixed Work task needs time");
    p.set_field_text(a, Field::Duration, "5d").unwrap();
    schedule(&mut p);
    let _ = format!("work {}", p.tasks[a].calc.work);
    assert_eq!(p.tasks[a].calc.work, 2400);
}

#[test]
fn noop_edits_leave_undo_steps() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    let (mut app, ctx) = app_with(p);
    app.select_single(RowRef::Task(app.project.tasks[0].uid));
    app.active_col = app.columns.iter().position(|c| c.id == ColumnId::ResourceNames).unwrap();
    app.execute(Command::ResetField, &ctx);
    assert!(!app.history.can_undo());
    assert!(!app.dirty);
}

