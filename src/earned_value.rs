//! Earned value management, Microsoft Project's way.
//!
//! Every figure is measured at the status date (or now) against the
//! baseline chosen in Options (Baseline, or Baseline 1–10):
//!
//! * **Planned value** (PV, BCWS): the baseline cost planned to be spent
//!   by then. It is timephased like Project's: a resource's cost that
//!   accrues at the start counts from the baseline start, one that
//!   accrues at the end at the baseline finish, and the rest is spread
//!   over the baseline's working time.
//! * **Earned value** (EV, BCWP): the baseline cost of the work done, by
//!   each task's earned value method: % Complete or Physical % Complete
//!   (Project's two), or the discrete techniques of DoD EVMS: 0/100,
//!   50/50 and level of effort.
//! * **Actual cost** (AC, ACWP): what the work done cost.
//!
//! The indices and forecasts follow from these: SV = EV − PV, SV% = SV ÷
//! PV, CV = EV − AC, CV% = CV ÷ EV, SPI = EV ÷ PV, CPI = EV ÷ AC, EAC (by
//! the chosen method), ETC = EAC − AC, VAC = BAC − EAC, TCPI = (BAC − EV) ÷
//! (BAC − AC), and TCPI to the EAC = (BAC − EV) ÷ (EAC − AC).
//!
//! For dates before the status date (the S-curve, the current period),
//! the work done on a task is spread evenly over the working time from its
//! actual start to where its progress reached: Project timephases actual
//! work the same way when it is entered as % complete.

use crate::model::{Accrue, Baseline, EacMethod, EvMethod, EvTerms, Project};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

/// Earned value figures for a task or the project.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EarnedValue {
    pub bcws: f64,
    pub bcwp: f64,
    pub acwp: f64,
    pub bac: f64,
    /// The scheduled cost now (actual + remaining): the bottom-up
    /// estimate at completion.
    pub cost: f64,
    pub eac_method: EacMethod,
}

impl EarnedValue {
    /// Schedule variance (EV − PV): negative = behind.
    pub fn sv(&self) -> f64 {
        self.bcwp - self.bcws
    }
    /// Cost variance (EV − AC): negative = over budget.
    pub fn cv(&self) -> f64 {
        self.bcwp - self.acwp
    }
    /// SV ÷ PV, as a percentage.
    pub fn sv_percent(&self) -> Option<f64> {
        (self.bcws.abs() > 1e-9).then(|| self.sv() / self.bcws * 100.0)
    }
    /// CV ÷ EV, as a percentage.
    pub fn cv_percent(&self) -> Option<f64> {
        (self.bcwp.abs() > 1e-9).then(|| self.cv() / self.bcwp * 100.0)
    }
    /// Schedule performance index; `None` before anything was planned.
    pub fn spi(&self) -> Option<f64> {
        (self.bcws > 0.0).then(|| self.bcwp / self.bcws)
    }
    /// Cost performance index; `None` before any actual cost.
    pub fn cpi(&self) -> Option<f64> {
        (self.acwp > 0.0).then(|| self.bcwp / self.acwp)
    }
    /// Estimate at completion, by [`EacMethod`]. While an index it needs
    /// is unknown (nothing spent or planned yet), the budget stands.
    pub fn eac(&self) -> f64 {
        let by_cpi = || match self.cpi() {
            Some(c) if c > 0.0 => self.acwp + (self.bac - self.bcwp) / c,
            _ => self.bac.max(self.acwp),
        };
        match self.eac_method {
            EacMethod::Cpi => by_cpi(),
            EacMethod::CpiSpi => match (self.cpi(), self.spi()) {
                (Some(c), Some(s)) if c * s > 0.0 => self.acwp + (self.bac - self.bcwp) / (c * s),
                _ => by_cpi(),
            },
            EacMethod::BudgetRate => self.acwp + (self.bac - self.bcwp),
            EacMethod::Schedule => self.cost.max(self.acwp),
        }
    }
    /// Estimate to complete (EAC − AC): what the rest should cost.
    pub fn etc(&self) -> f64 {
        (self.eac() - self.acwp).max(0.0)
    }
    /// Variance at completion (BAC − EAC): negative = forecast overrun.
    pub fn vac(&self) -> f64 {
        self.bac - self.eac()
    }
    /// VAC ÷ BAC, as a percentage.
    pub fn vac_percent(&self) -> Option<f64> {
        (self.bac.abs() > 1e-9).then(|| self.vac() / self.bac * 100.0)
    }
    /// To-complete performance index (Project's TCPI; DoD's TCPI to the
    /// BAC): the CPI the rest must achieve to finish on budget.
    pub fn tcpi(&self) -> Option<f64> {
        let left = self.bac - self.acwp;
        (left.abs() > 1e-9).then(|| (self.bac - self.bcwp) / left)
    }
    /// TCPI to the EAC: the CPI the rest must achieve to finish at the
    /// estimate (DoD and the DOE Gold Card).
    pub fn tcpi_eac(&self) -> Option<f64> {
        let left = self.eac() - self.acwp;
        (left.abs() > 1e-9).then(|| (self.bac - self.bcwp) / left)
    }
    /// Shares of the budget: planned (PV ÷ BAC), done (EV ÷ BAC) and spent
    /// (AC ÷ BAC), as percentages.
    pub fn percent_scheduled(&self) -> Option<f64> {
        (self.bac > 0.0).then(|| self.bcws / self.bac * 100.0)
    }
    pub fn percent_complete(&self) -> Option<f64> {
        (self.bac > 0.0).then(|| self.bcwp / self.bac * 100.0)
    }
    pub fn percent_spent(&self) -> Option<f64> {
        (self.bac > 0.0).then(|| self.acwp / self.bac * 100.0)
    }

