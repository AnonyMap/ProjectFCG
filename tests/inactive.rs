//! Inactive tasks (Project 2010+): in the plan, but without effect.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::duration::Lag;
use projectfcg::model::{LinkType, Project, ResourceType};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Phase (summary) with A (5d, $1000 fixed) → B (2d); C (3d) after A too.
fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let ph = p.append_task("Phase");
    let a = p.append_task("A");
    let b = p.append_task("B");
    let c = p.append_task("C");
    for (r, d) in [(a, "5d"), (b, "2d"), (c, "3d")] {
        p.tasks[r].outline_level = 2;
        p.set_field_text(r, Field::Duration, d).unwrap();
    }
    p.tasks[ph].outline_level = 1;
    p.normalize_outline();
    p.add_link(a, b, LinkType::FS, Lag::default()).unwrap();
    p.add_link(a, c, LinkType::FS, Lag::default()).unwrap();
    p.tasks[a].fixed_cost = 1000.0;
    schedule(&mut p);
    p
}

#[test]
fn an_inactive_task_moves_nothing_and_does_not_roll_up() {
    let mut p = plan();
    assert_eq!(p.tasks[2].calc.start, Some(dt(1, 12, 8)));
    assert_eq!(p.tasks[0].calc.finish, Some(dt(1, 14, 17)));
    p.tasks[1].active = false;
    schedule(&mut p);
    // successors no longer wait for A
    assert_eq!(p.tasks[2].calc.start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[3].calc.start, Some(dt(1, 5, 8)));
    // A keeps its own dates, but the summary spans only active work
    assert_eq!(p.tasks[1].calc.finish, Some(dt(1, 9, 17)));
    assert_eq!(p.tasks[0].calc.finish, Some(dt(1, 7, 17)), "C is the longest active task");
    // and its cost doesn't count
    assert_eq!(p.tasks[1].calc.cost, 1000.0);
    assert_eq!(p.tasks[0].calc.cost, 0.0);
    assert_eq!(p.calc.total_cost, 0.0);
    // the whole phase inactive: its subtasks are too
    p.tasks[1].active = true;
    p.tasks[0].active = false;
    schedule(&mut p);
    assert_eq!(p.inactive_mask(), vec![true, true, true, true]);
    assert_eq!(p.calc.total_cost, 0.0);
}

#[test]
fn inactive_tasks_book_nobody() {
    let mut p = plan();
    p.add_resource("Pat", ResourceType::Work);
    p.set_field_text(2, Field::ResourceNames, "Pat").unwrap();
    p.set_field_text(3, Field::ResourceNames, "Pat").unwrap();
    schedule(&mut p);
    assert!(p.calc.resources[0].overallocated, "B and C overlap");
    p.tasks[3].active = false;
    schedule(&mut p);
    assert!(!p.calc.resources[0].overallocated);
    assert_eq!(p.calc.resources[0].work, 2 * 480);
    // leveling leaves it alone and doesn't count its booking
    p.tasks[3].active = true;
    p.tasks[2].active = false;
    schedule(&mut p);
    let r = projectfcg::leveling::level(&mut p, None);
    assert!(r.delayed.is_empty(), "{:?}", r.delayed);
}

#[test]
fn active_survives_files_and_xml() {
    let mut p = plan();
    p.tasks[2].active = false;
    let q = projectfcg::io::from_json(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    assert!(!q.tasks[2].active && q.tasks[1].active);
    let mut v: serde_json::Value = serde_json::from_str(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    for t in v["project"]["tasks"].as_array_mut().unwrap() {
        t.as_object_mut().unwrap().remove("active");
    }
    let old = projectfcg::io::from_json(&v.to_string()).unwrap();
    assert!(old.tasks.iter().all(|t| t.active), "files from before 0.7 are all active");
    schedule(&mut p);
    let (x, _) = projectfcg::mspdi::from_xml(&projectfcg::mspdi::to_xml(&p)).unwrap();
    assert!(!x.tasks[2].active);
    assert_eq!(x.tasks[3].calc.start, p.tasks[3].calc.start);
}
