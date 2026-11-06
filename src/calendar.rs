//! Working-time calendars and working-time arithmetic.
//!
//! All scheduling math in ProjectFCG happens in *working minutes*, the same unit
//! Microsoft Project uses internally (it stores tenths of minutes). A calendar
//! defines, for every date, a list of working shifts expressed as
//! `[start, end)` minute-of-day pairs.
//!
//! Conventions that mirror Project's behaviour:
//! * A task **start** is always placed at a working instant (if a computed
//!   start lands at 17:00 or on a weekend it moves to the next shift start).
//! * A task **finish** produced by adding work lands at the end of the last
//!   working minute consumed (5:00 PM, not 8:00 AM the next day).
//! * Late finishes snap *backwards* to the end of the previous shift.

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

/// A working interval within a single day, in minutes after midnight.
/// `end` may be 1440 for shifts running to midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shift {
    pub start: u32,
    pub end: u32,
}

impl Shift {
    pub const fn new(start: u32, end: u32) -> Self {
        Shift { start, end }
    }
    pub fn minutes(&self) -> i64 {
        // a shift ending before it starts (a damaged file) has no time
        i64::from(self.end.saturating_sub(self.start))
    }
}

/// A date-range override of the weekly pattern (holiday, overtime day, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarException {
    pub name: String,
    pub start: NaiveDate,
    /// Inclusive.
    pub end: NaiveDate,
    /// Empty = nonworking.
    pub shifts: Vec<Shift>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub name: String,
    /// Index 0 = Monday … 6 = Sunday. Empty vec = nonworking day.
    pub week: [Vec<Shift>; 7],
    pub exceptions: Vec<CalendarException>,
    /// The exceptions are in date order and don't overlap, so lookups can
    /// binary-search them. Set by `intersect` (whose results can hold one
    /// exception per day for centuries); never saved.
    #[serde(skip)]
    pub disjoint: bool,
}

/// How far (in days) searches for the next/previous working time may go
/// before concluding the calendar has no working time at all.
const SEARCH_LIMIT_DAYS: i64 = 366 * 50;

pub fn minute_of_day(dt: NaiveDateTime) -> u32 {
    let t = dt.time();
    use chrono::Timelike;
    t.hour() * 60 + t.minute()
}

/// The overlap of two sorted shift lists.
pub fn intersect_shifts(a: &[Shift], b: &[Shift]) -> Vec<Shift> {
    let mut out = vec![];
    for x in a {
        for y in b {
            let (s, e) = (x.start.max(y.start), x.end.min(y.end));
            if s < e {
                out.push(Shift::new(s, e));
            }
        }
    }
    out.sort_by_key(|s| s.start);
    out
}

/// Most minutes `add_work`/`sub_work` move: four centuries of elapsed
/// time. Inputs are capped well below this; this keeps a damaged file
/// from overflowing the date arithmetic.
pub const MAX_WORK: i64 = 4 * crate::duration::MAX_MINUTES;

/// The dates ProjectFCG accepts, typed or from a file (Project's own range
/// is 1984–2149). Arithmetic from inside this range never comes near
/// chrono's limits.
pub fn min_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(1900, 1, 1).expect("valid")
}
pub fn max_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2200, 12, 31).expect("valid")
}
/// Clamp a date-time into the accepted range.
pub fn clamp_datetime(dt: NaiveDateTime) -> NaiveDateTime {
    let lo = min_date().and_hms_opt(0, 0, 0).expect("valid");
    let hi = max_date().and_hms_opt(23, 59, 0).expect("valid");
    dt.clamp(lo, hi)
}

/// `d + n days`, stopping at chrono's limits instead of panicking.
pub fn plus_days(d: NaiveDate, n: i64) -> NaiveDate {
    chrono::TimeDelta::try_days(n).and_then(|x| d.checked_add_signed(x)).unwrap_or(if n >= 0 { NaiveDate::MAX } else { NaiveDate::MIN })
}

/// `dt + n minutes`, stopping at chrono's limits instead of panicking.
pub fn plus_minutes(dt: NaiveDateTime, n: i64) -> NaiveDateTime {
    chrono::TimeDelta::try_minutes(n).and_then(|x| dt.checked_add_signed(x)).unwrap_or(if n >= 0 { NaiveDateTime::MAX } else { NaiveDateTime::MIN })
}

