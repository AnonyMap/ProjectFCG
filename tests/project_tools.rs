//! Project's everyday tools: Sort, Find / Replace, Move Task, and
//! Physical % Complete for earned value.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::app::columns::ColumnId;
use projectfcg::app::commands::Command;
use projectfcg::app::sort::{SortPreset, SortSpec};
use projectfcg::app::{DisplayRow, ProjectFcgApp, RowRef};
use projectfcg::model::{ConstraintType, EvMethod, Project, TaskMode};
use projectfcg::ops::Field;
use projectfcg::views::Test;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Phase A (Write 3d, Review 1d), Phase B (Build 5d, Ship 2d), Docs 4d.
fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    for (name, lvl, dur) in [("Phase A", 1, ""), ("Write", 2, "3d"), ("Review", 2, "1d"), ("Phase B", 1, ""), ("Build", 2, "5d"), ("Ship", 2, "2d"), ("Docs", 1, "4d")] {
        let r = p.append_task(name);
        p.tasks[r].outline_level = lvl;
        if !dur.is_empty() {
            p.set_field_text(r, Field::Duration, dur).unwrap();
        }
    }
    projectfcg::schedule::refresh_structure(&mut p);
    p.set_field_text(2, Field::Predecessors, "2").unwrap(); // Review after Write
    p.set_field_text(5, Field::Predecessors, "5").unwrap(); // Ship after Build
    projectfcg::schedule::schedule(&mut p);
    p
}

fn names(app: &ProjectFcgApp) -> Vec<String> {
    app.display_rows().iter().filter_map(|r| if let DisplayRow::Task(i) = r { Some(app.project.tasks[*i].name.clone()) } else { None }).collect()
}

#[test]
fn sort_keeps_the_outline_and_ids() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    app.view.sort = SortSpec { keys: vec![(ColumnId::Duration, true)], keep_outline: true };
    // longest first, subtasks under their own summary
    assert_eq!(names(&app), ["Phase B", "Build", "Ship", "Phase A", "Write", "Review", "Docs"]);
    // flat: no outline
    app.view.sort.keep_outline = false;
    // summaries are rows too: Phase B (7d), Build (5d), then the 4-day ties in ID order
    assert_eq!(names(&app)[..4], ["Phase B", "Build", "Phase A", "Docs"]);
    // the built-in sorts; by ID puts it back
    app.execute(Command::SortBy(SortPreset::FinishDate), &ctx);
    let n = names(&app);
    assert!(n.iter().position(|x| x == "Phase A") < n.iter().position(|x| x == "Phase B"));
    app.execute(Command::SortBy(SortPreset::Id), &ctx);
    assert_eq!(names(&app), ["Phase A", "Write", "Review", "Phase B", "Build", "Ship", "Docs"]);
    assert_eq!(app.project.tasks[0].name, "Phase A", "the tasks themselves didn't move");
    // ties keep ID order; empty values last
    app.view.sort = SortSpec { keys: vec![(ColumnId::Deadline, false)], keep_outline: false };
    assert_eq!(names(&app)[0], "Phase A");
}

#[test]
fn renumbering_moves_whole_summaries() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    let uid_build = app.project.tasks[4].uid;
    app.execute(Command::SortDlg, &ctx);
    let mut d = app.dialogs.sort.take().unwrap();
    d.keys[0] = (Some(ColumnId::Name), true);
    d.renumber = true;
    app.apply_sort(&d);
    let ids: Vec<&str> = app.project.tasks.iter().map(|t| t.name.as_str()).collect();
    // Z→A among top-level tasks (Phase B, Phase A, Docs), and within each
    assert_eq!(ids, ["Phase B", "Ship", "Build", "Phase A", "Write", "Review", "Docs"]);
    assert!(app.view.sort.is_by_id());
    // links and schedule survive: Ship still follows Build
    let ship = app.project.tasks.iter().position(|t| t.name == "Ship").unwrap();
    assert_eq!(app.project.tasks[ship].predecessors[0].pred, uid_build);
    assert_eq!(app.project.tasks[ship].calc.start, Some(dt(1, 12, 8)), "after Build (Mon 5 – Fri 9)");
    // one undo step
    app.execute(Command::Undo, &ctx);
    assert_eq!(app.project.tasks[1].name, "Write");
}

