//! Resources, assignments, task types, resource calendars, costs,
//! overallocation and leveling. Standard calendar, project start Mon 1/5/2026.

use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::calendar::CalendarException;
use projectfcg::duration::Lag;
use projectfcg::leveling::{clear_leveling, level, next_overallocation};
use projectfcg::model::{LevelingOrder, LinkType, Project, ResourceType, StartDriver, TaskType};
use projectfcg::ops::Field;
use projectfcg::schedule;

const H: i64 = 60;
const D: i64 = 480;

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
fn res(p: &mut Project, name: &str) -> u32 {
    let i = p.add_resource(name, ResourceType::Work);
    p.resources[i].uid
}
fn work_of(p: &Project, row: usize, r: u32) -> i64 {
    let u = p.tasks[row].uid;
    p.assignments.iter().find(|a| a.task == u && a.resource == r).unwrap().work
}
fn units_of(p: &Project, row: usize, r: u32) -> f64 {
    let u = p.tasks[row].uid;
    p.assignments.iter().find(|a| a.task == u && a.resource == r).unwrap().units
}

#[test]
fn full_time_assignment_gives_work_equals_duration() {
    let mut p = proj();
    let t = add(&mut p, "A", "5d");
    let alice = res(&mut p, "Alice");
    p.assign(t, alice, 1.0).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[t].calc.work, 40 * H);
    assert_eq!(p.tasks[t].calc.finish, Some(dt(1, 9, 17)));
}

#[test]
fn fixed_units_rules() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    let alice = res(&mut p, "Alice");
    p.assign(t, alice, 0.5).unwrap();
    // Work = 4d × 50% = 16h; duration unchanged
    assert_eq!(work_of(&p, t, alice), 16 * H);
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    // change work → duration changes (units fixed)
    p.set_field_text(t, Field::Work, "32h").unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 8 * D);
    // change duration → work changes (units fixed)
    p.set_field_text(t, Field::Duration, "2d").unwrap();
    assert_eq!(work_of(&p, t, alice), 8 * H);
    // change units → duration changes (work fixed)
    let ai = p.assignments_of_task(t)[0];
    p.set_assignment_units(ai, 1.0).unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 1 * D);
    assert_eq!(work_of(&p, t, alice), 8 * H);
}

#[test]
fn fixed_duration_rules() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    p.tasks[t].task_type = TaskType::FixedDuration;
    let alice = res(&mut p, "Alice");
    p.assign(t, alice, 1.0).unwrap();
    // units change → work changes, duration fixed
    let ai = p.assignments_of_task(t)[0];
    p.set_assignment_units(ai, 0.5).unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    assert_eq!(work_of(&p, t, alice), 16 * H);
    // work change → units change
    p.set_field_text(t, Field::Work, "8h").unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    assert!((units_of(&p, t, alice) - 0.25).abs() < 1e-9);
}

#[test]
fn fixed_work_rules() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    p.tasks[t].task_type = TaskType::FixedWork;
    let alice = res(&mut p, "Alice");
    p.assign(t, alice, 1.0).unwrap();
    // duration change → units change, work fixed
    p.set_field_text(t, Field::Duration, "8d").unwrap();
    assert_eq!(work_of(&p, t, alice), 32 * H);
    assert!((units_of(&p, t, alice) - 0.5).abs() < 1e-9);
}

#[test]
fn effort_driven_adding_and_removing_resources() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    p.tasks[t].effort_driven = true;
    let alice = res(&mut p, "Alice");
    let bob = res(&mut p, "Bob");
    p.assign(t, alice, 1.0).unwrap();
    p.assign(t, bob, 1.0).unwrap();
    // total work stays 32h, split across two people: duration halves
    assert_eq!(p.tasks[t].duration.minutes, 2 * D);
    assert_eq!(work_of(&p, t, alice), 16 * H);
    assert_eq!(work_of(&p, t, bob), 16 * H);
    p.unassign(t, bob);
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    assert_eq!(work_of(&p, t, alice), 32 * H);
}