pub fn at_minute(date: NaiveDate, minute: u32) -> NaiveDateTime {
    if minute >= 1440 {
        (plus_days(date, 1)).and_hms_opt(0, 0, 0).unwrap()
    } else {
        date.and_time(NaiveTime::from_hms_opt(minute / 60, minute % 60, 0).unwrap())
    }
}

fn weekday_index(w: Weekday) -> usize {
    w.num_days_from_monday() as usize
}

impl Calendar {
    /// Project's "Standard" base calendar: Mon–Fri 8:00–12:00, 13:00–17:00.
    pub fn standard() -> Self {
        let day = vec![Shift::new(8 * 60, 12 * 60), Shift::new(13 * 60, 17 * 60)];
        Calendar {
            name: "Standard".into(),
            week: [
                day.clone(),
                day.clone(),
                day.clone(),
                day.clone(),
                day,
                vec![],
                vec![],
            ],
            exceptions: vec![],
            disjoint: false,
        }
    }

    /// Project's "24 Hours" base calendar.
    pub fn twenty_four_hours() -> Self {
        let day = vec![Shift::new(0, 1440)];
        Calendar {
            name: "24 Hours".into(),
            week: std::array::from_fn(|_| day.clone()),
            exceptions: vec![],
            disjoint: false,
        }
    }

    /// Project's "Night Shift" base calendar (Mon night through Sat morning).
    pub fn night_shift() -> Self {
        let mid = vec![
            Shift::new(0, 3 * 60),
            Shift::new(4 * 60, 8 * 60),
            Shift::new(23 * 60, 1440),
        ];
        Calendar {
            name: "Night Shift".into(),
            week: [
                vec![Shift::new(23 * 60, 1440)],
                mid.clone(),
                mid.clone(),
                mid.clone(),
                mid.clone(),
                vec![Shift::new(0, 3 * 60), Shift::new(4 * 60, 8 * 60)],
                vec![],
            ],
            exceptions: vec![],
            disjoint: false,
        }
    }

    /// Working time both calendars have: a task with its own calendar,
    /// done by a resource with another, works when both do (Project's rule
    /// unless "Scheduling ignores resource calendars"). Days either one
    /// has an exception for become exceptions of the result.
    ///
    /// Exceptions are intersected as ranges: between consecutive exception
    /// boundaries each calendar is either inside one exception (fixed
    /// shifts) or on its weekly pattern, so the result repeats weekly
    /// there. A stretch that is the same every weekday (a year away: no
    /// work) stays one exception, however long; only a stretch that varies
    /// by weekday is written out day by day, merging runs.
    pub fn intersect(&self, other: &Calendar) -> Calendar {
        let week = std::array::from_fn(|d| intersect_shifts(&self.week[d], &other.week[d]));
        let mut cuts: Vec<NaiveDate> = vec![];
        for e in self.exceptions.iter().chain(&other.exceptions) {
            if e.end < e.start {
                continue;
            }
            cuts.push(e.start);
            cuts.push(e.end.succ_opt().unwrap_or(e.end));
        }
        cuts.sort();
        cuts.dedup();
        let mut exceptions: Vec<CalendarException> = vec![];
        let mut push = |d0: NaiveDate, d1: NaiveDate, shifts: Vec<Shift>| match exceptions.last_mut() {
            Some(last) if last.shifts == shifts && last.end.succ_opt() == Some(d0) => last.end = d1,
            _ => exceptions.push(CalendarException { name: String::new(), start: d0, end: d1, shifts }),
        };
        for w in cuts.windows(2) {
            let (a, Some(b)) = (w[0], w[1].pred_opt()) else { continue };
            if !self.in_exception(a) && !other.in_exception(a) {
                continue;
            }
            let days = (b - a).num_days() + 1;
            let day = |k: i64| {
                let d = plus_days(a, k);
                (d, intersect_shifts(self.shifts_on(d), other.shifts_on(d)))
            };
            // days that come out the same as the result's own week need no
            // exception (a 24-hour exception ∩ Standard is just Standard)
            let plain = |k: i64| {
                let (d, s) = day(k);
                s == week[weekday_index(d.weekday())]
            };
            let first: Vec<Vec<Shift>> = (0..days.min(7)).map(|k| day(k).1).collect();
            if (0..days.min(7)).all(plain) {
                continue;
            }
            if first.iter().all(|s| *s == first[0]) {
                push(a, b, first[0].clone());
            } else {
                for k in 0..days {
                    let (d, s) = day(k);
                    if s != week[weekday_index(d.weekday())] {
                        push(d, d, s);
                    }
                }
            }
        }
        Calendar { name: format!("{} ∩ {}", self.name, other.name), week, exceptions, disjoint: true }
    }

