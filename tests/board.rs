//! Kanban board: sprints, tickets, progress, burndown, velocity.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::board::Estimate;
use projectfcg::model::{Project, ResourceType, TaskType};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Project starting Mon 1/5/2026 with two 10-day sprints.
fn two_sprints() -> (Project, u32, u32) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let a = p.new_sprint();
    schedule(&mut p);
    let b = p.new_sprint();
    schedule(&mut p);
    let (ua, ub) = (p.tasks[a].uid, p.tasks[b].uid);
    (p, ua, ub)
}

fn pts(p: &mut Project, sprint: Option<u32>, title: &str, points: f64) -> u32 {
    let id = p.add_ticket(title, sprint).unwrap();
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].points = Some(points);
    id
}

#[test]
fn new_sprints_are_consecutive_fixed_duration_timeboxes() {
    let (p, ua, ub) = two_sprints();
    let a = p.index_of_uid(ua).unwrap();
    let b = p.index_of_uid(ub).unwrap();
    assert_eq!((p.tasks[a].name.as_str(), p.tasks[b].name.as_str()), ("Sprint 1", "Sprint 2"));
    assert!(p.tasks[a].is_sprint && p.tasks[b].is_sprint);
    assert_eq!(p.tasks[a].task_type, TaskType::FixedDuration);
    assert_eq!(p.tasks[a].duration.minutes, 10 * 480);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 16, 17)));
    // the second follows the first
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 19, 8)));
    assert_eq!(p.tasks[b].predecessors.len(), 1);
    assert_eq!(p.sprint_rows(), vec![a, b]);
}

#[test]
fn sprint_length_setting_and_names_stay_unique() {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    p.board.sprint_days = 5;
    let r = p.append_task("Sprint 1");
    let _ = r;
    let s = p.new_sprint();
    assert_eq!(p.tasks[s].name, "Sprint 2", "doesn't clash with an existing task name");
    assert_eq!(p.tasks[s].duration.minutes, 5 * 480);
}

#[test]
fn moving_tickets_orders_and_stamps_done() {
    let (mut p, ua, _) = two_sprints();
    let a = p.add_ticket("Login page", Some(ua)).unwrap();
    let b = p.add_ticket("Signup", Some(ua)).unwrap();
    let c = p.add_ticket("Logout", Some(ua)).unwrap();
    let col0: Vec<u32> = p.tickets_in(Some(ua)).iter().map(|&k| p.tickets[k].id).collect();
    assert_eq!(col0, vec![a, b, c]);
    // c before a in To Do
    p.move_ticket(c, Some(ua), 0, Some(a)).unwrap();
    let col0: Vec<u32> = p.tickets_in(Some(ua)).iter().map(|&k| p.tickets[k].id).collect();
    assert_eq!(col0, vec![c, a, b]);
    // into Done: stamped; back out: cleared
    let done = p.board.done_column();
    p.move_ticket(b, Some(ua), done, None).unwrap();
    assert!(p.ticket(b).unwrap().done_at.is_some());
    assert!(p.is_done(p.ticket(b).unwrap()));
    p.move_ticket(b, Some(ua), 1, None).unwrap();
    assert!(p.ticket(b).unwrap().done_at.is_none());
    // to the backlog and back
    p.move_ticket(a, None, 0, None).unwrap();
    assert_eq!(p.tickets_in(None).len(), 1);
    assert!(p.move_ticket(a, Some(12345), 0, None).is_err(), "not a sprint");
    assert!(p.add_ticket("   ", None).is_err(), "needs a title");
}

#[test]
fn sprint_progress_from_points_or_ticket_count() {
    let (mut p, ua, _) = two_sprints();
    let done = p.board.done_column();
    let a = pts(&mut p, Some(ua), "A", 5.0);
    let _b = pts(&mut p, Some(ua), "B", 3.0);
    let c = pts(&mut p, Some(ua), "C", 2.0);
    p.move_ticket(a, Some(ua), done, None).unwrap();
    schedule(&mut p);
    let row = p.index_of_uid(ua).unwrap();
    assert_eq!(p.tasks[row].percent_complete, 50.0, "5 of 10 points");
    // the timebox doesn't move
    assert_eq!(p.tasks[row].calc.finish, Some(dt(1, 16, 17)));
    // counting tickets instead
    p.board.estimate = Estimate::TicketCount;
    p.move_ticket(c, Some(ua), done, None).unwrap();
    schedule(&mut p);
    assert!((p.tasks[row].percent_complete - 66.7).abs() < 0.05, "2 of 3 tickets: {}", p.tasks[row].percent_complete);
    // % complete can't be typed over the board
    assert!(p.set_field_text(row, Field::PercentComplete, "10%").is_err());
    // a sprint without tickets keeps its typed % complete
    let other = p.sprint_rows()[1];
    p.set_field_text(other, Field::PercentComplete, "30%").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[other].percent_complete, 30.0);
}

