//! Minimal repros of the crashes and hangs found by tests/fuzz_stability.rs.
//! Each test asserts the *fixed* behaviour (no panic), so it fails until the
//! bug is fixed. Tests that abort the whole process (stack overflow) or
//! take minutes are #[ignore]d: run them one at a time with
//!   cargo test --release --test fuzz_repros -- --ignored <name>

use chrono::{NaiveDate, NaiveDateTime};
use egui::{pos2, vec2, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use projectfcg::app::ProjectFcgApp;
use projectfcg::calendar::{Calendar, CalendarException, Shift};
use projectfcg::model::{LinkType, Project};
use projectfcg::ops::Field;

fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, dd).unwrap()
}
fn t(y: i32, m: u32, dd: u32, h: u32) -> NaiveDateTime {
    d(y, m, dd).and_hms_opt(h, 0, 0).unwrap()
}
fn three() -> Project {
    let mut p = Project::new(d(2026, 1, 5));
    for n in ["Design", "Build", "Test"] {
        p.append_task(n);
    }
    projectfcg::schedule::schedule(&mut p);
    p
}

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}
impl H {
    fn new(p: Project) -> Self {
        let mut app = ProjectFcgApp::new(p);
        app.use_portal = false;
        let mut h = H { ctx: egui::Context::default(), app, t: 0.0 };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360.0, 820.0))), time: Some(self.t), events, ..Default::default() };
        let app = &mut self.app;
        self.ctx.run_ui(raw, |ui| app.frame(ui)).drop_without_applying_deltas();
    }
    fn click(&mut self, p: Pos2, button: PointerButton) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton { pos: p, button, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(vec![Event::PointerButton { pos: p, button, pressed: false, modifiers: Modifiers::NONE }]);
        self.frame(vec![]);
    }
}

/// #1: right-click a task row, choose "Cut Task" (first item; "Delete
/// Task" crashes the same way). Panics at src/app/table.rs:199 "index out
/// of bounds": the table paints with the display rows computed before the
/// context menu deleted a task.
#[test]
fn context_menu_cut_task_does_not_panic() {
    let mut h = H::new(three());
    let r = h.app.last_table_rect;
    let row0 = pos2(r.left() + 200.0, r.top() + projectfcg::app::theme::header_h() + projectfcg::app::theme::row_h() / 2.0);
    h.click(row0, PointerButton::Secondary);
    h.click(pos2(row0.x + 20.0, row0.y + 8.0), PointerButton::Primary); // "Cut Task"
    assert_eq!(h.app.project.tasks.len(), 2);
}

/// #2: typing a date at the end of chrono's range into Start or Finish.
/// Panics at src/calendar.rs:422 (`NaiveDate + TimeDelta` overflowed).
#[test]
fn typing_far_future_start_does_not_panic() {
    let mut p = three();
    let _ = p.set_field_text(0, Field::Start, "12/31/262142");
    projectfcg::schedule::schedule(&mut p);
}

/// #2b: a done Jira/board ticket dated at the end of the range, which a
/// task waits on, in a .pfcg file: panics on open at src/calendar.rs:253.
#[test]
fn far_future_ticket_done_date_loads() {
    let mut p = projectfcg::app::sample::agile_release_at(t(2026, 3, 2, 9));
    let k = p.tickets.iter().position(|t| t.done_at.is_some()).unwrap();
    p.tickets[k].done_at = Some(NaiveDate::MAX.and_hms_opt(23, 59, 0).unwrap());
    let json = projectfcg::io::to_json(&p).unwrap();
    let _ = projectfcg::io::from_json(&json);
}

/// #2c: a recurring task starting near the end of the range (start date as
/// typed in the Recurring Task dialog). Panics at src/recurring.rs:137.
#[test]
fn recurring_series_near_max_date_does_not_panic() {
    use projectfcg::recurring::{RecurEnd, RecurPattern, Recurrence};
    let mut p = three();
    let r = Recurrence {
        pattern: RecurPattern::Weekly,
        every: 1,
        weekdays: [true, false, false, false, false, false, false],
        working_days_only: false,
        start: d(262142, 12, 1),
        end: RecurEnd::After(10),
        duration: projectfcg::duration::Duration::days(1.0, &p.settings.durations),
        occurrences: vec![],
    };
    let _ = p.insert_recurring(0, "Weekly", r);
    projectfcg::schedule::schedule(&mut p);
}

