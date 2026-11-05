//! Large plans level in the background: the window keeps drawing, the
//! result is the same as leveling in place, and Cancel changes nothing.
mod common;
use common::{random_project, Shape};
use projectfcg::app::commands::Command;
use projectfcg::app::ProjectFcgApp;
use projectfcg::model::ConstraintType;
use std::time::{Duration, Instant};

fn frame(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| app.frame(ui));
}

fn big_plan() -> projectfcg::model::Project {
    let mut p = random_project(11, &Shape { tasks: 320, resources: 8, ..Default::default() });
    // make sure it has ALAP tasks
    let leaf = p.tasks.iter().position(|t| !t.calc.is_summary).unwrap();
    p.tasks[leaf].constraint = ConstraintType::ALAP;
    projectfcg::schedule::schedule(&mut p);
    p
}

#[test]
fn leveling_a_large_plan_runs_in_the_background_with_the_same_result() {
    let p = big_plan();
    let mut reference = p.clone();
    let expected = projectfcg::leveling::level(&mut reference, None);
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(p);
    assert!(app.level_in_background());
    app.execute(Command::LevelAll, &ctx);
    assert!(app.leveling_job.is_some(), "started, not run on the UI thread");
    // while it runs, nothing else may change the plan
    let before = app.project.clone();
    app.execute(Command::InsertTask, &ctx);
    assert_eq!(app.project, before);
    let t = Instant::now();
    while app.leveling_job.is_some() {
        frame(&mut app, &ctx);
        assert!(t.elapsed() < Duration::from_secs(60), "leveling never finished");
        std::thread::sleep(Duration::from_millis(5));
    }
    let delays = |p: &projectfcg::model::Project| p.tasks.iter().map(|t| t.leveling_delay).collect::<Vec<_>>();
    assert_eq!(delays(&app.project), delays(&reference));
    assert!(expected.delayed.len() > 0, "the plan needed leveling");
    // one undo step takes it all back
    app.execute(Command::Undo, &ctx);
    assert!(app.project.tasks.iter().all(|t| t.leveling_delay == 0));
}

#[test]
fn cancelling_leveling_changes_nothing() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(big_plan());
    let before = app.project.clone();
    app.execute(Command::LevelAll, &ctx);
    app.cancel_level_job();
    let t = Instant::now();
    while app.leveling_job.is_some() {
        frame(&mut app, &ctx);
        assert!(t.elapsed() < Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(app.project, before);
    assert!(!app.history.can_undo());
    assert!(app.status.contains("cancelled"), "{}", app.status);
}
