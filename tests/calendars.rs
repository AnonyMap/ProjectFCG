//! Task calendars: tasks on their own base calendar, with and without
//! resources, and through files and Project XML.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::calendars::Template;
use projectfcg::model::Project;
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

fn plan() -> (Project, usize) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()); // Mon
    let a = p.append_task("Server migration");
    p.set_field_text(a, Field::Duration, "3d").unwrap(); // 24 working hours
    (p, a)
}

#[test]
fn a_task_on_the_24_hour_calendar_runs_around_the_clock() {
    let (mut p, a) = plan();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 7, 17)), "Standard: Mon–Wed");
    let c = p.add_base_calendar("24 Hours", Template::TwentyFourHours).unwrap();
    p.set_task_calendar(a, Some(c), false).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 5, 8)));
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 8)), "24 hours of work: Tue 8 AM");
    assert!(p.calc.errors.is_empty());
    // its successor follows the early finish
    let b = p.append_task("Smoke test");
    p.set_field_text(b, Field::Duration, "1d").unwrap();
    p.set_field_text(b, Field::Predecessors, "1").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.start, Some(dt(1, 6, 8)));
}

#[test]
fn night_shift() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::Duration, "1d").unwrap();
    let c = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    p.set_task_calendar(a, Some(c), false).unwrap();
    schedule(&mut p);
    // Mon 11 PM – midnight, Tue 0–3 and 4–8 AM: 8 hours
    assert_eq!(p.tasks[a].calc.start, Some(dt(1, 5, 23)));
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 8)));
}

#[test]
fn with_resources_the_task_works_when_both_calendars_do() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    let day = p.add_base_calendar("24 Hours", Template::TwentyFourHours).unwrap();
    p.set_task_calendar(a, Some(day), false).unwrap();
    schedule(&mut p);
    // 24 Hours ∩ the resource's Standard = Standard
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 7, 17)));
    // unless scheduling ignores resource calendars
    p.set_task_calendar(a, Some(day), true).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 8)));
    // a night-shift task with a day-shift resource has no common time: reported
    let night = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    p.set_task_calendar(a, Some(night), false).unwrap();
    schedule(&mut p);
    assert!(p.calc.errors.iter().any(|e| e.contains("no working time in common")), "{:?}", p.calc.errors);
}

#[test]
fn holidays_of_either_calendar_count() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    let c = p.add_base_calendar("Plant", Template::Standard).unwrap();
    let mut cal = p.base_calendar(c).unwrap().clone();
    cal.exceptions.push(projectfcg::calendar::CalendarException { name: "Shutdown".into(), start: NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(), end: NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(), shifts: vec![] });
    p.update_base_calendar(c, cal).unwrap();
    p.set_task_calendar(a, Some(c), false).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 8, 17)), "the plant's Tuesday shutdown pushes it a day");
}

#[test]
fn names_deletes_summaries_and_repair() {
    let (mut p, a) = plan();
    assert!(p.add_base_calendar("Standard", Template::Standard).is_err(), "the project calendar's name");
    let c = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    assert!(p.add_base_calendar("nights", Template::NightShift).is_err());
    p.set_task_calendar(a, Some(c), true).unwrap();
    p.delete_base_calendar(c).unwrap();
    assert_eq!((p.tasks[a].calendar, p.tasks[a].ignore_resource_calendars), (None, false));
    let s = p.append_task("Phase");
    let k = p.append_task("Kid");
    p.tasks[k].outline_level = 2;
    projectfcg::schedule::refresh_structure(&mut p);
    let c = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    assert!(p.set_task_calendar(s, Some(c), false).is_err());
    // a file pointing at a calendar that isn't there
    p.tasks[k].calendar = Some(99);
    assert_eq!(p.repair_calendars(), 1);
    assert_eq!(p.tasks[k].calendar, None);
}

#[test]
fn files_undo_and_project_xml() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    let c = p.add_base_calendar("Around the clock", Template::TwentyFourHours).unwrap();
    p.set_task_calendar(a, Some(c), true).unwrap();
    schedule(&mut p);
    let fin = p.tasks[a].calc.finish;
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.tasks[a].calc.finish, fin);
    // Project XML: a base calendar, the task's CalendarUID and IgnoreResourceCalendar
    let xml = projectfcg::mspdi::to_xml(&p);
    assert!(xml.contains("<Name>Around the clock</Name>"));
    assert!(xml.contains("<IgnoreResourceCalendar>1</IgnoreResourceCalendar>"));
    let (r, warnings) = projectfcg::mspdi::from_xml(&xml).unwrap();
    assert!(warnings.iter().all(|w| !w.contains("calendar")), "{warnings:?}");
    let rt = r.tasks.iter().find(|t| t.name == "Server migration").unwrap();
    assert!(rt.ignore_resource_calendars);
    assert_eq!(rt.calendar.and_then(|u| r.base_calendar(u)).map(|c| c.name.as_str()), Some("Around the clock"));
    assert_eq!(rt.calc.finish, fin);
    // undo of a calendar change
    let mut app = projectfcg::app::ProjectFcgApp::new(p);
    app.edit_project("Task Calendar", |p| p.set_task_calendar(a, None, false));
    assert_eq!(app.project.tasks[a].calc.finish, Some(dt(1, 7, 17)));
    app.execute(projectfcg::app::commands::Command::Undo, &egui::Context::default());
    assert_eq!(app.project.tasks[a].calc.finish, fin);
}