#[test]
fn unestimated_tickets_fall_back_to_count() {
    let (mut p, ua, _) = two_sprints();
    let a = p.add_ticket("A", Some(ua)).unwrap();
    p.add_ticket("B", Some(ua)).unwrap();
    let st = p.sprint_stats(Some(ua));
    assert_eq!((st.tickets, st.unestimated, st.total), (2, 2, 0.0));
    let done = p.board.done_column();
    p.move_ticket(a, Some(ua), done, None).unwrap();
    assert_eq!(p.sprint_stats(Some(ua)).percent(), 50.0);
}

#[test]
fn unmarking_or_deleting_a_sprint_returns_tickets_to_backlog() {
    let (mut p, ua, ub) = two_sprints();
    p.add_ticket("A", Some(ua)).unwrap();
    p.add_ticket("B", Some(ub)).unwrap();
    let a = p.index_of_uid(ua).unwrap();
    p.set_sprint(a, false).unwrap();
    assert_eq!(p.tickets_in(None).len(), 1);
    let b = p.index_of_uid(ub).unwrap();
    p.delete_tasks(&[b]);
    assert_eq!(p.tickets_in(None).len(), 2);
    assert!(p.tickets.iter().all(|t| t.sprint.is_none()));
}

#[test]
fn summaries_cannot_be_sprints() {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let s = p.append_task("Phase");
    let c = p.append_task("Child");
    p.indent(&[c]).unwrap();
    schedule(&mut p);
    assert!(p.set_sprint(s, true).is_err());
    assert!(p.set_sprint(c, true).is_ok());
}

#[test]
fn burndown_follows_done_dates() {
    let (mut p, ua, _) = two_sprints();
    let done = p.board.done_column();
    let ids: Vec<u32> = (0..4).map(|k| pts(&mut p, Some(ua), &format!("T{k}"), 2.0)).collect();
    // two done on day 2 (Tue 1/6), one on day 4 (Thu 1/8)
    for (id, when) in [(ids[0], dt(1, 6, 10)), (ids[1], dt(1, 6, 15)), (ids[2], dt(1, 8, 11))] {
        p.move_ticket(id, Some(ua), done, None).unwrap();
        let k = p.ticket_index(id).unwrap();
        p.tickets[k].done_at = Some(when);
    }
    schedule(&mut p);
    let b = p.burndown(ua, dt(1, 9, 12)).unwrap(); // Friday noon of week 1
    assert_eq!(b.days.len(), 10);
    assert_eq!(b.total, 8.0);
    assert_eq!(b.ideal[0], 8.0);
    assert_eq!(*b.ideal.last().unwrap(), 0.0);
    assert_eq!(b.actual[0], Some(8.0));
    assert_eq!(b.actual[1], Some(8.0), "nothing done on Monday");
    assert_eq!(b.actual[2], Some(4.0), "two done Tuesday");
    assert_eq!(b.actual[3], Some(4.0));
    assert_eq!(b.actual[4], Some(2.0), "one more Thursday");
    assert_eq!(b.actual[5], Some(2.0), "today (live)");
    assert!(b.actual[6..].iter().all(|a| a.is_none()), "future days are blank");
    // before the sprint starts nothing is drawn
    let early = p.burndown(ua, dt(1, 1, 9)).unwrap();
    assert!(early.actual.iter().all(|a| a.is_none()));
}

