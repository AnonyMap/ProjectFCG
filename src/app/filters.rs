//! Filters and groups (View ▸ Data): Project's built-in filters, the
//! interactive Date Range and Using Resource filters, custom filters saved
//! in the file, and Group By.

use super::columns::{cell_text, ColumnId};
use super::commands::Filter;
use crate::model::{ConstraintType, Project};
use crate::views::{CustomFilter, Test};
use chrono::{Datelike, NaiveDateTime};
use std::collections::HashSet;

impl ColumnId {
    /// Name used to save a field in a custom filter.
    pub fn key(&self) -> String {
        format!("{self:?}")
    }
    pub fn from_key(k: &str) -> Option<ColumnId> {
        ColumnId::ALL.iter().copied().find(|c| c.key() == k)
    }
    /// Fields a filter or group can test (everything with a value).
    pub fn filterable() -> Vec<ColumnId> {
        ColumnId::ALL.iter().copied().filter(|c| !matches!(c, ColumnId::Indicators | ColumnId::Mode)).collect()
    }
}

/// A field's value, typed so that tests compare numbers as numbers and
/// dates as dates.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Num(f64),
    /// Durations and work, in minutes.
    Minutes(i64),
    Date(Option<NaiveDateTime>),
    Bool(bool),
}

pub fn field_value(p: &Project, row: usize, col: ColumnId) -> Value {
    use ColumnId as C;
    let t = &p.tasks[row];
    let c = &t.calc;
    let bl = t.baseline;
    let cal = &p.calendar;
    let ev = || p.earned_value(row);
    let idx = |v: Option<f64>| Value::Num(v.unwrap_or(f64::NAN));
    match col {
        C::Start => Value::Date(c.start),
        C::Finish => Value::Date(c.finish),
        C::EarlyStart => Value::Date(c.early_start),
        C::EarlyFinish => Value::Date(c.early_finish),
        C::LateStart => Value::Date(c.late_start),
        C::LateFinish => Value::Date(c.late_finish),
        C::ConstraintDate => Value::Date(t.constraint_date),
        C::Deadline => Value::Date(t.deadline),
        C::BaselineStart => Value::Date(bl.map(|b| b.start)),
        C::BaselineFinish => Value::Date(bl.map(|b| b.finish)),
        C::BaselineNStart(n) => Value::Date(t.baseline_slot(n as usize).map(|b| b.start)),
        C::BaselineNFinish(n) => Value::Date(t.baseline_slot(n as usize).map(|b| b.finish)),
        C::ActualStart if !c.is_summary => Value::Date(t.actual_start),
        C::ActualFinish if !c.is_summary => Value::Date(t.actual_finish),
        // a summary has started when any subtask has, finished when all have
        C::ActualStart => Value::Date(((row + 1)..p.subtree_end(row)).filter_map(|k| p.tasks[k].actual_start).min()),
        C::ActualFinish => {
            let kids: Vec<usize> = ((row + 1)..p.subtree_end(row)).filter(|&k| !p.tasks[k].calc.is_summary).collect();
            let all = !kids.is_empty() && kids.iter().all(|&k| p.tasks[k].actual_finish.is_some());
            Value::Date(if all { kids.iter().filter_map(|&k| p.tasks[k].actual_finish).max() } else { None })
        }
        C::Duration => Value::Minutes(if c.is_summary { c.summary_duration } else { t.duration.minutes }),
        C::TotalSlack => Value::Minutes(c.total_slack),
        C::FreeSlack => Value::Minutes(c.free_slack),
        C::StartSlack => Value::Minutes(c.start_slack),
        C::FinishSlack => Value::Minutes(c.finish_slack),
        C::BaselineDuration => Value::Minutes(bl.map_or(0, |b| b.duration)),
        C::StartVariance => Value::Minutes(match (bl, c.start) {
            (Some(b), Some(s)) => cal.work_between(b.start, s),
            _ => 0,
        }),
        C::FinishVariance => Value::Minutes(match (bl, c.finish) {
            (Some(b), Some(f)) => cal.work_between(b.finish, f),
            _ => 0,
        }),
        C::DurationVariance => Value::Minutes(bl.map_or(0, |b| (if c.is_summary { c.summary_duration } else { t.duration.minutes }) - b.duration)),
        C::LevelingDelay => Value::Minutes(t.leveling_delay),
        C::ActualDuration => Value::Minutes(p.actual_duration(row)),
        C::RemainingDuration => Value::Minutes(p.remaining_duration(row)),
        C::Work => Value::Minutes(c.work),
        C::ActualWork => Value::Minutes(c.actual_work),
        C::RemainingWork => Value::Minutes((c.work - c.actual_work).max(0)),
        C::BaselineWork => Value::Minutes(bl.map_or(0, |b| b.work)),
        C::PercentComplete => Value::Num(c.percent_complete),
        C::PercentWorkComplete => Value::Num(if c.work > 0 { c.actual_work as f64 / c.work as f64 * 100.0 } else { 0.0 }),
        C::Priority => Value::Num(t.priority as f64),
        C::UniqueId => Value::Num(t.uid as f64),
        C::Cost => Value::Num(c.cost),
        C::FixedCost => Value::Num(t.fixed_cost),
        C::ActualCost => Value::Num(c.actual_cost),
        C::RemainingCost => Value::Num((c.cost - c.actual_cost).max(0.0)),
        C::BaselineCost => Value::Num(bl.map_or(0.0, |b| b.cost)),
        C::Bcws => Value::Num(c.bcws),
        C::Bcwp => Value::Num(c.bcwp),
        C::Acwp => Value::Num(c.acwp),
        C::Bac => Value::Num(c.bac),
        C::Sv => Value::Num(ev().sv()),
        C::Cv => Value::Num(ev().cv()),
        C::Eac => Value::Num(ev().eac()),
        C::Vac => Value::Num(ev().vac()),
        C::Spi => idx(ev().spi()),
        C::Cpi => idx(ev().cpi()),
        C::Tcpi => idx(ev().tcpi()),
        C::Critical => Value::Bool(c.critical),
        C::Milestone => Value::Bool(t.is_milestone()),
        C::EffortDriven => Value::Bool(t.effort_driven),
        C::Overallocated => Value::Bool(c.overallocated),
        // the text a summary's actual dates show is derived; compare it as text
        _ => Value::Text(cell_text(p, row, col)),
    }
}