#[test]
fn effort_driven_fixed_duration_reduces_units() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    p.tasks[t].effort_driven = true;
    p.tasks[t].task_type = TaskType::FixedDuration;
    let alice = res(&mut p, "Alice");
    let bob = res(&mut p, "Bob");
    p.assign(t, alice, 1.0).unwrap();
    p.assign(t, bob, 1.0).unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    assert!((units_of(&p, t, alice) - 0.5).abs() < 1e-9);
    assert!((units_of(&p, t, bob) - 0.5).abs() < 1e-9);
}

#[test]
fn not_effort_driven_adding_resource_adds_work() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    let alice = res(&mut p, "Alice");
    let bob = res(&mut p, "Bob");
    p.assign(t, alice, 1.0).unwrap();
    p.assign(t, bob, 1.0).unwrap();
    assert_eq!(p.tasks[t].duration.minutes, 4 * D);
    assert_eq!(p.task_work(t), 64 * H);
}

#[test]
fn resource_names_notation_creates_and_updates() {
    let mut p = proj();
    let t = add(&mut p, "A", "2d");
    p.set_field_text(t, Field::ResourceNames, "Alice, Bob[50%], Cement[10 tons], Travel[$300]").unwrap();
    assert_eq!(p.resources.len(), 4);
    assert_eq!(p.resources[2].rtype, ResourceType::Material);
    assert_eq!(p.resources[2].material_label, "tons");
    assert_eq!(p.resources[3].rtype, ResourceType::Cost);
    assert_eq!(p.resource_names_text(t), "Alice,Bob[50%],Cement[10 tons],Travel[$300.00]");
    // re-typing changes units and drops Bob
    p.set_field_text(t, Field::ResourceNames, "alice[25%], Cement[12 tons]").unwrap();
    assert_eq!(p.resource_names_text(t), "Alice[25%],Cement[12 tons]");
    assert_eq!(p.resources.len(), 4, "names match case-insensitively; no duplicates");
    assert!(p.set_field_text(t, Field::ResourceNames, "Alice, Alice").is_err());
    p.settings.auto_add_resources = false;
    assert!(p.set_field_text(t, Field::ResourceNames, "Zed").is_err());
}

#[test]
fn summary_tasks_cannot_take_assignments() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    p.indent(&[a]).unwrap();
    let alice = res(&mut p, "Alice");
    assert!(p.assign(s, alice, 1.0).is_err());
}

#[test]
fn resource_vacation_extends_task() {
    let mut p = proj();
    let t = add(&mut p, "A", "5d");
    let alice = res(&mut p, "Alice");
    let w = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
    p.resources[0].exceptions.push(CalendarException { name: "Vacation".into(), start: w, end: w, shifts: vec![] });
    p.assign(t, alice, 1.0).unwrap();
    schedule(&mut p);
    // 5 working days for Alice skip Wednesday
    assert_eq!(p.tasks[t].calc.finish, Some(dt(1, 12, 17)));
    assert_eq!(p.tasks[t].duration.minutes, 5 * D);
}

#[test]
fn task_with_two_calendars_finishes_with_the_slower_resource() {
    let mut p = proj();
    let t = add(&mut p, "A", "4d");
    let alice = res(&mut p, "Alice");
    let bob = res(&mut p, "Bob");
    // Bob works Mon–Wed only
    let mut week = p.calendar.week.clone();
    week[3] = vec![];
    week[4] = vec![];
    p.resources[1].week = Some(week);
    p.assign(t, alice, 1.0).unwrap();
    p.assign(t, bob, 1.0).unwrap();
    schedule(&mut p);
    // Alice: Mon–Thu. Bob: Mon, Tue, Wed, next Mon.
    assert_eq!(p.tasks[t].calc.finish, Some(dt(1, 12, 17)));
    let a_bob = p.assignments.iter().position(|a| a.resource == bob).unwrap();
    assert_eq!(p.calc.assignments[a_bob].finish, Some(dt(1, 12, 17)));
    let a_alice = p.assignments.iter().position(|a| a.resource == alice).unwrap();
    assert_eq!(p.calc.assignments[a_alice].finish, Some(dt(1, 8, 17)));
}

