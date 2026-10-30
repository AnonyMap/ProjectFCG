//! Table columns ("fields" in Project terms) and their text conversion.

use crate::duration::{fmt_num, DurUnit};
use crate::model::{fmt_datetime, parse_date, ConstraintType, Project, TaskMode};
use crate::ops::Field;
use egui::Align;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Indicators,
    Mode,
    Name,
    Duration,
    Start,
    Finish,
    Predecessors,
    Successors,
    PercentComplete,
    TotalSlack,
    FreeSlack,
    EarlyStart,
    EarlyFinish,
    LateStart,
    LateFinish,
    ConstraintType,
    ConstraintDate,
    Deadline,
    Wbs,
    UniqueId,
    Critical,
    Milestone,
    Notes,
    StartSlack,
    FinishSlack,
    BaselineStart,
    BaselineFinish,
    BaselineDuration,
    StartVariance,
    FinishVariance,
    DurationVariance,
    Status,
    ResourceNames,
    ResourceInitials,
    Work,
    Cost,
    FixedCost,
    TaskType,
    EffortDriven,
    Priority,
    LevelingDelay,
    Overallocated,
    // tracking
    ActualStart,
    ActualFinish,
    ActualDuration,
    RemainingDuration,
    ActualWork,
    RemainingWork,
    PercentWorkComplete,
    ActualCost,
    RemainingCost,
    BaselineWork,
    BaselineCost,
    // earned value
    Bcws,
    Bcwp,
    Acwp,
    Sv,
    Cv,
    Spi,
    Cpi,
    Bac,
    Eac,
    Vac,
    Tcpi,
    /// Baseline 1–10 start / finish.
    BaselineNStart(u8),
    BaselineNFinish(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    ReadOnly,
    Text,
    Date,
    Choice,
}