#[test]
fn change_working_time_makes_edits_and_deletes_calendars() {
    use projectfcg::app::commands::Command;
    let (p, a) = plan();
    let mut app = projectfcg::app::ProjectFcgApp::new(p);
    let ctx = egui::Context::default();
    app.dialogs.open_working_time(&app.project);
    let st = app.dialogs.working_time.as_mut().unwrap();
    st.new_name = "Nights".into();
    st.new_template = Template::NightShift;
    let nights = st.create(None).unwrap();
    assert_eq!(st.target, Some(nights), "the new calendar is the one being edited");
    st.new_name = "nights".into();
    assert!(st.create(None).is_err(), "names are unique");
    st.new_name = "Weekends".into();
    st.new_template = Template::Standard;
    let weekends = st.create(None).unwrap();
    st.week_working = [false, false, false, false, false, true, true];
    st.week_text[5] = "9:00 AM-1:00 PM".into();
    st.week_text[6] = "9:00 AM-1:00 PM".into();
    // edits survive switching between calendars
    st.switch_to(None).unwrap();
    st.switch_to(Some(weekends)).unwrap();
    assert!(st.week_working[5] && !st.week_working[0]);
    let all = st.finish().unwrap();
    assert_eq!(all.len(), 3);
    let wk = all.iter().find(|(u, _)| *u == Some(weekends)).unwrap();
    assert!(wk.1.week[0].is_empty() && wk.1.week[5].len() == 1);
    // apply the way OK does
    let all2 = all.clone();
    app.edit_project("Change Working Time", move |p| {
        for (u, c) in all2 {
            match u {
                None => p.calendar = c,
                Some(uid) => p.calendars.push(projectfcg::model::BaseCalendar { uid, calendar: c }),
            }
        }
        Ok(())
    });
    app.dialogs.working_time = None;
    assert_eq!(app.project.calendars.len(), 2);
    app.edit_project("cal", |p| p.set_task_calendar(a, Some(weekends), false));
    // Sat/Sun 9–1: 24 hours of work is 6 weekend days
    assert_eq!(app.project.tasks[a].calc.start, Some(dt(1, 10, 9)));
    assert_eq!(app.project.tasks[a].calc.finish, Some(dt(1, 25, 13)));
    // deleting one in the dialog sends its tasks back to the project calendar
    app.dialogs.open_working_time(&app.project);
    let st = app.dialogs.working_time.as_mut().unwrap();
    st.switch_to(Some(weekends)).unwrap();
    st.delete_current();
    assert_eq!(st.target, None);
    assert!(!st.finish().unwrap().iter().any(|(u, _)| *u == Some(weekends)));
    app.dialogs.working_time = None;
    let _ = (nights, Command::Undo, ctx);
}

// --- 0.9 review: every path measures on the calendar the scheduler uses ---

fn dt_y(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

#[test]
fn actual_finish_measures_on_the_task_and_resource_calendars() {
    // 24 Hours ∩ the resource's Standard: Mon 8 – Wed 5 PM, 24h of work.
    // Typing that same finish as the actual finish must not change it.
    let (mut p, a) = plan();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    let day = p.add_base_calendar("24 Hours", Template::TwentyFourHours).unwrap();
    p.set_task_calendar(a, Some(day), false).unwrap();
    schedule(&mut p);
    let (s, f) = (p.tasks[a].calc.start.unwrap(), p.tasks[a].calc.finish.unwrap());
    assert_eq!(f, dt(1, 7, 17));
    p.set_actual_start(a, Some(s)).unwrap();
    p.set_actual_finish(a, Some(f)).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].duration.minutes, 3 * 480);
    assert_eq!(p.assignments[0].work, 24 * 60);
    assert_eq!(p.tasks[a].calc.finish, Some(f));
    // half way through (Tue 12:30 of Mon 8 – Wed 17) is 50% scheduled
    let mid = NaiveDate::from_ymd_opt(2026, 1, 6).unwrap().and_hms_opt(12, 30, 0).unwrap();
    assert!((p.scheduled_percent(a, mid) - 50.0).abs() < 0.01, "{}", p.scheduled_percent(a, mid));
}

