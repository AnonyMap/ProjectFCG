//! Critical path analysis, start drivers, task paths, status and baselines.

use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::analysis::{critical_tasks, task_path};
use projectfcg::duration::Lag;
use projectfcg::model::{ConstraintType, LinkType, Project, StartDriver, TaskMode, TaskStatus};
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
fn fs(p: &mut Project, a: usize, b: usize) {
    p.add_link(a, b, LinkType::FS, Lag::default()).unwrap();
}

#[test]
fn start_drivers() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "2d");
    let d = add(&mut p, "D", "1d");
    let m = add(&mut p, "M", "1d");
    fs(&mut p, a, b);
    fs(&mut p, c, b);
    p.set_constraint(d, ConstraintType::SNET, Some(dt(1, 7, 8))).unwrap();
    p.set_mode(m, TaskMode::Manual);
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.start_driver, StartDriver::ProjectStart);
    assert_eq!(p.tasks[b].calc.start_driver, StartDriver::Predecessors);
    // only A (5d) drives B; C (2d) has slack
    assert_eq!(p.tasks[b].calc.driving_links, vec![(p.tasks[a].uid, LinkType::FS)]);
    assert_eq!(p.tasks[b].calc.driving_preds, vec![p.tasks[a].uid]);
    assert_eq!(p.tasks[d].calc.start_driver, StartDriver::Constraint);
    assert_eq!(p.tasks[m].calc.start_driver, StartDriver::ManualDates);
}

#[test]
fn constraint_earlier_than_links_does_not_drive() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    fs(&mut p, a, b);
    p.set_constraint(b, ConstraintType::SNET, Some(dt(1, 6, 8))).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start_driver, StartDriver::Predecessors);
}

#[test]
fn parent_constraint_and_alap_drivers() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    p.indent(&[a]).unwrap();
    p.set_constraint(s, ConstraintType::SNET, Some(dt(1, 8, 8))).unwrap();
    let x = add(&mut p, "X", "5d");
    let y = add(&mut p, "Y", "1d");
    p.outdent(&[x, y]).unwrap();
    p.set_constraint(y, ConstraintType::ALAP, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 8, 8)));
    assert_eq!(p.tasks[a].calc.start_driver, StartDriver::ParentConstraint);
    assert_eq!(p.tasks[y].calc.start_driver, StartDriver::AsLateAsPossible);
}

#[test]
fn summary_predecessor_drives_through_latest_child() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.indent(&[a, b]).unwrap();
    let d = add(&mut p, "D", "1d");
    p.outdent(&[d]).unwrap();
    fs(&mut p, s, d);
    schedule(&mut p);
    assert_eq!(p.tasks[d].calc.driving_links, vec![(p.tasks[s].uid, LinkType::FS)]);
    assert_eq!(p.tasks[d].calc.driving_preds, vec![p.tasks[b].uid]);
}

#[test]
fn multiple_critical_paths() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "2d");
    let d = add(&mut p, "D", "1d");
    fs(&mut p, a, b);
    fs(&mut p, c, d);
    schedule(&mut p);
    assert!(!p.tasks[c].calc.critical);
    assert_eq!(p.tasks[c].calc.total_slack, 3 * 480);
    p.settings.multiple_critical_paths = true;
    schedule(&mut p);
    assert!(p.tasks[c].calc.critical, "second chain is its own critical path");
    assert!(p.tasks[d].calc.critical);
    assert!(p.tasks[a].calc.critical);
    assert_eq!(critical_tasks(&p), vec![a, c, d, b]);
}

#[test]
fn start_and_finish_slack() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    let c = add(&mut p, "C", "1d");
    fs(&mut p, a, c);
    fs(&mut p, b, c);
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start_slack, 3 * 480);
    assert_eq!(p.tasks[b].calc.finish_slack, 3 * 480);
    assert_eq!(p.tasks[a].calc.start_slack, 0);
}