impl ColumnId {
    pub const ALL: &'static [ColumnId] = &[
        ColumnId::Indicators,
        ColumnId::Mode,
        ColumnId::Name,
        ColumnId::Duration,
        ColumnId::Start,
        ColumnId::Finish,
        ColumnId::Predecessors,
        ColumnId::Successors,
        ColumnId::PercentComplete,
        ColumnId::TotalSlack,
        ColumnId::FreeSlack,
        ColumnId::EarlyStart,
        ColumnId::EarlyFinish,
        ColumnId::LateStart,
        ColumnId::LateFinish,
        ColumnId::ConstraintType,
        ColumnId::ConstraintDate,
        ColumnId::Deadline,
        ColumnId::Wbs,
        ColumnId::UniqueId,
        ColumnId::Critical,
        ColumnId::Milestone,
        ColumnId::Notes,
        ColumnId::StartSlack,
        ColumnId::FinishSlack,
        ColumnId::BaselineStart,
        ColumnId::BaselineFinish,
        ColumnId::BaselineDuration,
        ColumnId::StartVariance,
        ColumnId::FinishVariance,
        ColumnId::DurationVariance,
        ColumnId::Status,
        ColumnId::ResourceNames,
        ColumnId::ResourceInitials,
        ColumnId::Work,
        ColumnId::Cost,
        ColumnId::FixedCost,
        ColumnId::TaskType,
        ColumnId::EffortDriven,
        ColumnId::Priority,
        ColumnId::LevelingDelay,
        ColumnId::Overallocated,
        ColumnId::ActualStart,
        ColumnId::ActualFinish,
        ColumnId::ActualDuration,
        ColumnId::RemainingDuration,
        ColumnId::ActualWork,
        ColumnId::RemainingWork,
        ColumnId::PercentWorkComplete,
        ColumnId::ActualCost,
        ColumnId::RemainingCost,
        ColumnId::BaselineWork,
        ColumnId::BaselineCost,
        ColumnId::Bcws,
        ColumnId::Bcwp,
        ColumnId::Acwp,
        ColumnId::Sv,
        ColumnId::Cv,
        ColumnId::Spi,
        ColumnId::Cpi,
        ColumnId::Bac,
        ColumnId::Eac,
        ColumnId::Vac,
        ColumnId::Tcpi,
        ColumnId::BaselineNStart(1),
        ColumnId::BaselineNFinish(1),
        ColumnId::BaselineNStart(2),
        ColumnId::BaselineNFinish(2),
        ColumnId::BaselineNStart(3),
        ColumnId::BaselineNFinish(3),
        ColumnId::BaselineNStart(4),
        ColumnId::BaselineNFinish(4),
        ColumnId::BaselineNStart(5),
        ColumnId::BaselineNFinish(5),
        ColumnId::BaselineNStart(6),
        ColumnId::BaselineNFinish(6),
        ColumnId::BaselineNStart(7),
        ColumnId::BaselineNFinish(7),
        ColumnId::BaselineNStart(8),
        ColumnId::BaselineNFinish(8),
        ColumnId::BaselineNStart(9),
        ColumnId::BaselineNFinish(9),
        ColumnId::BaselineNStart(10),
        ColumnId::BaselineNFinish(10),
    ];

    /// Group for the Insert Column menus.
    pub fn category(&self) -> &'static str {
        use ColumnId::*;
        match self {
            Indicators | Mode | Name | Notes | Wbs | UniqueId | Milestone | Status => "General",
            Duration | Start | Finish | Predecessors | Successors | ConstraintType | ConstraintDate | Deadline | EarlyStart | EarlyFinish | LateStart | LateFinish => "Schedule",
            TotalSlack | FreeSlack | StartSlack | FinishSlack | Critical => "Slack & Critical",
            PercentComplete | ActualStart | ActualFinish | ActualDuration | RemainingDuration | ActualWork | RemainingWork | PercentWorkComplete => "Tracking",
            BaselineStart | BaselineFinish | BaselineDuration | BaselineWork | BaselineCost | StartVariance | FinishVariance | DurationVariance => "Baseline & Variance",
            BaselineNStart(_) | BaselineNFinish(_) => "Baselines 1–10",
            Cost | FixedCost | ActualCost | RemainingCost => "Cost",
            Bcws | Bcwp | Acwp | Sv | Cv | Spi | Cpi | Bac | Eac | Vac | Tcpi => "Earned Value",
            ResourceNames | ResourceInitials | Work | TaskType | EffortDriven | Overallocated => "Resources",
            Priority | LevelingDelay => "Leveling",
        }
    }

    pub const CATEGORIES: [&'static str; 10] = [
        "General",
        "Schedule",
        "Slack & Critical",
        "Tracking",
        "Baseline & Variance",
        "Baselines 1–10",
        "Cost",
        "Earned Value",
        "Resources",
        "Leveling",
    ];

    /// Project's default Gantt Chart "Entry" table (minus Resource Names).
    pub fn default_set() -> Vec<(ColumnId, f32)> {
        vec![
            (ColumnId::Indicators, 26.0),
            (ColumnId::Mode, 40.0),
            (ColumnId::Name, 220.0),
            (ColumnId::Duration, 74.0),
            (ColumnId::Start, 96.0),
            (ColumnId::Finish, 96.0),
            (ColumnId::Predecessors, 92.0),
            (ColumnId::ResourceNames, 140.0),
        ]
    }

    pub fn title(&self) -> String {
        if let ColumnId::BaselineNStart(n) = self {
            return format!("Baseline{n} Start");
        }
        if let ColumnId::BaselineNFinish(n) = self {
            return format!("Baseline{n} Finish");
        }
        self.static_title().to_string()
    }

    fn static_title(&self) -> &'static str {
        match self {
            ColumnId::BaselineNStart(_) | ColumnId::BaselineNFinish(_) => "",
            ColumnId::ActualStart => "Actual Start",
            ColumnId::ActualFinish => "Actual Finish",
            ColumnId::ActualDuration => "Actual Duration",
            ColumnId::RemainingDuration => "Remaining Duration",
            ColumnId::ActualWork => "Actual Work",
            ColumnId::RemainingWork => "Remaining Work",
            ColumnId::PercentWorkComplete => "% Work Complete",
            ColumnId::ActualCost => "Actual Cost",
            ColumnId::RemainingCost => "Remaining Cost",
            ColumnId::BaselineWork => "Baseline Work",
            ColumnId::BaselineCost => "Baseline Cost",
            ColumnId::Bcws => "BCWS",
            ColumnId::Bcwp => "BCWP",
            ColumnId::Acwp => "ACWP",
            ColumnId::Sv => "SV",
            ColumnId::Cv => "CV",
            ColumnId::Spi => "SPI",
            ColumnId::Cpi => "CPI",
            ColumnId::Bac => "BAC",
            ColumnId::Eac => "EAC",
            ColumnId::Vac => "VAC",
            ColumnId::Tcpi => "TCPI",
            ColumnId::Indicators => "ℹ",
            ColumnId::Mode => "Task Mode",
            ColumnId::Name => "Task Name",
            ColumnId::Duration => "Duration",
            ColumnId::Start => "Start",
            ColumnId::Finish => "Finish",
            ColumnId::Predecessors => "Predecessors",
            ColumnId::Successors => "Successors",
            ColumnId::PercentComplete => "% Complete",
            ColumnId::TotalSlack => "Total Slack",
            ColumnId::FreeSlack => "Free Slack",
            ColumnId::EarlyStart => "Early Start",
            ColumnId::EarlyFinish => "Early Finish",
            ColumnId::LateStart => "Late Start",
            ColumnId::LateFinish => "Late Finish",
            ColumnId::ConstraintType => "Constraint Type",
            ColumnId::ConstraintDate => "Constraint Date",
            ColumnId::Deadline => "Deadline",
            ColumnId::Wbs => "WBS",
            ColumnId::UniqueId => "Unique ID",
            ColumnId::Critical => "Critical",
            ColumnId::Milestone => "Milestone",
            ColumnId::Notes => "Notes",
            ColumnId::StartSlack => "Start Slack",
            ColumnId::FinishSlack => "Finish Slack",
            ColumnId::BaselineStart => "Baseline Start",
            ColumnId::BaselineFinish => "Baseline Finish",
            ColumnId::BaselineDuration => "Baseline Duration",
            ColumnId::StartVariance => "Start Variance",
            ColumnId::FinishVariance => "Finish Variance",
            ColumnId::DurationVariance => "Duration Variance",
            ColumnId::Status => "Status",
            ColumnId::ResourceNames => "Resource Names",
            ColumnId::ResourceInitials => "Resource Initials",
            ColumnId::Work => "Work",
            ColumnId::Cost => "Cost",
            ColumnId::FixedCost => "Fixed Cost",
            ColumnId::TaskType => "Type",
            ColumnId::EffortDriven => "Effort Driven",
            ColumnId::Priority => "Priority",
            ColumnId::LevelingDelay => "Leveling Delay",
            ColumnId::Overallocated => "Overallocated",
        }
    }

    pub fn default_width(&self) -> f32 {
        match self {
            ColumnId::Indicators => 26.0,
            ColumnId::Mode => 40.0,
            ColumnId::Name => 220.0,
            ColumnId::Notes => 160.0,
            ColumnId::ResourceNames => 140.0,
            ColumnId::ConstraintType => 150.0,
            ColumnId::Wbs | ColumnId::UniqueId | ColumnId::Critical | ColumnId::Milestone => 64.0,
            _ => 92.0,
        }
    }

    pub fn align(&self) -> Align {
        match self {
            ColumnId::Duration
            | ColumnId::PercentComplete
            | ColumnId::TotalSlack
            | ColumnId::FreeSlack
            | ColumnId::StartSlack
            | ColumnId::FinishSlack
            | ColumnId::BaselineDuration
            | ColumnId::StartVariance
            | ColumnId::FinishVariance
            | ColumnId::DurationVariance
            | ColumnId::Work
            | ColumnId::Cost
            | ColumnId::FixedCost
            | ColumnId::Priority
            | ColumnId::LevelingDelay
            | ColumnId::ActualDuration
            | ColumnId::RemainingDuration
            | ColumnId::ActualWork
            | ColumnId::RemainingWork
            | ColumnId::PercentWorkComplete
            | ColumnId::ActualCost
            | ColumnId::RemainingCost
            | ColumnId::BaselineWork
            | ColumnId::BaselineCost
            | ColumnId::Bcws
            | ColumnId::Bcwp
            | ColumnId::Acwp
            | ColumnId::Sv
            | ColumnId::Cv
            | ColumnId::Spi
            | ColumnId::Cpi
            | ColumnId::Bac
            | ColumnId::Eac
            | ColumnId::Vac
            | ColumnId::Tcpi
            | ColumnId::UniqueId => Align::RIGHT,
            ColumnId::Indicators | ColumnId::Mode => Align::Center,
            _ => Align::LEFT,
        }
    }

    pub fn edit_kind(&self) -> EditKind {
        match self {
            ColumnId::Name
            | ColumnId::Duration
            | ColumnId::Predecessors
            | ColumnId::PercentComplete
            | ColumnId::ResourceNames
            | ColumnId::Work
            | ColumnId::FixedCost
            | ColumnId::Priority
            | ColumnId::LevelingDelay
            | ColumnId::ActualDuration
            | ColumnId::RemainingDuration
            | ColumnId::Notes => EditKind::Text,
            ColumnId::Start | ColumnId::Finish | ColumnId::ConstraintDate | ColumnId::Deadline | ColumnId::ActualStart | ColumnId::ActualFinish => {
                EditKind::Date
            }
            ColumnId::Mode
            | ColumnId::ConstraintType
            | ColumnId::Milestone
            | ColumnId::TaskType
            | ColumnId::EffortDriven => EditKind::Choice,
            _ => EditKind::ReadOnly,
        }
    }

    pub fn choices(&self) -> Vec<&'static str> {
        match self {
            ColumnId::Mode => vec!["Manually Scheduled", "Auto Scheduled"],
            ColumnId::ConstraintType => ConstraintType::ALL.iter().map(|c| c.label()).collect(),
            ColumnId::Milestone | ColumnId::EffortDriven => vec!["Yes", "No"],
            ColumnId::TaskType => crate::model::TaskType::ALL.iter().map(|t| t.label()).collect(),
            _ => vec![],
        }
    }
}

