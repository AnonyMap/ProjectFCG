//! Dependencies between the board and the schedule: tasks that wait on
//! stories or epics, and stories that wait on tasks.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::board::{IssueType, LinkKind};
use projectfcg::duration::Lag;
use projectfcg::model::{LinkType, Project, StartDriver};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Mon 1/5/2026: Sprint 1 (1/5–1/16), Sprint 2 (1/19–1/30), Sprint 3
/// (2/2–2/13), and an ordinary 3-day task "Docs" at the end.
fn plan() -> (Project, [u32; 3], usize) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    p.board.key_prefix = "APP".into();
    let mut s = [0; 3];
    for k in &mut s {
        let r = p.new_sprint();
        schedule(&mut p);
        *k = p.tasks[r].uid;
    }
    let docs = p.append_task("Docs");
    p.set_field_text(docs, Field::Duration, "3d").unwrap();
    schedule(&mut p);
    (p, s, docs)
}

fn story(p: &mut Project, sprint: Option<u32>, title: &str) -> u32 {
    let id = p.add_ticket_of(title, sprint, IssueType::Story, None).unwrap();
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].points = Some(3.0);
    id
}

#[test]
fn a_task_follows_the_sprint_of_the_story_it_waits_on() {
    let (mut p, s, docs) = plan();
    assert_eq!(p.tasks[docs].calc.start, Some(dt(1, 5, 8)), "unlinked, it starts with the project");
    let api = story(&mut p, Some(s[1]), "Public API");
    p.add_board_dep(docs, api, Lag::default()).unwrap();
    schedule(&mut p);
    let t = &p.tasks[docs];
    assert_eq!(t.calc.start, Some(dt(2, 2, 8)), "after Sprint 2 ends (Fri 1/30)");
    assert_eq!(t.calc.start_driver, StartDriver::Predecessors);
    assert_eq!(t.calc.board_driving, vec![api]);
    assert_eq!(t.calc.board_waiting, vec![api]);
    assert!(p.calc.errors.is_empty());
    // the story slips to Sprint 3: the task moves with it
    p.move_ticket(api, Some(s[2]), 0, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[docs].calc.start, Some(dt(2, 16, 8)));
    // a lag: 2 working days after the sprint
    p.add_board_dep(docs, api, Lag::parse("+2d", &p.settings.durations).unwrap()).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[docs].calc.start, Some(dt(2, 18, 8)));
    // and the sprint now has a successor: no more slack than the task leaves it
    let s3 = p.index_of_uid(s[2]).unwrap();
    assert!(p.tasks[s3].calc.critical, "the sprint drives the last task, so it's critical");
}

#[test]
fn a_finished_story_releases_the_task_when_it_was_done() {
    let (mut p, s, docs) = plan();
    let api = story(&mut p, Some(s[1]), "Public API");
    p.add_board_dep(docs, api, Lag::default()).unwrap();
    let done = p.board.done_column();
    p.move_ticket(api, Some(s[1]), done, None).unwrap();
    let k = p.ticket_index(api).unwrap();
    p.tickets[k].done_at = Some(dt(1, 21, 15)); // Wed of Sprint 2, mid-afternoon
    schedule(&mut p);
    let t = &p.tasks[docs];
    assert_eq!(t.calc.start, Some(dt(1, 21, 15)), "it can start right when the story is done");
    assert_eq!(t.calc.start_driver, StartDriver::BoardItem);
    assert!(t.calc.board_waiting.is_empty());
}

#[test]
fn unplanned_stories_are_reported_not_scheduled() {
    let (mut p, _s, docs) = plan();
    let idea = story(&mut p, None, "Idea");
    p.add_board_dep(docs, idea, Lag::default()).unwrap();
    schedule(&mut p);
    let t = &p.tasks[docs];
    assert_eq!(t.calc.start, Some(dt(1, 5, 8)));
    assert_eq!(t.calc.board_waiting, vec![idea]);
    assert!(t.calc.board_notes[0].contains("isn't planned"), "{:?}", t.calc.board_notes);
}