#[test]
fn velocity_per_sprint_and_average() {
    let (mut p, ua, ub) = two_sprints();
    let done = p.board.done_column();
    let a1 = pts(&mut p, Some(ua), "A1", 5.0);
    pts(&mut p, Some(ua), "A2", 3.0);
    let b1 = pts(&mut p, Some(ub), "B1", 8.0);
    pts(&mut p, Some(ub), "B2", 1.0);
    pts(&mut p, None, "Backlog item", 13.0);
    p.move_ticket(a1, Some(ua), done, None).unwrap();
    p.move_ticket(b1, Some(ub), done, None).unwrap();
    schedule(&mut p);
    let v = p.velocity();
    assert_eq!(v.len(), 2, "backlog isn't a sprint");
    assert_eq!((v[0].planned, v[0].completed), (8.0, 5.0));
    assert_eq!((v[1].planned, v[1].completed), (9.0, 8.0));
    assert_eq!(p.average_velocity(dt(1, 10, 9)), None, "no sprint has ended yet");
    assert_eq!(p.average_velocity(dt(1, 20, 9)), Some(5.0), "only sprint 1 has ended");
    assert_eq!(p.average_velocity(dt(3, 1, 9)), Some(6.5));
    // ticket count mode
    p.board.estimate = Estimate::TicketCount;
    let v = p.velocity();
    assert_eq!((v[0].planned, v[0].completed), (2.0, 1.0));
    // current sprint
    assert_eq!(p.current_sprint(dt(1, 7, 9)), Some(ua));
    assert_eq!(p.current_sprint(dt(1, 20, 9)), Some(ub));
    assert_eq!(p.current_sprint(dt(6, 1, 9)), Some(ub), "after the last, the last");
    // the weekend after sprint 1 ends (Fri 1/16): still sprint 1, as in Jira,
    // until it's completed
    assert_eq!(p.current_sprint(dt(1, 17, 10)), Some(ua));
    p.complete_sprint(ua, None, dt(1, 16, 18)).unwrap();
    assert_eq!(p.current_sprint(dt(1, 17, 10)), Some(ub));
}

#[test]
fn assignees_are_labels_and_book_no_work() {
    let (mut p, ua, _) = two_sprints();
    let r = p.add_resource("Dev", ResourceType::Work);
    let uid = p.resources[r].uid;
    let id = pts(&mut p, Some(ua), "Story", 8.0);
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].assignee = Some(uid);
    schedule(&mut p);
    assert_eq!(p.calc.resources[r].work, 0);
    assert!(p.assignments.is_empty());
}

#[test]
fn board_survives_save_and_old_files_load() {
    let (mut p, ua, _) = two_sprints();
    let id = pts(&mut p, Some(ua), "Story", 3.0);
    p.board.estimate = Estimate::TicketCount;
    p.board.columns.insert(2, "QA".into());
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.tickets, p.tickets);
    assert_eq!(q.board, p.board);
    assert!(q.tasks.iter().filter(|t| t.is_sprint).count() == 2);
    assert_eq!(q.ticket(id).unwrap().points, Some(3.0));
    // a 0.4 file has none of the board fields
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let proj = v["project"].as_object_mut().unwrap();
    proj.remove("tickets");
    proj.remove("board");
    proj.remove("next_ticket_id");
    for t in proj["tasks"].as_array_mut().unwrap() {
        t.as_object_mut().unwrap().remove("is_sprint");
    }
    let old = projectfcg::io::from_json(&serde_json::to_string(&v).unwrap()).unwrap();
    assert!(old.tickets.is_empty());
    assert_eq!(old.board, Default::default());
    assert!(old.tasks.iter().all(|t| !t.is_sprint));
    // new ticket ids never collide with loaded ones
    let mut q = q;
    let n = q.add_ticket("Next", None).unwrap();
    assert!(n > id);
}

#[test]
fn agile_sample_runs_sprint_3_on_any_day_of_the_week() {
    // Mon 10/26 … Sun 11/1/2026, morning and evening
    for d in 26..=32u32 {
        for h in [9, 20] {
            let date = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap() + chrono::Duration::days(d as i64 - 1);
            let now = date.and_hms_opt(h, 0, 0).unwrap();
            let p = projectfcg::app::sample::agile_release_at(now);
            let s3 = p.tasks[p.sprint_rows()[2]].uid;
            assert_eq!(p.current_sprint(now), Some(s3), "{now}");
            assert!(p.board.sprint_meta(s3).is_some_and(|m| m.started.is_some() && m.completed.is_none()), "{now}");
            assert!(p.tickets.iter().filter(|t| t.sprint == Some(s3)).count() > 3, "{now}");
        }
    }
}

