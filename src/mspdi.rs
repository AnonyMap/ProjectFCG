//! Microsoft Project XML (MSPDI): the interchange format Project reads and
//! writes with File ▸ Save As ▸ XML. With Project Online retired, it's the
//! portable way in and out of Project desktop (and ProjectLibre, GanttProject
//! via MPXJ, Primavera, …).
//!
//! Units, as in Project's own files: durations and work are ISO 8601
//! ("PT16H0M0S", working time); link lag and leveling delay are integers in
//! tenths of a minute (percent lag: the plain percentage); currency is in
//! hundredths (cents); rates are per hour; booleans are 0/1. Element order
//! follows the schema's sequences (Project validates it on import).
//!
//! The board (sprints, tickets, epics) has no place in MSPDI and isn't
//! written; the sprint tasks themselves are ordinary tasks.

use crate::calendar::{Calendar, CalendarException, Shift};
use crate::duration::{DurUnit, Duration, Lag};
use crate::model::{Accrue, Assignment, Baseline, ConstraintType, Link, LinkType, Project, Resource, ResourceType, Task, TaskMode, TaskType};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use std::collections::HashMap;
use std::fmt::Write as _;

// ------------------------------------------------------------------ helpers

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            // XML 1.0 forbids most control characters
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\r' | '\t') => {}
            c => o.push(c),
        }
    }
    o
}

fn dt(d: NaiveDateTime) -> String {
    d.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn b(v: bool) -> &'static str {
    if v {
        "1"
    } else {
        "0"
    }
}

/// Minutes as an ISO 8601 duration the way Project writes it.
pub fn iso_duration(minutes: i64) -> String {
    let neg = minutes < 0;
    let m = minutes.abs();
    format!("{}PT{}H{}M0S", if neg { "-" } else { "" }, m / 60, m % 60)
}

/// Parse an ISO 8601 duration ("PT8H0M0S", "PT0.5H", "P2DT4H", "-PT30M")
/// into minutes. Days count 24 hours, as ISO says.
pub fn parse_iso_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let s = s.strip_prefix('P')?;
    let mut minutes = 0.0f64;
    let mut in_time = false;
    let mut num = String::new();
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' | '.' | ',' => num.push(if c == ',' { '.' } else { c }),
            _ => {
                let v: f64 = num.parse().ok()?;
                num.clear();
                minutes += v * match (c, in_time) {
                    ('Y', false) => 365.0 * 1440.0,
                    ('M', false) => 30.0 * 1440.0,
                    ('W', false) => 7.0 * 1440.0,
                    ('D', false) => 1440.0,
                    ('H', true) => 60.0,
                    ('M', true) => 1.0,
                    ('S', true) => 1.0 / 60.0,
                    _ => return None,
                };
            }
        }
    }
    if !num.is_empty() || !minutes.is_finite() || minutes.abs() > 1e12 {
        return None;
    }
    let m = minutes.round() as i64;
    Some(if neg { -m } else { m })
}

/// MSPDI DurationFormat / LagFormat code.
fn dur_format(unit: DurUnit, elapsed: bool, estimated: bool) -> u32 {
    let base = match unit {
        DurUnit::Minute => 3,
        DurUnit::Hour => 5,
        DurUnit::Day => 7,
        DurUnit::Week => 9,
        DurUnit::Month => 11,
    } + if elapsed { 1 } else { 0 };
    base + if estimated { 32 } else { 0 }
}

/// (unit, elapsed, estimated, percent) of a format code.
fn parse_dur_format(code: u32) -> (DurUnit, bool, bool, bool) {
    let estimated = code >= 35;
    let c = if estimated { code - 32 } else { code };
    let (unit, elapsed, pct) = match c {
        3 => (DurUnit::Minute, false, false),
        4 => (DurUnit::Minute, true, false),
        5 => (DurUnit::Hour, false, false),
        6 => (DurUnit::Hour, true, false),
        7 => (DurUnit::Day, false, false),
        8 => (DurUnit::Day, true, false),
        9 => (DurUnit::Week, false, false),
        10 => (DurUnit::Week, true, false),
        11 => (DurUnit::Month, false, false),
        12 => (DurUnit::Month, true, false),
        19 => (DurUnit::Day, false, true),
        20 => (DurUnit::Day, true, true),
        _ => (DurUnit::Day, false, false),
    };
    (unit, elapsed, estimated, pct)
}

fn link_type_code(k: LinkType) -> u32 {
    match k {
        LinkType::FF => 0,
        LinkType::FS => 1,
        LinkType::SF => 2,
        LinkType::SS => 3,
    }
}

fn constraint_code(c: ConstraintType) -> u32 {
    match c {
        ConstraintType::ASAP => 0,
        ConstraintType::ALAP => 1,
        ConstraintType::MSO => 2,
        ConstraintType::MFO => 3,
        ConstraintType::SNET => 4,
        ConstraintType::SNLT => 5,
        ConstraintType::FNET => 6,
        ConstraintType::FNLT => 7,
    }
}

/// Our week index (0 = Monday) → MSPDI DayType (1 = Sunday … 7 = Saturday).
fn day_type(idx: usize) -> u32 {
    ((idx + 1) % 7) as u32 + 1
}

fn hhmm(m: u32) -> String {
    let m = m % 1440;
    format!("{:02}:{:02}:00", m / 60, m % 60)
}

fn cents(v: f64) -> String {
    let c = (v * 100.0).round();
    format!("{c}")
}

fn num(v: f64) -> String {
    let r = (v * 10000.0).round() / 10000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

// ------------------------------------------------------------------ writer

struct W {
    out: String,
    depth: usize,
}

impl W {
    fn open(&mut self, tag: &str) {
        let _ = writeln!(self.out, "{}<{tag}>", "  ".repeat(self.depth));
        self.depth += 1;
    }
    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        let _ = writeln!(self.out, "{}</{tag}>", "  ".repeat(self.depth));
    }
    fn el(&mut self, tag: &str, v: impl AsRef<str>) {
        let _ = writeln!(self.out, "{}<{tag}>{}</{tag}>", "  ".repeat(self.depth), esc(v.as_ref()));
    }
    fn opt(&mut self, tag: &str, v: Option<String>) {
        if let Some(v) = v {
            self.el(tag, v);
        }
    }
}

fn write_shifts(w: &mut W, shifts: &[Shift]) {
    if shifts.is_empty() {
        return;
    }
    w.open("WorkingTimes");
    for s in shifts {
        w.open("WorkingTime");
        w.el("FromTime", hhmm(s.start));
        w.el("ToTime", hhmm(s.end));
        w.close("WorkingTime");
    }
    w.close("WorkingTimes");
}

