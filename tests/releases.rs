//! Releases (fix versions): progress, projected date, risk, the milestone
//! that stands for a release, and Jira CSV.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::board::IssueType;
use projectfcg::model::{LinkType, Project};
use projectfcg::ops::Field;
use projectfcg::releases::ReleaseStatus;
use projectfcg::schedule::schedule;

fn day(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap()
}
fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    day(m, d).and_hms_opt(h, 0, 0).unwrap()
}

/// Sprint 1 (1/5–1/16), Sprint 2 (1/19–1/30), Sprint 3 (2/2–2/13).
fn plan() -> (Project, [u32; 3]) {
    let mut p = Project::new(day(1, 5));
    p.board.key_prefix = "APP".into();
    let mut s = [0; 3];
    for k in &mut s {
        let r = p.new_sprint();
        schedule(&mut p);
        *k = p.tasks[r].uid;
    }
    (p, s)
}

fn story(p: &mut Project, sprint: Option<u32>, pts: f64) -> u32 {
    let id = p.add_ticket_of("Story", sprint, IssueType::Story, None).unwrap();
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].points = Some(pts);
    id
}

fn finish(p: &mut Project, id: u32, when: NaiveDateTime) {
    let k = p.ticket_index(id).unwrap();
    let sprint = p.tickets[k].sprint;
    p.move_ticket(id, sprint, p.board.done_column(), None).unwrap();
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].done_at = Some(when);
}

#[test]
fn progress_projection_and_risk() {
    let (mut p, s) = plan();
    let r = p.add_release("1.0", Some(day(1, 30))).unwrap();
    let a = story(&mut p, Some(s[0]), 3.0);
    let b = story(&mut p, Some(s[1]), 5.0);
    for id in [a, b] {
        p.set_ticket_release(id, Some(r)).unwrap();
    }
    schedule(&mut p);
    let st = p.release_stats(r);
    assert_eq!((st.tickets, st.total), (2, 8.0));
    assert_eq!(st.projected, Some(dt(1, 30, 17)), "the end of Sprint 2");
    assert_eq!(st.status, ReleaseStatus::OnTrack);
    // a story slips into Sprint 3: past the release date
    p.move_ticket(b, Some(s[2]), 0, None).unwrap();
    schedule(&mut p);
    let st = p.release_stats(r);
    assert_eq!(st.projected, Some(dt(2, 13, 17)));
    assert_eq!(st.status, ReleaseStatus::AtRisk);
    // unplanned work: no projection
    p.move_ticket(b, None, 0, None).unwrap();
    let st = p.release_stats(r);
    assert_eq!((st.unplanned, st.projected, st.status), (1, None, ReleaseStatus::Unplanned));
    // everything done: ready, projected at the last done date
    p.move_ticket(b, Some(s[0]), 0, None).unwrap();
    finish(&mut p, a, dt(1, 8, 12));
    finish(&mut p, b, dt(1, 14, 10));
    let st = p.release_stats(r);
    assert_eq!((st.status, st.projected, st.percent()), (ReleaseStatus::Ready, Some(dt(1, 14, 10)), 100.0));
}

#[test]
fn only_stories_tasks_and_bugs_carry_a_release() {
    let (mut p, s) = plan();
    let r = p.add_release("1.0", None).unwrap();
    let epic = p.add_ticket_of("Epic", None, IssueType::Epic, None).unwrap();
    let st = story(&mut p, Some(s[0]), 2.0);
    let sub = p.add_ticket_of("Sub", None, IssueType::Subtask, Some(st)).unwrap();
    assert!(p.set_ticket_release(epic, Some(r)).unwrap_err().contains("Epics"));
    assert!(p.set_ticket_release(sub, Some(r)).unwrap_err().contains("parent"));
    assert!(p.add_release("1.0", None).is_err(), "names are unique");
    assert!(p.add_release("  ", None).is_err());
}

#[test]
fn a_milestone_follows_its_release() {
    let (mut p, s) = plan();
    let r = p.add_release("2.0", Some(day(2, 6))).unwrap();
    let a = story(&mut p, Some(s[0]), 3.0);
    let b = story(&mut p, Some(s[1]), 3.0);
    p.set_ticket_release(a, Some(r)).unwrap();
    p.set_ticket_release(b, Some(r)).unwrap();
    let m = p.add_release_milestone(r).unwrap();
    let launch = p.append_task("Launch campaign");
    p.set_field_text(launch, Field::Duration, "2d").unwrap();
    p.set_field_text(launch, Field::Predecessors, &format!("{}", m + 1)).unwrap();
    schedule(&mut p);
    let mt = &p.tasks[m];
    assert!(mt.is_milestone());
    assert_eq!(mt.name, "Release 2.0");
    assert_eq!(mt.deadline, Some(dt(2, 6, 17)), "the release date is the milestone's deadline");
    assert_eq!(mt.calc.finish, Some(dt(1, 30, 17)), "after Sprint 2, which holds the last open story");
    assert_eq!(mt.calc.board_driving, vec![b]);
    assert_eq!(p.tasks[launch].calc.start, Some(dt(2, 2, 8)), "tasks linked to the milestone follow the release");
    // the story slips: the milestone, the campaign and the projection move
    p.move_ticket(b, Some(s[2]), 0, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[m].calc.finish, Some(dt(2, 13, 17)));
    assert_eq!(p.tasks[launch].calc.start, Some(dt(2, 16, 8)));
    let st = p.release_stats(r);
    assert_eq!(st.projected, p.tasks[m].calc.finish);
    assert_eq!(st.status, ReleaseStatus::AtRisk);
    assert!(p.tasks[m].calc.deadline_missed, "the missed deadline shows on the Gantt");
    // changing the date moves the deadline
    p.update_release(r, "2.0", "", None, Some(day(2, 20))).unwrap();
    assert_eq!(p.tasks[m].deadline, Some(dt(2, 20, 17)));
    // deleting the milestone unties the release
    p.delete_tasks(&[m]);
    assert_eq!(p.release(r).unwrap().task, None);
}

