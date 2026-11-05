//! Projects scheduled from their finish date (Project Information ▸
//! Schedule from: Project Finish Date).
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::model::{ConstraintType, Project};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Design 3d → Build 2d, plus a loose Docs 1d; must finish Fri Jan 16, 5 PM.
fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    p.finish_date = Some(dt(1, 16, 17));
    for (name, dur) in [("Design", "3d"), ("Build", "2d"), ("Docs", "1d")] {
        let r = p.append_task(name);
        p.set_field_text(r, Field::Duration, dur).unwrap();
    }
    p.set_field_text(1, Field::Predecessors, "1").unwrap();
    schedule(&mut p);
    p
}

#[test]
fn tasks_hang_from_the_finish_date_and_the_start_is_worked_out() {
    let p = plan();
    assert!(p.tasks.iter().all(|t| t.constraint == ConstraintType::ALAP), "new tasks are As Late As Possible");
    assert_eq!((p.tasks[1].calc.start, p.tasks[1].calc.finish), (Some(dt(1, 15, 8)), Some(dt(1, 16, 17))));
    assert_eq!((p.tasks[0].calc.start, p.tasks[0].calc.finish), (Some(dt(1, 12, 8)), Some(dt(1, 14, 17))));
    assert_eq!(p.tasks[2].calc.start, Some(dt(1, 16, 8)), "Docs as late as possible too");
    assert_eq!(p.start, dt(1, 12, 8), "the project starts with its first task");
    assert_eq!(p.calc.finish, Some(dt(1, 16, 17)));
    assert!(p.tasks[0].calc.critical && p.tasks[1].calc.critical);
    assert_eq!(p.tasks[0].calc.total_slack, 0);
}

#[test]
fn as_soon_as_possible_tasks_start_with_the_project() {
    let mut p = plan();
    p.set_constraint(2, ConstraintType::ASAP, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[2].calc.start, Some(dt(1, 12, 8)));
    assert!(p.tasks[2].calc.total_slack > 0);
    // a later finish moves everything with it
    p.finish_date = Some(dt(1, 23, 17));
    schedule(&mut p);
    assert_eq!(p.start, dt(1, 19, 8));
    assert_eq!(p.tasks[1].calc.finish, Some(dt(1, 23, 17)));
}

#[test]
fn a_task_that_cant_make_the_finish_date_shows_negative_slack() {
    let mut p = plan();
    p.set_constraint(0, ConstraintType::SNET, Some(dt(1, 14, 8))).unwrap();
    schedule(&mut p);
    // Design Wed–Fri, Build Mon–Tue: past the finish date
    assert_eq!(p.tasks[1].calc.finish, Some(dt(1, 20, 17)));
    assert!(p.tasks[1].calc.total_slack < 0, "{}", p.tasks[1].calc.total_slack);
    assert_eq!(p.calc.finish, Some(dt(1, 20, 17)));
}

#[test]
fn files_project_xml_and_undo() {
    let p = plan();
    let q = projectfcg::io::from_json(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    assert_eq!((q.finish_date, q.start), (p.finish_date, p.start));
    let xml = projectfcg::mspdi::to_xml(&p);
    assert!(xml.contains("<ScheduleFromStart>0</ScheduleFromStart>"));
    assert!(xml.contains("<FinishDate>2026-01-16T17:00:00</FinishDate>"));
    let (r, warnings) = projectfcg::mspdi::from_xml(&xml).unwrap();
    assert!(warnings.iter().all(|w| !w.contains("finish date")), "{warnings:?}");
    assert_eq!(r.finish_date, Some(dt(1, 16, 17)));
    assert_eq!(r.tasks[1].calc.finish, Some(dt(1, 16, 17)));
    // switching back to scheduling from the start, and undoing it
    let mut app = projectfcg::app::ProjectFcgApp::new(p);
    app.edit_project("Project Information", |p| {
        p.finish_date = None;
        p.start = dt(1, 5, 8);
        Ok(())
    });
    assert_eq!(app.project.tasks[0].calc.start, Some(dt(1, 5, 8)), "ALAP from a start-scheduled plan: still on the critical path");
    app.execute(projectfcg::app::commands::Command::Undo, &egui::Context::default());
    assert_eq!(app.project.finish_date, Some(dt(1, 16, 17)));
    assert_eq!(app.project.tasks[1].calc.finish, Some(dt(1, 16, 17)));
}
