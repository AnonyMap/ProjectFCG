//! Stability: panics in edits and frames don't close the window or lose
//! work; AutoRecover copies unsaved work and offers it back after a crash.
use chrono::NaiveDate;
use projectfcg::app::safety::GIVE_UP_AFTER;
use projectfcg::app::{MainView, ProjectFcgApp};
use projectfcg::model::Project;
use projectfcg::recovery::Recovery;
use std::time::{Duration, Instant, SystemTime};

fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    for n in ["Design", "Build", "Test"] {
        p.append_task(n);
    }
    p
}

fn frame(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| app.safe_frame(ui));
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("pfcg-safety-{tag}-{}", projectfcg::recovery::new_session_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn an_edit_that_panics_is_undone_and_reported() {
    let mut app = ProjectFcgApp::new(plan());
    let dir = tmp("edit");
    app.safety.state_dir = Some(dir.clone());
    let before = app.project.clone();
    let r: Option<()> = app.edit_project("Rename", |p| {
        p.tasks[0].name = "half-done".into();
        panic!("a bug in an edit");
    });
    assert!(r.is_none());
    assert_eq!(app.project, before, "the plan is as it was");
    assert!(!app.history.can_undo(), "no undo step for a failed edit");
    assert!(app.status_error && app.status.contains("Rename"), "{}", app.status);
    let reports: Vec<_> = std::fs::read_dir(dir.join("crash-reports")).unwrap().flatten().collect();
    assert_eq!(reports.len(), 1);
    let text = std::fs::read_to_string(reports[0].path()).unwrap();
    assert!(text.contains("a bug in an edit") && text.contains("Rename"), "{text}");
    assert!(!text.contains("Design"), "reports never contain the plan");
    // and the app keeps working
    app.edit_project("Rename", |p| {
        p.tasks[0].name = "Design 2".into();
        Ok(())
    });
    assert_eq!(app.project.tasks[0].name, "Design 2");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_frame_that_panics_closes_dialogs_and_keeps_running() {
    let mut app = ProjectFcgApp::new(plan());
    let ctx = egui::Context::default();
    frame(&mut app, &ctx);
    app.dialogs.open_options(&app.project);
    app.safety.inject_frame_panics = 1;
    frame(&mut app, &ctx);
    assert!(app.dialogs.options.is_none(), "what was open is closed");
    assert!(app.safety.notice.is_some());
    assert_eq!(app.project.tasks.len(), 3);
    frame(&mut app, &ctx);
    assert_eq!(app.safety.consecutive_panics, 0, "the next frame is fine");
}

#[test]
fn repeated_panics_go_back_to_the_last_plan_that_drew() {
    let mut app = ProjectFcgApp::new(plan());
    let ctx = egui::Context::default();
    app.view.main = MainView::ResourceSheet;
    frame(&mut app, &ctx);
    // something left the plan in a state that keeps failing
    app.project.tasks[1].name = "poison".into();
    app.safety.inject_frame_panics = 2;
    frame(&mut app, &ctx);
    assert_eq!(app.project.tasks[1].name, "poison", "one panic: only transient state is dropped");
    frame(&mut app, &ctx);
    assert_eq!(app.project.tasks[1].name, "Build", "again: the last plan that drew");
    assert_eq!(app.view.main, MainView::Gantt);
    assert!(!app.history.can_undo());
    // if nothing helps, it gives up rather than spinning forever
    app.safety.inject_frame_panics = GIVE_UP_AFTER;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..GIVE_UP_AFTER {
            frame(&mut app, &ctx);
        }
    }));
    assert!(r.is_err());
}

#[test]
fn autorecover_copies_unsaved_work_and_offers_it_after_a_crash() {
    let dir = tmp("autorecover");
    let rec = Recovery::new(dir.join("recovery"));
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.safety.enable(rec.clone(), dir.clone());
    app.safety.interval_override = Some(Duration::ZERO);
    let session = app.safety.session.clone();
    let copy = rec.dir.join(format!("{session}.pfcg"));
    frame(&mut app, &ctx);
    assert!(!copy.exists(), "nothing unsaved yet");
    app.edit_project("Rename", |p| {
        p.tasks[0].name = "Unsaved idea".into();
        Ok(())
    });
    frame(&mut app, &ctx);
    assert!(wait_for(|| std::fs::read_to_string(&copy).is_ok_and(|s| s.contains("Unsaved idea"))), "copied in the background");
    // saving makes the copy unnecessary
    let saved = dir.join("plan.pfcg");
    assert!(app.save_to(saved.clone()));
    frame(&mut app, &ctx);
    assert!(wait_for(|| !copy.exists()), "removed once saved");
    // more work, then the window dies without closing
    app.edit_project("Rename", |p| {
        p.tasks[1].name = "Lost?".into();
        Ok(())
    });
    frame(&mut app, &ctx);
    assert!(wait_for(|| std::fs::read_to_string(&copy).is_ok_and(|s| s.contains("Lost?"))));
    // a window that's still running is never offered as lost
    assert!(rec.orphans(SystemTime::now()).is_empty());
    let later = SystemTime::now() + Duration::from_secs(600);
    let orphans = rec.orphans(later);
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].meta.original.as_deref(), Some(saved.as_path()));
    assert_eq!(orphans[0].meta.title, "plan.pfcg");
    // the next start opens it as a new, unsaved plan
    let mut next = ProjectFcgApp::new(plan());
    next.safety.recovery = Some(rec.clone());
    next.safety.offers = orphans;
    next.open_recovered(0);
    assert_eq!(next.project.tasks[1].name, "Lost?");
    assert!(next.dirty && next.path.is_none(), "saving it never overwrites without asking");
    assert!(!copy.exists() && rec.orphans(later).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn closing_normally_leaves_nothing_behind() {
    let dir = tmp("close");
    let rec = Recovery::new(dir.join("recovery"));
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.safety.enable(rec.clone(), dir.clone());
    app.safety.interval_override = Some(Duration::ZERO);
    app.edit_project("Rename", |p| {
        p.tasks[0].name = "x".into();
        Ok(())
    });
    frame(&mut app, &ctx);
    let copy = rec.dir.join(format!("{}.pfcg", app.safety.session));
    assert!(wait_for(|| copy.exists()));
    app.session_closing(); // "Don't Save", then the window closes
    assert!(!copy.exists());
    std::thread::sleep(Duration::from_millis(100));
    assert!(rec.orphans(SystemTime::now() + Duration::from_secs(600)).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
