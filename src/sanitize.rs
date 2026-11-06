//! Make a plan from a file safe to schedule: every number finite and in a
//! sensible range, every date between 1900 and 2200, every unique ID
//! unique. A damaged or hand-edited file then opens with its odd values
//! pulled into range instead of crashing the scheduler (dates near the
//! end of time overflow date arithmetic; a duration of −9.2e18 minutes
//! recursed forever).
//!
//! Run on every file opened (ProjectFCG, Project XML, Jira CSV) before
//! anything is scheduled. Returns how many values it changed.

use crate::calendar::{clamp_datetime, max_date, min_date, CalendarException, Shift};
use crate::duration::{Lag, MAX_MINUTES};
use crate::model::Project;
use chrono::{NaiveDate, NaiveDateTime};

struct Fixer {
    n: usize,
}

impl Fixer {
    fn dt(&mut self, d: &mut NaiveDateTime) {
        let c = clamp_datetime(*d);
        if c != *d {
            *d = c;
            self.n += 1;
        }
    }
    fn odt(&mut self, d: &mut Option<NaiveDateTime>) {
        if let Some(x) = d {
            self.dt(x);
        }
    }
    fn date(&mut self, d: &mut NaiveDate) {
        let c = (*d).clamp(min_date(), max_date());
        if c != *d {
            *d = c;
            self.n += 1;
        }
    }
    fn odate(&mut self, d: &mut Option<NaiveDate>) {
        if let Some(x) = d {
            self.date(x);
        }
    }
    fn num(&mut self, v: &mut f64, lo: f64, hi: f64, default: f64) {
        let c = if v.is_finite() { v.clamp(lo, hi) } else { default };
        if c.to_bits() != v.to_bits() {
            *v = c;
            self.n += 1;
        }
    }
    fn int(&mut self, v: &mut i64, lo: i64, hi: i64) {
        let c = (*v).clamp(lo, hi);
        if c != *v {
            *v = c;
            self.n += 1;
        }
    }
    fn lag(&mut self, l: &mut Lag) {
        match l {
            Lag::Time { minutes, .. } => self.int(minutes, -MAX_MINUTES, MAX_MINUTES),
            Lag::Percent(p) => self.num(p, -100_000.0, 100_000.0, 0.0),
        }
    }
    /// Shifts inside the day, each ending after it starts, in order and
    /// not overlapping.
    fn shifts(&mut self, s: &mut Vec<Shift>) {
        let before = s.clone();
        s.retain(|x| x.start < x.end && x.end <= 1440);
        s.sort_by_key(|x| x.start);
        let mut out: Vec<Shift> = vec![];
        for x in s.drain(..) {
            match out.last_mut() {
                Some(last) if x.start < last.end => last.end = last.end.max(x.end),
                _ => out.push(x),
            }
        }
        *s = out;
        if *s != before {
            self.n += 1;
        }
    }
    fn exceptions(&mut self, ex: &mut Vec<CalendarException>) {
        for e in ex.iter_mut() {
            self.date(&mut e.start);
            self.date(&mut e.end);
            if e.end < e.start {
                std::mem::swap(&mut e.start, &mut e.end);
                self.n += 1;
            }
            self.shifts(&mut e.shifts);
        }
    }
    fn calendar(&mut self, c: &mut crate::calendar::Calendar) {
        for d in c.week.iter_mut() {
            self.shifts(d);
        }
        self.exceptions(&mut c.exceptions);
    }
}