    fn in_exception(&self, date: NaiveDate) -> bool {
        self.exception_on(date).is_some()
    }

    /// The exception that sets `date`'s working time, if any.
    fn exception_on(&self, date: NaiveDate) -> Option<&CalendarException> {
        if self.disjoint {
            let k = self.exceptions.partition_point(|e| e.start <= date);
            return k.checked_sub(1).map(|k| &self.exceptions[k]).filter(|e| e.end >= date);
        }
        // later exceptions take precedence, matching "last one wins" editing
        self.exceptions.iter().rev().find(|ex| date >= ex.start && date <= ex.end)
    }

    /// The exceptions that may touch `a..=b` (all of them, unless sorted).
    fn exceptions_near(&self, a: NaiveDate, b: NaiveDate) -> &[CalendarException] {
        if !self.disjoint {
            return &self.exceptions;
        }
        let lo = self.exceptions.partition_point(|e| e.end < a);
        let hi = self.exceptions.partition_point(|e| e.start <= b);
        &self.exceptions[lo..hi.max(lo)]
    }

    pub fn shifts_on(&self, date: NaiveDate) -> &[Shift] {
        match self.exception_on(date) {
            Some(ex) => &ex.shifts,
            None => &self.week[weekday_index(date.weekday())],
        }
    }

    pub fn is_working_day(&self, date: NaiveDate) -> bool {
        !self.shifts_on(date).is_empty()
    }

    pub fn working_minutes_on(&self, date: NaiveDate) -> i64 {
        self.shifts_on(date).iter().map(|s| s.minutes()).sum()
    }

    /// Working minutes of the weekly pattern (ignoring exceptions).
    pub fn week_minutes(&self) -> i64 {
        self.week.iter().map(|d| d.iter().map(|s| s.minutes()).sum::<i64>()).sum()
    }

    fn pattern_minutes(&self, date: NaiveDate) -> i64 {
        self.week[weekday_index(date.weekday())].iter().map(|s| s.minutes()).sum()
    }

    /// The first day at or after `date` that an exception covers.
    fn next_exception_start(&self, date: NaiveDate) -> Option<NaiveDate> {
        if self.disjoint {
            let i = self.exceptions.partition_point(|e| e.end < date);
            return self.exceptions.get(i).map(|e| e.start.max(date));
        }
        self.exceptions.iter().filter(|e| e.end >= date).map(|e| e.start.max(date)).min()
    }

    /// The last day at or before `date` that an exception covers.
    fn prev_exception_end(&self, date: NaiveDate) -> Option<NaiveDate> {
        if self.disjoint {
            let i = self.exceptions.partition_point(|e| e.start <= date);
            return i.checked_sub(1).map(|i| self.exceptions[i].end.min(date));
        }
        self.exceptions.iter().filter(|e| e.start <= date).map(|e| e.end.min(date)).max()
    }

    /// From `date` forward: the last day of the stretch over which one
    /// exception sets every day, and its working minutes a day. Walks can
    /// cross a long closure or a long special-hours period in one step.
    fn run_forward(&self, date: NaiveDate) -> Option<(NaiveDate, i64)> {
        let e = self.exception_on(date)?;
        // another exception starting inside the stretch may take over there
        let next = self.exceptions_near(date, e.end).iter().filter(|f| f.start > date && f.start <= e.end).map(|f| f.start).min();
        let last = next.map_or(e.end, |n| plus_days(n, -1));
        Some((last, e.shifts.iter().map(|s| s.minutes()).sum::<i64>().min(1440)))
    }