#[test]
fn waiting_on_an_epic_waits_for_all_its_stories() {
    let (mut p, s, docs) = plan();
    let e = p.add_ticket_of("Checkout", None, IssueType::Epic, None).unwrap();
    let a = p.add_ticket_of("Cart", Some(s[0]), IssueType::Story, Some(e)).unwrap();
    let b = p.add_ticket_of("Pay", Some(s[1]), IssueType::Story, Some(e)).unwrap();
    p.set_predecessors_text(docs, &format!("APP-{e}")).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[docs].calc.start, Some(dt(2, 2, 8)), "the latest sprint holding unfinished stories");
    assert_eq!(p.tasks[docs].calc.board_waiting, vec![a, b]);
    // finish both
    let done = p.board.done_column();
    for (id, at) in [(a, dt(1, 14, 12)), (b, dt(1, 22, 10))] {
        let sp = p.ticket(id).unwrap().sprint;
        p.move_ticket(id, sp, done, None).unwrap();
        let k = p.ticket_index(id).unwrap();
        p.tickets[k].done_at = Some(at);
    }
    schedule(&mut p);
    assert_eq!(p.tasks[docs].calc.start, Some(dt(1, 22, 10)));
    assert_eq!(p.tasks_waiting_on(a), vec![docs], "waiting on the epic means waiting on its stories");
}

#[test]
fn predecessor_text_takes_board_keys() {
    let (mut p, s, docs) = plan();
    let api = story(&mut p, Some(s[0]), "API");
    p.set_field_text(docs, Field::Predecessors, &format!("1, APP-{api}+1d")).unwrap();
    assert_eq!(p.predecessors_text(docs), format!("1,APP-{api}+1 day"));
    assert_eq!(p.tasks[docs].board_deps.len(), 1);
    // #N works too, FS is accepted, other link types aren't
    p.set_field_text(docs, Field::Predecessors, &format!("#{api}FS")).unwrap();
    assert_eq!(p.predecessors_text(docs), format!("APP-{api}"));
    assert!(p.set_field_text(docs, Field::Predecessors, &format!("APP-{api}SS")).is_err());
    assert!(p.set_field_text(docs, Field::Predecessors, "APP-999").is_err());
    assert!(p.set_field_text(docs, Field::Predecessors, "OTHER-1").is_err());
    // clearing the text clears board dependencies too
    p.set_field_text(docs, Field::Predecessors, "").unwrap();
    assert!(p.tasks[docs].board_deps.is_empty());
}

#[test]
fn loops_through_the_board_are_refused_or_ignored() {
    let (mut p, s, docs) = plan();
    // Sprint 2 can't start before Docs is finished
    let s2 = p.index_of_uid(s[1]).unwrap();
    p.add_link(docs, s2, LinkType::FS, Lag::default()).unwrap();
    let api = story(&mut p, Some(s[1]), "API");
    // Docs waiting on a story in Sprint 2 would be a loop: refused when typed
    let err = p.set_field_text(docs, Field::Predecessors, &format!("APP-{api}")).unwrap_err();
    assert!(err.contains("loop"), "{err}");
    assert!(p.tasks[docs].board_deps.is_empty());
    // (Sprint 3 follows Sprint 2, so it waits on Docs too)
    let s3_story = story(&mut p, Some(s[2]), "Later");
    assert!(p.set_field_text(docs, Field::Predecessors, &format!("APP-{s3_story}")).is_err());
    // made legal, then the story moves into the sprint: the loop is ignored and reported
    let other = story(&mut p, Some(s[0]), "Other");
    p.set_field_text(docs, Field::Predecessors, &format!("APP-{other}")).unwrap();
    p.move_ticket(other, Some(s[1]), 0, None).unwrap();
    schedule(&mut p);
    assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
    assert!(p.tasks[docs].calc.board_notes.iter().any(|n| n.contains("loop")));
    // a sprint can't wait on its own ticket
    assert!(p.add_board_dep(s2, api, Lag::default()).is_err());
}

