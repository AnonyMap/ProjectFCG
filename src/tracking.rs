//! Tracking progress against the plan: actual start/finish, % complete,
//! actual/remaining duration, Update Project, Mark on Track, and earned
//! value. The rules follow Microsoft Project's defaults:
//!
//! * % complete is actual duration ÷ duration. Entering it stamps the actual
//!   start (from the scheduled start), and at 100% the actual finish.
//! * An actual start pins the task: links and constraints no longer move it.
//!   An actual finish sets the duration to the working time between the
//!   two and makes the task 100% complete.
//! * Actual work and cost follow % complete ("updating task status updates
//!   resource status"). Each resource's cost accrues at start, at end, or
//!   prorated (its Accrue At setting); fixed costs are prorated.
//! * Earned value is measured at the status date (or today) against the
//!   baseline chosen in Options; see [`crate::earned_value`].

use crate::model::{Project, ResourceType};
use chrono::{NaiveDate, NaiveDateTime};

pub type OpResult<T = ()> = Result<T, String>;

/// Project ▸ Update Project ▸ "Update work as complete through".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateMode {
    /// Set 0%–100% complete in proportion to the time elapsed.
    Proportional,
    /// Only finished-by-then tasks become 100%; others stay as they are.
    ZeroOrHundred,
}

pub use crate::earned_value::EarnedValue;

impl Project {
    /// The date progress is measured at: the status date, or now.
    pub fn status_or_now(&self) -> NaiveDateTime {
        self.status_date.unwrap_or_else(|| chrono::Local::now().naive_local())
    }

    /// Set % complete with Project's side effects on actual start/finish.
    pub fn set_percent_complete(&mut self, row: usize, pct: f64) -> OpResult {
        if !(0.0..=100.0).contains(&pct) {
            return Err("Percent complete must be between 0% and 100%.".into());
        }
        if self.is_summary(row) {
            return Err("Summary percent complete is rolled up from subtasks.".into());
        }
        if self.progress_from_board(row) {
            return Err("This task's % complete comes from the Board (its tickets, or the epic that drives it).".into());
        }
        let t = &mut self.tasks[row];
        t.percent_complete = pct;
        if pct <= 0.0 {
            t.actual_start = None;
            t.actual_finish = None;
        } else {
            if t.actual_start.is_none() {
                t.actual_start = t.calc.start.or(Some(self.start));
            }
            if pct >= 100.0 {
                if t.actual_finish.is_none() {
                    t.actual_finish = t.calc.finish.filter(|&f| Some(f) >= t.actual_start);
                }
            } else {
                t.actual_finish = None;
            }
        }
        Ok(())
    }

    /// Actual Start (None = NA: the task is no longer started).
    pub fn set_actual_start(&mut self, row: usize, at: Option<NaiveDateTime>) -> OpResult {
        let at = at.map(crate::calendar::clamp_datetime);
        if self.is_summary(row) {
            return Err("Enter actuals on subtasks; summaries roll them up.".into());
        }
        let t = &mut self.tasks[row];
        match at {
            None => {
                t.actual_start = None;
                t.actual_finish = None;
                t.percent_complete = 0.0;
            }
            Some(a) => {
                if t.actual_finish.is_some_and(|f| f < a) {
                    return Err("The actual start can't be after the actual finish.".into());
                }
                t.actual_start = Some(a);
            }
        }
        Ok(())
    }

    /// Actual Finish: the task is 100% done, and its duration becomes the
    /// working time between actual start and finish.
    pub fn set_actual_finish(&mut self, row: usize, at: Option<NaiveDateTime>) -> OpResult {
        let at = at.map(crate::calendar::clamp_datetime);
        if self.is_summary(row) {
            return Err("Enter actuals on subtasks; summaries roll them up.".into());
        }
        let Some(f) = at else {
            let t = &mut self.tasks[row];
            t.actual_finish = None;
            if t.percent_complete >= 100.0 {
                t.percent_complete = 99.0_f64.min(t.percent_complete);
            }
            return Ok(());
        };
        let start = self.tasks[row].actual_start.or(self.tasks[row].calc.start).unwrap_or(self.start);
        if f < start {
            return Err("The actual finish can't be before the actual start.".into());
        }
        let mins = self.duration_calendar(row).work_between(start, f);
        if !self.tasks[row].duration.elapsed && mins != self.tasks[row].duration.minutes {
            let mut d = self.tasks[row].duration;
            d.minutes = mins.max(0);
            d.estimated = false;
            self.set_duration(row, d)?;
        }
        let t = &mut self.tasks[row];
        t.actual_start = Some(start);
        t.actual_finish = Some(f);
        t.percent_complete = 100.0;
        Ok(())
    }