    /// From `date` backward: the first day of the stretch one exception
    /// sets, and its minutes a day.
    fn run_backward(&self, date: NaiveDate) -> Option<(NaiveDate, i64)> {
        let e = self.exception_on(date)?;
        let prev = self.exceptions_near(e.start, date).iter().filter(|f| f.end < date && f.end >= e.start).map(|f| f.end).max();
        let first = prev.map_or(e.start, |p| plus_days(p, 1));
        Some((first, e.shifts.iter().map(|s| s.minutes()).sum::<i64>().min(1440)))
    }

    pub fn has_any_working_time(&self) -> bool {
        self.week.iter().any(|d| !d.is_empty())
            || self.exceptions.iter().any(|e| !e.shifts.is_empty())
    }

    /// First working instant at or after `dt`. An instant exactly at a shift
    /// end is *not* working (the shift is half-open), so 17:00 → next 8:00.
    pub fn next_working_start(&self, dt: NaiveDateTime) -> NaiveDateTime {
        if !self.has_any_working_time() {
            return dt;
        }
        let mut date = dt.date();
        let mut m = minute_of_day(dt);
        for _ in 0..SEARCH_LIMIT_DAYS {
            for s in self.shifts_on(date) {
                if m < s.end {
                    return at_minute(date, m.max(s.start));
                }
            }
            if date == NaiveDate::MAX {
                break; // the end of time: nothing further to walk
            }
            // a closure: past all of it at once
            if let Some((last, 0)) = self.run_forward(date) {
                date = last;
            }
            date = plus_days(date, 1);
            m = 0;
        }
        dt
    }

    /// Last working-period end at or before `dt`. An instant inside a shift
    /// (or exactly at its end) is returned unchanged; 8:00 Monday → Friday 17:00.
    pub fn prev_working_end(&self, dt: NaiveDateTime) -> NaiveDateTime {
        if !self.has_any_working_time() {
            return dt;
        }
        let mut date = dt.date();
        let mut m = minute_of_day(dt);
        // midnight counts as end-of-previous-day position 1440
        if m == 0 {
            date = plus_days(date, -(1));
            m = 1440;
        }
        for _ in 0..SEARCH_LIMIT_DAYS {
            for s in self.shifts_on(date).iter().rev() {
                if m > s.start {
                    return at_minute(date, m.min(s.end));
                }
            }
            if let Some((first, 0)) = self.run_backward(date) {
                date = first;
            }
            date = plus_days(date, -(1));
            m = 1440;
        }
        dt
    }

    /// True if `dt` lies at the end of a shift (e.g. 12:00 or 17:00), the
    /// natural resting place for a milestone or finish date.
    pub fn is_shift_end(&self, dt: NaiveDateTime) -> bool {
        let m = minute_of_day(dt);
        let date = dt.date();
        if m == 0 {
            let prev = plus_days(date, -(1));
            return self.shifts_on(prev).iter().any(|s| s.end == 1440);
        }
        self.shifts_on(date).iter().any(|s| s.end == m)
    }

    pub fn is_working_instant(&self, dt: NaiveDateTime) -> bool {
        let m = minute_of_day(dt);
        self.shifts_on(dt.date())
            .iter()
            .any(|s| m >= s.start && m < s.end)
    }