fn dur_days(p: &Project, minutes: i64) -> String {
    p.format_duration_minutes(minutes, DurUnit::Day)
}

/// Text shown in a cell.
pub fn cell_text(p: &Project, row: usize, col: ColumnId) -> String {
    let t = &p.tasks[row];
    let c = &t.calc;
    let df = p.format.layout.date_format;
    let opt_date = |d: Option<chrono::NaiveDateTime>| d.map(|d| df.format(d)).unwrap_or_else(|| "NA".into());
    let bl = t.baseline;
    let cal = &p.calendar;
    match col {
        ColumnId::Indicators => String::new(),
        ColumnId::Mode => String::new(),
        ColumnId::Name => t.name.clone(),
        ColumnId::Duration => {
            if c.is_summary {
                dur_days(p, c.summary_duration)
            } else {
                t.duration.format(&p.settings.durations)
            }
        }
        ColumnId::Start => opt_date(c.start),
        ColumnId::Finish => opt_date(c.finish),
        ColumnId::Predecessors => p.predecessors_text(row),
        ColumnId::Successors => {
            let mut v: Vec<String> = p
                .successors_of(row)
                .into_iter()
                .map(|(i, l)| {
                    if l.kind == crate::model::LinkType::FS && l.lag.is_zero() {
                        (i + 1).to_string()
                    } else {
                        format!("{}{}{}", i + 1, l.kind.code(), l.lag.format(&p.settings.durations))
                    }
                })
                .collect();
            v.sort_by_key(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<usize>().unwrap_or(0));
            v.join(",")
        }
        ColumnId::PercentComplete => format!("{}%", fmt_num(c.percent_complete.round())),
        ColumnId::TotalSlack => dur_days(p, c.total_slack),
        ColumnId::FreeSlack => dur_days(p, c.free_slack),
        ColumnId::EarlyStart => opt_date(c.early_start),
        ColumnId::EarlyFinish => opt_date(c.early_finish),
        ColumnId::LateStart => opt_date(c.late_start),
        ColumnId::LateFinish => opt_date(c.late_finish),
        ColumnId::ConstraintType => t.constraint.label().into(),
        ColumnId::ConstraintDate => opt_date(t.constraint_date),
        ColumnId::Deadline => opt_date(t.deadline),
        ColumnId::Wbs => c.outline_number.clone(),
        ColumnId::UniqueId => t.uid.to_string(),
        ColumnId::Critical => if c.critical { "Yes" } else { "No" }.into(),
        ColumnId::Milestone => if t.is_milestone() { "Yes" } else { "No" }.into(),
        ColumnId::Notes => t.notes.lines().next().unwrap_or("").to_string(),
        ColumnId::StartSlack => dur_days(p, c.start_slack),
        ColumnId::FinishSlack => dur_days(p, c.finish_slack),
        ColumnId::BaselineStart => opt_date(bl.map(|b| b.start)),
        ColumnId::BaselineFinish => opt_date(bl.map(|b| b.finish)),
        ColumnId::BaselineDuration => bl.map(|b| dur_days(p, b.duration)).unwrap_or_else(|| "0 days".into()),
        ColumnId::StartVariance => match (bl, c.start) {
            (Some(b), Some(s)) => dur_days(p, cal.work_between(b.start, s)),
            _ => "0 days".into(),
        },
        ColumnId::FinishVariance => match (bl, c.finish) {
            (Some(b), Some(f)) => dur_days(p, cal.work_between(b.finish, f)),
            _ => "0 days".into(),
        },
        ColumnId::DurationVariance => match bl {
            Some(b) => {
                let cur = if c.is_summary { c.summary_duration } else { t.duration.minutes };
                dur_days(p, cur - b.duration)
            }
            None => "0 days".into(),
        },
        ColumnId::Status => c.status.label().into(),
        ColumnId::ResourceNames => p.resource_names_text(row),
        ColumnId::ResourceInitials => p.resource_initials_text(row),
        ColumnId::Work => p.format_duration_minutes(c.work, DurUnit::Hour),
        ColumnId::Cost => crate::resources::fmt_money(c.cost, &p.settings.currency_symbol),
        ColumnId::FixedCost => crate::resources::fmt_money(t.fixed_cost, &p.settings.currency_symbol),
        ColumnId::TaskType => t.task_type.label().into(),
        ColumnId::EffortDriven => if t.effort_driven { "Yes" } else { "No" }.into(),
        ColumnId::Priority => t.priority.to_string(),
        ColumnId::LevelingDelay => dur_days(p, t.leveling_delay),
        ColumnId::Overallocated => if c.overallocated { "Yes" } else { "No" }.into(),
        ColumnId::ActualStart => {
            if c.is_summary {
                // a summary has started when any subtask has
                let kids = (row + 1)..p.subtree_end(row);
                opt_date(kids.filter_map(|k| p.tasks[k].actual_start).min())
            } else {
                opt_date(t.actual_start)
            }
        }
        ColumnId::ActualFinish => {
            if c.is_summary {
                let kids: Vec<usize> = ((row + 1)..p.subtree_end(row)).filter(|&k| !p.tasks[k].calc.is_summary).collect();
                let all = !kids.is_empty() && kids.iter().all(|&k| p.tasks[k].actual_finish.is_some());
                opt_date(if all { kids.iter().filter_map(|&k| p.tasks[k].actual_finish).max() } else { None })
            } else {
                opt_date(t.actual_finish)
            }
        }
        ColumnId::ActualDuration => dur_days(p, p.actual_duration(row)),
        ColumnId::RemainingDuration => dur_days(p, p.remaining_duration(row)),
        ColumnId::ActualWork => p.format_duration_minutes(c.actual_work, DurUnit::Hour),
        ColumnId::RemainingWork => p.format_duration_minutes((c.work - c.actual_work).max(0), DurUnit::Hour),
        ColumnId::PercentWorkComplete => {
            if c.work > 0 {
                format!("{}%", fmt_num((c.actual_work as f64 / c.work as f64 * 100.0).round()))
            } else {
                "0%".into()
            }
        }
        ColumnId::ActualCost => money(p, c.actual_cost),
        ColumnId::RemainingCost => money(p, (c.cost - c.actual_cost).max(0.0)),
        ColumnId::BaselineWork => p.format_duration_minutes(bl.map_or(0, |b| b.work), DurUnit::Hour),
        ColumnId::BaselineCost => money(p, bl.map_or(0.0, |b| b.cost)),
        ColumnId::Bcws => money(p, c.bcws),
        ColumnId::Bcwp => money(p, c.bcwp),
        ColumnId::Acwp => money(p, c.acwp),
        ColumnId::Sv => money(p, p.earned_value(row).sv()),
        ColumnId::Cv => money(p, p.earned_value(row).cv()),
        ColumnId::Spi => index(p.earned_value(row).spi()),
        ColumnId::Cpi => index(p.earned_value(row).cpi()),
        ColumnId::Bac => money(p, c.bac),
        ColumnId::Eac => money(p, p.earned_value(row).eac()),
        ColumnId::Vac => money(p, p.earned_value(row).vac()),
        ColumnId::Tcpi => index(p.earned_value(row).tcpi()),
        ColumnId::BaselineNStart(n) => opt_date(t.baseline_slot(n as usize).map(|b| b.start)),
        ColumnId::BaselineNFinish(n) => opt_date(t.baseline_slot(n as usize).map(|b| b.finish)),
    }
}

