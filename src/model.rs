//! Project data model. Field names and semantics follow Microsoft Project
//! (and its MSPDI XML schema) so the concepts map one-to-one.

use crate::calendar::Calendar;
use crate::duration::{DurUnit, Duration, DurationSettings, Lag};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkType {
    FS,
    SS,
    FF,
    SF,
}

impl LinkType {
    pub const ALL: [LinkType; 4] = [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF];
    pub fn code(&self) -> &'static str {
        match self {
            LinkType::FS => "FS",
            LinkType::SS => "SS",
            LinkType::FF => "FF",
            LinkType::SF => "SF",
        }
    }
    pub fn long_name(&self) -> &'static str {
        match self {
            LinkType::FS => "Finish-to-Start (FS)",
            LinkType::SS => "Start-to-Start (SS)",
            LinkType::FF => "Finish-to-Finish (FF)",
            LinkType::SF => "Start-to-Finish (SF)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// Unique ID of the predecessor task.
    pub pred: u32,
    pub kind: LinkType,
    pub lag: Lag,
}

/// A task waiting on a board item (a story, task, bug or a whole epic):
/// finish-to-start, with an optional lag. Until the item is done the task
/// follows the finish of the sprint it's planned in; once it's done, the
/// day it was finished.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoardDep {
    pub ticket: u32,
    #[serde(default)]
    pub lag: Lag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstraintType {
    ASAP,
    ALAP,
    MSO,
    MFO,
    SNET,
    SNLT,
    FNET,
    FNLT,
}

impl ConstraintType {
    pub const ALL: [ConstraintType; 8] = [
        ConstraintType::ASAP,
        ConstraintType::ALAP,
        ConstraintType::MSO,
        ConstraintType::MFO,
        ConstraintType::SNET,
        ConstraintType::SNLT,
        ConstraintType::FNET,
        ConstraintType::FNLT,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            ConstraintType::ASAP => "As Soon As Possible",
            ConstraintType::ALAP => "As Late As Possible",
            ConstraintType::MSO => "Must Start On",
            ConstraintType::MFO => "Must Finish On",
            ConstraintType::SNET => "Start No Earlier Than",
            ConstraintType::SNLT => "Start No Later Than",
            ConstraintType::FNET => "Finish No Earlier Than",
            ConstraintType::FNLT => "Finish No Later Than",
        }
    }
    pub fn needs_date(&self) -> bool {
        !matches!(self, ConstraintType::ASAP | ConstraintType::ALAP)
    }
    /// Flexible constraints don't tie a task to a date (shown without the
    /// red-dot indicator in Project).
    pub fn is_inflexible(&self) -> bool {
        matches!(self, ConstraintType::MSO | ConstraintType::MFO)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskMode {
    Auto,
    Manual,
}

/// Values computed by the scheduler; never persisted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Computed {
    pub start: Option<NaiveDateTime>,
    pub finish: Option<NaiveDateTime>,
    pub early_start: Option<NaiveDateTime>,
    pub early_finish: Option<NaiveDateTime>,
    pub late_start: Option<NaiveDateTime>,
    pub late_finish: Option<NaiveDateTime>,
    /// Working minutes.
    pub total_slack: i64,
    pub free_slack: i64,
    pub critical: bool,
    pub is_summary: bool,
    /// For summaries: rolled-up working duration.
    pub summary_duration: i64,
    /// Rolled-up percent complete for summaries.
    pub percent_complete: f64,
    pub outline_number: String,
    /// Scheduling conflict message, if any (shown as an indicator).
    pub conflict: Option<String>,
    pub deadline_missed: bool,
    /// What determined the task's start (Task Inspector).
    pub start_driver: StartDriver,
    /// Links that drive the start: (predecessor task uid as linked, link type).
    pub driving_links: Vec<(u32, LinkType)>,
    /// Leaf tasks whose dates drive this task's start (for Task Path).
    pub driving_preds: Vec<u32>,
    pub start_slack: i64,
    pub finish_slack: i64,
    pub status: TaskStatus,
    /// Behind schedule at the status date (Late Tasks bar style).
    pub late: bool,
    /// Total work of the task's assignments (working minutes); rolled up
    /// for summaries.
    pub work: i64,
    /// Fixed cost + assignment costs; rolled up for summaries.
    pub cost: f64,
    /// One of the task's resources is overallocated while it works on it.
    pub overallocated: bool,
    /// Where the task would start without its leveling delay (the early
    /// start the delay is measured from).
    pub unleveled_start: Option<NaiveDateTime>,
    /// Tracking (working minutes / money), rolled up for summaries.
    pub actual_work: i64,
    pub actual_cost: f64,
    /// Earned value at the status date: planned value (BCWS), earned
    /// value (BCWP), actual cost (ACWP); and the baseline cost (BAC).
    pub bcws: f64,
    pub bcwp: f64,
    pub acwp: f64,
    pub bac: f64,
    /// Board items it waits on that aren't done yet (ticket ids).
    pub board_waiting: Vec<u32>,
    /// Problems with its board dependencies (unplanned story, loop).
    pub board_notes: Vec<String>,
    /// Board items whose sprint drives its start (ticket ids).
    pub board_driving: Vec<u32>,
}