    /// Add `minutes` of working time to `start`. Zero returns `start`
    /// unchanged. The result is where the work ends (may be a shift end).
    pub fn add_work(&self, start: NaiveDateTime, minutes: i64) -> NaiveDateTime {
        if minutes < 0 {
            // saturating: -i64::MIN would be i64::MIN again, and the two
            // would call each other forever
            return self.sub_work(start, minutes.saturating_neg().min(MAX_WORK));
        }
        // far past any plan, and short of where dates overflow
        let minutes = minutes.min(MAX_WORK);
        if minutes == 0 || !self.has_any_working_time() {
            return plus_minutes(start, minutes);
        }
        let mut remaining = minutes;
        let mut cur = self.next_working_start(start);
        let week = self.week_minutes();
        for _ in 0..SEARCH_LIMIT_DAYS {
            let mut date = cur.date();
            let m = minute_of_day(cur);
            // whole weeks at a time while the remainder is longer than a
            // week and no exception interrupts the pattern (exact: the
            // pattern repeats every 7 days)
            if m == 0 && week > 0 && remaining > week {
                // all the whole weeks before the next exception, at once
                let free = match self.next_exception_start(date) {
                    Some(s) => (s - date).num_days().max(0),
                    None => i64::MAX,
                };
                let k = ((remaining - 1) / week).min(free / 7);
                if k > 0 {
                    remaining -= k * week;
                    date = plus_days(date, 7 * k);
                }
            }
            // inside one exception's stretch: whole days at once, keeping
            // the last day's work for the exact time below
            if m == 0 {
                if let Some((last, per)) = self.run_forward(date) {
                    let days = (last - date).num_days();
                    let k = if per == 0 { days } else { ((remaining - 1) / per).min(days) };
                    if k > 0 {
                        remaining -= k * per;
                        date = plus_days(date, k);
                    }
                }
            }
            for s in self.shifts_on(date) {
                if m >= s.end {
                    continue;
                }
                let from = m.max(s.start);
                let avail = (s.end - from) as i64;
                if remaining <= avail {
                    return at_minute(date, from + remaining as u32);
                }
                remaining -= avail;
            }
            cur = (plus_days(date, 1)).and_hms_opt(0, 0, 0).unwrap();
        }
        cur
    }

    /// Subtract `minutes` of working time from `finish` (walk backwards).
    pub fn sub_work(&self, finish: NaiveDateTime, minutes: i64) -> NaiveDateTime {
        if minutes < 0 {
            return self.add_work(finish, minutes.saturating_neg().min(MAX_WORK));
        }
        let minutes = minutes.min(MAX_WORK);
        if minutes == 0 || !self.has_any_working_time() {
            return plus_minutes(finish, -(minutes));
        }
        let mut remaining = minutes;
        let mut cur = self.prev_working_end(finish);
        let week = self.week_minutes();
        for _ in 0..SEARCH_LIMIT_DAYS {
            let mut date = cur.date();
            let mut m = minute_of_day(cur);
            if m == 0 {
                date = plus_days(date, -(1));
                m = 1440;
                // whole weeks backwards (see add_work)
                if week > 0 && remaining > week {
                    let free = match self.prev_exception_end(date) {
                        Some(e) => (date - e).num_days().max(0),
                        None => i64::MAX,
                    };
                    let k = ((remaining - 1) / week).min(free / 7);
                    if k > 0 {
                        remaining -= k * week;
                        date = plus_days(date, -7 * k);
                    }
                }
                // whole days of one exception's stretch (see add_work)
                if let Some((first, per)) = self.run_backward(date) {
                    let days = (date - first).num_days();
                    let k = if per == 0 { days } else { ((remaining - 1) / per).min(days) };
                    if k > 0 {
                        remaining -= k * per;
                        date = plus_days(date, -k);
                    }
                }
            }
            for s in self.shifts_on(date).iter().rev() {
                if m <= s.start {
                    continue;
                }
                let to = m.min(s.end);
                let avail = (to - s.start) as i64;
                if remaining <= avail {
                    return at_minute(date, to - remaining as u32);
                }
                remaining -= avail;
            }
            // continue from end of previous day
            cur = date.and_hms_opt(0, 0, 0).unwrap();
            // represent "end of previous day" — handled by m==0 rule above
        }
        cur
    }

    /// Signed working minutes from `a` to `b`.
    pub fn work_between(&self, a: NaiveDateTime, b: NaiveDateTime) -> i64 {
        if b < a {
            return -self.work_between(b, a);
        }
        if a == b {
            return 0;
        }
        if !self.has_any_working_time() {
            return (b - a).num_minutes();
        }
        let end_date = b.date();
        if (end_date - a.date()).num_days() > 8 {
            return self.work_between_fast(a, b);
        }
        self.work_between_days(a, b)
    }