#[test]
fn a_milestone_waiting_loop_is_reported_not_scheduled() {
    let (mut p, s) = plan();
    let r = p.add_release("1.0", None).unwrap();
    let a = story(&mut p, Some(s[1]), 3.0);
    p.set_ticket_release(a, Some(r)).unwrap();
    let m = p.add_release_milestone(r).unwrap();
    // Sprint 2 waits on the milestone (FS link), and the milestone waits on Sprint 2
    let s2 = p.index_of_uid(s[1]).unwrap();
    p.add_link(m, s2, LinkType::FS, Default::default()).unwrap();
    schedule(&mut p);
    assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
    assert!(p.tasks[m].calc.board_notes.iter().any(|n| n.contains("loop")), "{:?}", p.tasks[m].calc.board_notes);
}

#[test]
fn releasing_moves_unfinished_stories() {
    let (mut p, s) = plan();
    let r1 = p.add_release("1.0", None).unwrap();
    let r2 = p.add_release("1.1", None).unwrap();
    let a = story(&mut p, Some(s[0]), 3.0);
    let b = story(&mut p, Some(s[0]), 3.0);
    p.set_ticket_release(a, Some(r1)).unwrap();
    p.set_ticket_release(b, Some(r1)).unwrap();
    finish(&mut p, a, dt(1, 9, 9));
    assert!(p.release_version(r1, dt(1, 16, 17), Some(r1)).is_err());
    assert_eq!(p.release_version(r1, dt(1, 16, 17), Some(r2)).unwrap(), 1);
    assert_eq!(p.ticket(b).unwrap().release, Some(r2));
    assert_eq!(p.ticket(a).unwrap().release, Some(r1));
    assert_eq!(p.release_stats(r1).status, ReleaseStatus::Released);
    assert!(p.release_version(r1, dt(1, 16, 17), None).is_err());
    p.unrelease_version(r1).unwrap();
    // deleting a release keeps its stories
    p.delete_release(r2).unwrap();
    assert_eq!(p.ticket(b).unwrap().release, None);
}

#[test]
fn repair_cleans_releases() {
    let (mut p, s) = plan();
    let r = p.add_release("1.0", None).unwrap();
    let a = story(&mut p, Some(s[0]), 3.0);
    let k = p.ticket_index(a).unwrap();
    p.tickets[k].release = Some(99);
    p.board.releases.push(projectfcg::releases::Release { id: r, name: "dup".into(), ..Default::default() });
    p.board.releases[0].task = Some(s[0]); // a sprint can't stand for a release
    let fixed = p.repair_board();
    assert!(fixed >= 3);
    assert_eq!(p.ticket(a).unwrap().release, None);
    assert_eq!(p.board.releases.len(), 1);
    assert_eq!(p.board.releases[0].task, None);
}

#[test]
fn jira_csv_carries_fix_versions() {
    let (mut p, _) = plan();
    let csv = "Summary,Issue key,Issue Type,Status,Fix Version/s,Fix Version/s\n\
               Login,APP-1,Story,To Do,1.0,\n\
               Logout,APP-2,Story,To Do,2.0,2.1\n\
               Epic,APP-3,Epic,To Do,1.0,\n";
    let rep = projectfcg::jira::import_csv(&mut p, csv, &Default::default()).unwrap();
    let v1 = p.release_by_name("1.0").unwrap();
    let v2 = p.release_by_name("2.0").unwrap();
    let login = p.tickets.iter().find(|t| t.title == "Login").unwrap();
    let logout = p.tickets.iter().find(|t| t.title == "Logout").unwrap();
    assert_eq!(login.release, Some(v1));
    assert_eq!(logout.release, Some(v2));
    assert!(p.release_by_name("2.1").is_none(), "only the first fix version is kept");
    assert!(rep.warnings.iter().any(|n| n.contains("2.1")), "{:?}", rep.warnings);
    let out = projectfcg::jira::export_csv(&p);
    assert!(out.lines().next().unwrap().contains("Fix Version/s"));
    assert!(out.contains("Login") && out.contains(",1.0"), "{out}");
}
