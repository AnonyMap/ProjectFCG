//! Issue types, epics, sub-tasks, ticket links, and Jira CSV.
use chrono::NaiveDate;
use projectfcg::board::{IssueType, LinkKind, Priority};
use projectfcg::jira::{self, ImportOptions};
use projectfcg::model::Project;
use projectfcg::schedule::schedule;

fn two_sprints() -> (Project, u32, u32) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let a = p.new_sprint();
    schedule(&mut p);
    let b = p.new_sprint();
    schedule(&mut p);
    let (ua, ub) = (p.tasks[a].uid, p.tasks[b].uid);
    (p, ua, ub)
}

fn story(p: &mut Project, sprint: Option<u32>, title: &str, points: f64, epic: Option<u32>) -> u32 {
    let id = p.add_ticket_of(title, sprint, IssueType::Story, epic).unwrap();
    let k = p.ticket_index(id).unwrap();
    p.tickets[k].points = Some(points);
    id
}

#[test]
fn epics_stay_out_of_sprints_and_track_their_stories() {
    let (mut p, ua, ub) = two_sprints();
    let e = p.add_ticket_of("Checkout", Some(ua), IssueType::Epic, None).unwrap();
    assert_eq!(p.ticket(e).unwrap().sprint, None, "an epic never sits in a sprint");
    assert!(p.move_ticket(e, Some(ua), 0, None).is_err());
    let s1 = story(&mut p, Some(ua), "Cart", 3.0, Some(e));
    let _s2 = story(&mut p, Some(ub), "Pay", 5.0, Some(e));
    let _loose = story(&mut p, Some(ua), "Other", 2.0, None);
    // epic progress spans sprints
    let st = p.epic_stats(e);
    assert_eq!((st.tickets, st.total, st.done), (2, 8.0, 0.0));
    let done = p.board.done_column();
    p.move_ticket(s1, Some(ua), done, None).unwrap();
    assert_eq!(p.epic_stats(e).done, 3.0);
    // the epic itself counts nothing in sprint stats
    assert_eq!(p.sprint_stats(Some(ua)).total, 5.0);
    assert_eq!(p.sprint_stats(None).tickets, 0, "the epic isn't in the backlog count");
    // a story can only belong to an epic
    assert!(p.set_ticket_parent(s1, Some(_loose)).is_err());
}

#[test]
fn subtasks_follow_their_parent_and_count_nothing() {
    let (mut p, ua, ub) = two_sprints();
    let s = story(&mut p, Some(ua), "Login", 5.0, None);
    let sub = p.add_ticket_of("Form", None, IssueType::Subtask, Some(s)).unwrap();
    let k = p.ticket_index(sub).unwrap();
    p.tickets[k].points = Some(8.0);
    assert_eq!(p.ticket(sub).unwrap().sprint, Some(ua), "sub-task takes the parent's sprint");
    let st = p.sprint_stats(Some(ua));
    assert_eq!((st.tickets, st.total), (1, 5.0), "sub-task points aren't double-counted");
    // can't move a sub-task to another sprint on its own
    assert!(p.move_ticket(sub, Some(ub), 0, None).is_err());
    // but it can change column within the sprint
    p.move_ticket(sub, Some(ua), 1, None).unwrap();
    // moving the parent brings the sub-task along
    p.move_ticket(s, Some(ub), 0, None).unwrap();
    assert_eq!(p.ticket(sub).unwrap().sprint, Some(ub));
    assert_eq!(p.ticket(sub).unwrap().column, 1, "keeps its own status");
    // a sub-task needs a story/task/bug parent
    assert!(p.add_ticket_of("Orphan", None, IssueType::Subtask, None).is_err());
    let e = p.add_ticket_of("Epic", None, IssueType::Epic, None).unwrap();
    assert!(p.set_ticket_parent(sub, Some(e)).is_err());
    // a story with sub-tasks can't become an epic or sub-task
    assert!(p.set_ticket_kind(s, IssueType::Epic).is_err());
    assert!(p.set_ticket_kind(s, IssueType::Bug).is_ok());
    // deleting the parent deletes its sub-tasks
    p.delete_ticket(s).unwrap();
    assert!(p.ticket(sub).is_none());
}