fn money(p: &Project, v: f64) -> String {
    crate::resources::fmt_money(v, &p.settings.currency_symbol)
}

/// SPI/CPI/TCPI: two decimals, blank until defined (Project shows 0.00).
fn index(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "0.00".into())
}

/// Text placed in the editor when editing begins (more precise than the
/// display text for dates, which gain their time of day).
pub fn edit_text(p: &Project, row: usize, col: ColumnId) -> String {
    let t = &p.tasks[row];
    match col {
        ColumnId::Start => t.calc.start.map(fmt_datetime).unwrap_or_default(),
        ColumnId::Finish => t.calc.finish.map(fmt_datetime).unwrap_or_default(),
        ColumnId::ConstraintDate => t.constraint_date.map(fmt_datetime).unwrap_or_default(),
        ColumnId::Deadline => t.deadline.map(fmt_datetime).unwrap_or_default(),
        ColumnId::ActualStart => t.actual_start.map(fmt_datetime).unwrap_or_default(),
        ColumnId::ActualFinish => t.actual_finish.map(fmt_datetime).unwrap_or_default(),
        ColumnId::PercentComplete => fmt_num(t.percent_complete),
        ColumnId::Notes => t.notes.clone(),
        ColumnId::FixedCost => crate::duration::fmt_num(t.fixed_cost),
        ColumnId::Mode => match t.mode {
            TaskMode::Auto => "Auto Scheduled".into(),
            TaskMode::Manual => "Manually Scheduled".into(),
        },
        _ => cell_text(p, row, col),
    }
}