#[test]
fn find_looks_in_any_field_both_ways() {
    let mut app = ProjectFcgApp::new(plan());
    app.select_single(RowRef::Task(app.project.tasks[0].uid));
    app.dialogs.find_text = "R".into();
    app.find_next();
    assert_eq!(app.active_index(), Some(1), "Write has an r (case-insensitive)");
    app.dialogs.find.match_case = true;
    app.find_next();
    assert_eq!(app.active_index(), Some(2), "only Review has a capital R");
    app.find_next();
    assert_eq!(app.active_index(), Some(2), "and it wraps back to it");
    app.dialogs.find.match_case = false;
    // durations: is greater than 2 days, searching up from Docs
    app.dialogs.find.field = ColumnId::Duration;
    app.dialogs.find.test = Test::Greater;
    app.dialogs.find_text = "2d".into();
    app.dialogs.find.up = true;
    app.select_single(RowRef::Task(app.project.tasks[6].uid));
    app.find_next();
    assert_eq!(app.project.tasks[app.active_index().unwrap()].name, "Build", "Ship is only 2d");
    // a bad value for the field is reported, not "not found"
    app.dialogs.find_text = "soon".into();
    app.find_next();
    assert!(app.status_error, "{}", app.status);
    // collapsed summaries open to show a match
    app.project.tasks[3].collapsed = true;
    app.dialogs.find = Default::default();
    app.dialogs.find_text = "ship".into();
    app.find_next();
    assert!(!app.project.tasks[3].collapsed);
    assert_eq!(app.active_index(), Some(5));
}

#[test]
fn replace_one_and_all_in_one_undo_step() {
    let ctx = egui::Context::default();
    let mut p = plan();
    p.set_field_text(1, Field::ResourceNames, "Bob").unwrap();
    p.set_field_text(4, Field::ResourceNames, "Bob").unwrap();
    projectfcg::schedule::schedule(&mut p);
    let mut app = ProjectFcgApp::new(p);
    // replace in names, case-insensitively, keeping the rest
    app.dialogs.find_text = "phase".into();
    app.dialogs.find.replace_text = "Stage".into();
    assert_eq!(app.replace_all(), 2);
    assert_eq!(app.project.tasks[0].name, "Stage A");
    assert_eq!(app.project.tasks[3].name, "Stage B");
    app.execute(Command::Undo, &ctx);
    assert_eq!(app.project.tasks[3].name, "Phase B");
    // replacing in Resource Names reassigns the work
    app.dialogs.find.field = ColumnId::ResourceNames;
    app.dialogs.find_text = "bob".into();
    app.dialogs.find.replace_text = "Alice".into();
    app.select_single(RowRef::Task(app.project.tasks[0].uid));
    app.replace_one(); // Phase A doesn't match: finds Write
    assert_eq!(app.active_index(), Some(1));
    app.replace_one();
    assert_eq!(app.project.resource_names_text(1), "Alice");
    assert_eq!(app.project.resource_names_text(4), "Bob", "only one replaced");
    assert_eq!(app.active_index(), Some(4), "and moved on to the next");
    // read-only fields can't be replaced in
    app.dialogs.find.field = ColumnId::TotalSlack;
    assert_eq!(app.replace_all(), 0);
    assert!(app.status_error);
    // a replacement that's invalid for one task changes nothing at all
    app.dialogs.find.field = ColumnId::Duration;
    app.dialogs.find.test = Test::Contains;
    app.dialogs.find_text = "d".into();
    app.dialogs.find.replace_text = "x".into();
    let before = app.project.clone();
    assert_eq!(app.replace_all(), 0);
    assert_eq!(app.project, before);
    assert_eq!(projectfcg::app::find::replace_text("Ölçek ölçek", "ÖLÇ", "M", false), "Mek Mek");
}