fn write_calendar(w: &mut W, uid: u32, name: &str, base: Option<u32>, week: Option<&[Vec<Shift>; 7]>, exceptions: &[CalendarException]) {
    w.open("Calendar");
    w.el("UID", uid.to_string());
    w.el("Name", name);
    w.el("IsBaseCalendar", b(base.is_none()));
    w.el("BaseCalendarUID", base.map(|x| x as i64).unwrap_or(-1).to_string());
    if let Some(week) = week {
        w.open("WeekDays");
        // Sunday first, as Project writes them
        for idx in [6, 0, 1, 2, 3, 4, 5] {
            w.open("WeekDay");
            w.el("DayType", day_type(idx).to_string());
            w.el("DayWorking", b(!week[idx].is_empty()));
            write_shifts(w, &week[idx]);
            w.close("WeekDay");
        }
        w.close("WeekDays");
    }
    let exceptions = flatten_exceptions(exceptions);
    if !exceptions.is_empty() {
        w.open("Exceptions");
        for e in &exceptions {
            w.open("Exception");
            w.el("EnteredByOccurrences", "0");
            w.open("TimePeriod");
            w.el("FromDate", dt(e.start.and_hms_opt(0, 0, 0).unwrap()));
            w.el("ToDate", dt(e.end.and_hms_opt(23, 59, 0).unwrap()));
            w.close("TimePeriod");
            w.el("Occurrences", "1");
            w.el("Name", if e.name.is_empty() { "Exception" } else { &e.name });
            w.el("Type", "1");
            w.el("DayWorking", b(!e.shifts.is_empty()));
            write_shifts(w, &e.shifts);
            w.close("Exception");
        }
        w.close("Exceptions");
    }
    w.close("Calendar");
}

/// Project refuses overlapping exceptions. Where ours overlap, the later one
/// wins (as in the model), so resolve that day by day and write the result
/// as non-overlapping ranges.
pub fn flatten_exceptions(ex: &[CalendarException]) -> Vec<CalendarException> {
    let overlap = ex.iter().enumerate().any(|(i, a)| ex[i + 1..].iter().any(|b| a.start <= b.end && b.start <= a.end));
    if !overlap {
        let mut v = ex.to_vec();
        v.sort_by_key(|e| e.start);
        return v;
    }
    let (Some(first), Some(last)) = (ex.iter().map(|e| e.start).min(), ex.iter().map(|e| e.end).max()) else { return vec![] };
    let mut out: Vec<CalendarException> = vec![];
    let mut cur: Option<(usize, NaiveDate, NaiveDate)> = None;
    let mut d = first;
    let mut steps = 0;
    while d <= last && steps < 20 * 366 {
        steps += 1;
        let win = ex.iter().rposition(|e| e.start <= d && d <= e.end);
        match (cur, win) {
            (Some((k, a, _)), Some(w)) if k == w => cur = Some((k, a, d)),
            _ => {
                if let Some((k, a, b)) = cur.take() {
                    out.push(CalendarException { name: ex[k].name.clone(), start: a, end: b, shifts: ex[k].shifts.clone() });
                }
                cur = win.map(|w| (w, d, d));
            }
        }
        d = match d.succ_opt() {
            Some(x) => x,
            None => break,
        };
    }
    if let Some((k, a, b)) = cur {
        out.push(CalendarException { name: ex[k].name.clone(), start: a, end: b, shifts: ex[k].shifts.clone() });
    }
    out
}

fn write_baseline(w: &mut W, n: usize, bl: &Baseline) {
    w.open("Baseline");
    w.el("Number", n.to_string());
    w.el("Start", dt(bl.start));
    w.el("Finish", dt(bl.finish));
    w.el("Duration", iso_duration(bl.duration));
    w.el("DurationFormat", "7");
    w.el("EstimatedDuration", "0");
    w.el("Work", iso_duration(bl.work));
    w.el("Cost", cents(bl.cost));
    w.close("Baseline");
}