    /// Day-by-day walk (short ranges; also the reference for the fast path).
    fn work_between_days(&self, a: NaiveDateTime, b: NaiveDateTime) -> i64 {
        let mut total = 0i64;
        let mut date = a.date();
        let end_date = b.date();
        while date <= end_date {
            let lo = if date == a.date() { minute_of_day(a) } else { 0 };
            let hi = if date == end_date { minute_of_day(b) } else { 1440 };
            for s in self.shifts_on(date) {
                let from = s.start.max(lo);
                let to = s.end.min(hi);
                if to > from {
                    total += (to - from) as i64;
                }
            }
            if date == NaiveDate::MAX {
                break; // the end of time: nothing further to walk
            }
            date = plus_days(date, 1);
        }
        total
    }

    /// Long ranges: partial first/last days, whole weeks of the pattern in
    /// between, then a correction for each exception day in the interior.
    fn work_between_fast(&self, a: NaiveDateTime, b: NaiveDateTime) -> i64 {
        let d0 = a.date();
        let d9 = b.date();
        let first_end = (plus_days(d0, 1)).and_hms_opt(0, 0, 0).unwrap();
        let mut total = self.work_between_days(a, first_end) + self.work_between_days(d9.and_hms_opt(0, 0, 0).unwrap(), b);
        // interior whole days d1..=d2
        let d1 = plus_days(d0, 1);
        let d2 = plus_days(d9, -(1));
        let n = (d2 - d1).num_days() + 1;
        total += (n / 7) * self.week_minutes();
        let mut d = plus_days(d1, (n / 7) * 7);
        while d <= d2 {
            total += self.pattern_minutes(d);
            if d == NaiveDate::MAX {
                break; // the end of time: nothing further to walk
            }
            d = plus_days(d, 1);
        }
        // exceptions override the pattern on the days they cover: per
        // stretch between exception boundaries (where one exception rules
        // every day), its minutes a day less the pattern's, times the days.
        // Linear in exceptions, not days: a century's closure is one step.
        if d1 <= d2 {
            let near = self.exceptions_near(d1, d2);
            let mut cuts: Vec<NaiveDate> = vec![d1, plus_days(d2, 1)];
            for e in near.iter().filter(|e| e.end >= d1 && e.start <= d2) {
                cuts.push(e.start.max(d1));
                cuts.push(plus_days(e.end.min(d2), 1));
            }
            cuts.sort_unstable();
            cuts.dedup();
            for w in cuts.windows(2) {
                let (a, b) = (w[0], plus_days(w[1], -1));
                if b < a {
                    continue;
                }
                let Some(ex) = self.exception_on(a) else { continue };
                let per_day = ex.shifts.iter().map(|s| s.minutes()).sum::<i64>().min(1440);
                let days = (b - a).num_days() + 1;
                total += days * per_day - self.pattern_between(a, b);
            }
        }
        total
    }

    /// The weekly pattern's minutes over the days `a..=b`.
    fn pattern_between(&self, a: NaiveDate, b: NaiveDate) -> i64 {
        let n = (b - a).num_days() + 1;
        let mut total = (n / 7) * self.week_minutes();
        let mut d = plus_days(a, (n / 7) * 7);
        while d <= b {
            total += self.pattern_minutes(d);
            if d == NaiveDate::MAX {
                break; // the end of time: nothing further to walk
            }
            d = plus_days(d, 1);
        }
        total
    }

    /// Snap a computed start date onto working time. Zero-length tasks
    /// (milestones) may rest at a shift end, which is what Project shows
    /// for a milestone that follows a task finishing at 5:00 PM.
    pub fn snap_start(&self, dt: NaiveDateTime, zero_length: bool) -> NaiveDateTime {
        if zero_length && (self.is_shift_end(dt) || self.is_working_instant(dt)) {
            dt
        } else {
            self.next_working_start(dt)
        }
    }

    /// Snap a late finish backwards onto working time.
    pub fn snap_finish(&self, dt: NaiveDateTime, zero_length: bool) -> NaiveDateTime {
        if zero_length && self.is_working_instant(dt) {
            dt
        } else {
            self.prev_working_end(dt)
        }
    }

    /// First shift start of the day (used for default start times).
    pub fn day_start_minute(&self, date: NaiveDate) -> Option<u32> {
        self.shifts_on(date).first().map(|s| s.start)
    }

