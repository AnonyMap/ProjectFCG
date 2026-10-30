//! Recurring tasks (Task ▸ Insert ▸ Recurring Task): a summary task whose
//! subtasks are the occurrences, each held at its date by a Start No
//! Earlier Than constraint — the same structure Project builds. The
//! pattern is kept on the summary so it can be edited and regenerated.

use crate::duration::Duration;
use crate::model::{ConstraintType, Project};
use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};

pub type OpResult<T = ()> = Result<T, String>;

/// Project caps a series at 999 occurrences.
pub const MAX_OCCURRENCES: usize = 999;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurPattern {
    Daily,
    #[default]
    Weekly,
    Monthly,
    Yearly,
}

impl RecurPattern {
    pub const ALL: [RecurPattern; 4] = [RecurPattern::Daily, RecurPattern::Weekly, RecurPattern::Monthly, RecurPattern::Yearly];
    pub fn label(self) -> &'static str {
        match self {
            RecurPattern::Daily => "Daily",
            RecurPattern::Weekly => "Weekly",
            RecurPattern::Monthly => "Monthly",
            RecurPattern::Yearly => "Yearly",
        }
    }
    fn unit(self, n: u32) -> &'static str {
        match (self, n == 1) {
            (RecurPattern::Daily, true) => "day",
            (RecurPattern::Daily, false) => "days",
            (RecurPattern::Weekly, true) => "week",
            (RecurPattern::Weekly, false) => "weeks",
            (RecurPattern::Monthly, true) => "month",
            (RecurPattern::Monthly, false) => "months",
            (RecurPattern::Yearly, true) => "year",
            (RecurPattern::Yearly, false) => "years",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurEnd {
    After(u32),
    By(NaiveDate),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recurrence {
    pub pattern: RecurPattern,
    /// Every N days/weeks/months/years.
    pub every: u32,
    /// Weekly: which days (0 = Monday … 6 = Sunday).
    pub weekdays: [bool; 7],
    /// Daily: working days only.
    pub working_days_only: bool,
    pub start: NaiveDate,
    pub end: RecurEnd,
    /// Length of each occurrence.
    pub duration: Duration,
    /// The occurrence tasks (uids), so editing the series updates them in
    /// place: links and assignments on them survive.
    #[serde(default)]
    pub occurrences: Vec<u32>,
}

impl Recurrence {
    pub fn describe(&self) -> String {
        let every = if self.every <= 1 { format!("every {}", self.pattern.unit(1)) } else { format!("every {} {}", self.every, self.pattern.unit(self.every)) };
        let days = if self.pattern == RecurPattern::Weekly {
            const N: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            let d: Vec<&str> = (0..7).filter(|&i| self.weekdays[i]).map(|i| N[i]).collect();
            format!(" on {}", d.join(", "))
        } else {
            String::new()
        };
        let end = match self.end {
            RecurEnd::After(n) => format!("{n} times"),
            RecurEnd::By(d) => format!("until {}", d.format("%-m/%-d/%y")),
        };
        format!("{} {every}{days}, {end}", self.pattern.label())
    }
}

/// The dates of a series (before moving off nonworking days).
pub fn occurrence_dates(r: &Recurrence, cal: &crate::calendar::Calendar) -> Vec<NaiveDate> {
    let every = r.every.max(1);
    let limit = match r.end {
        RecurEnd::After(n) => (n as usize).min(MAX_OCCURRENCES),
        RecurEnd::By(_) => MAX_OCCURRENCES,
    };
    let until = match r.end {
        RecurEnd::By(d) => d,
        RecurEnd::After(_) => NaiveDate::MAX,
    };
    let mut out = vec![];
    let push = |d: NaiveDate, out: &mut Vec<NaiveDate>| -> bool {
        if d > until || out.len() >= limit {
            return false;
        }
        out.push(d);
        true
    };
    match r.pattern {
        RecurPattern::Daily => {
            let mut d = r.start;
            let mut k = 0u32;
            for _ in 0..40_000 {
                let counts = !r.working_days_only || cal.working_minutes_on(d) > 0;
                if counts {
                    if k % every == 0 && !push(d, &mut out) {
                        break;
                    }
                    k += 1;
                }
                if d > until {
                    break;
                }
                d = match d.succ_opt() {
                    Some(x) => x,
                    None => break,
                };
            }
        }
        RecurPattern::Weekly => {
            let days: Vec<usize> = (0..7).filter(|&i| r.weekdays[i]).collect();
            let days = if days.is_empty() { vec![r.start.weekday().num_days_from_monday() as usize] } else { days };
            let monday = r.start - chrono::Duration::days(r.start.weekday().num_days_from_monday() as i64);
            'weeks: for w in 0..5_000u32 {
                let wk = monday + chrono::Duration::weeks((w * every) as i64);
                for &dd in &days {
                    let d = wk + chrono::Duration::days(dd as i64);
                    if d < r.start {
                        continue;
                    }
                    if !push(d, &mut out) {
                        break 'weeks;
                    }
                }
            }
        }
        RecurPattern::Monthly | RecurPattern::Yearly => {
            let step = if r.pattern == RecurPattern::Monthly { every } else { every * 12 };
            let day = r.start.day();
            for k in 0..2_000u32 {
                let Some(first) = r.start.with_day(1).and_then(|f| f.checked_add_months(Months::new(k * step))) else { break };
                // the 31st in a 30-day month is the month's last day, as in Project
                let last = first.checked_add_months(Months::new(1)).and_then(|n| n.pred_opt()).map(|d| d.day()).unwrap_or(28);
                let Some(d) = first.with_day(day.min(last)) else { break };
                if !push(d, &mut out) {
                    break;
                }
            }
        }
    }
    out
}

impl Project {
    /// Insert a recurring task before row `at`. Returns the summary's row.
    pub fn insert_recurring(&mut self, at: usize, name: &str, r: Recurrence) -> OpResult<usize> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A recurring task needs a name.".into());
        }
        // check the pattern before touching the plan
        self.series_days(&r)?;
        let row = self.insert_task(at, name);
        self.tasks[row].recurrence = Some(Recurrence { occurrences: vec![], ..r });
        if let Err(e) = self.regenerate_recurring(row) {
            self.delete_tasks(&[row]);
            return Err(e);
        }
        Ok(row)
    }

    /// Change a recurring task's name or pattern. Occurrences are updated
    /// in place (links to them and their assignments are kept); extra ones
    /// are removed and new ones added. Other tasks put under the summary
    /// are left alone.
    pub fn update_recurring(&mut self, row: usize, name: &str, r: Recurrence) -> OpResult {
        let Some(old) = self.tasks.get(row).and_then(|t| t.recurrence.clone()) else {
            return Err("That isn't a recurring task.".into());
        };
        if name.trim().is_empty() {
            return Err("A recurring task needs a name.".into());
        }
        self.series_days(&r)?;
        self.tasks[row].name = name.trim().to_string();
        self.tasks[row].recurrence = Some(Recurrence { occurrences: old.occurrences, ..r });
        self.regenerate_recurring(row)
    }

    /// The working days a series lands on: each date moved off days off,
    /// without doubling up (a daily series over a weekend doesn't put three
    /// occurrences on Monday).
    fn series_days(&self, r: &Recurrence) -> OpResult<Vec<NaiveDate>> {
        if r.duration.minutes <= 0 {
            return Err("Each occurrence needs a duration.".into());
        }
        let mut days: Vec<NaiveDate> = vec![];
        for d in occurrence_dates(r, &self.calendar) {
            let mut day = d;
            for _ in 0..366 {
                if self.calendar.working_minutes_on(day) > 0 {
                    break;
                }
                day = day.succ_opt().unwrap_or(day);
            }
            if !days.contains(&day) {
                days.push(day);
            }
        }
        if days.is_empty() {
            return Err("That pattern has no occurrences: check its start and end.".into());
        }
        Ok(days)
    }

    /// Bring a recurring summary's occurrences in line with its pattern.
    pub fn regenerate_recurring(&mut self, row: usize) -> OpResult {
        let Some(r) = self.tasks[row].recurrence.clone() else { return Err("That isn't a recurring task.".into()) };
        let days = self.series_days(&r)?;
        let summary_uid = self.tasks[row].uid;
        // the existing occurrences, in row order, that are still under the summary
        let in_tree = |p: &Project, u: u32| p.index_of_uid(u).is_some_and(|i| p.parent_of(i).is_some_and(|a| p.tasks[a].uid == summary_uid));
        let mut kept: Vec<u32> = r.occurrences.iter().copied().filter(|&u| in_tree(self, u)).collect();
        kept.sort_by_key(|&u| self.index_of_uid(u));
        if kept.len() > days.len() {
            let extra: Vec<usize> = kept[days.len()..].iter().filter_map(|&u| self.index_of_uid(u)).collect();
            self.delete_tasks(&extra);
            kept.truncate(days.len());
        }
        let row = self.index_of_uid(summary_uid).ok_or("The recurring task was deleted.")?;
        let level = self.tasks[row].outline_level;
        let name = self.tasks[row].name.clone();
        let start_min = self.settings.default_start_minute;
        let mut uids = vec![];
        for (k, day) in days.iter().enumerate() {
            let at = match kept.get(k) {
                Some(&u) => self.index_of_uid(u).unwrap(),
                None => {
                    // after the last occurrence (or right under the summary)
                    let after = uids.last().and_then(|&u| self.index_of_uid(u)).unwrap_or(row);
                    let t = self.new_task("");
                    self.tasks.insert(after + 1, t);
                    self.tasks[after + 1].outline_level = level + 1;
                    after + 1
                }
            };
            let t = &mut self.tasks[at];
            t.name = format!("{name} {}", k + 1);
            t.duration = r.duration;
            t.mode = crate::model::TaskMode::Auto;
            t.constraint = ConstraintType::SNET;
            t.constraint_date = Some(crate::calendar::at_minute(*day, start_min));
            uids.push(t.uid);
        }
        self.normalize_outline();
        let row = self.index_of_uid(summary_uid).unwrap();
        if let Some(rc) = self.tasks[row].recurrence.as_mut() {
            rc.occurrences = uids;
        }
        Ok(())
    }

    /// Is `row` an occurrence of a recurring task?
    pub fn is_occurrence(&self, row: usize) -> bool {
        self.parent_of(row).is_some_and(|p| self.tasks[p].recurrence.is_some())
    }
}