#[test]
fn stories_waiting_on_tasks_flag_planning_conflicts() {
    let (mut p, s, docs) = plan();
    // Docs runs 1/5–1/7; a story in Sprint 1 waiting on it is planned too early
    p.status_date = Some(dt(1, 5, 8));
    let st = story(&mut p, Some(s[0]), "Use the docs");
    let docs_uid = p.tasks[docs].uid;
    p.add_ticket_task_dep(st, docs_uid).unwrap();
    schedule(&mut p);
    assert_eq!(p.waiting_on_tasks(st), vec![docs_uid]);
    let c = p.dependency_conflicts(st);
    assert_eq!(c.len(), 1, "{c:?}");
    assert!(c[0].contains("Sprint 1 starts"), "{}", c[0]);
    assert_eq!(p.first_fitting_sprint(st), Some(s[1]));
    p.move_ticket(st, Some(s[1]), 0, None).unwrap();
    assert!(p.dependency_conflicts(st).is_empty());
    // done task: nothing to wait for
    p.set_percent_complete(docs, 100.0).unwrap();
    schedule(&mut p);
    assert!(p.waiting_on_tasks(st).is_empty());
    // a ticket and a task can't wait on each other
    let t2 = story(&mut p, Some(s[2]), "T2");
    p.add_board_dep(docs, t2, Lag::default()).unwrap();
    assert!(p.add_ticket_task_dep(t2, docs_uid).is_err());
    // sprints aren't things to wait on (plan into a later sprint instead)
    assert!(p.add_ticket_task_dep(t2, s[0]).is_err());
}

#[test]
fn blockers_planned_later_are_conflicts() {
    let (mut p, s, _) = plan();
    let a = story(&mut p, Some(s[2]), "A");
    let b = story(&mut p, Some(s[1]), "B");
    p.add_ticket_link(a, LinkKind::Blocks, b).unwrap();
    p.status_date = Some(dt(1, 5, 8));
    let c = p.dependency_conflicts(b);
    assert!(c.iter().any(|x| x.contains("planned later")), "{c:?}");
    assert_eq!(p.first_fitting_sprint(b), None, "nothing starts after Sprint 3 ends");
    p.move_ticket(a, None, 0, None).unwrap();
    assert!(p.dependency_conflicts(b).iter().any(|x| x.contains("isn't planned")));
    p.move_ticket(a, Some(s[0]), 0, None).unwrap();
    assert!(p.dependency_conflicts(b).is_empty());
}

#[test]
fn deleting_either_side_drops_the_dependency_and_files_keep_it() {
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[0]), "A");
    let b = story(&mut p, Some(s[1]), "B");
    p.add_board_dep(docs, a, Lag::default()).unwrap();
    let docs_uid = p.tasks[docs].uid;
    p.add_ticket_task_dep(b, docs_uid).unwrap();
    // save/load keeps both directions
    let q = projectfcg::io::from_json(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    assert_eq!(q.tasks[docs].board_deps, p.tasks[docs].board_deps);
    assert_eq!(q.ticket(b).unwrap().after_tasks, vec![docs_uid]);
    // undo/redo deltas carry board dependencies
    let before = p.clone();
    p.remove_board_dep(docs, a).unwrap();
    let d = projectfcg::history::diff(&before, &p);
    let mut r = before.clone();
    projectfcg::history::apply(&mut r, d);
    assert!(r.tasks[docs].board_deps.is_empty());
    let mut p = before;
    p.delete_ticket(a).unwrap();
    assert!(p.tasks[docs].board_deps.is_empty());
    p.delete_tasks(&[docs]);
    assert!(p.ticket(b).unwrap().after_tasks.is_empty());
}

#[test]
fn leveling_moves_board_dependents_with_their_sprint() {
    let (mut p, s, docs) = plan();
    let api = story(&mut p, Some(s[0]), "API");
    p.add_board_dep(docs, api, Lag::default()).unwrap();
    // one person on both Sprint 1 and an unrelated task: leveling delays one
    let r = p.add_resource("Dev", projectfcg::model::ResourceType::Work);
    let _ = r;
    let s1 = p.index_of_uid(s[0]).unwrap();
    let other = p.append_task("Other");
    p.set_field_text(other, Field::Duration, "5d").unwrap();
    p.set_field_text(other, Field::ResourceNames, "Dev").unwrap();
    p.tasks[other].priority = 900;
    p.set_field_text(s1, Field::ResourceNames, "Dev").unwrap();
    schedule(&mut p);
    projectfcg::leveling::level(&mut p, None);
    schedule(&mut p);
    let s1f = p.tasks[s1].calc.finish.unwrap();
    assert!(p.tasks[docs].calc.start.unwrap() >= s1f, "Docs still follows Sprint 1 after leveling");
}