#[test]
fn agile_sample_is_consistent() {
    // a fixed "today": Tue 10/27/2026
    let now = dt(10, 27, 11);
    let p = projectfcg::app::sample::agile_release_at(now);
    assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
    let sprints = p.sprint_rows();
    assert_eq!(sprints.len(), 4);
    let v = p.velocity();
    assert_eq!(v.iter().map(|r| r.completed).collect::<Vec<_>>(), vec![19.0, 18.0, 7.0, 0.0]);
    assert_eq!(p.average_velocity(now), Some(18.5), "sprints 1 and 2 have ended");
    assert_eq!(p.tasks[sprints[0]].percent_complete, 100.0);
    assert_eq!(p.current_sprint(now), Some(p.tasks[sprints[2]].uid));
    // nothing is done in the future, and done dates fall inside their sprint
    for t in p.tickets.iter().filter(|t| p.is_done(t)) {
        let at = t.done_at.expect("done tickets are stamped");
        assert!(at <= now);
        let s = p.task_by_uid(t.sprint.unwrap()).unwrap();
        assert!(s.calc.start.unwrap() <= at && at <= s.calc.finish.unwrap(), "{}", t.title);
    }
    // the burndown of the running sprint has today's point and no later ones
    let b = p.burndown(p.tasks[sprints[2]].uid, now).unwrap();
    assert_eq!(b.total, 26.0);
    let shown = b.actual.iter().filter(|a| a.is_some()).count();
    assert_eq!(shown, 8, "start + Mon..Tue of week 2 (7 days elapsed, today live)");
    // round trip is exact
    let json = projectfcg::io::to_json(&p).unwrap();
    assert_eq!(projectfcg::io::to_json(&projectfcg::io::from_json(&json).unwrap()).unwrap(), json);
}

// ---------------------------------------------------------------- 0.7: sprint lifecycle, columns

#[test]
fn start_and_complete_sprint_freeze_the_numbers() {
    let (mut p, ua, ub) = two_sprints();
    let a = pts(&mut p, Some(ua), "a", 3.0);
    let b = pts(&mut p, Some(ua), "b", 5.0);
    let sub = p.add_ticket_of("sub", None, projectfcg::board::IssueType::Subtask, Some(b)).unwrap();
    p.start_sprint(ua, "Ship the API", dt(1, 5, 8)).unwrap();
    assert!(p.start_sprint(ua, "again", dt(1, 5, 9)).is_err());
    assert_eq!(p.board.sprint_meta(ua).unwrap().committed, Some(8.0));
    // scope added mid-sprint doesn't change the plan in the velocity report
    pts(&mut p, Some(ua), "late add", 2.0);
    let done = p.board.done_column();
    p.move_ticket(a, Some(ua), done, None).unwrap();
    let v = &p.velocity()[0];
    assert_eq!((v.planned, v.completed), (8.0, 3.0));
    // complete: unfinished tickets (with sub-tasks) move to the next sprint
    assert_eq!(p.next_sprint_after(ua), Some(ub));
    let moved = p.complete_sprint(ua, Some(ub), dt(1, 16, 17)).unwrap();
    assert_eq!(moved, 2, "b and the late add");
    assert_eq!(p.ticket(b).unwrap().sprint, Some(ub));
    assert_eq!(p.ticket(sub).unwrap().sprint, Some(ub));
    assert!(p.complete_sprint(ua, None, dt(1, 16, 17)).is_err());
    let m = p.board.sprint_meta(ua).unwrap();
    assert_eq!((m.completed_value, m.carried_over, m.goal.as_str()), (Some(3.0), 2, "Ship the API"));
    schedule(&mut p);
    let row = p.index_of_uid(ua).unwrap();
    assert_eq!(p.tasks[row].percent_complete, 100.0, "only finished tickets remain");
    // reopening the done ticket later doesn't rewrite the completed sprint's velocity
    p.move_ticket(a, Some(ua), 0, None).unwrap();
    assert_eq!(p.velocity()[0].completed, 3.0);
    p.reopen_sprint(ua).unwrap();
    assert_eq!(p.velocity()[0].completed, 0.0);
    // completing an unstarted sprint commits its current scope
    p.complete_sprint(ub, None, dt(1, 30, 17)).unwrap();
    assert!(p.board.sprint_meta(ub).unwrap().committed.is_some());
    assert_eq!(p.sprint_stats(Some(ub)).tickets, 0, "everything unfinished went to the backlog");
}