/// The project as MSPDI XML. Expects a scheduled project (`schedule(p)`).
pub fn to_xml(p: &Project) -> String {
    let mut w = W { out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Project xmlns=\"http://schemas.microsoft.com/project\">\n"), depth: 1 };
    let s = &p.settings;
    w.el("SaveVersion", "14");
    w.el("Name", format!("{}.xml", p.title));
    w.el("Title", &p.title);
    if !p.manager.is_empty() {
        w.el("Manager", &p.manager);
    }
    w.el("ScheduleFromStart", "1");
    w.el("StartDate", dt(p.start));
    w.opt("FinishDate", p.calc.finish.map(dt));
    // whole days in MSPDI; rounded rather than truncated
    w.el("CriticalSlackLimit", ((s.critical_slack_threshold as f64 / s.durations.minutes_per_day.max(1) as f64).round() as i64).to_string());
    w.el("CurrencySymbol", &s.currency_symbol);
    w.el("CalendarUID", "1");
    w.el("DefaultStartTime", hhmm(s.default_start_minute));
    w.el("DefaultFinishTime", hhmm(s.default_finish_minute));
    w.el("MinutesPerDay", s.durations.minutes_per_day.to_string());
    w.el("MinutesPerWeek", s.durations.minutes_per_week.to_string());
    w.el("DaysPerMonth", s.durations.days_per_month.to_string());
    w.el(
        "DefaultTaskType",
        match s.default_task_type {
            TaskType::FixedUnits => "0",
            TaskType::FixedDuration => "1",
            TaskType::FixedWork => "2",
        },
    );
    w.el("DurationFormat", dur_format(s.default_duration_unit, false, false).to_string());
    w.el("HonorConstraints", b(s.honor_constraints));
    w.el("MultipleCriticalPaths", b(s.multiple_critical_paths));
    w.el("NewTasksEffortDriven", b(s.new_tasks_effort_driven));
    w.el("AutoAddNewResourcesAndTasks", b(s.auto_add_resources));
    w.opt("StatusDate", p.status_date.map(dt));
    w.el("NewTasksAreManual", b(!s.new_tasks_auto));

    // ---- calendars: 1 = project, then one per work resource ----
    w.open("Calendars");
    write_calendar(&mut w, 1, &p.calendar.name, None, Some(&p.calendar.week), &p.calendar.exceptions);
    let mut res_cal: HashMap<u32, u32> = HashMap::new();
    let mut next_cal = 2;
    for r in p.resources.iter().filter(|r| r.rtype == ResourceType::Work) {
        write_calendar(&mut w, next_cal, &r.name, Some(1), r.week.as_ref(), &r.exceptions);
        res_cal.insert(r.uid, next_cal);
        next_cal += 1;
    }
    w.close("Calendars");

    // board dependencies become ordinary links to the sprint (the board
    // itself isn't in MSPDI), so Project schedules the same dates
    let graph = crate::schedule::build_graph(p, &crate::schedule::Structure::build(p));
    let mut board_links: HashMap<usize, Vec<(u32, Lag)>> = HashMap::new();
    for e in graph.links.iter().filter(|e| e.board.is_some()) {
        let v = board_links.entry(e.succ).or_default();
        let pred = p.tasks[e.pred].uid;
        if !v.iter().any(|x| x.0 == pred) && !p.tasks[e.succ].predecessors.iter().any(|l| l.pred == pred) {
            v.push((pred, e.link.lag));
        }
    }
    // ---- tasks ----
    w.open("Tasks");
    for (i, t) in p.tasks.iter().enumerate() {
        let c = &t.calc;
        w.open("Task");
        w.el("UID", t.uid.to_string());
        w.el("ID", (i + 1).to_string());
        w.el("Name", &t.name);
        w.el("Active", b(t.active));
        w.el("Manual", b(t.mode == TaskMode::Manual));
        w.el(
            "Type",
            match t.task_type {
                TaskType::FixedUnits => "0",
                TaskType::FixedDuration => "1",
                TaskType::FixedWork => "2",
            },
        );
        w.el("IsNull", "0");
        w.el("WBS", &c.outline_number);
        w.el("OutlineNumber", &c.outline_number);
        w.el("OutlineLevel", t.outline_level.to_string());
        w.el("Priority", t.priority.to_string());
        w.opt("Start", c.start.map(dt));
        w.opt("Finish", c.finish.map(dt));
        let (dur, fmt) = if c.is_summary { (c.summary_duration, dur_format(DurUnit::Day, false, false)) } else { (t.duration.minutes, dur_format(t.duration.unit, t.duration.elapsed, t.duration.estimated)) };
        w.el("Duration", iso_duration(dur));
        w.el("DurationFormat", fmt.to_string());
        w.el("Work", iso_duration(c.work));
        w.el("EffortDriven", b(t.effort_driven));
        let recurring = t.recurrence.is_some() || p.parent_of(i).is_some_and(|a| p.tasks[a].recurrence.is_some());
        if recurring {
            w.el("Recurring", "1");
        }
        w.el("Estimated", b(t.duration.estimated && !c.is_summary));
        w.el("Milestone", b(t.is_milestone()));
        w.el("Summary", b(c.is_summary));
        w.el("Critical", b(c.critical));
        if t.fixed_cost != 0.0 {
            w.el("FixedCost", cents(t.fixed_cost));
        }
        w.el("PercentComplete", (c.percent_complete.round() as i64).to_string());
        w.el("Cost", cents(c.cost));
        if !c.is_summary {
            w.opt("ActualStart", t.actual_start.map(dt));
            w.opt("ActualFinish", t.actual_finish.map(dt));
        }
        // a finished story it waits on: Project gets it as Start No Earlier Than
        let floor = if t.constraint == ConstraintType::ASAP && !c.is_summary {
            graph.floors[i]
                .iter()
                .map(|(done, lag, pdur, _)| match *lag {
                    Lag::Time { minutes, elapsed: true, .. } => *done + chrono::Duration::minutes(minutes),
                    Lag::Time { minutes, .. } => p.calendar.add_work(*done, minutes),
                    Lag::Percent(pct) => p.calendar.add_work(*done, (*pdur as f64 * pct / 100.0).round() as i64),
                })
                .max()
        } else {
            None
        };
        match floor {
            Some(f) => {
                w.el("ConstraintType", constraint_code(ConstraintType::SNET).to_string());
                w.el("CalendarUID", "-1");
                w.el("ConstraintDate", dt(f));
            }
            None => {
                w.el("ConstraintType", constraint_code(t.constraint).to_string());
                w.el("CalendarUID", "-1");
                if t.constraint.needs_date() {
                    w.opt("ConstraintDate", t.constraint_date.map(dt));
                }
            }
        }
        w.opt("Deadline", t.deadline.map(dt));
        if t.leveling_delay != 0 {
            w.el("LevelingDelay", (t.leveling_delay * 10).to_string());
            w.el("LevelingDelayFormat", "7");
        }
        if !t.notes.is_empty() {
            w.el("Notes", &t.notes);
        }
        let extra: Vec<Link> = board_links.get(&i).map(|v| v.iter().map(|(u, lag)| Link { pred: *u, kind: LinkType::FS, lag: *lag }).collect()).unwrap_or_default();
        for l in t.predecessors.iter().chain(extra.iter()) {
            w.open("PredecessorLink");
            w.el("PredecessorUID", l.pred.to_string());
            w.el("Type", link_type_code(l.kind).to_string());
            w.el("CrossProject", "0");
            match l.lag {
                Lag::Time { minutes, unit, elapsed } => {
                    w.el("LinkLag", (minutes * 10).to_string());
                    w.el("LagFormat", dur_format(unit, elapsed, false).to_string());
                }
                Lag::Percent(pct) => {
                    w.el("LinkLag", (pct.round() as i64).to_string());
                    w.el("LagFormat", "19");
                }
            }
            w.close("PredecessorLink");
        }
        for n in 0..=10 {
            if let Some(bl) = t.baseline_slot(n) {
                write_baseline(&mut w, n, &bl);
            }
        }
        if t.mode == TaskMode::Manual && !c.is_summary {
            w.opt("ManualStart", c.start.map(dt));
            w.opt("ManualFinish", c.finish.map(dt));
            w.el("ManualDuration", iso_duration(t.duration.minutes));
        }
        w.close("Task");
    }
    w.close("Tasks");

    // ---- resources ----
    w.open("Resources");
    for (i, r) in p.resources.iter().enumerate() {
        w.open("Resource");
        w.el("UID", r.uid.to_string());
        w.el("ID", (i + 1).to_string());
        w.el("Name", &r.name);
        w.el(
            "Type",
            match r.rtype {
                ResourceType::Work => "1",
                // Project writes cost resources as material + IsCostResource
                ResourceType::Material | ResourceType::Cost => "0",
            },
        );
        w.el("IsNull", "0");
        if !r.initials.is_empty() {
            w.el("Initials", &r.initials);
        }
        if r.rtype == ResourceType::Material && !r.material_label.is_empty() {
            w.el("MaterialLabel", &r.material_label);
        }
        if !r.group.is_empty() {
            w.el("Group", &r.group);
        }
        if !r.email.is_empty() {
            w.el("EmailAddress", &r.email);
        }
        if r.rtype == ResourceType::Work {
            w.el("MaxUnits", num(r.max_units));
        }
        w.el(
            "AccrueAt",
            match r.accrue {
                Accrue::Start => "1",
                Accrue::End => "2",
                Accrue::Prorated => "3",
            },
        );
        if r.rtype != ResourceType::Cost {
            w.el("StandardRate", num(r.std_rate));
            w.el("StandardRateFormat", if r.rtype == ResourceType::Material { "8" } else { "2" });
            w.el("OvertimeRate", num(r.ovt_rate));
            w.el("OvertimeRateFormat", "2");
            w.el("CostPerUse", cents(r.cost_per_use));
        }
        if let Some(cu) = res_cal.get(&r.uid) {
            w.el("CalendarUID", cu.to_string());
        }
        if !r.notes.is_empty() {
            w.el("Notes", &r.notes);
        }
        w.el("IsCostResource", b(r.rtype == ResourceType::Cost));
        w.close("Resource");
    }
    w.close("Resources");

    // ---- assignments ----
    w.open("Assignments");
    for (k, a) in p.assignments.iter().enumerate() {
        let Some(row) = p.index_of_uid(a.task) else { continue };
        let Some(r) = p.resources.iter().find(|r| r.uid == a.resource) else { continue };
        let ac = p.calc.assignments.get(k);
        w.open("Assignment");
        w.el("UID", (k + 1).to_string());
        w.el("TaskUID", a.task.to_string());
        w.el("ResourceUID", a.resource.to_string());
        w.el("PercentWorkComplete", (p.tasks[row].calc.percent_complete.round() as i64).to_string());
        let cost = if r.rtype == ResourceType::Cost { a.cost } else { ac.map(|c| c.cost).unwrap_or(0.0) };
        w.el("Cost", cents(cost));
        w.opt("Finish", ac.and_then(|c| c.finish).map(dt));
        w.opt("Start", ac.and_then(|c| c.start).map(dt));
        w.el("Units", num(if r.rtype == ResourceType::Cost { 1.0 } else { a.units }));
        if r.rtype == ResourceType::Work {
            w.el("Work", iso_duration(a.work));
        }
        w.close("Assignment");
    }
    w.close("Assignments");
    w.out.push_str("</Project>\n");
    w.out
}