#[test]
fn deleting_an_epic_keeps_its_stories() {
    let (mut p, ua, _) = two_sprints();
    let e = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let s = story(&mut p, Some(ua), "S", 1.0, Some(e));
    p.delete_ticket(e).unwrap();
    assert_eq!(p.ticket(s).unwrap().parent, None);
}

#[test]
fn links_both_directions_blockers_and_cycles() {
    let (mut p, ua, _) = two_sprints();
    let a = story(&mut p, Some(ua), "API", 3.0, None);
    let b = story(&mut p, Some(ua), "UI", 3.0, None);
    let c = story(&mut p, Some(ua), "Docs", 1.0, None);
    p.add_ticket_link(a, LinkKind::Blocks, b).unwrap();
    p.add_ticket_link(b, LinkKind::Blocks, c).unwrap();
    p.add_ticket_link(c, LinkKind::Relates, a).unwrap();
    assert!(p.add_ticket_link(a, LinkKind::Blocks, b).is_err(), "no duplicates");
    assert!(p.add_ticket_link(c, LinkKind::Blocks, a).is_err(), "no blocking cycles");
    assert!(p.add_ticket_link(a, LinkKind::Relates, c).is_err(), "relates is symmetric");
    assert!(p.add_ticket_link(a, LinkKind::Blocks, a).is_err());
    let lb = p.ticket_links(b);
    assert_eq!(lb.len(), 2);
    assert!(lb.iter().any(|l| l.verb() == "is blocked by" && l.other == a));
    assert!(lb.iter().any(|l| l.verb() == "blocks" && l.other == c));
    assert_eq!(p.blockers(b), vec![a]);
    // a done blocker no longer blocks
    let done = p.board.done_column();
    p.move_ticket(a, Some(ua), done, None).unwrap();
    assert!(p.blockers(b).is_empty());
    // removing from the inward side
    let inward = p.ticket_links(b).into_iter().find(|l| !l.outward).unwrap();
    p.remove_ticket_link(b, &inward).unwrap();
    assert!(p.ticket(a).unwrap().links.is_empty());
    // deleting a ticket removes links to it
    p.delete_ticket(c).unwrap();
    assert!(p.ticket(b).unwrap().links.is_empty());
}

#[test]
fn epic_drives_a_gantt_task() {
    let (mut p, ua, ub) = two_sprints();
    let row = p.append_task("Checkout release");
    schedule(&mut p);
    let uid = p.tasks[row].uid;
    let e = p.add_ticket_of("Checkout", None, IssueType::Epic, None).unwrap();
    let s1 = story(&mut p, Some(ua), "Cart", 3.0, Some(e));
    story(&mut p, Some(ub), "Pay", 1.0, Some(e));
    p.set_epic_task(e, Some(uid)).unwrap();
    assert!(p.set_epic_task(e, Some(ua)).is_err(), "not a sprint");
    assert!(p.set_epic_task(s1, Some(uid)).is_err(), "only epics");
    let done = p.board.done_column();
    p.move_ticket(s1, Some(ua), done, None).unwrap();
    schedule(&mut p);
    assert!(p.progress_from_board(row));
    assert_eq!(p.tasks[row].percent_complete, 75.0);
    assert!(p.set_percent_complete(row, 10.0).is_err());
    // deleting the task unlinks the epic
    p.delete_tasks(&[row]);
    assert_eq!(p.ticket(e).unwrap().task, None);
}