    /// Actual Duration: % complete follows; more than the duration extends
    /// the task (and completes it).
    pub fn set_actual_duration(&mut self, row: usize, minutes: i64) -> OpResult {
        let dur = self.tasks[row].duration.minutes;
        if minutes < 0 {
            return Err("Actual duration can't be negative.".into());
        }
        if minutes > dur {
            let mut d = self.tasks[row].duration;
            d.minutes = minutes;
            self.set_duration(row, d)?;
            return self.set_percent_complete(row, 100.0);
        }
        if dur == 0 {
            // a milestone with no actual time: its progress doesn't change
            // (marking it done is % Complete's job)
            return Ok(());
        }
        let pct = minutes as f64 / dur as f64 * 100.0;
        self.set_percent_complete(row, (pct * 100.0).round() / 100.0)
    }

    /// Remaining Duration: duration = actual + remaining; % complete follows.
    pub fn set_remaining_duration(&mut self, row: usize, minutes: i64) -> OpResult {
        if minutes < 0 {
            return Err("Remaining duration can't be negative.".into());
        }
        let actual = self.actual_duration(row);
        let mut d = self.tasks[row].duration;
        d.minutes = actual + minutes;
        if d.minutes != self.tasks[row].duration.minutes {
            self.set_duration(row, d)?;
        }
        let total = actual + minutes;
        let pct = if total > 0 { actual as f64 / total as f64 * 100.0 } else { 100.0 };
        self.set_percent_complete(row, (pct * 100.0).round() / 100.0)
    }

    /// Working minutes done (duration × % complete).
    pub fn actual_duration(&self, row: usize) -> i64 {
        let t = &self.tasks[row];
        let d = if t.calc.is_summary { t.calc.summary_duration } else { t.duration.minutes };
        (d as f64 * t.calc.percent_complete.max(if t.calc.is_summary { 0.0 } else { t.percent_complete }) / 100.0).round() as i64
    }

    pub fn remaining_duration(&self, row: usize) -> i64 {
        let t = &self.tasks[row];
        let d = if t.calc.is_summary { t.calc.summary_duration } else { t.duration.minutes };
        (d - self.actual_duration(row)).max(0)
    }