    pub fn add(&mut self, o: &EarnedValue) {
        self.bcws += o.bcws;
        self.bcwp += o.bcwp;
        self.acwp += o.acwp;
        self.bac += o.bac;
        self.cost += o.cost;
    }

    /// What changed between `earlier` and this (a period's figures; the
    /// budget and estimate stay this one's).
    pub fn since(&self, earlier: &EarnedValue) -> EarnedValue {
        EarnedValue { bcws: self.bcws - earlier.bcws, bcwp: self.bcwp - earlier.bcwp, acwp: self.acwp - earlier.acwp, ..*self }
    }
}

/// The quantities earned value reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Pv,
    Ev,
    Ac,
    Sv,
    SvPercent,
    Cv,
    CvPercent,
    Spi,
    Cpi,
    Bac,
    Eac,
    Etc,
    Vac,
    Tcpi,
    TcpiEac,
}

impl EvTerms {
    /// The short name (a column heading).
    pub fn abbr(self, m: Metric) -> &'static str {
        use Metric::*;
        match (self, m) {
            (EvTerms::Pmi, Pv) => "PV",
            (EvTerms::Pmi, Ev) => "EV",
            (EvTerms::Pmi, Ac) => "AC",
            (EvTerms::Dod, Pv) => "BCWS",
            (EvTerms::Dod, Ev) => "BCWP",
            (EvTerms::Dod, Ac) => "ACWP",
            (EvTerms::Both, Pv) => "PV (BCWS)",
            (EvTerms::Both, Ev) => "EV (BCWP)",
            (EvTerms::Both, Ac) => "AC (ACWP)",
            (EvTerms::Pmi, Tcpi) => "TCPI",
            (_, Tcpi) => "TCPI (BAC)",
            (_, TcpiEac) => "TCPI (EAC)",
            (_, Sv) => "SV",
            (_, SvPercent) => "SV%",
            (_, Cv) => "CV",
            (_, CvPercent) => "CV%",
            (_, Spi) => "SPI",
            (_, Cpi) => "CPI",
            (_, Bac) => "BAC",
            (_, Eac) => "EAC",
            (_, Etc) => "ETC",
            (_, Vac) => "VAC",
        }
    }

    /// The full name.
    pub fn name(self, m: Metric) -> &'static str {
        use Metric::*;
        match (self, m) {
            (EvTerms::Dod, Pv) => "Budgeted Cost of Work Scheduled",
            (EvTerms::Dod, Ev) => "Budgeted Cost of Work Performed",
            (EvTerms::Dod, Ac) => "Actual Cost of Work Performed",
            (EvTerms::Both, Pv) => "Planned Value (Budgeted Cost of Work Scheduled)",
            (EvTerms::Both, Ev) => "Earned Value (Budgeted Cost of Work Performed)",
            (EvTerms::Both, Ac) => "Actual Cost (Actual Cost of Work Performed)",
            (_, Pv) => "Planned Value",
            (_, Ev) => "Earned Value",
            (_, Ac) => "Actual Cost",
            (_, Sv) => "Schedule Variance",
            (_, SvPercent) => "Schedule Variance %",
            (_, Cv) => "Cost Variance",
            (_, CvPercent) => "Cost Variance %",
            (_, Spi) => "Schedule Performance Index",
            (_, Cpi) => "Cost Performance Index",
            (_, Bac) => "Budget at Completion",
            (_, Eac) => "Estimate at Completion",
            (_, Etc) => "Estimate to Complete",
            (_, Vac) => "Variance at Completion",
            (_, Tcpi) => "To-Complete Performance Index",
            (_, TcpiEac) => "To-Complete Performance Index (to the EAC)",
        }
    }
}

