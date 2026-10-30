//! Tracking and earned value. Expected numbers are worked by hand from
//! Project's definitions (Standard calendar, start Mon 1/5/2026).
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::duration::Lag;
use projectfcg::model::{Accrue, LinkType, Project, ResourceType, StartDriver};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;
use projectfcg::tracking::UpdateMode;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

fn proj() -> Project {
    Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
}

fn add(p: &mut Project, name: &str, dur: &str) -> usize {
    let r = p.append_task(name);
    p.set_field_text(r, Field::Duration, dur).unwrap();
    schedule(p);
    r
}

#[test]
fn percent_complete_stamps_actuals() {
    let mut p = proj();
    let a = add(&mut p, "A", "10d");
    p.set_percent_complete(a, 30.0).unwrap();
    assert_eq!(p.tasks[a].actual_start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[a].actual_finish, None);
    schedule(&mut p);
    assert_eq!(p.actual_duration(a), 3 * 480);
    assert_eq!(p.remaining_duration(a), 7 * 480);
    p.set_percent_complete(a, 100.0).unwrap();
    assert_eq!(p.tasks[a].actual_finish, Some(dt(1, 16, 17)));
    p.set_percent_complete(a, 0.0).unwrap();
    assert_eq!((p.tasks[a].actual_start, p.tasks[a].actual_finish), (None, None));
    assert!(p.set_percent_complete(a, 120.0).is_err());
}

#[test]
fn actual_start_pins_the_task() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.add_link(a, b, LinkType::FS, Lag::default()).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 7, 8)));
    // B really started on Tuesday
    p.set_actual_start(b, Some(dt(1, 6, 8))).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 6, 8)));
    assert_eq!(p.tasks[b].calc.start_driver, StartDriver::Actual);
    // now A slips: B doesn't move, it already started
    p.set_field_text(a, Field::Duration, "5d").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 6, 8)));
    // clearing the actual start (NA) un-starts it
    p.set_actual_start(b, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 12, 8)));
    assert_eq!(p.tasks[b].percent_complete, 0.0);
}

#[test]
fn actual_finish_sets_duration_and_completes() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    p.set_actual_start(a, Some(dt(1, 5, 8))).unwrap();
    p.set_actual_finish(a, Some(dt(1, 13, 17))).unwrap(); // took 7 days
    schedule(&mut p);
    assert_eq!(p.tasks[a].duration.minutes, 7 * 480);
    assert_eq!(p.tasks[a].percent_complete, 100.0);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 13, 17)));
    assert!(p.set_actual_finish(a, Some(dt(1, 2, 17))).is_err(), "before the start");
}

#[test]
fn actual_and_remaining_duration() {
    let mut p = proj();
    let a = add(&mut p, "A", "10d");
    p.set_actual_duration(a, 4 * 480).unwrap();
    assert_eq!(p.tasks[a].percent_complete, 40.0);
    // 4 days done, 8 still to go: the task is now 12 days, 33.33% done
    p.set_remaining_duration(a, 8 * 480).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].duration.minutes, 12 * 480);
    assert!((p.tasks[a].percent_complete - 33.33).abs() < 0.01);
    // more actual than planned extends and completes it
    p.set_actual_duration(a, 15 * 480).unwrap();
    assert_eq!((p.tasks[a].duration.minutes, p.tasks[a].percent_complete), (15 * 480, 100.0));
}