/// #3: an elapsed lag far outside the range in a .pfcg file panics on open
/// in chrono (`TimeDelta::minutes out of bounds`), called from
/// src/schedule.rs:361 (Ctx::shift).
#[test]
fn huge_elapsed_lag_in_file_loads() {
    let mut p = three();
    p.add_link(0, 1, LinkType::FS, Default::default()).unwrap();
    p.tasks[1].predecessors[0].lag = projectfcg::duration::Lag::Time { minutes: -9_223_372_036_854_775_807, unit: projectfcg::duration::DurUnit::Day, elapsed: true };
    let json = projectfcg::io::to_json(&p).unwrap();
    let _ = projectfcg::io::from_json(&json);
}

/// #4 (aborts the process): a duration of i64::MIN minutes in a .pfcg
/// file. add_work(-x) calls sub_work(x) and back; -i64::MIN == i64::MIN,
/// so they recurse until the stack overflows (src/calendar.rs:307 and :348).
#[test]
fn duration_i64_min_in_file_loads() {
    let mut p = three();
    p.tasks[0].duration.minutes = i64::MIN;
    let json = projectfcg::io::to_json(&p).unwrap();
    let _ = projectfcg::io::from_json(&json);
}

/// #4b (aborts the process): the same with a working-time lag.
#[test]
fn lag_i64_min_in_file_loads() {
    let mut p = three();
    p.add_link(0, 1, LinkType::FS, Default::default()).unwrap();
    p.tasks[1].predecessors[0].lag = projectfcg::duration::Lag::Time { minutes: i64::MIN, unit: projectfcg::duration::DurUnit::Day, elapsed: false };
    let json = projectfcg::io::to_json(&p).unwrap();
    let _ = projectfcg::io::from_json(&json);
}

/// #5 (hang): a calendar intersection (task calendar x resource calendar)
/// where one side has a long *working* exception produces one exception
/// per day, and add_work/shifts_on scan them all for every day walked:
/// days x exceptions. 1000 years: ~3.6 s per add_work; 8000 years: hang.
#[test]
fn long_working_exception_intersection_is_fast() {
    let mut a = Calendar::twenty_four_hours();
    a.exceptions.push(CalendarException { name: "x".into(), start: d(2026, 1, 1), end: d(3026, 1, 1), shifts: vec![Shift::new(0, 1440)] });
    let x = a.intersect(&Calendar::standard());
    let t0 = std::time::Instant::now();
    let _ = x.add_work(t(2026, 1, 5, 8), 480 * 250 * 100);
    assert!(t0.elapsed().as_secs_f64() < 0.5, "add_work took {:?}", t0.elapsed());
}

/// #6 (debug: overflow panic; release: duplicate uids). A task uid of
/// u32::MAX in a file makes from_json set next_uid = max_uid + 1
/// (src/io.rs:55), which wraps to 0; new tasks then reuse existing uids.
#[test]
fn max_uid_in_file_keeps_uids_unique() {
    let mut p = three();
    p.tasks[2].uid = u32::MAX;
    let json = projectfcg::io::to_json(&p).unwrap();
    let mut q = projectfcg::io::from_json(&json).unwrap();
    q.append_task("new 1");
    q.append_task("new 2");
    let mut uids: Vec<u32> = q.tasks.iter().map(|t| t.uid).collect();
    uids.sort();
    uids.dedup();
    assert_eq!(uids.len(), q.tasks.len(), "duplicate uids: {:?}", q.tasks.iter().map(|t| t.uid).collect::<Vec<_>>());
}

/// Broken state (no panic): a calendar with no working time within the
/// 50-year search limit gives a 1-day task a finish 50 years out, and with
/// a long holiday the task starts on a nonworking day.
#[test]
#[ignore = "by design: a calendar with one working minute can't fit a day of work; the task runs to the 50-year search limit"]
fn nearly_nonworking_calendar_gives_sane_dates() {
    let mut p = Project::new(d(2026, 1, 5));
    p.append_task("A");
    p.calendar = Calendar { name: "x".into(), week: Default::default(), exceptions: vec![CalendarException { name: "one".into(), start: d(2026, 3, 2), end: d(2026, 3, 2), shifts: vec![Shift::new(600, 601)] }], disjoint: false };
    projectfcg::schedule::schedule(&mut p);
    let (s, f) = (p.tasks[0].calc.start.unwrap(), p.tasks[0].calc.finish.unwrap());
    assert!((f - s).num_days() < 3650, "1-day task runs {s} .. {f}");
}