/// Share (0–1) of the working time from `s` to `f` that has passed at `at`.
fn share(cal: &crate::calendar::Calendar, s: NaiveDateTime, f: NaiveDateTime, at: NaiveDateTime) -> f64 {
    if at >= f {
        return 1.0;
    }
    if at <= s {
        return 0.0;
    }
    let total = cal.work_between(s, f);
    if total <= 0 {
        // no working time between: elapsed time instead
        let all = (f - s).num_minutes().max(1) as f64;
        return ((at - s).num_minutes() as f64 / all).clamp(0.0, 1.0);
    }
    (cal.work_between(s, at) as f64 / total as f64).clamp(0.0, 1.0)
}

/// Planned value of a task's baseline at `at`, by its earned value method.
pub fn planned(b: &Baseline, method: EvMethod, cal: &crate::calendar::Calendar, at: NaiveDateTime) -> f64 {
    let started = at > b.start || at >= b.finish;
    let finished = at >= b.finish;
    match method {
        EvMethod::ZeroHundred => {
            if finished {
                b.cost
            } else {
                0.0
            }
        }
        EvMethod::FiftyFifty => b.cost / 2.0 * (started as u8 as f64 + finished as u8 as f64),
        _ => {
            let (st, en) = (b.cost_at_start, b.cost_at_end);
            let prorated = b.cost - st - en;
            prorated * share(cal, b.start, b.finish, at) + if started { st } else { 0.0 } + if finished { en } else { 0.0 }
        }
    }
}

/// Earned value of a task with budget `bac`, % complete `pct` and
/// physical % complete `phys`, whose planned value is `pv`.
pub fn earned(method: EvMethod, bac: f64, pct: f64, phys: f64, pv: f64) -> f64 {
    let pct = pct.clamp(0.0, 100.0);
    match method {
        EvMethod::PercentComplete => bac * pct / 100.0,
        EvMethod::PhysicalPercentComplete => bac * phys.clamp(0.0, 100.0) / 100.0,
        EvMethod::ZeroHundred => {
            if pct >= 100.0 {
                bac
            } else {
                0.0
            }
        }
        EvMethod::FiftyFifty => {
            if pct >= 100.0 {
                bac
            } else if pct > 0.0 {
                bac / 2.0
            } else {
                0.0
            }
        }
        EvMethod::LevelOfEffort => pv,
    }
}

/// Actual cost of an assignment of `cost` at `pct` % complete.
pub(crate) fn accrued(cost: f64, accrue: Accrue, pct: f64) -> f64 {
    match accrue {
        Accrue::Start => {
            if pct > 0.0 {
                cost
            } else {
                0.0
            }
        }
        Accrue::End => {
            if pct >= 100.0 {
                cost
            } else {
                0.0
            }
        }
        Accrue::Prorated => cost * pct / 100.0,
    }
}

impl Project {
    /// The baseline earned value is measured against (Options).
    pub fn ev_baseline_of(&self, row: usize) -> Option<Baseline> {
        self.tasks[row].baseline_slot(self.settings.ev_baseline.min(10) as usize)
    }