/// Why a task starts when it does — the Task Inspector's "factors".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartDriver {
    #[default]
    ProjectStart,
    Predecessors,
    Constraint,
    ParentConstraint,
    /// Delayed by resource leveling (Leveling Delay).
    Leveling,
    /// Started: the actual start is a fact, not a calculation.
    Actual,
    /// A finished board item it waits on (the day it was done).
    BoardItem,
    ManualDates,
    AsLateAsPossible,
    Circular,
}

/// Project's Status field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    FutureTask,
    OnSchedule,
    Late,
    Complete,
}

impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::FutureTask => "Future Task",
            TaskStatus::OnSchedule => "On Schedule",
            TaskStatus::Late => "Late",
            TaskStatus::Complete => "Complete",
        }
    }
}

/// A saved baseline of a task (Project ▸ Set Baseline; Baseline 0–10).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub start: NaiveDateTime,
    pub finish: NaiveDateTime,
    /// Working minutes.
    pub duration: i64,
    /// Work (working minutes) and cost at the time (0.5 files: 0).
    #[serde(default)]
    pub work: i64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub uid: u32,
    pub name: String,
    pub outline_level: u32,
    pub mode: TaskMode,
    pub duration: Duration,
    /// For manually scheduled tasks: the user-entered start (finish is
    /// derived from duration). Also remembered for auto tasks so switching
    /// modes is lossless.
    pub manual_start: Option<NaiveDateTime>,
    pub constraint: ConstraintType,
    pub constraint_date: Option<NaiveDateTime>,
    pub deadline: Option<NaiveDateTime>,
    /// Explicit "Mark task as milestone" flag (zero-duration tasks are
    /// milestones automatically).
    pub milestone: bool,
    pub percent_complete: f64,
    pub notes: String,
    pub predecessors: Vec<Link>,
    pub collapsed: bool,
    /// "Display on Timeline" (Task Information ▸ General).
    #[serde(default)]
    pub on_timeline: bool,
    /// Format ▸ Format Bar colour override for this task's bar.
    #[serde(default)]
    pub bar_color: Option<crate::format::Rgb>,
    #[serde(default)]
    pub baseline: Option<Baseline>,
    /// Fixed Units / Fixed Duration / Fixed Work (Task Information ▸ Advanced).
    #[serde(default)]
    pub task_type: TaskType,
    #[serde(default)]
    pub effort_driven: bool,
    #[serde(default)]
    pub fixed_cost: f64,
    /// 0–1000, default 500; higher-priority tasks are levelled first.
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Delay added by resource leveling, in working minutes.
    #[serde(default)]
    pub leveling_delay: i64,
    /// A sprint: holds Kanban tickets, and its % complete comes from them.
    #[serde(default)]
    pub is_sprint: bool,
    /// Board items (stories, epics) this task waits on.
    #[serde(default)]
    pub board_deps: Vec<BoardDep>,
    /// Project's Active field: an inactive task stays in the plan (struck
    /// through) but doesn't drive successors, roll up into summaries, or
    /// book its resources.
    #[serde(default = "yes")]
    pub active: bool,
    /// A recurring task's pattern (on the summary of its occurrences).
    #[serde(default)]
    pub recurrence: Option<crate::recurring::Recurrence>,
    /// % complete was last set by the board (a sprint's tickets or an
    /// epic); when that source goes away it's reset rather than left stale.
    #[serde(default)]
    pub board_driven: bool,
    /// Tracking: when work really started / finished.
    #[serde(default)]
    pub actual_start: Option<NaiveDateTime>,
    #[serde(default)]
    pub actual_finish: Option<NaiveDateTime>,
    /// Baselines 1–10 (Baseline 0 is `baseline`).
    #[serde(default)]
    pub baselines: [Option<Baseline>; 10],
    #[serde(skip)]
    pub calc: Computed,
}

