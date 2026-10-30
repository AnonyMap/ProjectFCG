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

use chrono::{Datelike, Duration as ChronoDur, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
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
        (self.end - self.start) as i64
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
}

/// How far (in days) searches for the next/previous working time may go
/// before concluding the calendar has no working time at all.
const SEARCH_LIMIT_DAYS: i64 = 366 * 50;

pub fn minute_of_day(dt: NaiveDateTime) -> u32 {
    let t = dt.time();
    use chrono::Timelike;
    t.hour() * 60 + t.minute()
}

pub fn at_minute(date: NaiveDate, minute: u32) -> NaiveDateTime {
    if minute >= 1440 {
        (date + ChronoDur::days(1)).and_hms_opt(0, 0, 0).unwrap()
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
        }
    }

    /// Project's "24 Hours" base calendar.
    pub fn twenty_four_hours() -> Self {
        let day = vec![Shift::new(0, 1440)];
        Calendar {
            name: "24 Hours".into(),
            week: std::array::from_fn(|_| day.clone()),
            exceptions: vec![],
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
        }
    }

    pub fn shifts_on(&self, date: NaiveDate) -> &[Shift] {
        // Later exceptions take precedence, matching "last one wins" editing.
        for ex in self.exceptions.iter().rev() {
            if date >= ex.start && date <= ex.end {
                return &ex.shifts;
            }
        }
        &self.week[weekday_index(date.weekday())]
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

    /// True if any exception touches the inclusive date range.
    fn exception_in(&self, a: NaiveDate, b: NaiveDate) -> bool {
        self.exceptions.iter().any(|e| e.start <= b && e.end >= a)
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
            date += ChronoDur::days(1);
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
            date -= ChronoDur::days(1);
            m = 1440;
        }
        for _ in 0..SEARCH_LIMIT_DAYS {
            for s in self.shifts_on(date).iter().rev() {
                if m > s.start {
                    return at_minute(date, m.min(s.end));
                }
            }
            date -= ChronoDur::days(1);
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
            let prev = date - ChronoDur::days(1);
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
            return self.sub_work(start, -minutes);
        }
        if minutes == 0 || !self.has_any_working_time() {
            return start + ChronoDur::minutes(minutes);
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
            if m == 0 && week > 0 {
                while remaining > week && !self.exception_in(date, date + ChronoDur::days(6)) {
                    remaining -= week;
                    date += ChronoDur::days(7);
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
            cur = (date + ChronoDur::days(1)).and_hms_opt(0, 0, 0).unwrap();
        }
        cur
    }

    /// Subtract `minutes` of working time from `finish` (walk backwards).
    pub fn sub_work(&self, finish: NaiveDateTime, minutes: i64) -> NaiveDateTime {
        if minutes < 0 {
            return self.add_work(finish, -minutes);
        }
        if minutes == 0 || !self.has_any_working_time() {
            return finish - ChronoDur::minutes(minutes);
        }
        let mut remaining = minutes;
        let mut cur = self.prev_working_end(finish);
        let week = self.week_minutes();
        for _ in 0..SEARCH_LIMIT_DAYS {
            let mut date = cur.date();
            let mut m = minute_of_day(cur);
            if m == 0 {
                date -= ChronoDur::days(1);
                m = 1440;
                // whole weeks backwards (see add_work)
                if week > 0 {
                    while remaining > week && !self.exception_in(date - ChronoDur::days(6), date) {
                        remaining -= week;
                        date -= ChronoDur::days(7);
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
            date += ChronoDur::days(1);
        }
        total
    }

    /// Long ranges: partial first/last days, whole weeks of the pattern in
    /// between, then a correction for each exception day in the interior.
    fn work_between_fast(&self, a: NaiveDateTime, b: NaiveDateTime) -> i64 {
        let d0 = a.date();
        let d9 = b.date();
        let first_end = (d0 + ChronoDur::days(1)).and_hms_opt(0, 0, 0).unwrap();
        let mut total = self.work_between_days(a, first_end) + self.work_between_days(d9.and_hms_opt(0, 0, 0).unwrap(), b);
        // interior whole days d1..=d2
        let d1 = d0 + ChronoDur::days(1);
        let d2 = d9 - ChronoDur::days(1);
        let n = (d2 - d1).num_days() + 1;
        total += (n / 7) * self.week_minutes();
        let mut d = d1 + ChronoDur::days((n / 7) * 7);
        while d <= d2 {
            total += self.pattern_minutes(d);
            d += ChronoDur::days(1);
        }
        // exceptions override the pattern on the days they cover
        let mut days: Vec<NaiveDate> = vec![];
        for e in &self.exceptions {
            let s = e.start.max(d1);
            let f = e.end.min(d2);
            let mut x = s;
            while x <= f {
                days.push(x);
                x += ChronoDur::days(1);
            }
        }
        if !days.is_empty() {
            days.sort_unstable();
            days.dedup();
            for x in days {
                total += self.working_minutes_on(x).min(1440) - self.pattern_minutes(x);
            }
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
}