/// Apply edited text to a cell. Errors are user-facing messages.
pub fn set_cell(p: &mut Project, row: usize, col: ColumnId, text: &str) -> Result<(), String> {
    let field = match col {
        ColumnId::Name => Some(Field::Name),
        ColumnId::Duration => Some(Field::Duration),
        ColumnId::Start => Some(Field::Start),
        ColumnId::Finish => Some(Field::Finish),
        ColumnId::Predecessors => Some(Field::Predecessors),
        ColumnId::PercentComplete => Some(Field::PercentComplete),
        ColumnId::ResourceNames => Some(Field::ResourceNames),
        ColumnId::Work => Some(Field::Work),
        _ => None,
    };
    if let Some(f) = field {
        return p.set_field_text(row, f, text);
    }
    let txt = text.trim();
    match col {
        ColumnId::Mode => {
            let m = if txt.to_ascii_lowercase().starts_with('m') {
                TaskMode::Manual
            } else {
                TaskMode::Auto
            };
            p.set_mode(row, m);
            Ok(())
        }
        ColumnId::ConstraintType => {
            let kind = ConstraintType::ALL
                .into_iter()
                .find(|c| c.label().eq_ignore_ascii_case(txt) || format!("{c:?}").eq_ignore_ascii_case(txt))
                .ok_or_else(|| format!("\"{txt}\" is not a constraint type"))?;
            let date = if kind.needs_date() {
                Some(
                    p.tasks[row]
                        .constraint_date
                        .or(p.tasks[row].calc.start)
                        .unwrap_or(p.start),
                )
            } else {
                None
            };
            p.set_constraint(row, kind, date)
        }
        ColumnId::ConstraintDate => {
            if txt.is_empty() || txt.eq_ignore_ascii_case("na") {
                return p.set_constraint(row, ConstraintType::ASAP, None);
            }
            let dt = parse_date(txt, p.default_start_time())?;
            let kind = if p.tasks[row].constraint.needs_date() {
                p.tasks[row].constraint
            } else {
                ConstraintType::SNET
            };
            p.set_constraint(row, kind, Some(dt))
        }
        ColumnId::Deadline => {
            if txt.is_empty() || txt.eq_ignore_ascii_case("na") {
                p.tasks[row].deadline = None;
            } else {
                p.tasks[row].deadline = Some(parse_date(txt, p.default_finish_time())?);
            }
            Ok(())
        }
        ColumnId::Milestone => {
            if p.is_summary(row) {
                return Err("Summary tasks can't be marked as milestones.".into());
            }
            p.tasks[row].milestone = matches!(txt.to_ascii_lowercase().as_str(), "yes" | "y" | "true" | "1");
            Ok(())
        }
        ColumnId::Notes => {
            p.tasks[row].notes = text.to_string();
            Ok(())
        }
        ColumnId::FixedCost => {
            p.tasks[row].fixed_cost = crate::resources::parse_money(txt, &p.settings.currency_symbol)?;
            Ok(())
        }
        ColumnId::TaskType => {
            p.tasks[row].task_type = crate::model::TaskType::ALL
                .into_iter()
                .find(|t| t.label().eq_ignore_ascii_case(txt) || format!("{t:?}").eq_ignore_ascii_case(txt))
                .ok_or_else(|| format!("\"{txt}\" is not a task type"))?;
            Ok(())
        }
        ColumnId::EffortDriven => {
            p.tasks[row].effort_driven = matches!(txt.to_ascii_lowercase().as_str(), "yes" | "y" | "true" | "1");
            Ok(())
        }
        ColumnId::Priority => {
            let v: u32 = txt.parse().map_err(|_| format!("\"{txt}\" is not a valid priority (0–1000)"))?;
            if v > 1000 {
                return Err("Priority must be between 0 and 1000.".into());
            }
            p.tasks[row].priority = v;
            Ok(())
        }
        ColumnId::LevelingDelay => {
            let d = p.parse_duration(txt)?;
            if p.is_summary(row) {
                return Err("Leveling delay applies to subtasks.".into());
            }
            p.tasks[row].leveling_delay = d.minutes;
            Ok(())
        }
        ColumnId::ActualStart | ColumnId::ActualFinish => {
            let v = if txt.is_empty() || txt.eq_ignore_ascii_case("na") {
                None
            } else if col == ColumnId::ActualStart {
                Some(parse_date(txt, p.default_start_time())?)
            } else {
                Some(parse_date(txt, p.default_finish_time())?)
            };
            if col == ColumnId::ActualStart {
                p.set_actual_start(row, v)
            } else {
                p.set_actual_finish(row, v)
            }
        }
        ColumnId::ActualDuration | ColumnId::RemainingDuration => {
            if p.is_summary(row) {
                return Err("Enter actuals on subtasks; summaries roll them up.".into());
            }
            let d = p.parse_duration(txt)?;
            if col == ColumnId::ActualDuration {
                p.set_actual_duration(row, d.minutes)
            } else {
                p.set_remaining_duration(row, d.minutes)
            }
        }
        _ => Err(format!("The {} field is calculated and can't be edited.", col.title())),
    }
}