#[test]
fn assignments_work_when_the_task_does() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    let day = p.add_base_calendar("24 Hours", Template::TwentyFourHours).unwrap();
    // ignoring resource calendars: the assignment runs around the clock too
    p.set_task_calendar(a, Some(day), true).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 8)));
    assert_eq!((p.calc.assignments[0].start, p.calc.assignments[0].finish), (p.tasks[a].calc.start, p.tasks[a].calc.finish));
    // a Mon/Tue task calendar ∩ Standard: 3 days is Mon, Tue, next Mon
    let mut cal = projectfcg::calendar::Calendar::standard();
    cal.name = "24 Hours".into();
    for k in 2..7 {
        cal.week[k] = vec![];
    }
    p.update_base_calendar(day, cal).unwrap();
    p.set_task_calendar(a, Some(day), false).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 12, 17)));
    assert_eq!(p.calc.assignments[0].finish, Some(dt(1, 12, 17)), "the assignment follows the task's calendar");
    // and Resource Usage spreads its work over the same days: none on Wed
    let wed = (dt(1, 7, 0), dt(1, 8, 0));
    assert_eq!(p.assignment_work_between(0, wed.0, wed.1), 0.0);
    assert!((p.assignment_work_between(0, dt(1, 12, 0), dt(1, 13, 0)) - 480.0).abs() < 0.5);
}

#[test]
fn typed_finish_on_a_night_shift_task_counts_its_nights() {
    let (mut p, a) = plan();
    p.set_field_text(a, Field::Duration, "1d").unwrap();
    let c = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    p.set_task_calendar(a, Some(c), false).unwrap();
    p.tasks[a].mode = projectfcg::model::TaskMode::Manual;
    p.tasks[a].manual_start = Some(dt(1, 5, 23));
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 6, 8)));
    p.set_finish(a, dt(1, 7, 8)).unwrap(); // two nights
    schedule(&mut p);
    assert_eq!(p.tasks[a].duration.minutes, 960);
    assert_eq!(p.tasks[a].calc.finish, Some(dt(1, 7, 8)));
}

#[test]
fn long_exceptions_intersect_as_ranges() {
    use projectfcg::calendar::{Calendar, CalendarException, Shift};
    // a resource away 2027–2040; the task's Standard calendar: the task
    // waits for the resource whether it has its own calendar or not
    let (mut p, a) = plan();
    p.set_field_text(a, Field::Duration, "1d").unwrap();
    p.set_field_text(a, Field::ResourceNames, "Ops").unwrap();
    p.resources[0].exceptions.push(CalendarException { name: "Away".into(), start: NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(), end: NaiveDate::from_ymd_opt(2040, 12, 31).unwrap(), shifts: vec![] });
    p.set_constraint(a, projectfcg::model::ConstraintType::SNET, Some(dt_y(2038, 1, 4, 8))).unwrap();
    schedule(&mut p);
    let alone = p.tasks[a].calc.start;
    assert_eq!(alone, Some(dt_y(2041, 1, 1, 8)));
    let c = p.add_base_calendar("Plant", Template::Standard).unwrap();
    p.set_task_calendar(a, Some(c), false).unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.start, alone, "not during the absence");
    // one exception, however long the absence
    let away = p.resources[0].effective_calendar(&p.calendar);
    let both = away.intersect(&Calendar::standard());
    assert_eq!(both.exceptions.len(), 1, "{:?}", both.exceptions.len());
    // a working exception that varies by weekday is written out exactly
    let mut sat = Calendar::standard();
    sat.exceptions.push(CalendarException { name: "Rush".into(), start: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), end: NaiveDate::from_ymd_opt(2026, 1, 18).unwrap(), shifts: vec![Shift::new(9 * 60, 13 * 60)] });
    let r = sat.intersect(&Calendar::standard());
    let on = |m, d| r.shifts_on(NaiveDate::from_ymd_opt(2026, m, d).unwrap()).to_vec();
    assert_eq!(on(1, 7), vec![Shift::new(9 * 60, 12 * 60), Shift::new(13 * 60, 13 * 60)].into_iter().filter(|s| s.minutes() > 0).collect::<Vec<_>>());
    assert!(on(1, 10).is_empty(), "Saturday: the other calendar doesn't work");
    assert_eq!(on(1, 19), Calendar::standard().week[0], "after the exception: the plain week");
    assert!(r.exceptions.len() <= 6, "runs merge: {}", r.exceptions.len());
}

#[test]
fn a_pasted_task_doesnt_keep_a_deleted_calendar() {
    use projectfcg::app::commands::Command;
    use projectfcg::app::RowRef;
    let (mut p, a) = plan();
    let c = p.add_base_calendar("Nights", Template::NightShift).unwrap();
    p.set_task_calendar(a, Some(c), true).unwrap();
    let mut app = projectfcg::app::ProjectFcgApp::new(p);
    app.prefs_path = None;
    let ctx = egui::Context::default();
    app.select_single(RowRef::Task(app.project.tasks[a].uid));
    app.execute(Command::Copy, &ctx);
    app.edit_project("Delete Calendar", |p| p.delete_base_calendar(c));
    app.select_single(RowRef::Blank);
    app.execute(Command::Paste, &ctx);
    let pasted = app.project.tasks.last().unwrap();
    assert_eq!((pasted.calendar, pasted.ignore_resource_calendars), (None, false));
    // and a new calendar never takes over a uid a task still names
    let mut q = app.project.clone();
    q.tasks[0].calendar = Some(7);
    let n = q.add_base_calendar("Fresh", Template::Standard).unwrap();
    assert!(n > 7);
}