#[test]
fn ticket_keys() {
    let (mut p, ua, _) = two_sprints();
    let a = story(&mut p, Some(ua), "A", 1.0, None);
    assert_eq!(p.ticket_key(a), format!("#{a}"));
    p.board.key_prefix = "FCG".into();
    assert_eq!(p.ticket_key(a), format!("FCG-{a}"));
    assert_eq!(p.ticket_by_key(&format!("fcg-{a}")), Some(a));
    assert_eq!(p.ticket_by_key("FCG-999"), None);
}

const JIRA: &str = "\u{feff}Summary,Issue key,Issue id,Issue Type,Status,Priority,Assignee,Created,Resolved,Sprint,Sprint,Labels,Labels,Custom field (Story point estimate),Parent,Outward issue link (Blocks),Inward issue link (Blocks),Outward issue link (Relates),Description\r
Checkout epic,SHOP-1,10001,Epic,In Progress,High,,01/Jan/26 9:00 AM,,,,web,,,,,,,\"Everything to buy things\"\r
Cart page,SHOP-2,10002,Story,Done,Medium,Ada Lovelace,02/Jan/26 9:00 AM,09/Jan/26 4:30 PM,Sprint 1,Sprint 2,web,ui,5,10001,SHOP-3,,,\r
Payment API,SHOP-3,10003,Story,In Review,Highest,Grace Hopper,02/Jan/26 9:00 AM,,Sprint 2,,api,,8,10001,,,SHOP-5,\"Multi-line\nnotes, with comma\"\r
Card form,SHOP-4,10004,Sub-task,To Do,Low,Ada Lovelace,03/Jan/26 9:00 AM,,Sprint 2,,,,2,10003,,,,\r
Flaky test,SHOP-5,10005,Bug,Blocked,Lowest,,04/Jan/26 9:00 AM,,,,,,,,,SHOP-3,,\r
";

#[test]
fn jira_csv_import() {
    let (mut p, _ua, _ub) = two_sprints();
    let opts = ImportOptions { create_sprints: true, create_people: true };
    let rep = jira::import_csv(&mut p, JIRA, &opts).unwrap();
    assert_eq!(rep.tickets, 5);
    assert_eq!(rep.epics, 1);
    assert_eq!(rep.links, 3, "{rep:?}");
    assert!(rep.sprints_created.is_empty(), "Sprint 1/2 already exist: {rep:?}");
    assert_eq!(rep.people_created.len(), 2);
    assert_eq!(p.board.key_prefix, "SHOP");
    // keys kept
    let cart = p.ticket_by_key("SHOP-2").unwrap();
    assert_eq!(cart, 2);
    let t = p.ticket(cart).unwrap();
    assert_eq!(t.kind, IssueType::Story);
    assert_eq!(t.points, Some(5.0));
    assert_eq!(t.labels, vec!["web", "ui"]);
    assert_eq!(t.parent, Some(1));
    assert!(p.is_done(t));
    assert_eq!(t.done_at.unwrap().to_string(), "2026-01-09 16:30:00");
    // last sprint column wins
    let s2 = p.tasks.iter().find(|x| x.name == "Sprint 2").unwrap().uid;
    assert_eq!(t.sprint, Some(s2));
    let api = p.ticket(3).unwrap();
    assert_eq!(api.priority, Priority::Highest);
    assert_eq!(api.column, 2, "In Review → Review column");
    assert_eq!(api.description, "Multi-line\nnotes, with comma");
    let sub = p.ticket(4).unwrap();
    assert_eq!((sub.kind, sub.parent, sub.sprint), (IssueType::Subtask, Some(3), Some(s2)));
    // links: SHOP-2 blocks SHOP-3; SHOP-3 blocked SHOP-5 (inward on 5); 3 relates to 5
    assert_eq!(p.blockers(3), Vec::<u32>::new(), "SHOP-2 is done");
    assert_eq!(p.blockers(5), vec![3]);
    assert!(p.ticket_links(5).iter().any(|l| l.kind == LinkKind::Relates));
    // "Blocked" isn't a column: warned, first column
    assert_eq!(p.ticket(5).unwrap().column, 0);
    assert!(rep.warnings.iter().any(|w| w.contains("Blocked")), "{:?}", rep.warnings);
    assert_eq!(p.resources.iter().filter(|r| r.name == "Ada Lovelace").count(), 1);
    assert_eq!(p.epic_stats(1).total, 13.0);
}