fn default_priority() -> u32 {
    500
}

fn yes() -> bool {
    true
}

/// Which of Work = Duration × Units stays fixed when one of them changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    #[default]
    FixedUnits,
    FixedDuration,
    FixedWork,
}

impl TaskType {
    pub const ALL: [TaskType; 3] = [TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork];
    pub fn label(&self) -> &'static str {
        match self {
            TaskType::FixedUnits => "Fixed Units",
            TaskType::FixedDuration => "Fixed Duration",
            TaskType::FixedWork => "Fixed Work",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    #[default]
    Work,
    Material,
    Cost,
}

impl ResourceType {
    pub const ALL: [ResourceType; 3] = [ResourceType::Work, ResourceType::Material, ResourceType::Cost];
    pub fn label(&self) -> &'static str {
        match self {
            ResourceType::Work => "Work",
            ResourceType::Material => "Material",
            ResourceType::Cost => "Cost",
        }
    }
}

/// When a resource's cost is incurred (Resource Sheet ▸ Accrue At).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accrue {
    Start,
    #[default]
    Prorated,
    End,
}

impl Accrue {
    pub const ALL: [Accrue; 3] = [Accrue::Start, Accrue::Prorated, Accrue::End];
    pub fn label(&self) -> &'static str {
        match self {
            Accrue::Start => "Start",
            Accrue::Prorated => "Prorated",
            Accrue::End => "End",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resource {
    pub uid: u32,
    pub name: String,
    pub rtype: ResourceType,
    /// Unit of measure for material resources ("tons", "licences").
    pub material_label: String,
    pub initials: String,
    pub group: String,
    /// E-mail address (Jira assignees match on it too).
    pub email: String,
    /// 1.0 = 100% (one full-time person).
    pub max_units: f64,
    /// Per hour for work resources, per unit for material resources.
    pub std_rate: f64,
    pub ovt_rate: f64,
    pub cost_per_use: f64,
    pub accrue: Accrue,
    /// The resource's own calendar = project calendar + these changes.
    pub exceptions: Vec<crate::calendar::CalendarException>,
    pub week: Option<[Vec<crate::calendar::Shift>; 7]>,
    pub notes: String,
}

impl Default for Resource {
    fn default() -> Self {
        Resource {
            uid: 0,
            name: String::new(),
            rtype: ResourceType::Work,
            material_label: String::new(),
            initials: String::new(),
            group: String::new(),
            email: String::new(),
            max_units: 1.0,
            std_rate: 0.0,
            ovt_rate: 0.0,
            cost_per_use: 0.0,
            accrue: Accrue::Prorated,
            exceptions: vec![],
            week: None,
            notes: String::new(),
        }
    }
}

/// A resource working on a task.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub task: u32,
    pub resource: u32,
    /// Work resources: 1.0 = 100%. Material resources: quantity.
    pub units: f64,
    /// Work resources: working minutes.
    pub work: i64,
    /// Cost resources: the amount assigned.
    #[serde(default)]
    pub cost: f64,
}

/// Values computed per assignment by the scheduler.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssignmentCalc {
    pub start: Option<NaiveDateTime>,
    pub finish: Option<NaiveDateTime>,
    pub cost: f64,
    pub overallocated: bool,
}

/// Values computed per resource by the scheduler.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceCalc {
    pub work: i64,
    pub cost: f64,
    pub overallocated: bool,
    /// Periods where assigned units exceed Max Units.
    pub over_intervals: Vec<(NaiveDateTime, NaiveDateTime)>,
    /// Highest total units assigned at any moment.
    pub peak_units: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LevelingOrder {
    IdOnly,
    #[default]
    Standard,
    PriorityStandard,
}

impl LevelingOrder {
    pub const ALL: [LevelingOrder; 3] = [LevelingOrder::IdOnly, LevelingOrder::Standard, LevelingOrder::PriorityStandard];
    pub fn label(&self) -> &'static str {
        match self {
            LevelingOrder::IdOnly => "ID Only",
            LevelingOrder::Standard => "Standard",
            LevelingOrder::PriorityStandard => "Priority, Standard",
        }
    }
}

