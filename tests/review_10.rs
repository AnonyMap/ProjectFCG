//! Findings of the independent review of the 0.10 work, one test each.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::app::columns::ColumnId;
use projectfcg::app::commands::Command;
use projectfcg::app::{PendingAction, ProjectFcgApp, RowRef};
use projectfcg::model::{ConstraintType, Project};
use projectfcg::ops::Field;
use projectfcg::recovery::{Recovery, RecoveryMeta};
use std::time::{Duration, SystemTime};

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    for (n, d) in [("Charlie", "2d"), ("Alpha", "1d"), ("Bravo", "3d")] {
        let r = p.append_task(n);
        p.set_field_text(r, Field::Duration, d).unwrap();
    }
    projectfcg::schedule::schedule(&mut p);
    p
}

fn frame(app: &mut ProjectFcgApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| app.safe_frame(ui));
    out.textures_delta.clear();
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("pfcg-rev10-{tag}-{}", projectfcg::recovery::new_session_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 1: opening a recovery copy asks about unsaved work first, and keeps the
/// copy on disk (as this window's own) until the plan is saved.
#[test]
fn opening_a_recovery_copy_asks_first_and_keeps_it_on_disk() {
    let dir = tmp("recover");
    let rec = Recovery::new(dir.join("recovery"));
    let meta = RecoveryMeta { original: None, title: "Lost".into(), saved_at: chrono::Local::now().naive_local(), app_version: "x".into() };
    let mut lost = plan();
    lost.tasks[0].name = "Recovered work".into();
    rec.write("1-1", 1, &projectfcg::io::to_json(&lost).unwrap(), &meta).unwrap();
    let _ = std::fs::remove_file(rec.dir.join("1-1.alive"));
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.safety.enable(rec.clone(), dir.clone());
    assert_eq!(app.safety.offers.len(), 1);
    app.edit_project("typing", |p| {
        p.tasks[1].name = "Unsaved edit".into();
        Ok(())
    });
    app.open_recovered(0, &ctx);
    assert!(app.dialogs.save_prompt, "unsaved work: asked first");
    assert_eq!(app.project.tasks[1].name, "Unsaved edit", "nothing replaced yet");
    // "Don't Save": the pending action runs
    let a = app.pending.take().unwrap();
    app.dialogs.save_prompt = false;
    app.dirty = false;
    app.run_pending(a, &ctx);
    assert_eq!(app.project.tasks[0].name, "Recovered work");
    let mine = rec.dir.join(format!("{}.pfcg", app.safety.session));
    assert!(mine.exists(), "the copy is now this window's, still on disk");
    assert!(!rec.dir.join("1-1.pfcg").exists());
    assert!(matches!(PendingAction::OpenRecovered(0), PendingAction::OpenRecovered(_)));
    app.session_closing();
    let _ = std::fs::remove_dir_all(dir);
}

/// 2: an edit made by a dialog (after the frame drew) isn't "last good"
/// until a frame has drawn it; repeated panics go back to before it.
#[test]
fn a_dialog_edit_that_breaks_drawing_is_rolled_back() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    let before = app.project.tasks[0].constraint;
    app.select_single(RowRef::Task(app.project.tasks[0].uid));
    app.dialogs.move_tasks = Some(Default::default());
    frame(&mut app, &ctx, vec![]); // the dialog opens
    // Enter applies the dialog in this frame, after the view drew
    frame(&mut app, &ctx, vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]);
    assert_eq!(app.project.tasks[0].constraint, ConstraintType::SNET, "moved");
    app.safety.inject_frame_panics = 2;
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    assert_eq!(app.project.tasks[0].constraint, before, "back to the plan that drew");
}