// ------------------------------------------------------------------ reader

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;

fn kid<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    kid(n, name).and_then(|c| c.text()).map(str::trim).filter(|s| !s.is_empty())
}

fn int(n: Node, name: &str) -> Option<i64> {
    text(n, name).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite() && v.abs() < 1e12).map(|v| v.round() as i64)
}

/// A unique ID: positive and not absurdly large (0 and negative values are
/// Project's placeholders).
fn uid_of(n: Node, name: &str) -> Option<u32> {
    int(n, name).filter(|v| (1..=999_999_999).contains(v)).map(|v| v as u32)
}

/// Durations, work and lags beyond 20 years are damage, not plans.
const MAX_MINUTES: i64 = 20 * 366 * 1440;

fn dur_minutes(n: Node, name: &str) -> Option<i64> {
    text(n, name).and_then(parse_iso_duration).filter(|m| m.abs() <= MAX_MINUTES)
}

fn float(n: Node, name: &str) -> Option<f64> {
    text(n, name).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite() && v.abs() < 1e15)
}

fn flag(n: Node, name: &str) -> Option<bool> {
    text(n, name).map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
}

fn date_time(n: Node, name: &str) -> Option<NaiveDateTime> {
    let s = text(n, name)?;
    // dates outside 1900–2200 are damage (and would overflow date maths)
    let sane = |d: &NaiveDateTime| (1900..=2200).contains(&d.year());
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()
        .or_else(|| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.naive_local()))
        .or_else(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)))
        .filter(sane)
}

fn time_minute(s: &str) -> Option<u32> {
    // some tools write the end of the day as 24:00
    if s.trim().starts_with("24:00") {
        return Some(1440);
    }
    let t = NaiveTime::parse_from_str(s.trim(), "%H:%M:%S").or_else(|_| NaiveTime::parse_from_str(s.trim(), "%H:%M")).ok()?;
    Some(t.hour() * 60 + t.minute())
}

/// Working times of a day or exception. `DayWorking` set with no times
/// listed means the default working day (8–12, 13–17).
fn read_shifts(n: Node) -> Vec<Shift> {
    if kid(n, "WorkingTimes").is_none_or(|w| kids(w, "WorkingTime").next().is_none()) {
        return vec![Shift::new(8 * 60, 12 * 60), Shift::new(13 * 60, 17 * 60)];
    }
    let mut v = vec![];
    if let Some(wt) = kid(n, "WorkingTimes") {
        for t in kids(wt, "WorkingTime") {
            let (Some(a), Some(e)) = (text(t, "FromTime").and_then(time_minute), text(t, "ToTime").and_then(time_minute)) else { continue };
            // a shift ending at midnight is written as 00:00:00
            let e = if e <= a { 1440 } else { e };
            v.push(Shift::new(a, e));
        }
    }
    v.sort_by_key(|s| s.start);
    v
}

struct RawCal {
    name: String,
    base: Option<u32>,
    /// Days the calendar sets itself (our index 0 = Monday).
    week: [Option<Vec<Shift>>; 7],
    exceptions: Vec<CalendarException>,
}