/// Findings of the 0.7 review, pinned.
#[test]
fn review_regressions_0_7_dependencies() {
    // 1. non-ASCII after a key doesn't panic
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[0]), "A");
    for txt in [format!("APP-{a}€"), format!("#{a} 日本"), "1日".to_string(), "-1".to_string()] {
        assert!(p.set_field_text(docs, Field::Predecessors, &txt).is_err(), "{txt}");
    }
    assert!(p.tasks[docs].board_deps.is_empty(), "a refused edit leaves nothing behind");
    // a refused edit keeps the old dependencies
    p.set_field_text(docs, Field::Predecessors, &format!("APP-{a}")).unwrap();
    assert!(p.set_field_text(docs, Field::Predecessors, &format!("APP-{a}, 99")).is_err());
    assert_eq!(p.tasks[docs].board_deps.len(), 1);

    // 2. a sprint isn't held by its own finished story
    let (mut p, s, _) = plan();
    let e = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let st = p.add_ticket_of("S", Some(s[1]), IssueType::Story, Some(e)).unwrap();
    let s2 = p.index_of_uid(s[1]).unwrap();
    p.add_board_dep(s2, e, Lag::default()).unwrap();
    let done = p.board.done_column();
    p.move_ticket(st, Some(s[1]), done, None).unwrap();
    let k = p.ticket_index(st).unwrap();
    p.tickets[k].done_at = Some(dt(1, 21, 15));
    schedule(&mut p);
    assert_eq!(p.tasks[s2].calc.start, Some(dt(1, 19, 8)));

    // 10. % lag on a finished story is a share of its sprint
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[0]), "A");
    p.add_board_dep(docs, a, Lag::parse("+50%", &p.settings.durations).unwrap()).unwrap();
    schedule(&mut p);
    let open_start = p.tasks[docs].calc.start;
    let done = p.board.done_column();
    p.move_ticket(a, Some(s[0]), done, None).unwrap();
    let k = p.ticket_index(a).unwrap();
    p.tickets[k].done_at = Some(dt(1, 16, 17)); // the sprint's end
    schedule(&mut p);
    assert_eq!(p.tasks[docs].calc.start, open_start, "same date whether open or done at the sprint's end");

    // 11. no fitting sprint while a blocker is unplanned; never a past sprint
    let (mut p, s, _) = plan();
    let a = story(&mut p, None, "A");
    let b = story(&mut p, Some(s[2]), "B");
    p.add_ticket_link(a, LinkKind::Blocks, b).unwrap();
    assert_eq!(p.first_fitting_sprint(b), None);

    // 12. waiting on each other is refused from both sides, also through epics
    let (mut p, s, docs) = plan();
    let e = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let st = p.add_ticket_of("S", Some(s[0]), IssueType::Story, Some(e)).unwrap();
    let du = p.tasks[docs].uid;
    p.add_ticket_task_dep(st, du).unwrap();
    assert!(p.add_board_dep(docs, st, Lag::default()).is_err());
    assert!(p.add_board_dep(docs, e, Lag::default()).is_err(), "waiting on the epic waits on the story");
    let (mut p, s, docs) = plan();
    let e = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let st = p.add_ticket_of("S", Some(s[0]), IssueType::Story, Some(e)).unwrap();
    let du = p.tasks[docs].uid;
    p.add_board_dep(docs, e, Lag::default()).unwrap();
    assert!(p.add_ticket_task_dep(st, du).is_err());

    // 13. a link that closes a loop through a board dependency is refused, and
    //     an existing loop doesn't block unrelated edits
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[1]), "A");
    p.add_board_dep(docs, a, Lag::default()).unwrap();
    let s2 = p.index_of_uid(s[1]).unwrap();
    let err = p.add_link(docs, s2, LinkType::FS, Lag::default()).unwrap_err();
    assert!(err.contains("loop"), "{err}");
    assert!(p.set_field_text(s2, Field::Predecessors, &format!("{},{}", s2, docs + 1)).is_err());
    // a loop made later by moving a story doesn't block unrelated edits
    let (mut p, s, docs) = plan();
    let s2 = p.index_of_uid(s[1]).unwrap();
    p.add_link(docs, s2, LinkType::FS, Lag::default()).unwrap(); // Sprints 2 and 3 wait on Docs
    let b = story(&mut p, Some(s[0]), "B");
    let c = story(&mut p, Some(s[0]), "C");
    p.set_field_text(docs, Field::Predecessors, &format!("APP-{b}")).unwrap();
    p.move_ticket(b, Some(s[2]), 0, None).unwrap(); // now a loop exists
    schedule(&mut p);
    assert!(p.tasks[docs].calc.board_notes.iter().any(|n| n.contains("loop")));
    assert!(p.set_field_text(docs, Field::Predecessors, &format!("APP-{b}, APP-{c}")).is_ok(), "adding a second dependency isn't blocked by the old loop");

    // 16. finished-story waits survive a Project XML round trip
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[1]), "A");
    p.add_board_dep(docs, a, Lag::default()).unwrap();
    let done = p.board.done_column();
    p.move_ticket(a, Some(s[1]), done, None).unwrap();
    let k = p.ticket_index(a).unwrap();
    p.tickets[k].done_at = Some(dt(1, 21, 15));
    schedule(&mut p);
    let (q, _) = projectfcg::mspdi::from_xml(&projectfcg::mspdi::to_xml(&p)).unwrap();
    assert_eq!(q.tasks[docs].calc.start, p.tasks[docs].calc.start);

    // 17. a story in an inactive sprint is reported
    let (mut p, s, docs) = plan();
    let a = story(&mut p, Some(s[1]), "A");
    p.add_board_dep(docs, a, Lag::default()).unwrap();
    let s2 = p.index_of_uid(s[1]).unwrap();
    p.tasks[s2].active = false;
    schedule(&mut p);
    assert!(p.tasks[docs].calc.board_notes.iter().any(|n| n.contains("inactive")));
    // and an inactive task doesn't keep a card blocked
    let du = p.tasks[docs].uid;
    let c = story(&mut p, Some(s[2]), "C");
    p.add_ticket_task_dep(c, du).unwrap();
    p.tasks[docs].active = false;
    schedule(&mut p);
    assert!(p.waiting_on_tasks(c).is_empty());
}