    /// How complete the schedule says a task should be at `at` (0–100).
    pub fn scheduled_percent(&self, row: usize, at: NaiveDateTime) -> f64 {
        let t = &self.tasks[row];
        let (Some(s), Some(f)) = (t.calc.start, t.calc.finish) else { return 0.0 };
        if at >= f {
            return 100.0;
        }
        if at <= s {
            return 0.0;
        }
        let cal = self.scheduled_duration_calendar(row);
        let total = cal.work_between(s, f);
        if total <= 0 {
            return 100.0;
        }
        (cal.work_between(s, at) as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }

    /// Project ▸ Update Project. Returns the rows that changed. Progress is
    /// only ever raised, so work already reported isn't lost. With
    /// `reschedule_after`, unstarted tasks scheduled before that date get a
    /// Start No Earlier Than it (what Project does for them).
    pub fn update_project(&mut self, through: NaiveDateTime, mode: UpdateMode, rows: Option<&[usize]>, reschedule_after: Option<NaiveDateTime>) -> Vec<usize> {
        let through = crate::calendar::clamp_datetime(through);
        let reschedule_after = reschedule_after.map(crate::calendar::clamp_datetime);
        let targets: Vec<usize> = match rows {
            Some(r) => r.iter().copied().filter(|&i| i < self.tasks.len()).collect(),
            None => (0..self.tasks.len()).collect(),
        };
        let mut changed = vec![];
        for row in targets {
            if self.is_summary(row) || self.progress_from_board(row) {
                continue;
            }
            let want = match mode {
                UpdateMode::Proportional => self.scheduled_percent(row, through),
                UpdateMode::ZeroOrHundred => {
                    if self.scheduled_percent(row, through) >= 100.0 {
                        100.0
                    } else {
                        0.0
                    }
                }
            };
            let want = (want * 10.0).round() / 10.0;
            if want > self.tasks[row].percent_complete + 1e-9 && self.set_percent_complete(row, want).is_ok() {
                changed.push(row);
            }
        }
        if let Some(after) = reschedule_after {
            changed.extend(self.reschedule_uncompleted(after, rows));
        }
        changed.sort();
        changed.dedup();
        changed
    }

    /// Update Project ▸ Reschedule uncompleted work to start after: unstarted
    /// auto-scheduled tasks (all, or `rows`) that would start earlier get a
    /// Start No Earlier Than `after`.
    pub fn reschedule_uncompleted(&mut self, after: NaiveDateTime, rows: Option<&[usize]>) -> Vec<usize> {
        let after = crate::calendar::clamp_datetime(after);
        let targets: Vec<usize> = match rows {
            Some(r) => r.iter().copied().filter(|&i| i < self.tasks.len()).collect(),
            None => (0..self.tasks.len()).collect(),
        };
        let mut changed = vec![];
        for row in targets {
            let t = &self.tasks[row];
            if t.calc.is_summary || t.actual_start.is_some() || t.mode != crate::model::TaskMode::Auto {
                continue;
            }
            if t.calc.start.is_some_and(|s| s < after) && self.set_constraint(row, crate::model::ConstraintType::SNET, Some(after)).is_ok() {
                changed.push(row);
            }
        }
        changed
    }

    /// Task ▸ Mark on Track: % complete = what the schedule expects at the
    /// status date (or now).
    pub fn mark_on_track(&mut self, rows: &[usize]) -> OpResult<usize> {
        let at = self.status_or_now();
        let mut n = 0;
        for &row in rows {
            if row >= self.tasks.len() || self.is_summary(row) || self.progress_from_board(row) {
                continue;
            }
            let pct = (self.scheduled_percent(row, at) * 10.0).round() / 10.0;
            self.set_percent_complete(row, pct)?;
            n += 1;
        }
        if n == 0 {
            return Err("Select one or more subtasks to mark on track.".into());
        }
        Ok(n)
    }

    /// Earned value of a task (from its computed values).
    pub fn earned_value(&self, row: usize) -> EarnedValue {
        let c = &self.tasks[row].calc;
        EarnedValue { bcws: c.bcws, bcwp: c.bcwp, acwp: c.acwp, bac: c.bac, cost: c.cost, eac_method: self.settings.eac_method }
    }

    /// Earned value of the whole project (top-level tasks summed).
    pub fn project_earned_value(&self) -> EarnedValue {
        let mut ev = EarnedValue { eac_method: self.settings.eac_method, ..Default::default() };
        for i in 0..self.tasks.len() {
            if self.parent_of(i).is_none() {
                ev.add(&self.earned_value(i));
            }
        }
        ev
    }
}



/// Fill actual work/cost and earned value into every task's `calc` (after
/// costs are known). Leaves first, then rolled up through summaries.
pub(crate) fn compute_tracking(p: &mut Project, acost: &[f64], parents: &[Option<usize>]) {
    let n = p.tasks.len();
    let idx: std::collections::HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    let ridx: std::collections::HashMap<u32, usize> = p.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
    let mut actual_work = vec![0i64; n];
    for a in p.assignments.iter() {
        let (Some(&ti), Some(&ri)) = (idx.get(&a.task), ridx.get(&a.resource)) else { continue };
        if p.tasks[ti].calc.is_summary {
            continue;
        }
        if p.resources[ri].rtype == ResourceType::Work {
            actual_work[ti] += (a.work as f64 * p.tasks[ti].percent_complete / 100.0).round() as i64;
        }
    }
    for i in (0..n).rev() {
        if let Some(par) = parents[i] {
            actual_work[par] += actual_work[i];
        }
    }
    // actual cost is the earned value's ACWP at the status date
    let ev = p.earned_value_with(p.status_or_now(), acost, parents);
    for i in 0..n {
        let c = &mut p.tasks[i].calc;
        c.actual_work = actual_work[i];
        c.actual_cost = (ev[i].acwp * 100.0).round() / 100.0;
        c.acwp = c.actual_cost;
        c.bcws = ev[i].bcws;
        c.bcwp = ev[i].bcwp;
        c.bac = ev[i].bac;
    }
}

/// Points of the progress line at `at`, one per row: where each task's
/// progress has reached (x as a date). Tasks that should have started show
/// how far behind (left of the line) or ahead (right) they are; tasks not
/// due yet and finished tasks touch the line itself.
pub fn progress_point(p: &Project, row: usize, at: NaiveDateTime) -> Option<NaiveDateTime> {
    let t = &p.tasks[row];
    let (s, f) = (t.calc.start?, t.calc.finish?);
    let pct = t.calc.percent_complete.max(t.percent_complete);
    if pct >= 100.0 || s >= at {
        return Some(at);
    }
    // the point reached by the work done
    let cal = p.scheduled_duration_calendar(row);
    let total = cal.work_between(s, f);
    let done = (total as f64 * pct / 100.0).round() as i64;
    Some(if done <= 0 { s } else { cal.add_work(cal.next_working_start(s), done) })
}

/// Working days in a date range (for the Update Tasks dialog's hints).
pub fn day_count(p: &Project, a: NaiveDate, b: NaiveDate) -> i64 {
    let mut d = a;
    let mut n = 0;
    while d <= b {
        if p.calendar.working_minutes_on(d) > 0 {
            n += 1;
        }
        d = d.succ_opt().unwrap_or(d);
        if n > 10_000 {
            break;
        }
    }
    n
}