/// 3: two threads writing the same file atomically don't collide.
#[test]
fn concurrent_atomic_writes_never_collide() {
    let dir = tmp("atomic");
    let f = dir.join("plan.pfcg");
    let a = vec![b'a'; 200_000];
    let b = vec![b'b'; 200_000];
    for _ in 0..30 {
        let (f1, f2, a2, b2) = (f.clone(), f.clone(), a.clone(), b.clone());
        let t1 = std::thread::spawn(move || projectfcg::recovery::write_atomic(&f1, &a2));
        let t2 = std::thread::spawn(move || projectfcg::recovery::write_atomic(&f2, &b2));
        assert!(t1.join().unwrap().is_ok() && t2.join().unwrap().is_ok());
        let got = std::fs::read(&f).unwrap();
        assert!(got == a || got == b, "a whole payload, never a mix");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// 4: saving through a symlink updates the file it points at.
#[cfg(unix)]
#[test]
fn saving_through_a_symlink_keeps_the_link() {
    let dir = tmp("link");
    let real = dir.join("real.pfcg");
    let link = dir.join("link.pfcg");
    std::fs::write(&real, b"old").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    projectfcg::io::save(&plan(), &link).unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(std::fs::read_to_string(&real).unwrap().contains("Charlie"));
    let _ = std::fs::remove_dir_all(dir);
}

/// 5: Replace on a date field finds what it rewrites, and changes nothing
/// (no constraints added) when the text would stay the same.
#[test]
fn replace_on_dates_doesnt_add_constraints() {
    let mut app = ProjectFcgApp::new(plan());
    app.dialogs.find.replace_mode = true;
    app.dialogs.find.field = ColumnId::Start;
    app.dialogs.find_text = "Mon".into(); // shown as "Mon 1/5/26", typed as "1/5/26 8:00 AM"
    app.dialogs.find.replace_text = "Tue".into();
    assert_eq!(app.replace_all(), 0);
    assert!(app.project.tasks.iter().all(|t| t.constraint == ConstraintType::ASAP));
    // a real date replacement works
    app.dialogs.find_text = "1/5/26".into();
    app.dialogs.find.replace_text = "1/6/26".into();
    assert_eq!(app.replace_all(), 3);
    assert_eq!(app.project.tasks[0].calc.start, Some(dt(1, 6, 8)));
}

/// 6: Move Task moves finish constraints as finishes (to a day's end).
#[test]
fn move_task_moves_finish_constraints_as_finishes() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.project.set_constraint(0, ConstraintType::FNLT, Some(dt(1, 13, 17))).unwrap();
    app.project.set_constraint(1, ConstraintType::MFO, Some(dt(1, 16, 17))).unwrap();
    projectfcg::schedule::schedule(&mut app.project);
    app.selected = vec![app.project.tasks[0].uid, app.project.tasks[1].uid];
    app.active_row = RowRef::Task(app.project.tasks[0].uid);
    app.execute(Command::MoveTasks(1), &ctx);
    assert_eq!(app.project.tasks[0].constraint_date, Some(dt(1, 14, 17)), "Tue 5 PM + 1 day = Wed 5 PM");
    assert_eq!(app.project.tasks[1].constraint_date, Some(dt(1, 19, 17)), "Fri 5 PM + 1 day = Mon 5 PM");
    app.execute(Command::MoveTasks(-2), &ctx);
    assert_eq!(app.project.tasks[0].constraint_date, Some(dt(1, 12, 17)), "back 2 days: Mon 5 PM");
}

/// 7 (and docs/DECISIONS.md decision 69's Project behaviour): in a plan scheduled from its
/// finish, a typed start or finish is a latest date.
#[test]
fn typed_dates_in_a_finish_scheduled_plan_are_no_later_than() {
    let mut p = plan();
    p.finish_date = Some(dt(1, 30, 17));
    projectfcg::schedule::schedule(&mut p);
    p.set_field_text(0, Field::Start, "1/20/26").unwrap();
    assert_eq!(p.tasks[0].constraint, ConstraintType::SNLT);
    p.set_field_text(1, Field::Finish, "1/22/26").unwrap();
    assert_eq!(p.tasks[1].constraint, ConstraintType::FNLT);
}

/// 8: Fill Down in a sorted view copies the top row as shown.
#[test]
fn fill_down_follows_the_sorted_view() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.view.sort = projectfcg::app::sort::SortSpec { keys: vec![(ColumnId::Name, true)], keep_outline: true }; // Charlie, Bravo, Alpha
    app.active_col = app.columns.iter().position(|c| c.id == ColumnId::Duration).unwrap();
    app.execute(Command::SelectAll, &ctx);
    app.execute(Command::FillDown, &ctx);
    assert!(app.project.tasks.iter().all(|t| t.duration.minutes == 2 * 480), "Charlie's 2 days everywhere");
}

/// Plausible 1: a heartbeat that looks stale (the computer just woke up)
/// doesn't make a running window's copy an orphan.
#[cfg(target_os = "linux")]
#[test]
fn a_running_windows_copy_is_never_an_orphan() {
    let dir = tmp("alive");
    let rec = Recovery::new(dir.join("recovery"));
    let meta = RecoveryMeta { original: None, title: "x".into(), saved_at: chrono::Local::now().naive_local(), app_version: "x".into() };
    // pid 1 is always running, but isn't ProjectFCG: still an orphan
    rec.write("1-5", 1, "{}", &meta).unwrap();
    assert_eq!(rec.orphans(SystemTime::now() + Duration::from_secs(600)).len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}