#[test]
fn overallocation_detection() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    let c = add(&mut p, "C", "2d");
    let alice = res(&mut p, "Alice");
    let bob = res(&mut p, "Bob");
    p.assign(a, alice, 1.0).unwrap();
    p.assign(b, alice, 1.0).unwrap();
    p.assign(a, bob, 0.5).unwrap();
    p.assign(c, bob, 0.5).unwrap();
    schedule(&mut p);
    assert!(p.calc.resources[0].overallocated, "Alice 200% on Mon–Tue");
    assert_eq!(p.calc.resources[0].over_intervals, vec![(dt(1, 5, 8), dt(1, 6, 17))]);
    assert!((p.calc.resources[0].peak_units - 2.0).abs() < 1e-9);
    assert!(!p.calc.resources[1].overallocated, "Bob 50% + 50% is fine");
    assert!(p.tasks[a].calc.overallocated && p.tasks[b].calc.overallocated);
    assert!(!p.tasks[c].calc.overallocated);
    assert_eq!(next_overallocation(&p, None), Some(a));
    // sequencing the tasks removes the overallocation
    p.add_link(a, b, LinkType::FS, Lag::default()).unwrap();
    schedule(&mut p);
    assert!(!p.calc.resources[0].overallocated);
}

#[test]
fn costs_roll_up() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let t = add(&mut p, "A", "1d");
    p.indent(&[t]).unwrap();
    p.set_field_text(t, Field::ResourceNames, "Alice, Cement[10 tons], Travel[$300]").unwrap();
    let ri = p.resource_by_name("Alice").unwrap();
    p.resources[ri].std_rate = 50.0;
    p.resources[ri].cost_per_use = 100.0;
    let ci = p.resource_by_name("Cement").unwrap();
    p.resources[ci].std_rate = 20.0;
    p.tasks[t].fixed_cost = 50.0;
    p.tasks[s].fixed_cost = 25.0;
    schedule(&mut p);
    // 8h × $50 + $100 + 10 × $20 + $300 + $50 fixed
    assert!((p.tasks[t].calc.cost - 1050.0).abs() < 1e-6);
    assert!((p.tasks[s].calc.cost - 1075.0).abs() < 1e-6);
    assert!((p.calc.total_cost - 1075.0).abs() < 1e-6);
    assert!((p.calc.resources[ri].cost - 500.0).abs() < 1e-6);
    assert_eq!(p.calc.resources[ri].work, 8 * H);
    assert_eq!(p.tasks[s].calc.work, 8 * H, "material/cost resources add no work");
}

#[test]
fn usage_spreads_work_evenly() {
    let mut p = proj();
    let t = add(&mut p, "A", "5d");
    let alice = res(&mut p, "Alice");
    p.assign(t, alice, 0.5).unwrap();
    schedule(&mut p);
    let w = p.assignment_work_between(0, dt(1, 5, 0), dt(1, 7, 0));
    assert!((w - (8 * H) as f64).abs() < 1e-6, "2 days at 50% = 8h, got {w}");
}

#[test]
fn delete_cascades_to_assignments() {
    let mut p = proj();
    let t = add(&mut p, "A", "1d");
    let u = add(&mut p, "B", "1d");
    p.set_field_text(t, Field::ResourceNames, "Alice, Bob").unwrap();
    p.set_field_text(u, Field::ResourceNames, "Alice").unwrap();
    p.delete_resources(&[1]);
    assert_eq!(p.assignments.len(), 2);
    p.delete_tasks(&[t]);
    assert_eq!(p.assignments.len(), 1);
}

#[test]
fn leveling_resolves_parallel_overallocation() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    let alice = res(&mut p, "Alice");
    p.assign(a, alice, 1.0).unwrap();
    p.assign(b, alice, 1.0).unwrap();
    schedule(&mut p);
    assert!(p.calc.resources[0].overallocated);
    let r = level(&mut p, None);
    assert!(r.unresolved.is_empty(), "{:?}", r.unresolved);
    assert!(!p.calc.resources[0].overallocated);
    // Standard order: equal starts → less slack first. B (3d) is critical,
    // so A is delayed until B finishes.
    assert_eq!(r.delayed, vec![a]);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 8, 8)));
    assert_eq!(p.tasks[a].leveling_delay, 3 * D);
    assert_eq!(p.tasks[a].calc.start_driver, StartDriver::Leveling);
    clear_leveling(&mut p, None);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 5, 8)));
}