#[test]
fn update_project_through_a_date() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d"); // Mon–Tue
    let b = add(&mut p, "B", "4d"); // Wed–Mon (after A)
    let c = add(&mut p, "C", "2d"); // after B
    p.add_link(a, b, LinkType::FS, Lag::default()).unwrap();
    p.add_link(b, c, LinkType::FS, Lag::default()).unwrap();
    schedule(&mut p);
    let mut q = p.clone();
    // through Thu 1/8 end of day: A done, B half done (Wed+Thu of 4 days), C not started
    let changed = p.update_project(dt(1, 8, 17), UpdateMode::Proportional, None, None);
    assert_eq!(changed, vec![a, b]);
    assert_eq!(p.tasks[a].percent_complete, 100.0);
    assert_eq!(p.tasks[b].percent_complete, 50.0);
    assert_eq!(p.tasks[c].percent_complete, 0.0);
    // 0%/100% only
    q.update_project(dt(1, 8, 17), UpdateMode::ZeroOrHundred, None, None);
    assert_eq!((q.tasks[a].percent_complete, q.tasks[b].percent_complete), (100.0, 0.0));
    // progress is never lowered
    p.set_percent_complete(b, 80.0).unwrap();
    p.update_project(dt(1, 8, 17), UpdateMode::Proportional, None, None);
    assert_eq!(p.tasks[b].percent_complete, 80.0);
    // rescheduling unstarted work after a date sets Start No Earlier Than
    let mut r = proj();
    let x = add(&mut r, "X", "2d");
    r.update_project(dt(1, 5, 8), UpdateMode::Proportional, None, Some(dt(1, 12, 8)));
    schedule(&mut r);
    assert_eq!(r.tasks[x].calc.start, Some(dt(1, 12, 8)));
}

#[test]
fn mark_on_track_uses_status_date() {
    let mut p = proj();
    let a = add(&mut p, "A", "10d");
    p.status_date = Some(dt(1, 9, 17)); // end of week 1
    p.mark_on_track(&[a]).unwrap();
    assert_eq!(p.tasks[a].percent_complete, 50.0);
}

/// 10-day task, one person at $100/hr full time → $8,000.
fn ev_project(accrue: Accrue) -> (Project, usize) {
    let mut p = proj();
    let a = add(&mut p, "Build", "10d");
    let r = p.add_resource("Dev", ResourceType::Work);
    p.resources[r].std_rate = 100.0;
    p.resources[r].accrue = accrue;
    p.set_field_text(a, Field::ResourceNames, "Dev").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.cost, 8000.0);
    p.set_baseline(None);
    (p, a)
}

#[test]
fn earned_value_by_hand() {
    let (mut p, a) = ev_project(Accrue::Prorated);
    // status: end of day 5, but only 30% done
    p.status_date = Some(dt(1, 9, 17));
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    let ev = p.earned_value(a);
    assert_eq!(ev.bac, 8000.0);
    assert_eq!(ev.bcws, 4000.0, "half the baseline should be done");
    assert_eq!(ev.bcwp, 2400.0, "30% of the budget earned");
    assert_eq!(ev.acwp, 2400.0, "prorated actual cost");
    assert_eq!(ev.sv(), -1600.0);
    assert_eq!(ev.cv(), 0.0);
    assert_eq!(ev.spi(), Some(0.6));
    assert_eq!(ev.cpi(), Some(1.0));
    assert_eq!(ev.eac(), 8000.0);
    assert_eq!(p.tasks[a].calc.actual_work, 24 * 60);
    // it took longer than planned: 30% done costs more if the task grew
    p.set_field_text(a, Field::Duration, "15d").unwrap();
    schedule(&mut p);
    let ev = p.earned_value(a);
    assert_eq!(ev.acwp, 12000.0 * 0.3, "actual cost follows the bigger task");
    assert!(ev.cpi().unwrap() < 1.0);
    assert!(ev.eac() > ev.bac && ev.vac() < 0.0);
    assert_eq!(p.project_earned_value(), ev, "one task = the project");
}

#[test]
fn accrual_rules_for_actual_cost() {
    let (mut p, a) = ev_project(Accrue::Start);
    p.set_percent_complete(a, 10.0).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.actual_cost, 8000.0, "accrues at start");
    let (mut q, b) = ev_project(Accrue::End);
    q.set_percent_complete(b, 90.0).unwrap();
    schedule(&mut q);
    assert_eq!(q.tasks[b].calc.actual_cost, 0.0, "nothing until it ends");
    q.set_percent_complete(b, 100.0).unwrap();
    schedule(&mut q);
    assert_eq!(q.tasks[b].calc.actual_cost, 8000.0);
    // fixed cost is prorated
    let mut r = proj();
    let t = add(&mut r, "Licence", "4d");
    r.tasks[t].fixed_cost = 1000.0;
    r.set_percent_complete(t, 25.0).unwrap();
    schedule(&mut r);
    assert_eq!(r.tasks[t].calc.actual_cost, 250.0);
}