/// Expand one MSPDI calendar exception (possibly recurring) into date
/// ranges.
fn read_exception(e: Node, warnings: &mut Vec<String>) -> Vec<CalendarException> {
    let Some(tp) = kid(e, "TimePeriod") else { return vec![] };
    let (Some(from), Some(to)) = (date_time(tp, "FromDate"), date_time(tp, "ToDate")) else { return vec![] };
    let (from, to) = (from.date(), to.date().max(from.date()));
    let name = text(e, "Name").unwrap_or("").to_string();
    let working = flag(e, "DayWorking").unwrap_or(false);
    let shifts = if working { read_shifts(e) } else { vec![] };
    let kind = int(e, "Type").unwrap_or(1);
    let period = int(e, "Period").unwrap_or(1).clamp(1, 1000);
    let occurrences = int(e, "Occurrences").unwrap_or(0).max(0) as usize;
    let one = |s: NaiveDate, f: NaiveDate| CalendarException { name: name.clone(), start: s, end: f, shifts: shifts.clone() };
    if kind == 1 && period == 1 {
        return vec![one(from, to)];
    }
    let month_item = int(e, "MonthItem").unwrap_or(0);
    let month_pos = int(e, "MonthPosition").unwrap_or(0).clamp(0, 4);
    let month = int(e, "Month").unwrap_or(0).clamp(0, 11) as u32 + 1;
    let month_day = int(e, "MonthDay").unwrap_or(1).clamp(1, 31) as u32;
    let days_of_week = int(e, "DaysOfWeek").unwrap_or(0);
    // MonthItem 3..9 = Sunday..Saturday; MonthPosition 0..3 = first..fourth, 4 = last
    // MonthItem: 0 day, 1 weekday, 2 weekend day, 3..9 Sunday..Saturday
    let item_matches = |d: NaiveDate| -> bool {
        use chrono::Weekday::*;
        let wd = d.weekday();
        match month_item {
            0 => true,
            1 => !matches!(wd, Sat | Sun),
            2 => matches!(wd, Sat | Sun),
            3 => wd == Sun,
            4 => wd == Mon,
            5 => wd == Tue,
            6 => wd == Wed,
            7 => wd == Thu,
            8 => wd == Fri,
            9 => wd == Sat,
            _ => false,
        }
    };
    // the first..fourth (or last) matching day of a month
    let nth_weekday = |y: i32, m: u32| -> Option<NaiveDate> {
        let first = NaiveDate::from_ymd_opt(y, m, 1)?;
        let next = first.checked_add_months(chrono::Months::new(1))?;
        let days: Vec<NaiveDate> = first.iter_days().take_while(|d| *d < next).filter(|d| item_matches(*d)).collect();
        if month_pos >= 4 {
            days.last().copied()
        } else {
            days.get(month_pos as usize).copied()
        }
    };
    let mut dates: Vec<NaiveDate> = vec![];
    let mut d = from;
    let mut guard = 0;
    while d <= to && guard < 40_000 && dates.len() < 3_000 {
        guard += 1;
        let hit = match kind {
            1 | 7 => (d - from).num_days() % period == 0,
            6 => {
                let bit = 1 << d.weekday().num_days_from_sunday();
                let week = (d - from).num_days() / 7;
                days_of_week & bit != 0 && week % period == 0
            }
            4 => {
                let months = (d.year() - from.year()) * 12 + d.month() as i32 - from.month() as i32;
                d.day() == month_day && months as i64 % period == 0
            }
            5 => {
                let months = (d.year() - from.year()) * 12 + d.month() as i32 - from.month() as i32;
                months as i64 % period == 0 && nth_weekday(d.year(), d.month()) == Some(d)
            }
            2 => d.month() == month && d.day() == month_day,
            3 => d.month() == month && nth_weekday(d.year(), d.month()) == Some(d),
            _ => {
                warnings.push(format!("Calendar exception \"{name}\" has a recurrence ProjectFCG doesn't know; used its date range."));
                return vec![one(from, to)];
            }
        };
        if hit {
            dates.push(d);
            if occurrences > 0 && dates.len() >= occurrences {
                break;
            }
        }
        d = match d.succ_opt() {
            Some(x) => x,
            None => break,
        };
    }
    dates.into_iter().map(|x| one(x, x)).collect()
}