#[test]
fn editing_columns_remaps_tickets_and_done() {
    let (mut p, ua, _) = two_sprints();
    let todo = pts(&mut p, Some(ua), "todo", 1.0);
    let review = pts(&mut p, Some(ua), "review", 1.0);
    let done = pts(&mut p, Some(ua), "done", 1.0);
    p.move_ticket(review, Some(ua), 2, None).unwrap();
    p.move_ticket(done, Some(ua), 3, None).unwrap();
    // remove Review, add QA before Done, WIP 2 on In Progress
    let cols = vec![
        ("To Do".to_string(), Some(0), None),
        ("In Progress".to_string(), Some(1), Some(2)),
        ("QA".to_string(), None, None),
        ("Done".to_string(), Some(3), None),
    ];
    p.set_board_columns(&cols, dt(1, 7, 9)).unwrap();
    assert_eq!(p.ticket(todo).unwrap().column, 0);
    assert_eq!(p.ticket(review).unwrap().column, 1, "Review's cards move to the column before it");
    assert_eq!(p.ticket(done).unwrap().column, 3);
    assert!(p.is_done(p.ticket(done).unwrap()));
    assert_eq!(p.board.wip_limit(1), Some(2));
    // a new last column becomes Done: what was done isn't any more
    let mut cols2: Vec<(String, Option<usize>, Option<u32>)> = p.board.columns.iter().enumerate().map(|(i, c)| (c.clone(), Some(i), None)).collect();
    cols2.push(("Released".into(), None, None));
    p.set_board_columns(&cols2, dt(1, 7, 10)).unwrap();
    assert!(!p.is_done(p.ticket(done).unwrap()));
    assert_eq!(p.ticket(done).unwrap().done_at, None);
    assert!(p.set_board_columns(&[("Only".into(), Some(0), None)], dt(1, 7, 11)).is_err());
}

#[test]
fn cumulative_flow_from_column_history() {
    let (mut p, ua, _) = two_sprints();
    let a = pts(&mut p, Some(ua), "A", 3.0);
    let b = pts(&mut p, Some(ua), "B", 3.0);
    let c = pts(&mut p, Some(ua), "C", 3.0);
    let done = p.board.done_column();
    for id in [a, b, c] {
        let k = p.ticket_index(id).unwrap();
        p.tickets[k].created = Some(dt(1, 5, 8));
    }
    // A: In Progress Tue, Done Thu. B: In Progress Wed. C: untouched.
    let k = p.ticket_index(a).unwrap();
    p.tickets[k].history = vec![(dt(1, 6, 10), 1), (dt(1, 8, 15), done)];
    p.tickets[k].column = done;
    p.tickets[k].done_at = Some(dt(1, 8, 15));
    let k = p.ticket_index(b).unwrap();
    p.tickets[k].history = vec![(dt(1, 7, 9), 1)];
    p.tickets[k].column = 1;
    schedule(&mut p);
    let now = dt(1, 9, 12); // Friday noon
    let cf = p.cumulative_flow(ua, now).unwrap();
    assert_eq!(cf.columns.len(), 4);
    let day = |m, d| cf.days.iter().position(|x| *x == NaiveDate::from_ymd_opt(2026, m, d).unwrap()).unwrap();
    assert_eq!(cf.counts[day(1, 5)], Some(vec![3, 0, 0, 0]));
    assert_eq!(cf.counts[day(1, 6)], Some(vec![2, 1, 0, 0]));
    assert_eq!(cf.counts[day(1, 7)], Some(vec![1, 2, 0, 0]));
    assert_eq!(cf.counts[day(1, 8)], Some(vec![1, 1, 0, 1]));
    assert_eq!(cf.counts[day(1, 9)], Some(vec![1, 1, 0, 1]), "today: where they are now");
    assert_eq!(cf.counts[day(1, 12)], None, "the future is blank");
    // a ticket with no recorded moves: To Do until it was done
    let k = p.ticket_index(c).unwrap();
    p.tickets[k].column = done;
    p.tickets[k].done_at = Some(dt(1, 7, 11));
    assert_eq!(p.column_at(&p.tickets[k], dt(1, 6, 23), now), Some(0));
    assert_eq!(p.column_at(&p.tickets[k], dt(1, 7, 23), now), Some(done));
    // moving records history, and editing columns remaps it
    p.move_ticket(b, Some(ua), 2, None).unwrap();
    let hb = p.ticket(b).unwrap().history.clone();
    assert_eq!(hb.len(), 2);
    assert_eq!(hb[1].1, 2);
    let cols: Vec<(String, Option<usize>, Option<u32>)> = vec![("To Do".into(), Some(0), None), ("Doing".into(), Some(1), None), ("Done".into(), Some(3), None)];
    p.set_board_columns(&cols, dt(1, 9, 13)).unwrap();
    let hb = &p.ticket(b).unwrap().history;
    assert_eq!(hb.iter().map(|h| h.1).collect::<Vec<_>>(), vec![1, 1], "Review folded into Doing");
}