#[test]
fn earned_value_rolls_up() {
    let mut p = proj();
    let s = p.append_task("Phase");
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    p.indent(&[a, b]).unwrap();
    p.tasks[a].fixed_cost = 100.0;
    p.tasks[b].fixed_cost = 300.0;
    schedule(&mut p);
    p.set_baseline(None);
    p.status_date = Some(dt(1, 6, 17));
    p.set_percent_complete(a, 100.0).unwrap();
    schedule(&mut p);
    let ev = p.earned_value(s);
    assert_eq!((ev.bac, ev.bcws, ev.bcwp, ev.acwp), (400.0, 400.0, 100.0, 100.0));
    assert_eq!(p.project_earned_value(), ev);
}

#[test]
fn baselines_zero_to_ten() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.set_baseline_n(3, Some(&[b]));
    assert!(p.tasks[a].baseline_slot(3).is_none());
    let bl = p.tasks[b].baseline_slot(3).unwrap();
    assert_eq!((bl.start, bl.duration), (dt(1, 5, 8), 3 * 480));
    assert!(p.baseline_saved[3].is_some() && p.baseline_saved[0].is_none());
    assert!(p.has_baseline_n(3) && !p.has_baseline_n(0));
    p.set_baseline(None);
    assert!(p.tasks[a].baseline.is_some());
    p.clear_baseline_n(3, None);
    assert!(!p.has_baseline_n(3) && p.baseline_saved[3].is_none());
    assert!(p.has_baseline_n(0), "other baselines untouched");
}

#[test]
fn leveling_leaves_started_tasks_alone() {
    let mut p = proj();
    let r = p.add_resource("Dev", ResourceType::Work);
    let _ = r;
    let a = add(&mut p, "A", "3d");
    let b = add(&mut p, "B", "3d");
    p.set_field_text(a, Field::ResourceNames, "Dev").unwrap();
    p.set_field_text(b, Field::ResourceNames, "Dev").unwrap();
    // B already started: leveling must delay A instead
    p.set_actual_start(b, Some(dt(1, 5, 8))).unwrap();
    schedule(&mut p);
    projectfcg::leveling::level(&mut p, None);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 5, 8)));
    assert!(p.tasks[a].leveling_delay > 0);
    assert!(p.calc.resources[0].overallocated == false);
}

#[test]
fn progress_line_points() {
    let mut p = proj();
    let a = add(&mut p, "A", "10d");
    let at = dt(1, 9, 17);
    // 30% done of a task that should be 50% done: point at day 3's end (behind)
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    assert_eq!(projectfcg::tracking::progress_point(&p, a, at), Some(dt(1, 7, 17)));
    p.set_percent_complete(a, 100.0).unwrap();
    schedule(&mut p);
    assert_eq!(projectfcg::tracking::progress_point(&p, a, at), Some(at), "done = on the line");
}

#[test]
fn tracking_survives_save_and_old_baselines_load() {
    let (mut p, a) = ev_project(Accrue::Prorated);
    p.set_percent_complete(a, 40.0).unwrap();
    p.set_baseline_n(10, None);
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.tasks[a].actual_start, p.tasks[a].actual_start);
    assert_eq!(q.tasks[a].baselines[9], p.tasks[a].baselines[9]);
    assert_eq!(q.tasks[a].baseline.unwrap().cost, 8000.0);
    // a 0.5 baseline has no work/cost
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for t in v["project"]["tasks"].as_array_mut().unwrap() {
        if let Some(b) = t["baseline"].as_object_mut() {
            b.remove("work");
            b.remove("cost");
        }
        t.as_object_mut().unwrap().remove("baselines");
        t.as_object_mut().unwrap().remove("actual_start");
    }
    let r = projectfcg::io::from_json(&serde_json::to_string(&v).unwrap()).unwrap();
    let b = r.tasks[a].baseline.unwrap();
    assert_eq!((b.work, b.cost), (0, 0.0));
    assert!(r.tasks[a].baselines.iter().all(|b| b.is_none()));
}