    pub fn day_end_minute(&self, date: NaiveDate) -> Option<u32> {
        self.shifts_on(date).last().map(|s| s.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDur;

    fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn one_day_from_monday_morning() {
        let c = Calendar::standard();
        // 2026-01-05 is a Monday
        assert_eq!(c.add_work(dt(2026, 1, 5, 8, 0), 480), dt(2026, 1, 5, 17, 0));
    }

    #[test]
    fn five_days_spans_week() {
        let c = Calendar::standard();
        assert_eq!(c.add_work(dt(2026, 1, 5, 8, 0), 5 * 480), dt(2026, 1, 9, 17, 0));
        // starting Wednesday, 5 days ends next Tuesday
        assert_eq!(c.add_work(dt(2026, 1, 7, 8, 0), 5 * 480), dt(2026, 1, 13, 17, 0));
    }

    #[test]
    fn next_start_after_finish_skips_weekend() {
        let c = Calendar::standard();
        assert_eq!(c.next_working_start(dt(2026, 1, 9, 17, 0)), dt(2026, 1, 12, 8, 0));
        assert_eq!(c.next_working_start(dt(2026, 1, 5, 12, 0)), dt(2026, 1, 5, 13, 0));
        assert_eq!(c.next_working_start(dt(2026, 1, 5, 9, 30)), dt(2026, 1, 5, 9, 30));
    }

    #[test]
    fn prev_end_before_monday_is_friday() {
        let c = Calendar::standard();
        assert_eq!(c.prev_working_end(dt(2026, 1, 12, 8, 0)), dt(2026, 1, 9, 17, 0));
        assert_eq!(c.prev_working_end(dt(2026, 1, 12, 0, 0)), dt(2026, 1, 9, 17, 0));
        assert_eq!(c.prev_working_end(dt(2026, 1, 12, 13, 0)), dt(2026, 1, 12, 12, 0));
    }

    #[test]
    fn subtract_is_inverse_of_add() {
        let c = Calendar::standard();
        let s = dt(2026, 1, 7, 8, 0);
        for mins in [60, 240, 300, 480, 2400, 4800] {
            let f = c.add_work(s, mins);
            assert_eq!(c.sub_work(f, mins), s, "mins={mins}");
            assert_eq!(c.work_between(s, f), mins);
        }
    }

    #[test]
    fn half_day_lands_at_noon() {
        let c = Calendar::standard();
        assert_eq!(c.add_work(dt(2026, 1, 5, 8, 0), 240), dt(2026, 1, 5, 12, 0));
        assert_eq!(c.add_work(dt(2026, 1, 5, 12, 0), 240), dt(2026, 1, 5, 17, 0));
    }

    #[test]
    fn holiday_exception_is_skipped() {
        let mut c = Calendar::standard();
        let hol = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        c.exceptions.push(CalendarException {
            name: "Holiday".into(),
            start: hol,
            end: hol,
            shifts: vec![],
        });
        assert_eq!(c.add_work(dt(2026, 1, 5, 8, 0), 3 * 480), dt(2026, 1, 8, 17, 0));
        assert_eq!(c.work_between(dt(2026, 1, 5, 8, 0), dt(2026, 1, 9, 17, 0)), 4 * 480);
    }

    #[test]
    fn twenty_four_hour_calendar() {
        let c = Calendar::twenty_four_hours();
        assert_eq!(c.add_work(dt(2026, 1, 10, 0, 0), 1440), dt(2026, 1, 11, 0, 0));
        assert_eq!(c.sub_work(dt(2026, 1, 11, 0, 0), 1440), dt(2026, 1, 10, 0, 0));
        assert_eq!(c.work_between(dt(2026, 1, 10, 0, 0), dt(2026, 1, 12, 0, 0)), 2880);
    }

    #[test]
    fn work_between_negative() {
        let c = Calendar::standard();
        assert_eq!(c.work_between(dt(2026, 1, 9, 17, 0), dt(2026, 1, 8, 17, 0)), -480);
    }

    #[test]
    fn empty_calendar_does_not_hang() {
        let c = Calendar {
            name: "none".into(),
            week: Default::default(),
            exceptions: vec![],
            disjoint: false,
        };
        let s = dt(2026, 1, 5, 8, 0);
        assert_eq!(c.add_work(s, 60), s + ChronoDur::minutes(60));
    }

    /// The fast paths must agree exactly with the day-by-day definitions on
    /// arbitrary calendars (night shifts, odd weeks, overlapping exceptions,
    /// exception working days) and arbitrary instants.
    #[test]
    fn fast_paths_match_reference() {
        let mut seed = 12345u64;
        let mut rnd = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for case in 0..300 {
            let mut cal = match case % 3 {
                0 => Calendar::standard(),
                1 => Calendar::night_shift(),
                _ => {
                    let mut c = Calendar::standard();
                    c.week[2] = vec![]; // no Wednesdays
                    c.week[5] = vec![Shift::new(9 * 60, 13 * 60)];
                    c
                }
            };
            for _ in 0..rnd(6) {
                let s = base + ChronoDur::days(rnd(400) as i64);
                let e = s + ChronoDur::days(rnd(20) as i64);
                let shifts = if rnd(3) == 0 { vec![Shift::new(10 * 60, 14 * 60)] } else { vec![] };
                cal.exceptions.push(CalendarException { name: "x".into(), start: s, end: e, shifts });
            }
            for _ in 0..20 {
                let a = base.and_hms_opt(0, 0, 0).unwrap() + ChronoDur::minutes(rnd(400 * 1440) as i64);
                let b = a + ChronoDur::minutes(rnd(300 * 1440) as i64);
                assert_eq!(cal.work_between(a, b), cal.work_between_days(a, b), "work_between {a} {b} case {case}");
                assert_eq!(cal.work_between(b, a), -cal.work_between_days(a, b));
                let w = rnd(200 * 480) as i64 + 1;
                let f = cal.add_work(a, w);
                assert_eq!(cal.work_between(cal.next_working_start(a), f), w, "add_work {a} +{w} case {case}");
                let s2 = cal.sub_work(f, w);
                assert_eq!(cal.work_between(s2, f), w, "sub_work {f} -{w} case {case}");
                assert!(s2 <= f);
            }
        }
    }

    /// The day-by-day walks (no week or exception jumps) the fast paths
    /// must agree with, on calendars with overlapping, long, closed and
    /// special-hours exceptions.
    #[test]
    fn jumps_across_exceptions_match_a_day_by_day_walk() {
        fn naive_add(c: &Calendar, start: NaiveDateTime, minutes: i64) -> NaiveDateTime {
            let mut remaining = minutes;
            let mut date = start.date();
            let mut m = minute_of_day(start);
            loop {
                for s in c.shifts_on(date) {
                    if m >= s.end {
                        continue;
                    }
                    let from = m.max(s.start);
                    let avail = (s.end - from) as i64;
                    if remaining <= avail {
                        return at_minute(date, from + remaining as u32);
                    }
                    remaining -= avail;
                }
                date = plus_days(date, 1);
                m = 0;
            }
        }
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for case in 0..300 {
            let mut c = Calendar::standard();
            for _ in 0..rnd(6) {
                let s = plus_days(base, rnd(900) as i64);
                let long = rnd(3) == 0;
                let e = plus_days(s, rnd(if long { 2000 } else { 15 }) as i64);
                let shifts = match rnd(3) {
                    0 => vec![],
                    1 => vec![Shift::new(9 * 60, 13 * 60)],
                    _ => vec![Shift::new(6 * 60, 10 * 60), Shift::new(12 * 60, 20 * 60)],
                };
                c.exceptions.push(CalendarException { name: String::new(), start: s, end: e, shifts });
            }
            if !c.has_any_working_time() {
                continue;
            }
            let start = c.next_working_start(base.and_hms_opt(8, 0, 0).unwrap() + ChronoDur::minutes(rnd(600 * 1440) as i64));
            let mins = 1 + rnd(400 * 480) as i64;
            let fast = c.add_work(start, mins);
            assert_eq!(fast, naive_add(&c, start, mins), "case {case}: add_work {start} + {mins}");
            assert_eq!(c.work_between(start, fast), mins, "case {case}: work_between agrees");
            let back = c.sub_work(fast, mins);
            assert_eq!(c.work_between(back, fast), mins, "case {case}: sub_work {fast} - {mins} gave {back}");
            assert_eq!(c.next_working_start(back), c.next_working_start(start), "case {case}: back to the start");
        }
    }
}