impl Task {
    pub fn new(uid: u32, name: impl Into<String>, mode: TaskMode, settings: &DurationSettings) -> Self {
        let mut d = Duration::days(1.0, settings);
        d.estimated = true;
        Task {
            uid,
            name: name.into(),
            outline_level: 1,
            mode,
            duration: d,
            manual_start: None,
            constraint: ConstraintType::ASAP,
            constraint_date: None,
            deadline: None,
            milestone: false,
            percent_complete: 0.0,
            notes: String::new(),
            predecessors: vec![],
            collapsed: false,
            on_timeline: false,
            bar_color: None,
            baseline: None,
            task_type: TaskType::FixedUnits,
            effort_driven: false,
            fixed_cost: 0.0,
            priority: 500,
            leveling_delay: 0,
            is_sprint: false,
            board_driven: false,
            board_deps: vec![],
            recurrence: None,
            active: true,
            actual_start: None,
            actual_finish: None,
            baselines: [None; 10],
            calc: Computed::default(),
        }
    }

    /// Baseline `n` (0 = Baseline, 1–10 = Baseline 1…10).
    pub fn baseline_slot(&self, n: usize) -> Option<Baseline> {
        match n {
            0 => self.baseline,
            1..=10 => self.baselines[n - 1],
            _ => None,
        }
    }

    pub fn baseline_slot_mut(&mut self, n: usize) -> &mut Option<Baseline> {
        match n {
            0 => &mut self.baseline,
            _ => &mut self.baselines[(n.clamp(1, 10)) - 1],
        }
    }