fn parse_num(s: &str) -> Option<f64> {
    let clean: String = s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
    clean.parse().ok()
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" | "y" => Some(true),
        "no" | "false" | "0" | "n" => Some(false),
        _ => None,
    }
}

/// Parse a filter's typed-in value for a field; `None` if it doesn't parse.
fn parse_like(p: &Project, like: &Value, s: &str) -> Option<Value> {
    let s = s.trim();
    Some(match like {
        Value::Text(_) => Value::Text(s.to_lowercase()),
        Value::Num(_) => Value::Num(parse_num(s)?),
        Value::Minutes(_) => Value::Minutes(p.parse_duration(s).ok()?.minutes),
        Value::Date(_) => {
            if s.eq_ignore_ascii_case("na") || s.is_empty() {
                Value::Date(None)
            } else {
                Value::Date(Some(crate::model::parse_date(s, p.default_start_time()).ok()?))
            }
        }
        Value::Bool(_) => Value::Bool(parse_bool(s)?),
    })
}

/// Compare two values of the same kind; dates compare by day, as
/// Project's filters do. `None` when they can't be ordered.
fn order(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => Some(x.to_lowercase().cmp(y)),
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y),
        (Value::Minutes(x), Value::Minutes(y)) => Some(x.cmp(y)),
        (Value::Date(Some(x)), Value::Date(Some(y))) => Some(x.date().cmp(&y.date())),
        (Value::Date(None), Value::Date(None)) => Some(std::cmp::Ordering::Equal),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Check that a condition's values make sense for its field.
pub fn validate(p: &Project, c: &crate::views::Condition) -> Result<(), String> {
    let col = ColumnId::from_key(&c.field).ok_or("Pick a field for every condition.")?;
    if matches!(c.test, Test::Contains | Test::NotContains) {
        return Ok(());
    }
    // the kind of value the field holds, from a scratch task
    let mut probe = Project::new(p.start.date());
    probe.insert_task(0, "probe");
    let like = field_value(&probe, 0, col);
    let kind = match like {
        Value::Text(_) => "text",
        Value::Num(_) => "number",
        Value::Minutes(_) => "duration",
        Value::Date(_) => "date",
        Value::Bool(_) => "Yes or No",
    };
    let check = |v: &str| parse_like(p, &like, v).map(|_| ()).ok_or_else(|| format!("{}: \"{}\" isn't a {kind}.", col.title(), v.trim()));
    check(&c.value)?;
    if c.test.two_values() {
        check(&c.value2)?;
    }
    Ok(())
}