fn read_calendar(c: Node, warnings: &mut Vec<String>) -> (u32, RawCal) {
    let uid = int(c, "UID").filter(|v| (0..=999_999_999).contains(v)).unwrap_or(0) as u32;
    let base = int(c, "BaseCalendarUID").filter(|b| (0..=999_999_999).contains(b)).map(|b| b as u32).filter(|&b| b != uid).filter(|_| !flag(c, "IsBaseCalendar").unwrap_or(false));
    let mut week: [Option<Vec<Shift>>; 7] = Default::default();
    let mut exceptions = vec![];
    if let Some(wds) = kid(c, "WeekDays") {
        for wd in kids(wds, "WeekDay") {
            let t = int(wd, "DayType").unwrap_or(-1);
            let working = flag(wd, "DayWorking").unwrap_or(false);
            let shifts = if working { read_shifts(wd) } else { vec![] };
            match t {
                1..=7 => {
                    // DayType 1 = Sunday … 7 = Saturday
                    let idx = ((t + 5) % 7) as usize;
                    week[idx] = Some(shifts);
                }
                0 => {
                    // Project 2003-style exception
                    if let Some(tp) = kid(wd, "TimePeriod") {
                        if let (Some(f), Some(tt)) = (date_time(tp, "FromDate"), date_time(tp, "ToDate")) {
                            exceptions.push(CalendarException { name: String::new(), start: f.date(), end: tt.date().max(f.date()), shifts });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(ex) = kid(c, "Exceptions") {
        // Project 2007+ also writes each exception as an old-style DayType 0
        // week day; the Exceptions list is the complete one
        exceptions.clear();
        for e in kids(ex, "Exception") {
            exceptions.extend(read_exception(e, warnings));
        }
    }
    if kid(c, "WorkWeeks").is_some_and(|ww| kids(ww, "WorkWeek").next().is_some()) {
        let name = text(c, "Name").unwrap_or("");
        warnings.push(format!("Calendar \"{name}\" has alternate work weeks; ProjectFCG uses its default week."));
    }
    // file order is kept: as in the model, a later exception wins
    (uid, RawCal { name: text(c, "Name").unwrap_or("Calendar").to_string(), base, week, exceptions })
}

/// The effective week of a calendar (own days over its base chain).
fn resolve_week(cals: &HashMap<u32, RawCal>, uid: u32) -> [Vec<Shift>; 7] {
    let mut out: [Option<Vec<Shift>>; 7] = Default::default();
    let mut cur = Some(uid);
    let mut guard = 0;
    while let Some(u) = cur {
        guard += 1;
        if guard > 20 {
            break;
        }
        let Some(c) = cals.get(&u) else { break };
        for i in 0..7 {
            if out[i].is_none() {
                out[i] = c.week[i].clone();
            }
        }
        cur = c.base;
    }
    let std = Calendar::standard();
    std::array::from_fn(|i| out[i].clone().unwrap_or_else(|| std.week[i].clone()))
}

/// Read MSPDI XML. Returns the scheduled project and notes on anything that
/// didn't carry over exactly.
pub fn from_xml(xml: &str) -> Result<(Project, Vec<String>), String> {
    let doc = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).map_err(|e| format!("Not a valid XML file: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err("This isn't a Microsoft Project XML file (the root element isn't <Project>).".into());
    }
    let mut warnings: Vec<String> = vec![];
    let start = date_time(root, "StartDate").unwrap_or_else(|| chrono::Local::now().date_naive().and_hms_opt(8, 0, 0).unwrap());
    let mut p = Project::new(start.date());
    p.start = start;
    p.title = text(root, "Title").map(str::to_string).or_else(|| text(root, "Name").map(|n| n.trim_end_matches(".xml").trim_end_matches(".mpp").to_string())).unwrap_or_else(|| "Imported project".into());
    p.manager = text(root, "Manager").unwrap_or("").to_string();
    p.status_date = date_time(root, "StatusDate");
    {
        let s = &mut p.settings;
        if let Some(v) = int(root, "MinutesPerDay").filter(|v| *v > 0) {
            s.durations.minutes_per_day = v;
        }
        if let Some(v) = int(root, "MinutesPerWeek").filter(|v| *v > 0) {
            s.durations.minutes_per_week = v;
        }
        if let Some(v) = int(root, "DaysPerMonth").filter(|v| *v > 0) {
            s.durations.days_per_month = v;
        }
        if let Some(m) = text(root, "DefaultStartTime").and_then(time_minute) {
            s.default_start_minute = m;
        }
        if let Some(m) = text(root, "DefaultFinishTime").and_then(time_minute) {
            s.default_finish_minute = m;
        }
        s.default_task_type = match int(root, "DefaultTaskType") {
            Some(1) => TaskType::FixedDuration,
            Some(2) => TaskType::FixedWork,
            _ => TaskType::FixedUnits,
        };
        if let Some(c) = int(root, "DurationFormat") {
            let (u, _, _, _) = parse_dur_format(c.clamp(0, 100) as u32);
            s.default_duration_unit = u;
        }
        if let Some(v) = flag(root, "HonorConstraints") {
            s.honor_constraints = v;
        }
        if let Some(v) = flag(root, "MultipleCriticalPaths") {
            s.multiple_critical_paths = v;
        }
        if let Some(v) = flag(root, "NewTasksEffortDriven") {
            s.new_tasks_effort_driven = v;
        }
        if let Some(v) = flag(root, "NewTasksAreManual") {
            s.new_tasks_auto = !v;
        }
        if let Some(v) = flag(root, "AutoAddNewResourcesAndTasks") {
            s.auto_add_resources = v;
        }
        if let Some(v) = text(root, "CurrencySymbol") {
            s.currency_symbol = v.to_string();
        }
        if let Some(v) = int(root, "CriticalSlackLimit").filter(|v| *v > 0) {
            s.critical_slack_threshold = v * s.durations.minutes_per_day;
        }
    }
    if flag(root, "ScheduleFromStart") == Some(false) {
        warnings.push("The project was scheduled from its finish date; ProjectFCG schedules forward from the start date.".into());
    }

    // ---- calendars ----
    let mut cals: HashMap<u32, RawCal> = HashMap::new();
    if let Some(cs) = kid(root, "Calendars") {
        for c in kids(cs, "Calendar") {
            let (uid, rc) = read_calendar(c, &mut warnings);
            cals.insert(uid, rc);
        }
    }
    let proj_cal = int(root, "CalendarUID").filter(|v| (0..=999_999_999).contains(v)).map(|u| u as u32).filter(|u| cals.contains_key(u)).or_else(|| cals.iter().filter(|(_, c)| c.base.is_none()).map(|(u, _)| *u).min());
    if let Some(u) = proj_cal {
        let week = resolve_week(&cals, u);
        // exceptions of the calendar and its bases
        // base calendars first, so the derived calendar's exceptions win
        let mut chain = vec![];
        let mut cur = Some(u);
        while let Some(c) = cur.and_then(|x| cals.get(&x)) {
            if chain.len() > 20 {
                break;
            }
            chain.push(c);
            cur = c.base;
        }
        let ex: Vec<CalendarException> = chain.iter().rev().flat_map(|c| c.exceptions.iter().cloned()).collect();
        p.calendar = Calendar { name: cals[&u].name.clone(), week, exceptions: ex };
    }

    // ---- tasks ----
    let mut rows: Vec<(i64, Task, Option<NaiveDateTime>)> = vec![];
    let mut task_cals = 0;
    let mut cross = 0;
    let mut dup_uids = 0;
    if let Some(ts) = kid(root, "Tasks") {
        for (k, tn) in kids(ts, "Task").enumerate() {
            let level = int(tn, "OutlineLevel").unwrap_or(1);
            // UID 0 is the project summary task; IsNull marks blank rows
            let Some(uid) = uid_of(tn, "UID") else { continue };
            if level <= 0 || flag(tn, "IsNull") == Some(true) {
                continue;
            }
            let id = int(tn, "ID").unwrap_or(k as i64 + 1);
            let mut t = Task::new(uid, text(tn, "Name").unwrap_or(""), TaskMode::Auto, &p.settings.durations);
            t.outline_level = level.min(100) as u32;
            t.priority = int(tn, "Priority").unwrap_or(500).clamp(0, 1000) as u32;
            t.task_type = match int(tn, "Type") {
                Some(1) => TaskType::FixedDuration,
                Some(2) => TaskType::FixedWork,
                _ => TaskType::FixedUnits,
            };
            t.effort_driven = flag(tn, "EffortDriven").unwrap_or(false);
            t.active = flag(tn, "Active").unwrap_or(true);
            let fmt = int(tn, "DurationFormat").unwrap_or(7).clamp(0, 100) as u32;
            let (unit, elapsed, est, _) = parse_dur_format(fmt);
            let minutes = dur_minutes(tn, "Duration").unwrap_or(p.settings.durations.minutes_per_day);
            t.duration = Duration { minutes: minutes.max(0), unit, elapsed, estimated: est || flag(tn, "Estimated").unwrap_or(false) };
            t.milestone = flag(tn, "Milestone").unwrap_or(false) && minutes > 0;
            if flag(tn, "Manual") == Some(true) {
                t.mode = TaskMode::Manual;
                t.manual_start = date_time(tn, "ManualStart").or_else(|| date_time(tn, "Start"));
                if let Some(m) = dur_minutes(tn, "ManualDuration") {
                    t.duration.minutes = m.max(0);
                }
            }
            t.percent_complete = float(tn, "PercentComplete").unwrap_or(0.0).clamp(0.0, 100.0);
            t.actual_start = date_time(tn, "ActualStart");
            t.actual_finish = date_time(tn, "ActualFinish").filter(|_| t.actual_start.is_some());
            t.fixed_cost = float(tn, "FixedCost").unwrap_or(0.0) / 100.0;
            t.constraint = match int(tn, "ConstraintType").unwrap_or(0) {
                1 => ConstraintType::ALAP,
                2 => ConstraintType::MSO,
                3 => ConstraintType::MFO,
                4 => ConstraintType::SNET,
                5 => ConstraintType::SNLT,
                6 => ConstraintType::FNET,
                7 => ConstraintType::FNLT,
                _ => ConstraintType::ASAP,
            };
            t.constraint_date = date_time(tn, "ConstraintDate").filter(|_| t.constraint.needs_date());
            if t.constraint.needs_date() && t.constraint_date.is_none() {
                t.constraint = ConstraintType::ASAP;
            }
            t.deadline = date_time(tn, "Deadline");
            t.notes = text(tn, "Notes").unwrap_or("").replace("\r\n", "\n");
            if let Some(ld) = int(tn, "LevelingDelay").filter(|v| *v != 0) {
                let (_, el, _, _) = parse_dur_format(int(tn, "LevelingDelayFormat").unwrap_or(8).clamp(0, 100) as u32);
                let m = (ld / 10).clamp(0, MAX_MINUTES);
                t.leveling_delay = if el { m * p.settings.durations.minutes_per_day / 1440 } else { m };
            }
            if int(tn, "CalendarUID").is_some_and(|c| c >= 0) {
                task_cals += 1;
            }
            for l in kids(tn, "PredecessorLink") {
                if flag(l, "CrossProject") == Some(true) {
                    cross += 1;
                    continue;
                }
                let Some(pred) = uid_of(l, "PredecessorUID") else { continue };
                let kind = match int(l, "Type").unwrap_or(1) {
                    0 => LinkType::FF,
                    2 => LinkType::SF,
                    3 => LinkType::SS,
                    _ => LinkType::FS,
                };
                let raw = int(l, "LinkLag").unwrap_or(0);
                let (lu, le, _, pct) = parse_dur_format(int(l, "LagFormat").unwrap_or(7).clamp(0, 100) as u32);
                let lag = if pct { Lag::Percent(raw.clamp(-10_000, 10_000) as f64) } else { Lag::Time { minutes: (raw / 10).clamp(-MAX_MINUTES, MAX_MINUTES), unit: lu, elapsed: le } };
                t.predecessors.push(Link { pred, kind, lag });
            }
            for bl in kids(tn, "Baseline") {
                let n = int(bl, "Number").unwrap_or(0);
                if !(0..=10).contains(&n) {
                    continue;
                }
                let (Some(s), Some(f)) = (date_time(bl, "Start"), date_time(bl, "Finish")) else { continue };
                *t.baseline_slot_mut(n as usize) = Some(Baseline {
                    start: s,
                    finish: f,
                    duration: dur_minutes(bl, "Duration").unwrap_or(0),
                    work: dur_minutes(bl, "Work").unwrap_or(0),
                    cost: float(bl, "Cost").unwrap_or(0.0) / 100.0,
                });
            }
            if rows.iter().any(|r: &(i64, Task, Option<NaiveDateTime>)| r.1.uid == t.uid) {
                dup_uids += 1;
                continue;
            }
            rows.push((id, t, date_time(tn, "Start")));
        }
    }
    rows.sort_by_key(|r| r.0);
    if task_cals > 0 {
        warnings.push(if task_cals == 1 {
            "1 task used a task calendar; ProjectFCG schedules it on the project and resource calendars.".to_string()
        } else {
            format!("{task_cals} tasks used task calendars; ProjectFCG schedules them on the project and resource calendars.")
        });
    }
    if dup_uids > 0 {
        warnings.push(format!("{dup_uids} task{} repeated another task's UID and {} skipped.", if dup_uids == 1 { "" } else { "s" }, if dup_uids == 1 { "was" } else { "were" }));
    }
    if cross > 0 {
        warnings.push(if cross == 1 { "1 link to another project was left out.".to_string() } else { format!("{cross} links to other projects were left out.") });
    }
    let file_starts: HashMap<u32, NaiveDateTime> = rows.iter().filter_map(|(_, t, s)| s.map(|s| (t.uid, s))).collect();
    p.tasks = rows.into_iter().map(|r| r.1).collect();
    p.normalize_outline();
    let uids: std::collections::HashSet<u32> = p.tasks.iter().map(|t| t.uid).collect();
    let mut dangling = 0;
    for t in p.tasks.iter_mut() {
        let before = t.predecessors.len();
        t.predecessors.retain(|l| uids.contains(&l.pred) && l.pred != t.uid);
        dangling += before - t.predecessors.len();
    }
    if dangling > 0 {
        warnings.push(if dangling == 1 { "1 link pointed at a missing task and was dropped.".to_string() } else { format!("{dangling} links pointed at missing tasks and were dropped.") });
    }
    p.next_uid = p.tasks.iter().map(|t| t.uid + 1).max().unwrap_or(1);
    // baselines saved: assume when the file was made
    for n in 0..=10 {
        if p.tasks.iter().any(|t| t.baseline_slot(n).is_some()) {
            p.baseline_saved[n] = date_time(root, "LastSaved").or(Some(start));
        }
    }

    // ---- resources ----
    let mut res_cal_uid: HashMap<u32, u32> = HashMap::new();
    if let Some(rs) = kid(root, "Resources") {
        for rn in kids(rs, "Resource") {
            let name = text(rn, "Name").unwrap_or("");
            let Some(uid) = uid_of(rn, "UID") else { continue };
            if name.is_empty() || flag(rn, "IsNull") == Some(true) {
                continue;
            }
            let rtype = if flag(rn, "IsCostResource") == Some(true) {
                ResourceType::Cost
            } else {
                match int(rn, "Type") {
                    Some(0) => ResourceType::Material,
                    Some(2) => ResourceType::Cost,
                    _ => ResourceType::Work,
                }
            };
            if p.resources.iter().any(|x| x.uid == uid) {
                continue;
            }
            let mut r = Resource { uid, name: name.to_string(), rtype, ..Default::default() };
            r.initials = text(rn, "Initials").map(str::to_string).unwrap_or_else(|| name.chars().take(1).collect());
            r.material_label = text(rn, "MaterialLabel").unwrap_or("").to_string();
            r.group = text(rn, "Group").unwrap_or("").to_string();
            r.email = text(rn, "EmailAddress").unwrap_or("").to_string();
            r.max_units = if rtype == ResourceType::Work { float(rn, "MaxUnits").filter(|v| *v > 0.0).unwrap_or(1.0) } else { 0.0 };
            r.std_rate = float(rn, "StandardRate").unwrap_or(0.0);
            r.ovt_rate = float(rn, "OvertimeRate").unwrap_or(0.0);
            r.cost_per_use = float(rn, "CostPerUse").unwrap_or(0.0) / 100.0;
            r.accrue = match int(rn, "AccrueAt") {
                Some(1) => Accrue::Start,
                Some(2) => Accrue::End,
                _ => Accrue::Prorated,
            };
            r.notes = text(rn, "Notes").unwrap_or("").to_string();
            if let Some(cu) = int(rn, "CalendarUID").filter(|c| (0..=999_999_999).contains(c)) {
                res_cal_uid.insert(r.uid, cu as u32);
            }
            p.resources.push(r);
        }
    }
    let mut other_base = 0;
    for r in p.resources.iter_mut() {
        let Some(cu) = res_cal_uid.get(&r.uid) else { continue };
        let Some(c) = cals.get(cu) else { continue };
        if c.base.is_some_and(|b| Some(b) != proj_cal) {
            other_base += 1;
        }
        if c.week.iter().any(|d| d.is_some()) {
            let w = resolve_week(&cals, *cu);
            if w != p.calendar.week {
                r.week = Some(w);
            }
        }
        // the resource's own changes only: the project calendar's holidays
        // already apply to everyone
        if Some(*cu) != proj_cal {
            r.exceptions = c.exceptions.iter().filter(|e| !p.calendar.exceptions.contains(e)).cloned().collect();
        }
    }
    if other_base > 0 {
        warnings.push(if other_base == 1 {
            "1 resource calendar was based on a calendar other than the project's; it now builds on the project calendar.".to_string()
        } else {
            format!("{other_base} resource calendars were based on a calendar other than the project's; they now build on the project calendar.")
        });
    }
    p.next_res_uid = p.resources.iter().map(|r| r.uid + 1).max().unwrap_or(1);

    // ---- assignments ----
    let mut lost_assignments = 0;
    if let Some(asn) = kid(root, "Assignments") {
        for a in kids(asn, "Assignment") {
            // a missing or negative ResourceUID is Project's "unassigned" placeholder
            let (Some(tu), Some(ru)) = (uid_of(a, "TaskUID"), uid_of(a, "ResourceUID")) else { continue };
            let (Some(r), Some(row)) = (p.resources.iter().find(|r| r.uid == ru), p.index_of_uid(tu)) else {
                lost_assignments += 1;
                continue;
            };
            if p.is_summary(row) {
                continue;
            }
            let units = float(a, "Units").unwrap_or(1.0).max(0.0);
            let work = dur_minutes(a, "Work").unwrap_or(0).max(0);
            let cost = float(a, "Cost").unwrap_or(0.0) / 100.0;
            let asg = match r.rtype {
                ResourceType::Work => Assignment { task: tu, resource: ru, units, work, cost: 0.0 },
                ResourceType::Material => Assignment { task: tu, resource: ru, units, work: 0, cost: 0.0 },
                ResourceType::Cost => Assignment { task: tu, resource: ru, units: 0.0, work: 0, cost },
            };
            if !p.assignments.iter().any(|x| x.task == tu && x.resource == ru) {
                p.assignments.push(asg);
            }
        }
    }

    if lost_assignments > 0 {
        warnings.push(if lost_assignments == 1 {
            "1 assignment referred to a task or resource that isn't in the file and was left out.".to_string()
        } else {
            format!("{lost_assignments} assignments referred to tasks or resources that aren't in the file and were left out.")
        });
    }
    crate::schedule::schedule(&mut p);
    // ---- how close is our schedule to the file's? ----
    let mut moved: Vec<String> = vec![];
    for (i, t) in p.tasks.iter().enumerate() {
        if t.calc.is_summary || t.mode == TaskMode::Manual {
            continue;
        }
        if let (Some(fs), Some(ours)) = (file_starts.get(&t.uid), t.calc.start) {
            if (ours - *fs).num_minutes().abs() > 1 {
                moved.push(format!("{} {}", i + 1, t.name));
            }
        }
    }
    if !moved.is_empty() {
        let n = moved.len();
        let shown: Vec<String> = moved.into_iter().take(3).collect();
        warnings.push(format!(
            "{n} task{} start{} at a different time than in the file (e.g. {}). Usually that's a feature ProjectFCG doesn't have yet, such as task calendars or splits.",
            if n == 1 { "" } else { "s" },
            if n == 1 { "s" } else { "" },
            shown.join("; ")
        ));
    }
    Ok((p, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_durations() {
        assert_eq!(parse_iso_duration("PT8H0M0S"), Some(480));
        assert_eq!(parse_iso_duration("PT0.5H"), Some(30));
        assert_eq!(parse_iso_duration("P1DT2H"), Some(1440 + 120));
        assert_eq!(parse_iso_duration("-PT30M"), Some(-30));
        assert_eq!(parse_iso_duration("PT1H30M15S"), Some(90));
        assert_eq!(parse_iso_duration("8h"), None);
        assert_eq!(iso_duration(16 * 60 + 5), "PT16H5M0S");
        assert_eq!(parse_iso_duration(&iso_duration(-725)), Some(-725));
    }

    #[test]
    fn formats_round_trip() {
        for u in [DurUnit::Minute, DurUnit::Hour, DurUnit::Day, DurUnit::Week, DurUnit::Month] {
            for el in [false, true] {
                for est in [false, true] {
                    let (u2, el2, est2, pct) = parse_dur_format(dur_format(u, el, est));
                    assert_eq!((u2, el2, est2, pct), (u, el, est, false));
                }
            }
        }
        assert!(parse_dur_format(19).3);
        assert_eq!(dur_format(DurUnit::Day, false, true), 39);
    }

    #[test]
    fn day_types() {
        assert_eq!(day_type(0), 2); // Monday
        assert_eq!(day_type(6), 1); // Sunday
        assert_eq!(day_type(5), 7); // Saturday
    }
}