#[test]
fn task_path_sets() {
    // A(5d) → B(1d) → C(1d) ; D(2d) → C
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "1d");
    let d = add(&mut p, "D", "2d");
    let e = add(&mut p, "E", "1d");
    fs(&mut p, a, b);
    fs(&mut p, b, c);
    fs(&mut p, d, c);
    fs(&mut p, c, e);
    schedule(&mut p);
    let tp = task_path(&p, &[c]);
    assert_eq!(tp.predecessors.iter().copied().collect::<Vec<_>>(), vec![a, b, d]);
    assert_eq!(tp.driving_predecessors.iter().copied().collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(tp.successors.iter().copied().collect::<Vec<_>>(), vec![e]);
    assert_eq!(tp.driven_successors.iter().copied().collect::<Vec<_>>(), vec![e]);
    let tp = task_path(&p, &[d]);
    assert!(tp.driven_successors.is_empty(), "D has slack, it drives nothing");
    assert_eq!(tp.successors.iter().copied().collect::<Vec<_>>(), vec![c, e]);
}

#[test]
fn task_path_through_summary_links() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "2d");
    p.indent(&[a]).unwrap();
    let x = add(&mut p, "X", "1d");
    p.outdent(&[x]).unwrap();
    fs(&mut p, s, x);
    schedule(&mut p);
    let tp = task_path(&p, &[x]);
    assert!(tp.predecessors.contains(&a));
    assert!(tp.driving_predecessors.contains(&a));
}

#[test]
fn status_and_late() {
    let mut p = proj();
    // 10-day task Mon 1/5 – Fri 1/16; status date end of Fri 1/9 = 50% expected
    let a = add(&mut p, "On schedule", "10d");
    let b = add(&mut p, "Behind", "10d");
    let c = add(&mut p, "Future", "1d");
    let d = add(&mut p, "Done", "1d");
    p.set_constraint(c, ConstraintType::SNET, Some(dt(1, 20, 8))).unwrap();
    p.tasks[a].percent_complete = 50.0;
    p.tasks[b].percent_complete = 20.0;
    p.tasks[d].percent_complete = 100.0;
    p.status_date = Some(dt(1, 9, 17));
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.status, TaskStatus::OnSchedule);
    assert_eq!(p.tasks[b].calc.status, TaskStatus::Late);
    assert!(p.tasks[b].calc.late);
    assert_eq!(p.tasks[c].calc.status, TaskStatus::FutureTask);
    assert_eq!(p.tasks[d].calc.status, TaskStatus::Complete);
}

#[test]
fn summary_status_rolls_up() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "4d");
    let b = add(&mut p, "B", "4d");
    p.indent(&[a, b]).unwrap();
    p.status_date = Some(dt(1, 7, 17));
    schedule(&mut p);
    assert_eq!(p.tasks[s].calc.status, TaskStatus::Late);
    p.tasks[a].percent_complete = 100.0;
    p.tasks[b].percent_complete = 100.0;
    schedule(&mut p);
    assert_eq!(p.tasks[s].calc.status, TaskStatus::Complete);
}

#[test]
fn baseline_set_change_and_clear() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.indent(&[a, b]).unwrap();
    fs(&mut p, a, b);
    schedule(&mut p);
    p.set_baseline(None);
    assert_eq!(p.tasks[b].baseline.unwrap().finish, dt(1, 9, 17));
    assert_eq!(p.tasks[s].baseline.unwrap().duration, 5 * 480);
    p.set_field_text(a, Field::Duration, "4d").unwrap();
    schedule(&mut p);
    // baseline keeps the old plan
    assert_eq!(p.tasks[b].baseline.unwrap().start, dt(1, 7, 8));
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 9, 8)));
    // re-baseline just B: summary rolls up
    p.set_baseline(Some(&[b]));
    assert_eq!(p.tasks[s].baseline.unwrap().finish, dt(1, 13, 17));
    p.clear_baseline(None);
    assert!(!p.has_baseline());
}

#[test]
fn old_files_without_new_fields_still_load() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    let mut v: serde_json::Value = serde_json::from_str(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    let proj = v["project"].as_object_mut().unwrap();
    proj.remove("format");
    proj["settings"].as_object_mut().unwrap().remove("multiple_critical_paths");
    let t = proj["tasks"][0].as_object_mut().unwrap();
    t.remove("bar_color");
    t.remove("baseline");
    let q = projectfcg::io::from_json(&v.to_string()).unwrap();
    assert_eq!(q.tasks.len(), 1);
    assert_eq!(q.format, projectfcg::format::GanttFormat::default());
}