    pub fn is_milestone(&self) -> bool {
        !self.calc.is_summary && (self.milestone || self.duration.is_zero())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub durations: DurationSettings,
    pub default_start_minute: u32,
    pub default_finish_minute: u32,
    /// "New tasks created: Auto Scheduled" (status-bar toggle in Project).
    pub new_tasks_auto: bool,
    /// "Tasks will always honor their constraint dates".
    pub honor_constraints: bool,
    /// "Tasks are critical if slack is less than or equal to" (minutes).
    pub critical_slack_threshold: i64,
    pub default_duration_unit: DurUnit,
    /// "Calculate multiple critical paths": every chain end is critical
    /// relative to its own finish, not the project finish.
    pub multiple_critical_paths: bool,
    pub default_task_type: TaskType,
    pub new_tasks_effort_driven: bool,
    /// Typing an unknown name in Resource Names creates the resource.
    pub auto_add_resources: bool,
    pub currency_symbol: String,
    pub leveling_order: LevelingOrder,
    pub level_within_slack: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            durations: DurationSettings::default(),
            default_start_minute: 8 * 60,
            default_finish_minute: 17 * 60,
            new_tasks_auto: true,
            honor_constraints: true,
            critical_slack_threshold: 0,
            default_duration_unit: DurUnit::Day,
            multiple_critical_paths: false,
            default_task_type: TaskType::FixedUnits,
            new_tasks_effort_driven: false,
            auto_add_resources: true,
            currency_symbol: "$".into(),
            leveling_order: LevelingOrder::Standard,
            level_within_slack: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub title: String,
    pub manager: String,
    pub start: NaiveDateTime,
    pub status_date: Option<NaiveDateTime>,
    pub calendar: Calendar,
    pub settings: Settings,
    pub tasks: Vec<Task>,
    pub next_uid: u32,
    /// Gantt view formatting (bar styles, gridlines, layout, timescale).
    #[serde(default)]
    pub format: crate::format::GanttFormat,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
    #[serde(default = "one")]
    pub next_res_uid: u32,
    /// Kanban tickets (stories), in board order.
    #[serde(default)]
    pub tickets: Vec<crate::board::Ticket>,
    #[serde(default = "one")]
    pub next_ticket_id: u32,
    #[serde(default)]
    pub board: crate::board::BoardSettings,
    /// When each baseline (0–10) was last saved.
    #[serde(default)]
    pub baseline_saved: [Option<NaiveDateTime>; 11],
    /// Custom filters (View ▸ Filter ▸ New Filter…).
    #[serde(default)]
    pub views: crate::views::Views,
    #[serde(skip)]
    pub calc: ProjectCalc,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectCalc {
    pub finish: Option<NaiveDateTime>,
    pub errors: Vec<String>,
    /// Parallel to `Project::assignments`.
    pub assignments: Vec<AssignmentCalc>,
    /// Parallel to `Project::resources`.
    pub resources: Vec<ResourceCalc>,
    /// Effective calendar per resource (project calendar + its changes).
    pub resource_calendars: Vec<crate::calendar::Calendar>,
    pub total_cost: f64,
    pub total_work: i64,
}

impl Project {
    pub fn new(start_date: NaiveDate) -> Self {
        let settings = Settings::default();
        let start = start_date.and_time(minute_time(settings.default_start_minute));
        Project {
            title: "Project1".into(),
            manager: String::new(),
            start,
            status_date: None,
            calendar: Calendar::standard(),
            settings,
            tasks: vec![],
            next_uid: 1,
            format: Default::default(),
            resources: vec![],
            assignments: vec![],
            next_res_uid: 1,
            tickets: vec![],
            next_ticket_id: 1,
            board: Default::default(),
            baseline_saved: [None; 11],
            views: Default::default(),
            calc: ProjectCalc::default(),
        }
    }

    pub fn alloc_uid(&mut self) -> u32 {
        let u = self.next_uid;
        self.next_uid += 1;
        u
    }

    pub fn index_of_uid(&self, uid: u32) -> Option<usize> {
        self.tasks.iter().position(|t| t.uid == uid)
    }

    pub fn task_by_uid(&self, uid: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.uid == uid)
    }

    /// Row ID (1-based) as shown in the ID column.
    pub fn id_of_uid(&self, uid: u32) -> Option<usize> {
        self.index_of_uid(uid).map(|i| i + 1)
    }

    /// Rows that are inactive themselves or under an inactive summary.
    pub fn inactive_mask(&self) -> Vec<bool> {
        let mut out = vec![false; self.tasks.len()];
        let mut stack: Vec<(u32, bool)> = vec![];
        for (i, t) in self.tasks.iter().enumerate() {
            while stack.last().is_some_and(|&(l, _)| l >= t.outline_level) {
                stack.pop();
            }
            let inherited = stack.last().is_some_and(|&(_, x)| x);
            out[i] = inherited || !t.active;
            stack.push((t.outline_level, out[i]));
        }
        out
    }

    pub fn is_summary(&self, idx: usize) -> bool {
        idx + 1 < self.tasks.len() && self.tasks[idx + 1].outline_level > self.tasks[idx].outline_level
    }

    /// Exclusive end index of the subtree rooted at `idx`.
    pub fn subtree_end(&self, idx: usize) -> usize {
        let lvl = self.tasks[idx].outline_level;
        let mut j = idx + 1;
        while j < self.tasks.len() && self.tasks[j].outline_level > lvl {
            j += 1;
        }
        j
    }

    pub fn parent_of(&self, idx: usize) -> Option<usize> {
        let lvl = self.tasks[idx].outline_level;
        // a top-level task has no parent: don't scan back to row 0 for one
        if lvl <= 1 {
            return None;
        }
        (0..idx).rev().find(|&j| self.tasks[j].outline_level < lvl)
    }

    pub fn ancestors_of(&self, idx: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut cur = idx;
        while let Some(p) = self.parent_of(cur) {
            out.push(p);
            cur = p;
        }
        out
    }

    /// True if row `idx` is hidden because an ancestor is collapsed.
    pub fn is_hidden(&self, idx: usize) -> bool {
        self.ancestors_of(idx).iter().any(|&a| self.tasks[a].collapsed)
    }

    /// Visible row indices in display order.
    pub fn visible_rows(&self) -> Vec<usize> {
        let mut out = vec![];
        let mut i = 0;
        while i < self.tasks.len() {
            out.push(i);
            if self.tasks[i].collapsed && self.is_summary(i) {
                i = self.subtree_end(i);
            } else {
                i += 1;
            }
        }
        out
    }

    /// Ensure outline levels form a valid tree: first task level 1, and no
    /// task more than one level deeper than its predecessor row.
    pub fn normalize_outline(&mut self) {
        let mut prev = 0u32;
        for t in self.tasks.iter_mut() {
            if t.outline_level < 1 {
                t.outline_level = 1;
            }
            if t.outline_level > prev + 1 {
                t.outline_level = prev + 1;
            }
            prev = t.outline_level;
        }
    }

    pub fn default_start_time(&self) -> NaiveTime {
        minute_time(self.settings.default_start_minute)
    }
    pub fn default_finish_time(&self) -> NaiveTime {
        minute_time(self.settings.default_finish_minute)
    }

    /// Parse a duration using this project's settings.
    pub fn parse_duration(&self, text: &str) -> Result<Duration, String> {
        Duration::parse(text, &self.settings.durations, self.settings.default_duration_unit)
    }

    pub fn format_duration_minutes(&self, minutes: i64, unit: DurUnit) -> String {
        Duration::from_minutes(minutes, unit).format(&self.settings.durations)
    }
}

pub fn minute_time(m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt((m / 60).min(23), m % 60, 0).unwrap()
}

/// Format like Project's default date format: "Mon 1/5/26".
pub fn fmt_date(dt: NaiveDateTime) -> String {
    format!(
        "{} {}/{}/{:02}",
        dt.format("%a"),
        dt.month(),
        dt.day(),
        dt.year() % 100
    )
}

/// Long form with time: "Mon 1/5/26 8:00 AM".
pub fn fmt_datetime(dt: NaiveDateTime) -> String {
    let h = dt.hour();
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{} {}:{:02} {}", fmt_date(dt), h12, dt.minute(), ampm)
}

/// Parse a date as typed into a Start/Finish/constraint cell. Accepts
/// "1/5/26", "1/5/2026", "Mon 1/5/26", "2026-01-05", each optionally
/// followed by a time ("8:00 AM", "17:00"). When no time is given,
/// `default_time` is used (Project uses 8 AM for starts, 5 PM for finishes).
pub fn parse_date(text: &str, default_time: NaiveTime) -> Result<NaiveDateTime, String> {
    let err = || format!("\"{}\" is not a valid date", text.trim());
    let mut t = text.trim().to_string();
    if t.is_empty() {
        return Err(err());
    }
    // strip a leading weekday name
    if let Some((first, rest)) = t.split_once(' ') {
        if first.chars().all(|c| c.is_ascii_alphabetic()) && first.len() >= 2 {
            t = rest.trim().to_string();
        }
    }
    let (date_part, time_part) = match t.split_once(' ') {
        Some((d, r)) => (d.to_string(), Some(r.trim().to_string())),
        None => (t.clone(), None),
    };
    let date = parse_date_only(&date_part).ok_or_else(err)?;
    let time = match time_part {
        None => default_time,
        Some(tp) => parse_time(&tp).ok_or_else(err)?,
    };
    Ok(date.and_time(time))
}

fn parse_date_only(s: &str) -> Option<NaiveDate> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    let parts: Vec<&str> = s.split(['/', '-', '.']).collect();
    if parts.len() == 3 {
        let m: u32 = parts[0].parse().ok()?;
        let d: u32 = parts[1].parse().ok()?;
        let mut y: i32 = parts[2].parse().ok()?;
        if parts[2].len() <= 2 {
            y += if y < 70 { 2000 } else { 1900 };
        }
        return NaiveDate::from_ymd_opt(y, m, d);
    }
    if parts.len() == 2 {
        // "1/5" → current-ish year is ambiguous; use 2000+ rule with today's year
        let m: u32 = parts[0].parse().ok()?;
        let d: u32 = parts[1].parse().ok()?;
        let y = chrono::Local::now().year();
        return NaiveDate::from_ymd_opt(y, m, d);
    }
    None
}

pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let u = s.trim().to_ascii_uppercase().replace('.', "");
    let (body, ampm) = if let Some(b) = u.strip_suffix("AM") {
        (b.trim().to_string(), Some(false))
    } else if let Some(b) = u.strip_suffix("PM") {
        (b.trim().to_string(), Some(true))
    } else {
        (u.clone(), None)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok()?, 0),
    };
    let h = match ampm {
        Some(false) => {
            if h == 12 {
                0
            } else {
                h
            }
        }
        Some(true) => {
            if h == 12 {
                12
            } else {
                h + 12
            }
        }
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_formats() {
        let d = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_hms_opt(8, 0, 0).unwrap();
        assert_eq!(fmt_date(d), "Mon 1/5/26");
        assert_eq!(fmt_datetime(d), "Mon 1/5/26 8:00 AM");
        let d = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap().and_hms_opt(17, 0, 0).unwrap();
        assert_eq!(fmt_datetime(d), "Fri 12/25/26 5:00 PM");
    }

    #[test]
    fn date_parsing() {
        let t8 = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        let want = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_time(t8);
        assert_eq!(parse_date("1/5/26", t8).unwrap(), want);
        assert_eq!(parse_date("Mon 1/5/26", t8).unwrap(), want);
        assert_eq!(parse_date("1/5/2026", t8).unwrap(), want);
        assert_eq!(parse_date("2026-01-05", t8).unwrap(), want);
        assert_eq!(
            parse_date("1/5/26 5:00 PM", t8).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_hms_opt(17, 0, 0).unwrap()
        );
        assert_eq!(
            parse_date("Mon 1/5/26 13:30", t8).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_hms_opt(13, 30, 0).unwrap()
        );
        assert!(parse_date("13/45/26", t8).is_err());
        assert!(parse_date("tomorrow", t8).is_err());
    }
}