/// Does task `row` pass one condition?
pub fn condition_matches(p: &Project, row: usize, c: &crate::views::Condition) -> bool {
    let Some(col) = ColumnId::from_key(&c.field) else { return false };
    if matches!(c.test, Test::Contains | Test::NotContains) {
        let has = cell_text(p, row, col).to_lowercase().contains(&c.value.trim().to_lowercase());
        return has == (c.test == Test::Contains);
    }
    let v = field_value(p, row, col);
    let Some(want) = parse_like(p, &v, &c.value) else { return false };
    use std::cmp::Ordering::*;
    let o = order(&v, &want);
    match c.test {
        Test::Equals => o == Some(Equal),
        Test::NotEquals => o != Some(Equal),
        Test::Greater => o == Some(Greater),
        Test::GreaterOrEqual => matches!(o, Some(Greater | Equal)),
        Test::Less => o == Some(Less),
        Test::LessOrEqual => matches!(o, Some(Less | Equal)),
        Test::Within | Test::NotWithin => {
            let Some(hi) = parse_like(p, &v, &c.value2) else { return false };
            let inside = matches!(o, Some(Greater | Equal)) && matches!(order(&v, &hi), Some(Less | Equal));
            inside == (c.test == Test::Within)
        }
        Test::Contains | Test::NotContains => unreachable!(),
    }
}

pub fn custom_matches(p: &Project, row: usize, f: &CustomFilter) -> bool {
    crate::views::combine(&f.conditions, |c| condition_matches(p, row, c))
}

/// A filter ready to test many rows (precomputes what it can).
pub struct FilterEval<'a> {
    p: &'a Project,
    f: Filter,
    using: Option<HashSet<u32>>,
}

impl<'a> FilterEval<'a> {
    pub fn new(p: &'a Project, f: Filter) -> Self {
        let using = match f {
            Filter::UsingResource(r) => Some(p.assignments.iter().filter(|a| a.resource == r).map(|a| a.task).collect()),
            _ => None,
        };
        FilterEval { p, f, using }
    }

    pub fn matches(&self, i: usize) -> bool {
        let p = self.p;
        let t = &p.tasks[i];
        match self.f {
            Filter::AllTasks => true,
            Filter::Critical => t.calc.critical,
            Filter::Milestones => t.is_milestone(),
            Filter::Summary => t.calc.is_summary,
            Filter::Incomplete => t.calc.percent_complete < 100.0,
            Filter::Completed => t.calc.percent_complete >= 100.0,
            Filter::Late => t.calc.late,
            Filter::Slipping => match (t.baseline, t.calc.finish) {
                (Some(b), Some(f)) => p.calendar.work_between(b.finish, f) > 0,
                _ => false,
            },
            Filter::DateRange(a, b) => match (t.calc.start, t.calc.finish) {
                // tasks that are under way at any time in the range
                (Some(s), Some(f)) => s.date() <= b && f.date() >= a,
                _ => false,
            },
            Filter::UsingResource(_) => self.using.as_ref().is_some_and(|u| u.contains(&t.uid)),
            Filter::Custom(k) => p.views.filters.get(k).is_none_or(|cf| custom_matches(p, i, cf)),
        }
    }

    /// Show summary rows above matching tasks?
    pub fn related(&self, toggle: bool) -> bool {
        match self.f {
            Filter::AllTasks => false,
            Filter::Custom(k) => toggle || self.p.views.filters.get(k).is_some_and(|f| f.related_summaries),
            _ => toggle,
        }
    }
}

// ---------------------------------------------------------------- groups

/// View ▸ Group by. Project's built-in task groups, or any field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GroupBy {
    #[default]
    None,
    CompleteIncomplete,
    ConstraintType,
    Critical,
    Duration,
    Milestones,
    Priority,
    Resource,
    Status,
    Active,
    Field(ColumnId),
}

impl GroupBy {
    pub const BUILT_IN: [GroupBy; 10] = [
        GroupBy::None,
        GroupBy::Active,
        GroupBy::CompleteIncomplete,
        GroupBy::ConstraintType,
        GroupBy::Critical,
        GroupBy::Duration,
        GroupBy::Milestones,
        GroupBy::Priority,
        GroupBy::Resource,
        GroupBy::Status,
    ];
    pub fn label(&self) -> String {
        match self {
            GroupBy::None => "No Group".into(),
            GroupBy::CompleteIncomplete => "Complete and Incomplete Tasks".into(),
            GroupBy::ConstraintType => "Constraint Type".into(),
            GroupBy::Critical => "Critical".into(),
            GroupBy::Duration => "Duration".into(),
            GroupBy::Milestones => "Milestones".into(),
            GroupBy::Priority => "Priority".into(),
            GroupBy::Resource => "Resource".into(),
            GroupBy::Status => "Status".into(),
            GroupBy::Active => "Active v. Inactive".into(),
            GroupBy::Field(c) => c.title(),
        }
    }
}