    /// The fraction (0–1) of a task's current progress that had been made
    /// by `at`: progress is spread over the working time from the actual
    /// start to where it reached (at most the status date).
    pub fn progress_share_at(&self, row: usize, at: NaiveDateTime) -> f64 {
        let status = self.status_or_now();
        if at >= status {
            return 1.0;
        }
        let t = &self.tasks[row];
        let Some(a) = t.actual_start.or(t.calc.start) else { return 1.0 };
        if at <= a {
            return 0.0;
        }
        let pct = t.calc.percent_complete.max(t.percent_complete);
        let reached = if pct >= 100.0 { t.actual_finish.or(t.calc.finish) } else { crate::tracking::progress_point(self, row, status) };
        let end = reached.unwrap_or(status).min(status);
        if end <= a {
            return 1.0;
        }
        share(&self.scheduled_duration_calendar(row), a, end, at)
    }

    /// Earned value of every task at `at` (summaries rolled up), with the
    /// cost of each assignment from `acost`.
    pub(crate) fn earned_value_with(&self, at: NaiveDateTime, acost: &[f64], parents: &[Option<usize>]) -> Vec<EarnedValue> {
        let n = self.tasks.len();
        let method = self.settings.eac_method;
        let status = self.status_or_now();
        let past = at < status;
        let factor: Vec<f64> = (0..n).map(|i| if past { self.progress_share_at(i, at) } else { 1.0 }).collect();
        let mut ev: Vec<EarnedValue> = self.tasks.iter().map(|t| EarnedValue { cost: t.calc.cost, eac_method: method, ..Default::default() }).collect();
        // actual cost: each assignment by its resource's accrual
        let idx: std::collections::HashMap<u32, usize> = self.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        let ridx: std::collections::HashMap<u32, usize> = self.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
        for (ai, a) in self.assignments.iter().enumerate() {
            let (Some(&ti), Some(&ri)) = (idx.get(&a.task), ridx.get(&a.resource)) else { continue };
            if self.tasks[ti].calc.is_summary {
                continue;
            }
            let pct = self.tasks[ti].percent_complete * factor[ti];
            ev[ti].acwp += accrued(acost.get(ai).copied().unwrap_or(0.0), self.resources[ri].accrue, pct);
        }
        for i in 0..n {
            let t = &self.tasks[i];
            if t.calc.is_summary {
                continue;
            }
            ev[i].acwp += t.fixed_cost * t.percent_complete * factor[i] / 100.0;
            if let Some(b) = self.ev_baseline_of(i) {
                let cal = self.scheduled_duration_calendar(i);
                let pv = planned(&b, t.ev_method, &cal, at);
                ev[i].bac = b.cost;
                ev[i].bcws = pv;
                ev[i].bcwp = earned(t.ev_method, b.cost, t.percent_complete * factor[i], t.physical_percent * factor[i], pv);
            }
        }
        // summaries' own fixed cost: spent and earned with their rolled-up
        // progress, planned over their baseline (and part of their budget)
        for i in (0..n).rev() {
            let t = &self.tasks[i];
            if t.calc.is_summary && t.fixed_cost != 0.0 {
                let f = if past {
                    match (t.calc.start, t.calc.finish) {
                        (Some(s), Some(e)) => share(&self.calendar, s, e.min(status).max(s), at),
                        _ => 1.0,
                    }
                } else {
                    1.0
                };
                let pct = t.calc.percent_complete * f;
                ev[i].acwp += t.fixed_cost * pct / 100.0;
                if let Some(b) = self.ev_baseline_of(i) {
                    let pv = t.fixed_cost * share(&self.calendar, b.start, b.finish, at);
                    ev[i].bac += t.fixed_cost;
                    ev[i].bcws += pv;
                    ev[i].bcwp += earned(EvMethod::PercentComplete, t.fixed_cost, pct, 0.0, pv);
                }
            }
            if let Some(par) = parents[i] {
                let c = ev[i];
                let p = &mut ev[par];
                p.bcws += c.bcws;
                p.bcwp += c.bcwp;
                p.acwp += c.acwp;
                p.bac += c.bac;
            }
        }
        ev
    }

    /// Earned value of every task at `at` (needs a computed schedule).
    pub fn earned_value_at(&self, at: NaiveDateTime) -> Vec<EarnedValue> {
        let acost: Vec<f64> = self.calc.assignments.iter().map(|a| a.cost).collect();
        let parents: Vec<Option<usize>> = (0..self.tasks.len()).map(|i| self.parent_of(i)).collect();
        self.earned_value_with(at, &acost, &parents)
    }