impl Project {
    /// See the module docs. Returns the number of values changed.
    pub fn sanitize(&mut self) -> usize {
        let mut f = Fixer { n: 0 };
        // ---- project ----
        f.dt(&mut self.start);
        f.odt(&mut self.status_date);
        f.odt(&mut self.finish_date);
        for b in self.baseline_saved.iter_mut() {
            f.odt(b);
        }
        let s = &mut self.settings;
        f.int(&mut s.durations.minutes_per_day, 1, 1440);
        f.int(&mut s.durations.minutes_per_week, s.durations.minutes_per_day, 7 * 1440);
        f.int(&mut s.durations.days_per_month, 1, 31);
        for m in [&mut s.default_start_minute, &mut s.default_finish_minute] {
            if *m > 1439 {
                *m = 1439;
                f.n += 1;
            }
        }
        f.int(&mut s.critical_slack_threshold, -MAX_MINUTES, MAX_MINUTES);
        f.calendar(&mut self.calendar);
        for bc in self.calendars.iter_mut() {
            f.calendar(&mut bc.calendar);
        }
        // ---- tasks ----
        for t in self.tasks.iter_mut() {
            f.int(&mut t.duration.minutes, 0, MAX_MINUTES);
            f.odt(&mut t.manual_start);
            f.odt(&mut t.constraint_date);
            f.odt(&mut t.deadline);
            f.odt(&mut t.actual_start);
            f.odt(&mut t.actual_finish);
            f.num(&mut t.percent_complete, 0.0, 100.0, 0.0);
            f.num(&mut t.physical_percent, 0.0, 100.0, 0.0);
            f.num(&mut t.fixed_cost, -1e12, 1e12, 0.0);
            if t.priority > 1000 {
                t.priority = 1000;
                f.n += 1;
            }
            f.int(&mut t.leveling_delay, 0, MAX_MINUTES);
            if t.outline_level == 0 {
                t.outline_level = 1;
                f.n += 1;
            }
            for l in t.predecessors.iter_mut() {
                f.lag(&mut l.lag);
            }
            for d in t.board_deps.iter_mut() {
                f.lag(&mut d.lag);
            }
            for b in std::iter::once(&mut t.baseline).chain(t.baselines.iter_mut()).flatten() {
                f.dt(&mut b.start);
                f.dt(&mut b.finish);
                f.int(&mut b.duration, 0, MAX_MINUTES);
                f.int(&mut b.work, 0, crate::calendar::MAX_WORK);
                f.num(&mut b.cost, -1e12, 1e12, 0.0);
            }
            if let Some(r) = t.recurrence.as_mut() {
                if !(1..=99).contains(&r.every) {
                    r.every = r.every.clamp(1, 99);
                    f.n += 1;
                }
                f.date(&mut r.start);
                f.int(&mut r.duration.minutes, 0, MAX_MINUTES);
                match &mut r.end {
                    crate::recurring::RecurEnd::After(k) if *k > 999 => {
                        *k = 999;
                        f.n += 1;
                    }
                    crate::recurring::RecurEnd::By(d) => f.date(d),
                    _ => {}
                }
            }
        }
        // ---- resources and assignments ----
        for r in self.resources.iter_mut() {
            f.num(&mut r.max_units, 0.0, 1e6, 1.0);
            f.num(&mut r.std_rate, -1e9, 1e9, 0.0);
            f.num(&mut r.ovt_rate, -1e9, 1e9, 0.0);
            f.num(&mut r.cost_per_use, -1e12, 1e12, 0.0);
            f.exceptions(&mut r.exceptions);
            if let Some(w) = r.week.as_mut() {
                for d in w.iter_mut() {
                    f.shifts(d);
                }
            }
        }
        for a in self.assignments.iter_mut() {
            f.num(&mut a.units, 0.0, 1e6, 1.0);
            f.int(&mut a.work, 0, crate::calendar::MAX_WORK);
            f.num(&mut a.cost, -1e12, 1e12, 0.0);
        }
        // ---- board ----
        for t in self.tickets.iter_mut() {
            if let Some(p) = t.points.as_mut() {
                f.num(p, 0.0, 1e6, 0.0);
            }
            f.odt(&mut t.done_at);
            f.odt(&mut t.created);
            for (d, _) in t.history.iter_mut() {
                f.dt(d);
            }
        }
        for r in self.board.releases.iter_mut() {
            f.odate(&mut r.start);
            f.odate(&mut r.date);
            f.odt(&mut r.released);
        }
        // ---- unique IDs ----
        f.n += self.repair_uids();
        f.n
    }

    /// Task unique IDs: duplicates get fresh ones (links and assignments
    /// keep pointing at the first task with the ID), and the next ID can't
    /// run past the end of the range.
    pub fn repair_uids(&mut self) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut dups = vec![];
        for (i, t) in self.tasks.iter().enumerate() {
            if t.uid == 0 || !seen.insert(t.uid) {
                dups.push(i);
            }
        }
        let max = self.tasks.iter().map(|t| t.uid).max().unwrap_or(0);
        if self.next_uid <= max {
            self.next_uid = max.saturating_add(1);
        }
        self.next_uid = self.next_uid.max(1);
        for &i in &dups {
            let u = self.alloc_uid();
            self.tasks[i].uid = u;
        }
        dups.len()
    }
}