/// One group header row and its rollup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupRow {
    pub label: String,
    pub collapsed: bool,
    pub count: usize,
    pub start: Option<NaiveDateTime>,
    pub finish: Option<NaiveDateTime>,
    pub work: i64,
    pub cost: f64,
    /// Duration-weighted % complete, as summary tasks roll up.
    pub percent_complete: f64,
}

/// Sort key and label of task `i`'s group. Lower keys come first.
pub fn group_key(p: &Project, i: usize, g: GroupBy, inactive: &[bool]) -> ((i64, String), String) {
    let t = &p.tasks[i];
    let c = &t.calc;
    let yes_no = |name: &str, b: bool| ((b as i64, String::new()), format!("{name}: {}", if b { "Yes" } else { "No" }));
    match g {
        GroupBy::None => ((0, String::new()), String::new()),
        GroupBy::CompleteIncomplete => {
            let pc = c.percent_complete;
            if pc <= 0.0 {
                ((0, String::new()), "0% Complete".into())
            } else if pc < 100.0 {
                ((1, String::new()), "1-99% Complete".into())
            } else {
                ((2, String::new()), "100% Complete".into())
            }
        }
        GroupBy::ConstraintType => {
            let k = ConstraintType::ALL.iter().position(|x| *x == t.constraint).unwrap_or(0) as i64;
            ((k, String::new()), format!("Constraint Type: {}", t.constraint.label()))
        }
        GroupBy::Critical => yes_no("Critical", c.critical),
        GroupBy::Milestones => yes_no("Milestone", t.is_milestone()),
        GroupBy::Duration => {
            let m = if c.is_summary { c.summary_duration } else { t.duration.minutes };
            ((m, String::new()), format!("Duration: {}", cell_text(p, i, ColumnId::Duration)))
        }
        GroupBy::Priority => ((t.priority as i64, String::new()), format!("Priority: {}", t.priority)),
        GroupBy::Resource => {
            let names = p.resource_names_text(i);
            if names.trim().is_empty() {
                ((0, String::new()), "No Resource".into())
            } else {
                ((1, names.to_lowercase()), format!("Resource: {names}"))
            }
        }
        GroupBy::Status => {
            use crate::model::TaskStatus as S;
            let k = match c.status {
                S::Complete => 0,
                S::OnSchedule => 1,
                S::Late => 2,
                S::FutureTask => 3,
            };
            ((k, String::new()), format!("Status: {}", c.status.label()))
        }
        GroupBy::Active => {
            let off = inactive.get(i).copied().unwrap_or(false);
            ((off as i64, String::new()), if off { "Inactive".into() } else { "Active".into() })
        }
        GroupBy::Field(col) => {
            let title = col.title();
            match field_value(p, i, col) {
                Value::Num(v) if v.is_finite() => ((((v * 1000.0).round()) as i64, String::new()), format!("{title}: {}", cell_text(p, i, col))),
                Value::Num(_) => ((i64::MIN, String::new()), format!("{title}: NA")),
                Value::Minutes(m) => ((m, String::new()), format!("{title}: {}", cell_text(p, i, col))),
                Value::Bool(b) => yes_no(&title, b),
                // dates group by week, as Project's default date interval
                Value::Date(Some(d)) => {
                    let monday = d.date() - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
                    let shown = p.format.layout.date_format.format(monday.and_time(chrono::NaiveTime::MIN));
                    ((monday.num_days_from_ce() as i64, String::new()), format!("{title}: Week of {shown}"))
                }
                Value::Date(None) => ((i64::MIN, String::new()), format!("{title}: NA")),
                Value::Text(s) => {
                    let s = s.trim().to_string();
                    if s.is_empty() {
                        ((0, String::new()), format!("{title}: No value"))
                    } else {
                        ((1, s.to_lowercase()), format!("{title}: {s}"))
                    }
                }
            }
        }
    }
}

/// Roll a group's tasks up the way a summary task would.
pub fn rollup(p: &Project, label: String, rows: &[usize], collapsed: bool) -> GroupRow {
    let mut g = GroupRow { label, collapsed, count: rows.len(), ..Default::default() };
    let (mut wsum, mut dsum) = (0.0, 0.0);
    for &i in rows {
        let c = &p.tasks[i].calc;
        g.start = match (g.start, c.start) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        g.finish = match (g.finish, c.finish) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        g.work += c.work;
        g.cost += c.cost;
        let d = p.tasks[i].duration.minutes.max(0) as f64;
        wsum += c.percent_complete * d;
        dsum += d;
    }
    g.percent_complete = if dsum > 0.0 {
        wsum / dsum
    } else if rows.is_empty() {
        0.0
    } else {
        rows.iter().map(|&i| p.tasks[i].calc.percent_complete).sum::<f64>() / rows.len() as f64
    };
    g
}