#[test]
fn jira_import_creates_missing_sprints_in_order() {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let csv = "Summary,Sprint\nA,Team Sprint 10\nB,Team Sprint 9\nC,\n";
    let rep = jira::import_csv(&mut p, csv, &ImportOptions { create_sprints: true, create_people: false }).unwrap();
    assert_eq!(rep.sprints_created, vec!["Team Sprint 9", "Team Sprint 10"]);
    schedule(&mut p);
    let rows = p.sprint_rows();
    assert_eq!(p.tasks[rows[0]].name, "Team Sprint 9");
    // without creating, they land in the backlog
    let mut q = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let rep = jira::import_csv(&mut q, csv, &ImportOptions::default()).unwrap();
    assert_eq!(q.sprint_stats(None).tickets, 3);
    assert!(!rep.warnings.is_empty());
    assert!(jira::import_csv(&mut q, "Name,Value\nx,y\n", &ImportOptions::default()).is_err());
}

#[test]
fn jira_import_renumbers_on_clash() {
    let (mut p, ua, _) = two_sprints();
    story(&mut p, Some(ua), "Existing", 1.0, None); // id 1
    let csv = "Summary,Issue key,Issue Type,Parent key\nE,ABC-1,Epic,\nS,ABC-2,Story,ABC-1\n";
    let rep = jira::import_csv(&mut p, csv, &ImportOptions::default()).unwrap();
    assert_eq!(rep.tickets, 2);
    assert!(rep.warnings.iter().any(|w| w.contains("renumbered")));
    let s = p.tickets.iter().find(|t| t.title == "S").unwrap();
    let e = p.tickets.iter().find(|t| t.title == "E").unwrap();
    assert_eq!(s.parent, Some(e.id), "parent resolved by key after renumbering");
}

#[test]
fn jira_csv_round_trip() {
    let (mut p, _, _) = two_sprints();
    jira::import_csv(&mut p, JIRA, &ImportOptions { create_sprints: true, create_people: true }).unwrap();
    let out = jira::export_csv(&p);
    assert!(out.starts_with("Issue key,Issue id,Summary,Issue Type"));
    assert!(out.contains("Outward issue link (Blocks)"));
    // epics before stories before sub-tasks
    let pos = |s: &str| out.find(s).unwrap();
    assert!(pos("Checkout epic") < pos("Cart page") && pos("Payment API") < pos("Card form"));
    // import the export into a fresh project with the same sprints
    let (mut q, _, _) = two_sprints();
    jira::import_csv(&mut q, &out, &ImportOptions { create_sprints: true, create_people: true }).unwrap();
    assert_eq!(q.tickets.len(), p.tickets.len());
    for t in &p.tickets {
        let u = q.ticket(t.id).unwrap_or_else(|| panic!("missing {}", t.id));
        assert_eq!((&u.title, u.kind, u.parent, u.points, u.priority, &u.labels, u.column), (&t.title, t.kind, t.parent, t.points, t.priority, &t.labels, t.column), "{}", t.title);
        assert_eq!(u.description, t.description);
        let sprint_name = |pr: &Project, s: Option<u32>| s.and_then(|x| pr.task_by_uid(x)).map(|x| x.name.clone());
        assert_eq!(sprint_name(&q, u.sprint), sprint_name(&p, t.sprint));
        let mut a = t.links.clone();
        let mut b = u.links.clone();
        a.sort_by_key(|l| (l.to, l.kind as u8));
        b.sort_by_key(|l| (l.to, l.kind as u8));
        assert_eq!(a, b, "{}", t.title);
    }
}

