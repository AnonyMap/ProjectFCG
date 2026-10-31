//! A sample schedule that exercises most scheduling features.

use crate::board::{IssueType, LinkKind, Priority};
use crate::calendar::CalendarException;
use crate::duration::Lag;
use crate::model::{ConstraintType, LinkType, Project};
use crate::ops::Field;
use crate::schedule::schedule;
use chrono::NaiveDate;

pub fn software_release() -> Project {
    let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
    let mut p = Project::new(d(2026, 11, 2));
    p.title = "Software Release 2.0".into();
    p.manager = "Project Manager".into();
    for (name, a, b) in [
        ("Thanksgiving", d(2026, 11, 26), d(2026, 11, 27)),
        ("Winter holiday", d(2026, 12, 24), d(2027, 1, 1)),
    ] {
        p.calendar.exceptions.push(CalendarException {
            name: name.into(),
            start: a,
            end: b,
            shifts: vec![],
        });
    }
    // (name, outline level, duration)
    let rows: &[(&str, u32, &str)] = &[
        ("Planning", 1, "1d"),                      // 1
        ("Define requirements", 2, "5d"),           // 2
        ("Architecture review", 2, "3d"),           // 3
        ("Planning complete", 2, "0d"),             // 4
        ("Development", 1, "1d"),                   // 5
        ("Backend API", 2, "10d"),                  // 6
        ("Frontend UI", 2, "12d"),                  // 7
        ("Database migration", 2, "4d"),            // 8
        ("Code complete", 2, "0d"),                 // 9
        ("Testing", 1, "1d"),                       // 10
        ("Integration testing", 2, "6d"),           // 11
        ("Performance testing", 2, "4d"),           // 12
        ("User acceptance testing", 2, "5d"),       // 13
        ("Release", 1, "1d"),                       // 14
        ("User documentation", 2, "6d"),            // 15
        ("Release candidate build", 2, "1d"),       // 16
        ("Production deployment", 2, "4h"),         // 17
        ("Release 2.0 shipped", 2, "0d"),           // 18
    ];
    for (name, lvl, dur) in rows {
        let i = p.append_task(name);
        p.tasks[i].outline_level = *lvl;
        p.set_field_text(i, Field::Duration, dur).ok();
    }
    p.normalize_outline();
    let lag = |s: &str, p: &Project| Lag::parse(s, &p.settings.durations).unwrap();
    let links: &[(usize, usize, LinkType, &str)] = &[
        (2, 3, LinkType::FS, ""),
        (3, 4, LinkType::FS, ""),
        (4, 5, LinkType::FS, ""), // milestone → Development phase
        (6, 7, LinkType::SS, "+3d"),
        (6, 8, LinkType::FS, ""),
        (7, 9, LinkType::FS, ""),
        (8, 9, LinkType::FS, ""),
        (9, 11, LinkType::FS, ""),
        (11, 12, LinkType::SS, "+2d"),
        (11, 13, LinkType::FS, ""),
        (7, 15, LinkType::FF, "+2d"),
        (12, 16, LinkType::FS, ""),
        (13, 16, LinkType::FS, ""),
        (16, 17, LinkType::FS, "+1d"),
        (17, 18, LinkType::FS, ""),
    ];
    for &(a, b, k, l) in links {
        let lg = lag(l, &p);
        p.add_link(a - 1, b - 1, k, lg).expect("sample link");
    }
    p.tasks[1].percent_complete = 100.0;
    p.tasks[2].percent_complete = 60.0;
    p.tasks[1].notes = "Signed off by product and engineering leads.".into();
    p.set_constraint(14, ConstraintType::ALAP, None).ok();
    p.tasks[12].deadline = Some(d(2027, 1, 15).and_hms_opt(17, 0, 0).unwrap());
    for i in [0, 4, 9, 13, 3, 8, 17] {
        p.tasks[i].on_timeline = true;
    }
    // ---- team ----
    use crate::model::ResourceType;
    let people: &[(&str, &str, &str, f64, f64)] = &[
        // name, initials, group, max units, $/hr
        ("Priya Shah", "PS", "Management", 1.0, 85.0),
        ("Marco Ruiz", "MR", "Engineering", 1.0, 75.0),
        ("Lena Park", "LP", "Engineering", 1.0, 72.0),
        ("Sam Okafor", "SO", "QA", 1.0, 60.0),
        ("Devi Rao", "DR", "Docs", 0.5, 55.0),
        ("Ops Team", "OPS", "Operations", 2.0, 90.0),
    ];
    for (name, ini, group, max, rate) in people {
        let i = p.add_resource(name, ResourceType::Work);
        let r = &mut p.resources[i];
        r.initials = ini.to_string();
        r.group = group.to_string();
        r.max_units = *max;
        r.std_rate = *rate;
        r.ovt_rate = rate * 1.5;
    }
    let dev = p.add_resource("Test devices", ResourceType::Material);
    p.resources[dev].material_label = "devices".into();
    p.resources[dev].std_rate = 120.0;
    p.add_resource("Cloud hosting", ResourceType::Cost);
    // Lena is on vacation the week of Thanksgiving
    let lena = p.resource_by_name("Lena Park").unwrap();
    p.resources[lena].exceptions.push(CalendarException {
        name: "Vacation".into(),
        start: d(2026, 11, 23),
        end: d(2026, 11, 25),
        shifts: vec![],
    });
    let assign: &[(usize, &str)] = &[
        (1, "Priya Shah"),
        (2, "Priya Shah[50%],Marco Ruiz"),
        (5, "Marco Ruiz"),
        (6, "Lena Park"),
        (7, "Marco Ruiz[50%]"),
        (10, "Sam Okafor"),
        (11, "Sam Okafor,Test devices[5 devices]"),
        (12, "Sam Okafor[50%],Priya Shah[25%]"),
        (14, "Devi Rao[50%]"),
        (15, "Ops Team"),
        (16, "Ops Team[200%],Cloud hosting[$1,200]"),
    ];
    for (row, names) in assign {
        p.set_field_text(*row, Field::ResourceNames, names).expect("sample assignment");
    }
    p.tasks[16].fixed_cost = 500.0;
    schedule(&mut p);
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_schedules_cleanly() {
        let p = super::software_release();
        assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
        assert!(p.tasks.iter().all(|t| t.calc.start.is_some()));
        // the release milestone is critical and is the last thing to happen
        let last = p.tasks.last().unwrap();
        assert!(last.calc.critical);
        assert_eq!(last.calc.finish, p.calc.finish);
    }
}

