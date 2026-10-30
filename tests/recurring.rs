//! Recurring tasks.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::calendar::CalendarException;
use projectfcg::duration::Duration;
use projectfcg::model::{ConstraintType, Project};
use projectfcg::recurring::{occurrence_dates, RecurEnd, RecurPattern, Recurrence};
use projectfcg::schedule::schedule;

fn d(m: u32, dd: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, dd).unwrap()
}
fn dt(m: u32, dd: u32, h: u32) -> NaiveDateTime {
    d(m, dd).and_hms_opt(h, 0, 0).unwrap()
}

fn weekly(p: &Project, days: &[usize], end: RecurEnd) -> Recurrence {
    let mut w = [false; 7];
    for &x in days {
        w[x] = true;
    }
    Recurrence { pattern: RecurPattern::Weekly, every: 1, weekdays: w, working_days_only: true, start: d(1, 5), end, duration: p.parse_duration("1h").unwrap(), occurrences: vec![] }
}

#[test]
fn weekly_meeting_becomes_a_summary_of_constrained_occurrences() {
    let mut p = Project::new(d(1, 5));
    let r = weekly(&p, &[0, 3], RecurEnd::After(6)); // Mon + Thu
    let row = p.insert_recurring(0, "Stand-up review", r.clone()).unwrap();
    schedule(&mut p);
    assert_eq!(row, 0);
    assert!(p.tasks[0].calc.is_summary);
    assert_eq!(p.tasks.len(), 7);
    let starts: Vec<_> = (1..7).map(|i| p.tasks[i].calc.start.unwrap()).collect();
    assert_eq!(starts, vec![dt(1, 5, 8), dt(1, 8, 8), dt(1, 12, 8), dt(1, 15, 8), dt(1, 19, 8), dt(1, 22, 8)]);
    assert!(p.tasks[1..].iter().all(|t| t.constraint == ConstraintType::SNET && t.duration.minutes == 60));
    assert_eq!(p.tasks[3].name, "Stand-up review 3");
    assert!(p.is_occurrence(3));
    assert_eq!(r.describe(), "Weekly every week on Mon, Thu, 6 times");
    // edit the series: every other Monday until 2/16
    let mut r2 = weekly(&p, &[0], RecurEnd::By(d(2, 16)));
    r2.every = 2;
    p.update_recurring(0, "Review", r2).unwrap();
    schedule(&mut p);
    let starts: Vec<_> = (1..p.tasks.len()).map(|i| p.tasks[i].calc.start.unwrap()).collect();
    assert_eq!(starts, vec![dt(1, 5, 8), dt(1, 19, 8), dt(2, 2, 8), dt(2, 16, 8)]);
    assert_eq!(p.tasks[1].name, "Review 1");
}

#[test]
fn holidays_move_occurrences_and_patterns_cover_months() {
    let mut p = Project::new(d(1, 5));
    p.calendar.exceptions.push(CalendarException { name: "Holiday".into(), start: d(1, 19), end: d(1, 19), shifts: vec![] });
    let r = weekly(&p, &[0], RecurEnd::After(3));
    p.insert_recurring(0, "M", r).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[2].calc.start, Some(dt(1, 12, 8)));
    assert_eq!(p.tasks[3].calc.start, Some(dt(1, 20, 8)), "Monday 1/19 is a holiday: Tuesday");
    // monthly on the 31st clamps to short months; yearly; daily working days
    let cal = projectfcg::calendar::Calendar::standard();
    let mk = |pattern, start, end| Recurrence { pattern, every: 1, weekdays: [false; 7], working_days_only: true, start, end, duration: Duration::days(1.0, &Default::default()), occurrences: vec![] };
    let m = occurrence_dates(&mk(RecurPattern::Monthly, d(1, 31), RecurEnd::After(4)), &cal);
    assert_eq!(m, vec![d(1, 31), d(2, 28), d(3, 31), d(4, 30)]);
    let y = occurrence_dates(&mk(RecurPattern::Yearly, d(3, 15), RecurEnd::After(2)), &cal);
    assert_eq!(y, vec![d(3, 15), NaiveDate::from_ymd_opt(2027, 3, 15).unwrap()]);
    // starting on Saturday 1/10, working days only
    let dd = occurrence_dates(&mk(RecurPattern::Daily, d(1, 10), RecurEnd::After(3)), &cal);
    assert_eq!(dd, vec![d(1, 12), d(1, 13), d(1, 14)]);
}