#[test]
fn leveling_respects_priority() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    let alice = res(&mut p, "Alice");
    p.assign(a, alice, 1.0).unwrap();
    p.assign(b, alice, 1.0).unwrap();
    p.tasks[a].priority = 900;
    p.settings.leveling_order = LevelingOrder::PriorityStandard;
    level(&mut p, None);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 5, 8)), "high priority goes first");
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 7, 8)));
    // priority 1000 = never level
    p.tasks[b].priority = 1000;
    p.tasks[a].priority = 500;
    level(&mut p, None);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 8, 8)));
}

#[test]
fn leveling_within_slack_refuses_to_slip_the_project() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    let alice = res(&mut p, "Alice");
    p.assign(a, alice, 1.0).unwrap();
    p.assign(b, alice, 1.0).unwrap();
    p.settings.level_within_slack = true;
    let r = level(&mut p, None);
    assert!(!r.unresolved.is_empty());
    assert!(p.calc.resources[0].overallocated);
    assert_eq!(p.calc.finish, Some(dt(1, 6, 17)));
}

#[test]
fn leveling_keeps_links_valid_and_handles_chains() {
    // A(Alice) → C(Bob); B(Alice) → D(Bob). Leveling Alice delays one chain.
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    let c = add(&mut p, "C", "1d");
    let d = add(&mut p, "D", "1d");
    p.add_link(a, c, LinkType::FS, Lag::default()).unwrap();
    p.add_link(b, d, LinkType::FS, Lag::default()).unwrap();
    for (row, who) in [(a, "Alice"), (b, "Alice"), (c, "Bob"), (d, "Bob")] {
        p.set_field_text(row, Field::ResourceNames, who).unwrap();
    }
    let r = level(&mut p, None);
    assert!(r.unresolved.is_empty(), "{:?}", r.unresolved);
    assert!(p.calc.resources.iter().all(|x| !x.overallocated));
    for (pre, suc) in [(a, c), (b, d)] {
        assert!(p.tasks[suc].calc.start.unwrap() >= p.tasks[pre].calc.finish.unwrap());
    }
}

#[test]
fn level_selection_only_moves_selected_tasks() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "2d");
    let alice = res(&mut p, "Alice");
    p.assign(a, alice, 1.0).unwrap();
    p.assign(b, alice, 1.0).unwrap();
    level(&mut p, Some(&[a]));
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 7, 8)));
}

#[test]
fn leveling_cannot_fix_assignment_above_max_units() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    let alice = res(&mut p, "Alice");
    p.assign(a, alice, 2.0).unwrap();
    let r = level(&mut p, None);
    assert!(r.unresolved.iter().any(|m| m.contains("maximum")), "{:?}", r.unresolved);
}

#[test]
fn projects_with_resources_roundtrip_through_json() {
    let mut p = proj();
    let t = add(&mut p, "A", "2d");
    p.set_field_text(t, Field::ResourceNames, "Alice[50%], Travel[$300]").unwrap();
    p.resources[0].std_rate = 42.0;
    p.tasks[t].task_type = TaskType::FixedWork;
    schedule(&mut p);
    let q = projectfcg::io::from_json(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    assert_eq!(q.resources, p.resources);
    assert_eq!(q.assignments, p.assignments);
    assert_eq!(q.tasks[0].task_type, TaskType::FixedWork);
    assert_eq!(q.calc.total_cost, p.calc.total_cost);
}

#[test]
fn leveling_performance() {
    let mut p = proj();
    let names = ["A", "B", "C", "D", "E"];
    for (k, n) in names.iter().enumerate() {
        let i = p.add_resource(n, ResourceType::Work);
        let _ = (k, i);
    }
    for i in 0..300 {
        let r = add(&mut p, &format!("T{i}"), &format!("{}d", 1 + i % 4));
        p.set_field_text(r, Field::ResourceNames, names[i % 5]).unwrap();
    }
    let t0 = std::time::Instant::now();
    let r = level(&mut p, None);
    let el = t0.elapsed();
    assert!(r.unresolved.is_empty(), "{:?}", r.unresolved);
    assert!(p.calc.resources.iter().all(|x| !x.overallocated));
    assert!(el.as_secs() < 20, "leveling 300 tasks took {el:?}");
    eprintln!("leveled 300 tasks in {el:?}");
}