#[test]
fn legacy_tickets_load_as_stories() {
    let (mut p, ua, _) = two_sprints();
    story(&mut p, Some(ua), "Old", 2.0, None);
    let mut v: serde_json::Value = serde_json::from_str(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    for t in v["project"]["tickets"].as_array_mut().unwrap() {
        let o = t.as_object_mut().unwrap();
        for k in ["kind", "priority", "labels", "parent", "links", "task"] {
            o.remove(k);
        }
    }
    let q = projectfcg::io::from_json(&v.to_string()).unwrap();
    assert_eq!(q.tickets[0].kind, IssueType::Story);
    assert_eq!(q.tickets[0].priority, Priority::Medium);
}

#[path = "common/mod.rs"]
mod common;

/// The hierarchy rules hold after any sequence of board operations (each
/// either succeeds or leaves the project untouched), and every step undoes
/// exactly.
fn check_board(p: &Project, ctx: &str) {
    let mut ids: Vec<u32> = p.tickets.iter().map(|t| t.id).collect();
    ids.sort();
    assert!(ids.windows(2).all(|w| w[0] != w[1]), "{ctx}: duplicate ids");
    assert!(ids.last().is_none_or(|&m| p.next_ticket_id > m), "{ctx}: next id");
    for t in &p.tickets {
        let par = t.parent.and_then(|x| p.ticket(x));
        match t.kind {
            IssueType::Epic => assert!(t.sprint.is_none() && t.parent.is_none(), "{ctx}: epic {}", t.id),
            IssueType::Subtask => {
                let pp = par.unwrap_or_else(|| panic!("{ctx}: sub-task {} has no parent", t.id));
                assert!(pp.kind.is_standard(), "{ctx}: sub-task {} under {:?}", t.id, pp.kind);
                assert_eq!(t.sprint, pp.sprint, "{ctx}: sub-task {} sprint", t.id);
            }
            _ => {
                if t.parent.is_some() {
                    assert_eq!(par.map(|x| x.kind), Some(IssueType::Epic), "{ctx}: {} parent", t.id);
                }
                assert!(t.task.is_none(), "{ctx}: only epics drive tasks");
            }
        }
        assert!(t.parent.is_none() || par.is_some(), "{ctx}: dangling parent on {}", t.id);
        for l in &t.links {
            assert!(l.to != t.id && p.ticket(l.to).is_some(), "{ctx}: link {} → {}", t.id, l.to);
        }
        if let Some(s) = t.sprint {
            assert!(p.task_by_uid(s).is_some_and(|x| x.is_sprint), "{ctx}: sprint of {}", t.id);
        }
        assert!(t.column <= p.board.done_column());
    }
}

#[test]
fn random_board_operations_keep_the_rules() {
    use common::Rng;
    for seed in 1..=60u64 {
        let mut r = Rng::new(seed);
        let (mut p, ua, ub) = two_sprints();
        let row = p.append_task("Feature");
        schedule(&mut p);
        let feature = p.tasks[row].uid;
        let sprints = [None, Some(ua), Some(ub)];
        for step in 0..120 {
            let before = p.clone();
            let ids: Vec<u32> = p.tickets.iter().map(|t| t.id).collect();
            let any = |r: &mut Rng| if ids.is_empty() { 999 } else { *r.pick(&ids) };
            // a parent that usually fits the kind (sometimes deliberately not)
            let parent_for = |r: &mut Rng, p: &Project, k: IssueType| -> Option<u32> {
                let want: Vec<u32> = p
                    .tickets
                    .iter()
                    .filter(|t| match k {
                        IssueType::Subtask => t.kind.is_standard(),
                        IssueType::Epic => false,
                        _ => t.kind == IssueType::Epic,
                    })
                    .map(|t| t.id)
                    .collect();
                if r.chance(15) {
                    Some(any(r))
                } else if !want.is_empty() && r.chance(70) {
                    Some(*r.pick(&want))
                } else {
                    None
                }
            };
            let kinds = IssueType::ALL;
            let res: Result<(), String> = match r.below(20) {
                0..=6 => {
                    let k = *r.pick(&kinds);
                    let par = parent_for(&mut r, &p, k);
                    p.add_ticket_of(&format!("t{step}"), *r.pick(&sprints), k, par).map(|_| ())
                }
                7..=9 => p.move_ticket(any(&mut r), *r.pick(&sprints), r.below(4) as usize, None),
                10 => p.set_ticket_kind(any(&mut r), *r.pick(&kinds)),
                11 => {
                    let k = *r.pick(&kinds);
                    let par = parent_for(&mut r, &p, k);
                    p.set_ticket_type(any(&mut r), k, par)
                }
                12 | 13 => {
                    let id = any(&mut r);
                    let k = p.ticket(id).map(|t| t.kind).unwrap_or_default();
                    let par = parent_for(&mut r, &p, k);
                    p.set_ticket_parent(id, par)
                }
                14 | 15 => p.add_ticket_link(any(&mut r), *r.pick(&LinkKind::ALL), any(&mut r)),
                16 => p.delete_ticket(any(&mut r)),
                17 => p.set_epic_task(any(&mut r), if r.chance(70) { Some(feature) } else { None }),
                _ => {
                    let id = any(&mut r);
                    match p.ticket_links(id).first().cloned() {
                        Some(l) => p.remove_ticket_link(id, &l),
                        None => Ok(()),
                    }
                }
            };
            let ctx = format!("seed {seed} step {step}");
            if res.is_err() {
                // ops either succeed or fail — callers roll back via edit_project; here we do it by hand
                p = before.clone();
            }
            check_board(&p, &ctx);
            schedule(&mut p);
            // undo/redo via history deltas is exact
            let d = projectfcg::history::diff(&before, &p);
            let mut q = before.clone();
            let inv = projectfcg::history::apply(&mut q, d);
            assert_eq!(q.tickets, p.tickets, "{ctx}: redo");
            projectfcg::history::apply(&mut q, inv);
            assert_eq!(q.tickets, before.tickets, "{ctx}: undo");
        }
        // blocking links never form a cycle
        for t in &p.tickets {
            let mut stack: Vec<u32> = t.links.iter().filter(|l| l.kind == LinkKind::Blocks).map(|l| l.to).collect();
            let mut seen = std::collections::HashSet::new();
            while let Some(x) = stack.pop() {
                assert_ne!(x, t.id, "seed {seed}: blocking cycle through {}", t.id);
                if seen.insert(x) {
                    stack.extend(p.ticket(x).unwrap().links.iter().filter(|l| l.kind == LinkKind::Blocks).map(|l| l.to));
                }
            }
        }
        // Jira round trip of whatever we ended with
        if p.tickets.is_empty() {
            continue;
        }
        let csv = jira::export_csv(&p);
        let (mut q, _, _) = two_sprints();
        jira::import_csv(&mut q, &csv, &ImportOptions { create_sprints: true, create_people: true }).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        check_board(&q, &format!("seed {seed} reimport"));
    }
}

#[test]
fn hand_edited_boards_are_repaired_on_load() {
    let (mut p, ua, _) = two_sprints();
    let e = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let s = story(&mut p, Some(ua), "S", 3.0, Some(e));
    let sub = p.add_ticket_of("sub", None, IssueType::Subtask, Some(s)).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    let tickets = v["project"]["tickets"].as_array_mut().unwrap();
    let mut dup = tickets[1].clone();
    dup["title"] = "duplicate id".into();
    for t in tickets.iter_mut() {
        let id = t["id"].as_u64().unwrap() as u32;
        if id == e {
            t["sprint"] = ua.into(); // epic in a sprint
        }
        if id == sub {
            t["sprint"] = serde_json::Value::Null; // sub-task left behind
            t["column"] = 42.into();
        }
        if id == s {
            t["links"] = serde_json::json!([{ "kind": "Blocks", "to": 999 }, { "kind": "Relates", "to": s }]);
        }
    }
    tickets.push(dup);
    let q = projectfcg::io::from_json(&v.to_string()).unwrap();
    assert_eq!(q.tickets.len(), 3);
    check_board(&q, "repaired");
    assert_eq!(q.ticket(sub).unwrap().sprint, Some(ua));
    assert!(q.ticket(s).unwrap().links.is_empty());
}

/// Findings from the 0.6 review, pinned.
#[test]
fn review_regressions_board_and_jira() {
    let (mut p, ua, _) = two_sprints();
    // a sprint's % complete resets when its last ticket leaves
    let a = story(&mut p, Some(ua), "a", 1.0, None);
    let b = story(&mut p, Some(ua), "b", 1.0, None);
    let done = p.board.done_column();
    p.move_ticket(a, Some(ua), done, None).unwrap();
    schedule(&mut p);
    let row = p.index_of_uid(ua).unwrap();
    assert_eq!(p.tasks[row].percent_complete, 50.0);
    p.move_ticket(a, None, 0, None).unwrap();
    p.move_ticket(b, None, 0, None).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[row].percent_complete, 0.0, "not left at 50%");
    // a typed % on a sprint without tickets is kept
    p.tasks[row].percent_complete = 30.0;
    schedule(&mut p);
    assert_eq!(p.tasks[row].percent_complete, 30.0);
    // keys of another project aren't ours
    p.board.key_prefix = "APP".into();
    assert_eq!(p.ticket_by_key("OTHER-1"), None);
    assert_eq!(p.ticket_by_key(&format!("APP-{a}")), Some(a));

    // Jira: sub-tasks whose parent doesn't fit become tasks and keep their sprint
    let mut q = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let csv = "Summary,Issue key,Issue Type,Parent key,Sprint\nEp,K-1,Epic,,\nSub,K-2,Sub-task,K-1,S1\nSub2,K-3,Sub-task,K-2,S1\n";
    jira::import_csv(&mut q, csv, &ImportOptions { create_sprints: true, create_people: false }).unwrap();
    let s1 = q.tasks.iter().find(|t| t.name == "S1").map(|t| t.uid);
    for t in q.tickets.iter().filter(|t| t.kind != IssueType::Epic) {
        assert!(t.kind != IssueType::Subtask || t.parent.is_some(), "{}", t.title);
        assert_eq!(t.sprint, s1, "{}", t.title);
    }
    assert_eq!(q.repair_board(), 0, "import leaves nothing to repair");
    // sprint creation order: numbers in order, others where they appeared
    let mut q = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let csv = "Summary,Sprint\na,Sprint 10\nb,Hardening\nc,Sprint 2\nd,Sprint 1\n";
    let rep = jira::import_csv(&mut q, csv, &ImportOptions { create_sprints: true, create_people: false }).unwrap();
    assert_eq!(rep.sprints_created, vec!["Sprint 1", "Hardening", "Sprint 2", "Sprint 10"]);
    // negated statuses are open work
    let mut q = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    jira::import_csv(&mut q, "Summary,Status\nA,Incomplete\nB,Not started\nC,Unresolved\nD,Not Done\nE,Under Review\nF,Closed\n", &ImportOptions::default()).unwrap();
    let col = |t: &str| q.tickets.iter().find(|x| x.title == t).unwrap().column;
    assert_eq!([col("A"), col("B"), col("C"), col("D")], [0, 0, 0, 0]);
    assert_eq!(col("E"), 2);
    assert_eq!(col("F"), q.board.done_column());
}