#[test]
fn bad_patterns_are_refused_and_series_survive_save_and_undo() {
    let mut p = Project::new(d(1, 5));
    let mut r = weekly(&p, &[0], RecurEnd::By(d(1, 1)));
    assert!(p.insert_recurring(0, "x", r.clone()).is_err(), "ends before it starts");
    r.end = RecurEnd::After(2);
    assert!(p.insert_recurring(0, " ", r.clone()).is_err());
    r.duration.minutes = 0;
    assert!(p.insert_recurring(0, "x", r.clone()).is_err());
    let mut p = Project::new(d(1, 5));
    r.duration.minutes = 30;
    let before = p.clone();
    p.insert_recurring(0, "Sync", r).unwrap();
    schedule(&mut p);
    let q = projectfcg::io::from_json(&projectfcg::io::to_json(&p).unwrap()).unwrap();
    assert_eq!(q.tasks[0].recurrence, p.tasks[0].recurrence);
    let delta = projectfcg::history::diff(&before, &p);
    let mut u = before.clone();
    let inv = projectfcg::history::apply(&mut u, delta);
    assert_eq!(u.tasks.len(), 3);
    projectfcg::history::apply(&mut u, inv);
    assert!(u.tasks.is_empty());
    // Project's XML marks the series as recurring
    let x = projectfcg::mspdi::to_xml(&p);
    assert_eq!(x.matches("<Recurring>1</Recurring>").count(), 3);
}

#[test]
fn editing_a_series_keeps_links_and_extra_tasks() {
    let mut p = Project::new(d(1, 5));
    let r = weekly(&p, &[0], RecurEnd::After(3));
    p.insert_recurring(0, "Review", r.clone()).unwrap();
    let after = p.append_task("Follow-up");
    p.tasks[after].outline_level = 1;
    p.normalize_outline();
    p.add_link(2, after, projectfcg::model::LinkType::FS, Default::default()).unwrap();
    let occ2 = p.tasks[2].uid;
    // a task the user put under the summary stays
    let extra = p.insert_task(4, "Prepare slides");
    p.tasks[extra].outline_level = 2;
    p.normalize_outline();
    let mut r4 = r.clone();
    r4.end = RecurEnd::After(4);
    p.update_recurring(0, "Review", r4).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[2].uid, occ2, "occurrences are updated in place");
    let fu = p.tasks.iter().position(|t| t.name == "Follow-up").unwrap();
    assert_eq!(p.tasks[fu].predecessors.len(), 1, "the link to occurrence 2 survived");
    assert!(p.tasks.iter().any(|t| t.name == "Prepare slides"));
    assert_eq!(p.tasks.iter().filter(|t| t.name.starts_with("Review ")).count(), 4);
    // shrinking removes only the extra occurrences
    let mut r2 = r.clone();
    r2.end = RecurEnd::After(2);
    p.update_recurring(0, "Review", r2).unwrap();
    assert_eq!(p.tasks.iter().filter(|t| t.name.starts_with("Review ")).count(), 2);
    // a failed edit changes nothing
    let before = p.clone();
    let mut bad = r.clone();
    bad.end = RecurEnd::By(d(1, 1));
    assert!(p.update_recurring(0, "Renamed", bad.clone()).is_err());
    assert_eq!(p.tasks, before.tasks);
    let n = p.tasks.len();
    assert!(p.insert_recurring(0, "x", bad).is_err());
    assert_eq!(p.tasks.len(), n, "no leftover summary");
}

#[test]
fn daily_over_weekends_does_not_pile_up() {
    let mut p = Project::new(d(1, 5));
    let mut r = weekly(&p, &[], RecurEnd::After(4));
    r.pattern = RecurPattern::Daily;
    r.working_days_only = false;
    r.start = d(1, 9); // Friday
    p.insert_recurring(0, "D", r).unwrap();
    schedule(&mut p);
    let starts: Vec<_> = p.tasks[1..].iter().map(|t| t.calc.start.unwrap()).collect();
    assert_eq!(starts, vec![dt(1, 9, 8), dt(1, 12, 8)], "Sat and Sun fold into Monday once");
}