/// Ctrl+Delete: reset a field to its default.
pub fn reset_cell(p: &mut Project, row: usize, col: ColumnId) -> Result<(), String> {
    match col {
        ColumnId::Start | ColumnId::Finish | ColumnId::ConstraintDate | ColumnId::ConstraintType => {
            if p.tasks[row].mode == TaskMode::Manual {
                p.tasks[row].manual_start = None;
            }
            p.set_constraint(row, ConstraintType::ASAP, None)
        }
        ColumnId::Predecessors => {
            p.tasks[row].predecessors.clear();
            Ok(())
        }
        ColumnId::Deadline => {
            p.tasks[row].deadline = None;
            Ok(())
        }
        ColumnId::Notes => {
            p.tasks[row].notes.clear();
            Ok(())
        }
        ColumnId::PercentComplete => p.set_field_text(row, Field::PercentComplete, "0"),
        ColumnId::Duration => p.set_field_text(row, Field::Duration, "1d?"),
        _ => Ok(()),
    }
}

/// Insert Column menu, grouped by category. Returns the column picked.
pub fn insert_column_menu(ui: &mut egui::Ui, present: &[ColumnId]) -> Option<ColumnId> {
    let mut picked = None;
    for cat in ColumnId::CATEGORIES {
        let cols: Vec<ColumnId> = ColumnId::ALL.iter().copied().filter(|c| c.category() == cat && !present.contains(c)).collect();
        if cols.is_empty() {
            continue;
        }
        ui.menu_button(cat, |ui| {
            for c in cols {
                if ui.button(c.title()).clicked() {
                    picked = Some(c);
                    ui.close();
                }
            }
        });
    }
    picked
}