#[test]
fn review_regressions_0_7_sprints_columns_inactive() {
    // 6/7. inactive tasks don't set the finish; an all-inactive summary drives nothing
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let a = p.append_task("A");
    let b = p.append_task("B");
    p.set_field_text(a, Field::Duration, "5d").unwrap();
    p.set_field_text(b, Field::Duration, "20d").unwrap();
    p.tasks[b].active = false;
    schedule(&mut p);
    assert_eq!(p.calc.finish, Some(dt(1, 9, 17)));
    assert!(p.tasks[a].calc.critical);
    let ph = p.append_task("Phase");
    let c = p.append_task("C");
    let d = p.append_task("D");
    p.tasks[c].outline_level = 2;
    p.normalize_outline();
    p.tasks[d].outline_level = 1;
    p.set_field_text(c, Field::Duration, "5d").unwrap();
    p.add_link(ph, d, LinkType::FS, Lag::default()).unwrap();
    p.tasks[c].active = false;
    schedule(&mut p);
    assert_eq!(p.tasks[d].calc.start, Some(dt(1, 5, 8)));

    // 8/9. completing into a completed sprint, or leaving sub-tasks behind
    let (mut p, s, _) = plan();
    let st = story(&mut p, Some(s[0]), "St");
    let sub = p.add_ticket_of("sub", None, IssueType::Subtask, Some(st)).unwrap();
    let done = p.board.done_column();
    p.move_ticket(st, Some(s[0]), done, None).unwrap();
    assert!(p.complete_sprint(s[0], None, dt(1, 16, 17)).unwrap_err().contains("sub-task"));
    p.move_ticket(sub, Some(s[0]), done, None).unwrap();
    p.complete_sprint(s[1], None, dt(1, 30, 17)).unwrap();
    assert!(p.complete_sprint(s[0], Some(s[1]), dt(1, 16, 17)).is_err());
    p.complete_sprint(s[0], Some(s[2]), dt(1, 16, 17)).unwrap();

    // 18. columns: an empty list from a hand-edited file is repaired, edits don't panic
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    p.board.columns.clear();
    assert!(p.repair_board() > 0);
    assert_eq!(p.board.columns.len(), 4);
    p.board.columns.clear();
    let cols = vec![("A".to_string(), Some(0), None), ("B".to_string(), Some(5), None)];
    p.set_board_columns(&cols, dt(1, 5, 8)).unwrap();
}