    /// The whole project's earned value from per-task figures (top-level
    /// tasks summed).
    pub fn total_earned_value(&self, per_task: &[EarnedValue]) -> EarnedValue {
        let mut ev = EarnedValue { eac_method: self.settings.eac_method, ..Default::default() };
        for (i, e) in per_task.iter().enumerate() {
            if self.parent_of(i).is_none() {
                ev.add(e);
            }
        }
        ev
    }

    /// Earned value over time, for the S-curve: one point per period from
    /// the earliest baseline (or scheduled) start to the latest finish.
    pub fn earned_value_series(&self, period: EvPeriod) -> EvSeries {
        let status = self.status_or_now();
        let mut lo: Option<NaiveDateTime> = None;
        let mut hi: Option<NaiveDateTime> = None;
        let mut widen = |s: NaiveDateTime, f: NaiveDateTime| {
            lo = Some(lo.map_or(s, |l| l.min(s)));
            hi = Some(hi.map_or(f, |h| h.max(f)));
        };
        let mut baselined = false;
        for (i, t) in self.tasks.iter().enumerate() {
            if let Some(b) = self.ev_baseline_of(i) {
                widen(b.start, b.finish);
                baselined = true;
            }
            if let (Some(s), Some(f)) = (t.calc.start, t.calc.finish) {
                widen(s, f);
            }
        }
        let (Some(lo), Some(hi)) = (lo, hi) else { return EvSeries::default() };
        let hi = hi.max(status.min(hi + Duration::days(366)));
        let mut dates = vec![];
        let mut d = period.start_of(lo.date());
        while d.and_hms_opt(0, 0, 0).unwrap() <= hi && dates.len() < 400 {
            d = period.next(d);
            dates.push(d.and_hms_opt(0, 0, 0).unwrap());
        }
        let acost: Vec<f64> = self.calc.assignments.iter().map(|a| a.cost).collect();
        let parents: Vec<Option<usize>> = (0..self.tasks.len()).map(|i| self.parent_of(i)).collect();
        let mut points: Vec<(NaiveDateTime, EarnedValue)> = vec![];
        let start = period.start_of(lo.date()).and_hms_opt(0, 0, 0).unwrap();
        points.push((start, EarnedValue { eac_method: self.settings.eac_method, ..Default::default() }));
        for &at in &dates {
            let per = self.earned_value_with(at, &acost, &parents);
            points.push((at, self.total_earned_value(&per)));
        }
        let now = self.total_earned_value(&self.earned_value_with(status, &acost, &parents));
        EvSeries { points, status, now, baselined, finish: self.calc.finish }
    }
}

/// Earned value over time (project totals). Values after the status date
/// are planned only: EV and AC stop at it.
#[derive(Clone, Debug, Default)]
pub struct EvSeries {
    pub points: Vec<(NaiveDateTime, EarnedValue)>,
    pub status: NaiveDateTime,
    /// The figures at the status date.
    pub now: EarnedValue,
    pub baselined: bool,
    /// The scheduled finish (where the estimate at completion lands).
    pub finish: Option<NaiveDateTime>,
}

/// The reporting period: the S-curve's step and the "current period" of
/// the period report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EvPeriod {
    Week,
    #[default]
    Month,
}

impl EvPeriod {
    pub fn label(self) -> &'static str {
        match self {
            EvPeriod::Week => "Week",
            EvPeriod::Month => "Month",
        }
    }
    /// The first day of the period holding `d` (weeks start on Monday).
    pub fn start_of(self, d: NaiveDate) -> NaiveDate {
        match self {
            EvPeriod::Week => d - Duration::days(d.weekday().num_days_from_monday() as i64),
            EvPeriod::Month => d.with_day(1).unwrap_or(d),
        }
    }
    pub fn next(self, d: NaiveDate) -> NaiveDate {
        match self {
            EvPeriod::Week => d + Duration::days(7),
            EvPeriod::Month => {
                let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
                NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(d + Duration::days(31))
            }
        }
    }
    /// The same moment one period earlier (the start of the current period
    /// ending at `at`).
    pub fn back(self, at: NaiveDateTime) -> NaiveDateTime {
        match self {
            EvPeriod::Week => at - Duration::days(7),
            EvPeriod::Month => at.checked_sub_months(chrono::Months::new(1)).unwrap_or(at - Duration::days(30)),
        }
    }
}