/// Project's built-in tables (View ▸ Tables): which columns the sheet shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Table {
    #[default]
    Entry,
    Schedule,
    Tracking,
    Cost,
    Work,
    EarnedValue,
    Variance,
}

impl Table {
    pub const ALL: [Table; 7] = [Table::Entry, Table::Schedule, Table::Tracking, Table::Cost, Table::Work, Table::EarnedValue, Table::Variance];
    pub fn label(self) -> &'static str {
        match self {
            Table::Entry => "Entry",
            Table::Schedule => "Schedule",
            Table::Tracking => "Tracking",
            Table::Cost => "Cost",
            Table::Work => "Work",
            Table::EarnedValue => "Earned Value",
            Table::Variance => "Variance",
        }
    }
    pub fn columns(self) -> Vec<ColumnId> {
        use ColumnId::*;
        match self {
            Table::Entry => ColumnId::default_set().into_iter().map(|c| c.0).collect(),
            Table::Schedule => vec![Indicators, Name, Start, Finish, LateStart, LateFinish, FreeSlack, TotalSlack],
            Table::Tracking => vec![Indicators, Name, ActualStart, ActualFinish, PercentComplete, PercentWorkComplete, ActualDuration, RemainingDuration, ActualCost, ActualWork],
            Table::Cost => vec![Indicators, Name, FixedCost, Cost, BaselineCost, ActualCost, RemainingCost],
            Table::Work => vec![Indicators, Name, Work, BaselineWork, ActualWork, RemainingWork, PercentWorkComplete],
            Table::EarnedValue => vec![Indicators, Name, Bcws, Bcwp, Acwp, Sv, Cv, Spi, Cpi, Bac, Eac, Vac],
            Table::Variance => vec![Indicators, Name, Start, Finish, BaselineStart, BaselineFinish, StartVariance, FinishVariance],
        }
    }
}