/// A hybrid plan: a discovery phase, four two-week sprints holding Kanban
/// tickets, then launch. Dates are relative to today so that two sprints
/// have finished (velocity), one is running (burndown) and one is ahead.
pub fn agile_release() -> Project {
    agile_release_at(chrono::Local::now().naive_local())
}

pub fn agile_release_at(now: chrono::NaiveDateTime) -> Project {
    use crate::model::ResourceType;
    use chrono::{Datelike, Duration as D};
    let today = now.date();
    // after Friday's working day, anchor on the coming Monday so that
    // sprint 3 is still running (it would otherwise have just ended)
    let wd = today.weekday().num_days_from_monday() as i64;
    let week_over = wd >= 5 || (wd == 4 && now.time() >= chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap());
    let monday = if week_over { today + D::days(7 - wd) } else { today - D::days(wd) };
    let start = monday - D::weeks(6);
    let mut p = Project::new(start);
    p.title = "Mobile App 1.0".into();
    p.manager = "Product Owner".into();
    let rows: &[(&str, u32, &str)] = &[
        ("Discovery", 1, ""),
        ("User interviews", 2, "3d"),
        ("Scope & backlog", 2, "2d"),
        ("Build", 1, ""),
        ("Sprint 1", 2, "10d"),
        ("Sprint 2", 2, "10d"),
        ("Sprint 3", 2, "10d"),
        ("Sprint 4", 2, "10d"),
        ("Launch", 1, ""),
        ("App store review", 2, "5ed"),
        ("Release", 2, "0d"),
    ];
    for (name, lvl, dur) in rows {
        let i = p.append_task(name);
        p.tasks[i].outline_level = *lvl;
        if !dur.is_empty() {
            p.set_field_text(i, Field::Duration, dur).ok();
        }
    }
    p.normalize_outline();
    for (a, b) in [(1, 2), (2, 4), (4, 5), (5, 6), (6, 7), (7, 9), (9, 10)] {
        p.add_link(a, b, LinkType::FS, Lag::default()).expect("agile sample link");
    }
    for r in [1, 2] {
        p.tasks[r].percent_complete = 100.0;
    }
    for r in 4..=7 {
        p.set_sprint(r, true).expect("sprint");
    }
    for r in [0, 3, 8, 10] {
        p.tasks[r].on_timeline = true;
    }
    // team: the Dev Team carries the sprints' workload; people are labels
    let team = p.add_resource("Dev Team", ResourceType::Work);
    p.resources[team].max_units = 4.0;
    p.resources[team].std_rate = 80.0;
    p.resources[team].initials = "DEV".into();
    let mut people = vec![];
    for (n, ini) in [("Ana Lopez", "AL"), ("Ben Ito", "BI"), ("Chloe Moss", "CM"), ("Dan Novak", "DN")] {
        let i = p.add_resource(n, ResourceType::Work);
        p.resources[i].initials = ini.into();
        p.resources[i].std_rate = 80.0;
        people.push(p.resources[i].uid);
    }
    for r in 4..=7 {
        p.set_field_text(r, Field::ResourceNames, "Dev Team[400%]").ok();
    }
    p.set_field_text(1, Field::ResourceNames, "Ana Lopez").ok();
    schedule(&mut p);
    // tickets: (sprint row or backlog, title, points, column)
    let s = |r: usize, p: &Project| Some(p.tasks[r].uid);
    let plan: Vec<(Option<u32>, &str, f64, usize)> = vec![
        (s(4, &p), "Sign up with email", 5.0, 3),
        (s(4, &p), "Log in / log out", 3.0, 3),
        (s(4, &p), "Password reset", 3.0, 3),
        (s(4, &p), "App shell & navigation", 5.0, 3),
        (s(4, &p), "CI build for iOS and Android", 3.0, 3),
        (s(5, &p), "Product list", 5.0, 3),
        (s(5, &p), "Product detail page", 3.0, 3),
        (s(5, &p), "Search", 8.0, 3),
        (s(5, &p), "Image caching", 2.0, 3),
        (s(5, &p), "Offline mode", 8.0, 0),
        (s(6, &p), "Shopping cart", 5.0, 3),
        (s(6, &p), "Checkout with card", 8.0, 2),
        (s(6, &p), "Order history", 3.0, 1),
        (s(6, &p), "Push notifications", 5.0, 1),
        (s(6, &p), "Accessibility pass", 3.0, 0),
        (s(6, &p), "Crash reporting", 2.0, 3),
        (s(7, &p), "Apple Pay / Google Pay", 5.0, 0),
        (s(7, &p), "Reviews and ratings", 5.0, 0),
        (s(7, &p), "Performance tuning", 3.0, 0),
        (None, "Offline mode (carried over)", 8.0, 0),
        (None, "Dark mode", 3.0, 0),
        (None, "Wish list", 5.0, 0),
        (None, "Referral codes", 8.0, 0),
        (None, "Tablet layout", 13.0, 0),
        (None, "Localisation (FR, DE)", 5.0, 0),
    ];
    for (k, (sprint, title, pts, col)) in plan.into_iter().enumerate() {
        // "Offline mode" didn't make Sprint 2; it sits in the backlog instead
        if title == "Offline mode" {
            continue;
        }
        let id = p.add_ticket(title, sprint).expect("ticket");
        let i = p.ticket_index(id).unwrap();
        p.tickets[i].points = Some(pts);
        p.tickets[i].assignee = sprint.map(|_| people[k % people.len()]);
        p.tickets[i].column = col;
        if col == p.board.done_column() {
            // done on a working day inside its sprint (spread out), never in the future
            let row = p.tasks.iter().position(|t| Some(t.uid) == sprint).unwrap();
            let (ss, sf) = (p.tasks[row].calc.start.unwrap(), p.tasks[row].calc.finish.unwrap());
            let span = p.calendar.work_between(ss, sf);
            let frac = [0.15, 0.35, 0.5, 0.7, 0.9][k % 5];
            let at = p.calendar.add_work(ss, (span as f64 * frac) as i64).min(now - D::hours(2));
            p.tickets[i].done_at = Some(at);
        }
    }
    p.tickets[0].description = "As a new user I can create an account with my email address and a password.".into();
    // Jira-style structure: keys, epics, types, priorities, labels, links
    p.board.key_prefix = "APP".into();
    let id = |p: &Project, title: &str| p.tickets.iter().find(|t| t.title == title).map(|t| t.id).expect("sample ticket");
    let epics = [
        ("Accounts", &["Sign up with email", "Log in / log out", "Password reset"][..]),
        ("Catalog", &["Product list", "Product detail page", "Search", "Image caching", "Offline mode (carried over)", "Wish list"][..]),
        ("Checkout", &["Shopping cart", "Checkout with card", "Order history", "Apple Pay / Google Pay"][..]),
        ("Quality & release", &["App shell & navigation", "CI build for iOS and Android", "Accessibility pass", "Crash reporting", "Performance tuning"][..]),
    ];
    for (name, stories) in epics {
        let e = p.add_ticket_of(name, None, IssueType::Epic, None).expect("epic");
        for st in stories {
            let t = id(&p, st);
            p.set_ticket_parent(t, Some(e)).expect("epic child");
        }
    }
    let k = |p: &Project, title: &str| p.ticket_index(id(p, title)).unwrap();
    for t in ["CI build for iOS and Android", "Crash reporting", "Image caching", "Performance tuning"] {
        let i = k(&p, t);
        p.tickets[i].kind = IssueType::Task;
    }
    for (t, pr) in [("Checkout with card", Priority::Highest), ("Shopping cart", Priority::High), ("Push notifications", Priority::High), ("Accessibility pass", Priority::Low), ("Dark mode", Priority::Low), ("Tablet layout", Priority::Lowest)] {
        let i = k(&p, t);
        p.tickets[i].priority = pr;
    }
    for (t, ls) in [("Checkout with card", &["payments"][..]), ("Apple Pay / Google Pay", &["payments"][..]), ("Search", &["backend"][..]), ("Push notifications", &["backend", "ios"][..]), ("Accessibility pass", &["a11y"][..])] {
        let i = k(&p, t);
        p.tickets[i].labels = ls.iter().map(|x| x.to_string()).collect();
    }
    let checkout = id(&p, "Checkout with card");
    for (title, col) in [("Card form UI", 3), ("3-D Secure flow", 1), ("Payment error states", 0)] {
        let st = p.add_ticket_of(title, None, IssueType::Subtask, Some(checkout)).expect("sub-task");
        p.move_ticket(st, p.ticket(checkout).unwrap().sprint, col, None).expect("sub-task column");
        if col == p.board.done_column() {
            let i = p.ticket_index(st).unwrap();
            p.tickets[i].done_at = Some(now - D::days(1));
        }
    }
    let bug = p.add_ticket_of("Cart badge doesn't update after removing an item", None, IssueType::Bug, Some(id(&p, "Checkout"))).expect("bug");
    let i = p.ticket_index(bug).unwrap();
    p.tickets[i].points = Some(2.0);
    p.tickets[i].priority = Priority::High;
    p.add_ticket_link(id(&p, "Shopping cart"), LinkKind::Blocks, checkout).expect("link");
    p.add_ticket_link(checkout, LinkKind::Blocks, id(&p, "Apple Pay / Google Pay")).expect("link");
    p.add_ticket_link(id(&p, "Push notifications"), LinkKind::Relates, id(&p, "Order history")).expect("link");
    p.add_ticket_link(bug, LinkKind::Relates, id(&p, "Shopping cart")).expect("link");
    // Jira's sprint lifecycle: sprints 1 and 2 were started and completed,
    // sprint 3 is running; WIP limits on the middle columns
    for (r, goal) in [(4, "Users can sign up and log in on both platforms"), (5, "Browse and search the catalog"), (6, "Customers can pay for an order")] {
        let uid = p.tasks[r].uid;
        let start = p.tasks[r].calc.start.unwrap();
        p.start_sprint(uid, goal, start).expect("start sprint");
        let fin = p.tasks[r].calc.finish.unwrap();
        if fin < now {
            p.complete_sprint(uid, None, fin).expect("complete sprint");
        }
    }
    p.board.wip = vec![None, Some(3), Some(2), None];
    // board ↔ schedule dependencies: a task waiting on an epic, and a story
    // waiting on a task (planned a sprint too early, so it's flagged)
    let partners = p.append_task("Partners");
    let contract = p.append_task("Payment provider contract");
    let faq = p.append_task("Payment FAQ & support macros");
    p.tasks[contract].outline_level = 2;
    p.tasks[faq].outline_level = 2;
    p.tasks[partners].outline_level = 1;
    p.normalize_outline();
    p.set_field_text(contract, Field::Duration, "40d").ok();
    p.set_field_text(faq, Field::Duration, "3d").ok();
    p.set_field_text(contract, Field::ResourceNames, "Ben Ito[20%]").ok();
    schedule(&mut p);
    p.set_percent_complete(contract, 70.0).ok();
    let checkout_epic = id(&p, "Checkout");
    p.add_board_dep(faq, checkout_epic, Lag::default()).expect("board dep");
    let contract_uid = p.tasks[contract].uid;
    p.add_ticket_task_dep(id(&p, "Apple Pay / Google Pay"), contract_uid).expect("task dep");
    schedule(&mut p);
    // releases: 1.0 ships what's planned (behind the "Release" milestone),
    // 1.1 collects the backlog
    let launch = p.tasks[10].calc.finish.map(|f| f.date());
    let v1 = p.add_release("1.0", launch).expect("release");
    let v11 = p.add_release("1.1", launch.map(|d| d + D::weeks(6))).expect("release");
    let in_sprints: Vec<u32> = p.tickets.iter().filter(|t| t.kind.is_standard() && t.sprint.is_some()).map(|t| t.id).collect();
    for t in in_sprints {
        p.set_ticket_release(t, Some(v1)).expect("fix version");
    }
    for t in ["Dark mode", "Wish list", "Tablet layout"] {
        p.set_ticket_release(id(&p, t), Some(v11)).expect("fix version");
    }
    p.update_release(v1, "1.0", "First public release on the App Store and Google Play.", None, launch).expect("release");
    let release_uid = p.tasks[10].uid;
    p.set_release_task(v1, Some(release_uid)).expect("release milestone");
    schedule(&mut p);
    p
}