#[test]
fn move_task_sets_start_no_earlier_than() {
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(plan());
    // auto task: forward a week → SNET a week later
    app.select_single(RowRef::Task(app.project.tasks[6].uid)); // Docs, Mon Jan 5
    app.execute(Command::MoveTasks(5), &ctx);
    let docs = &app.project.tasks[6];
    assert_eq!((docs.constraint, docs.constraint_date), (ConstraintType::SNET, Some(dt(1, 12, 8))));
    assert_eq!(docs.calc.start, Some(dt(1, 12, 8)));
    // and back a day: Fri Jan 9
    app.execute(Command::MoveTasks(-1), &ctx);
    assert_eq!(app.project.tasks[6].calc.start, Some(dt(1, 9, 8)));
    // a summary moves its subtasks; Review follows Write via its link
    app.select_single(RowRef::Task(app.project.tasks[0].uid));
    app.execute(Command::MoveTasks(1), &ctx);
    assert_eq!(app.project.tasks[1].calc.start, Some(dt(1, 6, 8)));
    assert_eq!(app.project.tasks[2].constraint, ConstraintType::SNET);
    // manual tasks just move; Must Start On keeps its kind
    app.project.tasks[4].mode = TaskMode::Manual;
    app.project.tasks[4].manual_start = Some(dt(1, 5, 8));
    app.project.set_constraint(5, ConstraintType::MSO, Some(dt(1, 19, 8))).unwrap();
    projectfcg::schedule::schedule(&mut app.project);
    app.selected = vec![app.project.tasks[4].uid, app.project.tasks[5].uid];
    app.execute(Command::MoveTasks(2), &ctx);
    assert_eq!(app.project.tasks[4].manual_start, Some(dt(1, 7, 8)));
    assert_eq!(app.project.tasks[5].constraint, ConstraintType::MSO);
    assert_eq!(app.project.tasks[5].constraint_date, Some(dt(1, 21, 8)));
    // started tasks stay
    app.project.set_actual_start(6, Some(dt(1, 9, 8))).unwrap();
    app.select_single(RowRef::Task(app.project.tasks[6].uid));
    let before = app.project.clone();
    app.execute(Command::MoveTasks(1), &ctx);
    assert_eq!(app.project, before);
    assert!(app.status_error);
}

#[test]
fn physical_percent_complete_drives_earned_value() {
    let mut p = plan();
    p.set_field_text(6, Field::ResourceNames, "Writer").unwrap();
    p.resources[0].std_rate = 100.0;
    projectfcg::schedule::schedule(&mut p);
    p.set_baseline(None);
    p.set_percent_complete(6, 50.0).unwrap();
    projectfcg::app::columns::set_cell(&mut p, 6, ColumnId::PhysicalPercentComplete, "25%").unwrap();
    projectfcg::schedule::schedule(&mut p);
    let bac = p.tasks[6].calc.bac;
    assert!(bac > 0.0);
    assert!((p.tasks[6].calc.bcwp - bac * 0.5).abs() < 0.01, "% Complete by default");
    projectfcg::app::columns::set_cell(&mut p, 6, ColumnId::EarnedValueMethod, "Physical % Complete").unwrap();
    projectfcg::schedule::schedule(&mut p);
    assert!((p.tasks[6].calc.bcwp - bac * 0.25).abs() < 0.01);
    assert_eq!(projectfcg::app::columns::cell_text(&p, 6, ColumnId::PhysicalPercentComplete), "25%");
    assert!(projectfcg::app::columns::set_cell(&mut p, 6, ColumnId::PhysicalPercentComplete, "120").is_err());
    assert!(projectfcg::app::columns::set_cell(&mut p, 0, ColumnId::PhysicalPercentComplete, "10").is_err(), "not on summaries");
    // new tasks take the project's default method
    p.settings.default_ev_method = EvMethod::PhysicalPercentComplete;
    let r = p.append_task("New");
    assert_eq!(p.tasks[r].ev_method, EvMethod::PhysicalPercentComplete);
    // Project XML
    let xml = projectfcg::mspdi::to_xml(&p);
    assert!(xml.contains("<PhysicalPercentComplete>25</PhysicalPercentComplete>"));
    assert!(xml.contains("<DefaultTaskEVMethod>1</DefaultTaskEVMethod>"));
    let (q, _) = projectfcg::mspdi::from_xml(&xml).unwrap();
    let d = q.tasks.iter().find(|t| t.name == "Docs").unwrap();
    assert_eq!((d.physical_percent, d.ev_method), (25.0, EvMethod::PhysicalPercentComplete));
    assert_eq!(q.settings.default_ev_method, EvMethod::PhysicalPercentComplete);
}
