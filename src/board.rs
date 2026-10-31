//! Kanban board: tickets (stories) planned into sprints.
//!
//! A sprint is an ordinary Gantt task with `is_sprint` set: one bar on the
//! schedule, a fixed timebox. Tickets live on the board, not in the Gantt.
//! They belong to a sprint or to the backlog (`sprint == None`). A sprint's
//! % complete is derived from its tickets: story points (or ticket count)
//! in the last column ÷ the total. Its dates never change because of the
//! board; scope flexes, the timebox doesn't.
//!
//! Ticket assignees are labels for the board only. The team's workload
//! comes from resources assigned to the sprint task, so nothing is counted
//! twice.

use crate::model::{LinkType, Project, TaskType};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

pub type OpResult<T = ()> = Result<T, String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ticket {
    pub id: u32,
    pub title: String,
    pub description: String,
    /// Story points; `None` = not estimated yet.
    pub points: Option<f64>,
    /// Index into `BoardSettings::columns`.
    pub column: usize,
    /// Sprint task uid, or `None` for the backlog.
    pub sprint: Option<u32>,
    /// Resource uid, shown on the card (books no work).
    pub assignee: Option<u32>,
    /// When it last entered the done (last) column.
    pub done_at: Option<NaiveDateTime>,
    pub created: Option<NaiveDateTime>,
    /// Story, Task, Bug, Epic or Sub-task.
    pub kind: IssueType,
    pub priority: Priority,
    pub labels: Vec<String>,
    /// The epic of a story/task/bug, or the story/task/bug of a sub-task.
    pub parent: Option<u32>,
    /// Outward links ("this blocks …"); inward ones are derived.
    pub links: Vec<TicketLink>,
    /// Epics only: the Gantt task whose % complete this epic drives.
    pub task: Option<u32>,
    /// Gantt tasks (uids) this ticket can't start before: their work has to
    /// be finished first.
    pub after_tasks: Vec<u32>,
    /// The release (fix version) it ships in. Stories, tasks and bugs only.
    pub release: Option<u32>,
    /// When it entered each column (for the cumulative flow diagram).
    pub history: Vec<(NaiveDateTime, usize)>,
}

impl Default for Ticket {
    fn default() -> Self {
        Ticket {
            id: 0,
            title: String::new(),
            description: String::new(),
            points: None,
            column: 0,
            sprint: None,
            assignee: None,
            done_at: None,
            created: None,
            kind: IssueType::Story,
            priority: Priority::Medium,
            labels: vec![],
            parent: None,
            links: vec![],
            task: None,
            after_tasks: vec![],
            release: None,
            history: vec![],
        }
    }
}

/// Jira's standard issue types. Epics group work across sprints and sit
/// outside them; sub-tasks split a story and travel with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueType {
    #[default]
    Story,
    Task,
    Bug,
    Epic,
    Subtask,
}

impl IssueType {
    pub const ALL: [IssueType; 5] = [IssueType::Story, IssueType::Task, IssueType::Bug, IssueType::Epic, IssueType::Subtask];
    pub fn label(self) -> &'static str {
        match self {
            IssueType::Story => "Story",
            IssueType::Task => "Task",
            IssueType::Bug => "Bug",
            IssueType::Epic => "Epic",
            IssueType::Subtask => "Sub-task",
        }
    }
    /// Badge letter on a card.
    pub fn letter(self) -> &'static str {
        match self {
            IssueType::Story => "S",
            IssueType::Task => "T",
            IssueType::Bug => "B",
            IssueType::Epic => "E",
            IssueType::Subtask => "s",
        }
    }
    /// Standard issues are the ones planned into sprints and counted.
    pub fn is_standard(self) -> bool {
        !matches!(self, IssueType::Epic | IssueType::Subtask)
    }
    /// Jira's names (case-insensitive; "Sub-task", "Subtask", "Sub task").
    pub fn from_name(s: &str) -> Option<IssueType> {
        let n: String = s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        Some(match n.as_str() {
            "story" | "userstory" => IssueType::Story,
            "task" => IssueType::Task,
            "bug" | "defect" => IssueType::Bug,
            "epic" => IssueType::Epic,
            "subtask" | "technicalsubtask" | "subbug" => IssueType::Subtask,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    Highest,
    High,
    #[default]
    Medium,
    Low,
    Lowest,
}

impl Priority {
    pub const ALL: [Priority; 5] = [Priority::Highest, Priority::High, Priority::Medium, Priority::Low, Priority::Lowest];
    pub fn label(self) -> &'static str {
        match self {
            Priority::Highest => "Highest",
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
            Priority::Lowest => "Lowest",
        }
    }
    pub fn from_name(s: &str) -> Option<Priority> {
        let n = s.trim().to_lowercase();
        Some(match n.as_str() {
            "highest" | "blocker" | "critical" | "p0" | "p1" => Priority::Highest,
            "high" | "major" | "p2" => Priority::High,
            "medium" | "normal" | "p3" => Priority::Medium,
            "low" | "minor" | "p4" => Priority::Low,
            "lowest" | "trivial" | "p5" => Priority::Lowest,
            _ => return None,
        })
    }
}

/// Jira's default link types. Stored once, on the outward side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkKind {
    #[default]
    Blocks,
    Relates,
    Duplicates,
    Clones,
}

impl LinkKind {
    pub const ALL: [LinkKind; 4] = [LinkKind::Blocks, LinkKind::Relates, LinkKind::Duplicates, LinkKind::Clones];
    pub fn outward(self) -> &'static str {
        match self {
            LinkKind::Blocks => "blocks",
            LinkKind::Relates => "relates to",
            LinkKind::Duplicates => "duplicates",
            LinkKind::Clones => "clones",
        }
    }
    pub fn inward(self) -> &'static str {
        match self {
            LinkKind::Blocks => "is blocked by",
            LinkKind::Relates => "relates to",
            LinkKind::Duplicates => "is duplicated by",
            LinkKind::Clones => "is cloned by",
        }
    }
    /// The link type's name in Jira (CSV column "Outward issue link (…)").
    pub fn jira_name(self) -> &'static str {
        match self {
            LinkKind::Blocks => "Blocks",
            LinkKind::Relates => "Relates",
            LinkKind::Duplicates => "Duplicate",
            LinkKind::Clones => "Cloners",
        }
    }
    pub fn from_jira_name(s: &str) -> Option<LinkKind> {
        let n = s.trim().to_lowercase();
        Some(match n.as_str() {
            "blocks" | "blocker" | "block" => LinkKind::Blocks,
            "relates" | "relate" | "related" | "relates to" => LinkKind::Relates,
            "duplicate" | "duplicates" => LinkKind::Duplicates,
            "cloners" | "clones" | "clone" => LinkKind::Clones,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketLink {
    pub kind: LinkKind,
    pub to: u32,
}

/// One link as seen from a ticket: "blocks FCG-7" or "is blocked by FCG-3".
#[derive(Clone, Debug, PartialEq)]
pub struct LinkView {
    pub kind: LinkKind,
    pub outward: bool,
    pub other: u32,
}

impl LinkView {
    pub fn verb(&self) -> &'static str {
        if self.outward {
            self.kind.outward()
        } else {
            self.kind.inward()
        }
    }
}

/// What a sprint's progress, burndown and velocity are measured in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Estimate {
    #[default]
    StoryPoints,
    TicketCount,
}

impl Estimate {
    pub const ALL: [Estimate; 2] = [Estimate::StoryPoints, Estimate::TicketCount];
    pub fn label(self) -> &'static str {
        match self {
            Estimate::StoryPoints => "Story points",
            Estimate::TicketCount => "Ticket count",
        }
    }
    /// Unit for numbers ("12 pts", "5 tickets").
    pub fn unit(self, v: f64) -> String {
        let n = crate::duration::fmt_num(v);
        match self {
            Estimate::StoryPoints => format!("{n} {}", if v == 1.0 { "pt" } else { "pts" }),
            Estimate::TicketCount => format!("{n} {}", if v == 1.0 { "ticket" } else { "tickets" }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardSettings {
    /// Column names, left to right; the last one means done.
    pub columns: Vec<String>,
    pub estimate: Estimate,
    /// Length of a new sprint, in working days.
    pub sprint_days: u32,
    /// Ticket keys read "PREFIX-12"; empty shows "#12".
    pub key_prefix: String,
    /// Work-in-progress limit per column (cards in it, in one sprint).
    pub wip: Vec<Option<u32>>,
    /// Goal, commitment and completion of each sprint (by task uid).
    pub sprints: Vec<SprintMeta>,
    /// Releases (fix versions), in the order shown.
    pub releases: Vec<crate::releases::Release>,
    pub next_release_id: u32,
}

/// Jira's sprint lifecycle on top of the timebox: the goal, what was
/// committed when it was started, and what was finished when it was
/// completed (so velocity reports don't change when scope moves later).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SprintMeta {
    pub uid: u32,
    pub goal: String,
    pub started: Option<NaiveDateTime>,
    /// Scope at the start, in the estimate unit, and as a ticket count.
    pub committed: Option<f64>,
    pub committed_tickets: usize,
    pub completed: Option<NaiveDateTime>,
    pub completed_value: Option<f64>,
    pub completed_tickets: usize,
    /// Unfinished tickets moved out when it was completed.
    pub carried_over: usize,
}

impl Default for BoardSettings {
    fn default() -> Self {
        BoardSettings {
            columns: ["To Do", "In Progress", "Review", "Done"].iter().map(|s| s.to_string()).collect(),
            estimate: Estimate::StoryPoints,
            sprint_days: 10,
            key_prefix: String::new(),
            wip: vec![],
            sprints: vec![],
            releases: vec![],
            next_release_id: 1,
        }
    }
}

impl BoardSettings {
    pub fn done_column(&self) -> usize {
        self.columns.len().saturating_sub(1)
    }

    pub fn wip_limit(&self, column: usize) -> Option<u32> {
        self.wip.get(column).copied().flatten().filter(|&n| n > 0)
    }

    pub fn sprint_meta(&self, uid: u32) -> Option<&SprintMeta> {
        self.sprints.iter().find(|m| m.uid == uid)
    }

    fn sprint_meta_mut(&mut self, uid: u32) -> &mut SprintMeta {
        if let Some(i) = self.sprints.iter().position(|m| m.uid == uid) {
            return &mut self.sprints[i];
        }
        self.sprints.push(SprintMeta { uid, ..Default::default() });
        self.sprints.last_mut().unwrap()
    }
}

/// Totals for one sprint (or the backlog) in the project's estimate unit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SprintStats {
    pub tickets: usize,
    pub done_tickets: usize,
    /// Planned (all tickets) and completed (done column), in the unit.
    pub total: f64,
    pub done: f64,
    /// Tickets without story points (only counted in point mode).
    pub unestimated: usize,
}

impl SprintStats {
    pub fn percent(&self) -> f64 {
        if self.total > 0.0 {
            (self.done / self.total * 100.0).clamp(0.0, 100.0)
        } else if self.tickets > 0 {
            // nothing estimated yet: fall back to the share of tickets done
            self.done_tickets as f64 / self.tickets as f64 * 100.0
        } else {
            0.0
        }
    }
}

/// Burndown of one sprint: remaining work at each working-day boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Burndown {
    /// Working days of the sprint.
    pub days: Vec<NaiveDate>,
    pub total: f64,
    /// `days.len() + 1` points: before day 1, after day 1, … after the last.
    pub ideal: Vec<f64>,
    /// Same points; `None` for boundaries still in the future.
    pub actual: Vec<Option<f64>>,
}

/// Moves kept per ticket.
pub const HISTORY_CAP: usize = 64;

/// Cumulative flow: tickets in each column at the end of each working day.
#[derive(Clone, Debug, PartialEq)]
pub struct CumulativeFlow {
    pub days: Vec<NaiveDate>,
    pub columns: Vec<String>,
    /// `counts[day][column]`; `None` for days still to come.
    pub counts: Vec<Option<Vec<usize>>>,
}

/// What a board dependency resolves to (see `resolve_board_dep`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DepTarget {
    /// Sprints (task uids) whose end the waiting task follows.
    pub sprints: Vec<u32>,
    /// When the finished items were done, the sprint they were in, and
    /// which item it was.
    pub done: Vec<(NaiveDateTime, Option<u32>, u32)>,
    /// Unfinished items not planned into any sprint.
    pub unplanned: Vec<u32>,
    /// Every unfinished item.
    pub waiting: Vec<u32>,
    /// For each sprint in `sprints`: the first unfinished item in it.
    pub by_sprint: Vec<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VelocityRow {
    pub sprint_uid: u32,
    pub name: String,
    pub start: Option<NaiveDateTime>,
    pub finish: Option<NaiveDateTime>,
    pub planned: f64,
    pub completed: f64,
    pub tickets: usize,
    pub done_tickets: usize,
}

impl Project {
    fn now() -> NaiveDateTime {
        chrono::Local::now().naive_local()
    }

    pub fn ticket_index(&self, id: u32) -> Option<usize> {
        self.tickets.iter().position(|t| t.id == id)
    }

    pub fn ticket(&self, id: u32) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.id == id)
    }

    /// A ticket's size in the project's estimate unit. Epics and sub-tasks
    /// count 0: an epic's size is its stories', and a sub-task is part of
    /// its parent's estimate (Jira's sprint reports work the same way).
    pub fn ticket_units(&self, t: &Ticket) -> f64 {
        if !t.kind.is_standard() {
            return 0.0;
        }
        match self.board.estimate {
            Estimate::StoryPoints => t.points.unwrap_or(0.0),
            Estimate::TicketCount => 1.0,
        }
    }

    pub fn is_done(&self, t: &Ticket) -> bool {
        t.column >= self.board.done_column()
    }

    /// Sprint rows, earliest first (then by row).
    pub fn sprint_rows(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.tasks.len()).filter(|&i| self.tasks[i].is_sprint).collect();
        v.sort_by_key(|&i| (self.tasks[i].calc.start, i));
        v
    }

    /// Tickets of a sprint (or the backlog), in board order.
    pub fn tickets_in(&self, sprint: Option<u32>) -> Vec<usize> {
        (0..self.tickets.len()).filter(|&k| self.tickets[k].sprint == sprint).collect()
    }

    pub fn sprint_stats(&self, sprint: Option<u32>) -> SprintStats {
        let mut s = SprintStats::default();
        for t in self.tickets.iter().filter(|t| t.sprint == sprint && t.kind.is_standard()) {
            let u = self.ticket_units(t);
            s.tickets += 1;
            s.total += u;
            if self.is_done(t) {
                s.done_tickets += 1;
                s.done += u;
            }
            if self.board.estimate == Estimate::StoryPoints && t.points.is_none() {
                s.unestimated += 1;
            }
        }
        s
    }

    /// Does this task's % complete come from the board? (A sprint with
    /// tickets, or a task an epic drives.)
    pub fn progress_from_board(&self, row: usize) -> bool {
        let t = &self.tasks[row];
        if t.calc.is_summary {
            return false;
        }
        if t.is_sprint {
            self.tickets.iter().any(|k| k.sprint == Some(t.uid) && k.kind.is_standard())
        } else {
            self.tickets.iter().any(|k| k.kind == IssueType::Epic && k.task == Some(t.uid))
        }
    }

    /// Set every sprint's % complete from its tickets, and every
    /// epic-driven task's from its epics' stories (called by the scheduler,
    /// so the Gantt always agrees with the board). One pass over the
    /// tickets, so it stays cheap for big boards on every reschedule.
    pub fn apply_sprint_progress(&mut self) {
        if self.tickets.is_empty() {
            return;
        }
        use std::collections::HashMap;
        let add = |s: &mut SprintStats, u: f64, done: bool, unest: bool| {
            s.tickets += 1;
            s.total += u;
            if done {
                s.done_tickets += 1;
                s.done += u;
            }
            if unest {
                s.unestimated += 1;
            }
        };
        let points = self.board.estimate == Estimate::StoryPoints;
        let mut by_sprint: HashMap<u32, SprintStats> = HashMap::new();
        let mut by_epic: HashMap<u32, SprintStats> = HashMap::new();
        for t in self.tickets.iter().filter(|t| t.kind.is_standard()) {
            let (u, done, unest) = (self.ticket_units(t), self.is_done(t), points && t.points.is_none());
            if let Some(sp) = t.sprint {
                add(by_sprint.entry(sp).or_default(), u, done, unest);
            }
            if let Some(e) = t.parent {
                add(by_epic.entry(e).or_default(), u, done, unest);
            }
        }
        let mut by_task: HashMap<u32, SprintStats> = HashMap::new();
        for e in self.tickets.iter().filter(|t| t.kind == IssueType::Epic) {
            let Some(task) = e.task else { continue };
            let acc = by_task.entry(task).or_default();
            if let Some(x) = by_epic.get(&e.id) {
                acc.tickets += x.tickets;
                acc.done_tickets += x.done_tickets;
                acc.total += x.total;
                acc.done += x.done;
                acc.unestimated += x.unestimated;
            }
        }
        for row in 0..self.tasks.len() {
            if self.is_summary(row) {
                continue;
            }
            let t = &self.tasks[row];
            let st = if t.is_sprint { by_sprint.get(&t.uid) } else { by_task.get(&t.uid) };
            match st {
                Some(st) => {
                    self.tasks[row].percent_complete = (st.percent() * 10.0).round() / 10.0;
                    self.tasks[row].board_driven = true;
                }
                // its tickets or epic went away: don't leave the old figure
                None if t.board_driven => {
                    self.tasks[row].percent_complete = 0.0;
                    self.tasks[row].board_driven = false;
                }
                None => {}
            }
        }
    }

    /// "FCG-12", or "#12" without a key prefix.
    pub fn ticket_key(&self, id: u32) -> String {
        let pre = self.board.key_prefix.trim();
        if pre.is_empty() {
            format!("#{id}")
        } else {
            format!("{pre}-{id}")
        }
    }

    /// Find a ticket by its key ("FCG-12", "#12" or "12").
    pub fn ticket_by_key(&self, key: &str) -> Option<u32> {
        let k = key.trim();
        // another project's key ("OTHER-1") isn't one of ours
        if let Some((pre, _)) = k.rsplit_once('-') {
            let ours = self.board.key_prefix.trim();
            if !pre.trim().is_empty() && !pre.trim().eq_ignore_ascii_case(ours) {
                return None;
            }
        }
        let num = k.rsplit(['-', '#']).next()?.trim();
        let id: u32 = num.parse().ok()?;
        self.ticket(id).map(|t| t.id)
    }

    pub fn epics(&self) -> Vec<usize> {
        (0..self.tickets.len()).filter(|&k| self.tickets[k].kind == IssueType::Epic).collect()
    }

    /// Direct children: an epic's stories/tasks/bugs, or a story's sub-tasks.
    pub fn ticket_children(&self, id: u32) -> Vec<usize> {
        (0..self.tickets.len()).filter(|&k| self.tickets[k].parent == Some(id)).collect()
    }

    /// The epic a ticket belongs to (a sub-task's through its parent).
    pub fn epic_of(&self, t: &Ticket) -> Option<u32> {
        match t.kind {
            IssueType::Epic => None,
            IssueType::Subtask => t.parent.and_then(|p| self.ticket(p)).and_then(|p| p.parent),
            _ => t.parent,
        }
        .filter(|&e| self.ticket(e).is_some_and(|x| x.kind == IssueType::Epic))
    }

    /// Progress of an epic over its stories/tasks/bugs, in the estimate unit.
    pub fn epic_stats(&self, epic: u32) -> SprintStats {
        let mut s = SprintStats::default();
        for t in self.tickets.iter().filter(|t| t.parent == Some(epic) && t.kind.is_standard()) {
            let u = self.ticket_units(t);
            s.tickets += 1;
            s.total += u;
            if self.is_done(t) {
                s.done_tickets += 1;
                s.done += u;
            }
            if self.board.estimate == Estimate::StoryPoints && t.points.is_none() {
                s.unestimated += 1;
            }
        }
        s
    }

    /// Every link touching a ticket, outward ones first.
    pub fn ticket_links(&self, id: u32) -> Vec<LinkView> {
        let mut v: Vec<LinkView> = vec![];
        if let Some(t) = self.ticket(id) {
            v.extend(t.links.iter().map(|l| LinkView { kind: l.kind, outward: true, other: l.to }));
        }
        for t in &self.tickets {
            for l in t.links.iter().filter(|l| l.to == id) {
                v.push(LinkView { kind: l.kind, outward: false, other: t.id });
            }
        }
        v
    }

    /// Tickets that block `id` and aren't done yet.
    pub fn blockers(&self, id: u32) -> Vec<u32> {
        self.tickets
            .iter()
            .filter(|t| !self.is_done(t) && t.links.iter().any(|l| l.kind == LinkKind::Blocks && l.to == id))
            .map(|t| t.id)
            .collect()
    }

    pub fn add_ticket_link(&mut self, from: u32, kind: LinkKind, to: u32) -> OpResult {
        if from == to {
            return Err("A ticket can't link to itself.".into());
        }
        if self.ticket(to).is_none() {
            return Err("No such ticket to link to.".into());
        }
        let exists = self.ticket_links(from).iter().any(|l| l.kind == kind && l.other == to && (l.outward || kind == LinkKind::Relates));
        if exists {
            return Err(format!("{} already {} {}.", self.ticket_key(from), kind.outward(), self.ticket_key(to)));
        }
        if kind == LinkKind::Blocks && self.blocks_transitively(to, from) {
            return Err(format!("{} already (indirectly) blocks {}; that would be a cycle.", self.ticket_key(to), self.ticket_key(from)));
        }
        let k = self.ticket_index(from).ok_or("No such ticket.")?;
        self.tickets[k].links.push(TicketLink { kind, to });
        Ok(())
    }

    /// Does `a` block `b`, directly or through other tickets?
    fn blocks_transitively(&self, a: u32, b: u32) -> bool {
        let mut stack = vec![a];
        let mut seen = std::collections::HashSet::new();
        while let Some(x) = stack.pop() {
            if x == b {
                return true;
            }
            if !seen.insert(x) {
                continue;
            }
            if let Some(t) = self.ticket(x) {
                stack.extend(t.links.iter().filter(|l| l.kind == LinkKind::Blocks).map(|l| l.to));
            }
        }
        false
    }

    /// Remove a link as seen from `id` (either direction).
    pub fn remove_ticket_link(&mut self, id: u32, view: &LinkView) -> OpResult {
        let (src, dst) = if view.outward { (id, view.other) } else { (view.other, id) };
        let k = self.ticket_index(src).ok_or("No such ticket.")?;
        let before = self.tickets[k].links.len();
        self.tickets[k].links.retain(|l| !(l.kind == view.kind && l.to == dst));
        if self.tickets[k].links.len() == before {
            return Err("That link doesn't exist.".into());
        }
        Ok(())
    }

    /// Change a ticket's type, keeping its parent if that still fits (an
    /// epic for a story/task/bug, a story/task/bug for a sub-task). Making
    /// a sub-task needs a parent: use `set_ticket_type`.
    pub fn set_ticket_kind(&mut self, id: u32, kind: IssueType) -> OpResult {
        let t = self.ticket(id).ok_or("No such ticket.")?;
        let fits = |p: u32| {
            self.ticket(p).is_some_and(|x| match kind {
                IssueType::Epic => false,
                IssueType::Subtask => x.kind.is_standard() && x.id != id,
                _ => x.kind == IssueType::Epic,
            })
        };
        let parent = t.parent.filter(|&p| fits(p));
        self.set_ticket_type(id, kind, parent)
    }

    /// Change a ticket's type and parent together, so the hierarchy is valid
    /// at every step: an epic has no parent and no sprint; a sub-task has a
    /// story/task/bug parent and shares its sprint; a story/task/bug may
    /// belong to an epic. A ticket with children can't become something
    /// that can't have them.
    pub fn set_ticket_type(&mut self, id: u32, kind: IssueType, parent: Option<u32>) -> OpResult {
        let k = self.ticket_index(id).ok_or("No such ticket.")?;
        let old = self.tickets[k].kind;
        let has_children = self.tickets.iter().any(|t| t.parent == Some(id));
        if has_children && old != kind && (kind == IssueType::Epic || kind == IssueType::Subtask || old == IssueType::Epic) {
            return Err(match old {
                IssueType::Epic => "This epic still has stories; move them out of it first.".into(),
                _ => format!("{} has sub-tasks; delete or move them first.", self.ticket_key(id)),
            });
        }
        if parent == Some(id) {
            return Err("A ticket can't be its own parent.".into());
        }
        let sprint = match (kind, parent) {
            (IssueType::Epic, Some(_)) => return Err("An epic has no parent.".into()),
            (IssueType::Epic, None) => None,
            (IssueType::Subtask, None) => return Err("A sub-task needs a parent story, task or bug.".into()),
            (IssueType::Subtask, Some(p)) => {
                let pt = self.ticket(p).filter(|_| p != id).ok_or("No such parent ticket.")?;
                if !pt.kind.is_standard() {
                    return Err("A sub-task's parent must be a story, task or bug.".into());
                }
                pt.sprint
            }
            (_, Some(p)) => {
                if self.ticket(p).map(|x| x.kind) != Some(IssueType::Epic) {
                    return Err("A story, task or bug can only belong to an epic.".into());
                }
                self.tickets[k].sprint
            }
            (_, None) => self.tickets[k].sprint,
        };
        let t = &mut self.tickets[k];
        t.kind = kind;
        t.parent = parent;
        t.sprint = sprint;
        if kind != IssueType::Epic {
            t.task = None;
        }
        Ok(())
    }

    /// Set a ticket's parent: an epic for a story/task/bug, a story/task/bug
    /// for a sub-task (which then follows its parent's sprint).
    pub fn set_ticket_parent(&mut self, id: u32, parent: Option<u32>) -> OpResult {
        let k = self.ticket_index(id).ok_or("No such ticket.")?;
        let kind = self.tickets[k].kind;
        if parent == Some(id) {
            return Err("A ticket can't be its own parent.".into());
        }
        match (kind, parent) {
            (IssueType::Epic, Some(_)) => return Err("An epic has no parent.".into()),
            (IssueType::Subtask, None) => return Err("A sub-task needs a parent story, task or bug.".into()),
            (_, None) => {}
            (IssueType::Subtask, Some(p)) => {
                let pt = self.ticket(p).ok_or("No such parent ticket.")?;
                if !pt.kind.is_standard() {
                    return Err("A sub-task's parent must be a story, task or bug.".into());
                }
                let s = pt.sprint;
                self.tickets[k].sprint = s;
            }
            (_, Some(p)) => {
                let pt = self.ticket(p).ok_or("No such epic.")?;
                if pt.kind != IssueType::Epic {
                    return Err("A story, task or bug can only belong to an epic.".into());
                }
            }
        }
        self.tickets[k].parent = parent;
        Ok(())
    }

    /// Let an epic drive a Gantt task's % complete (or stop, with `None`).
    pub fn set_epic_task(&mut self, epic: u32, task_uid: Option<u32>) -> OpResult {
        let k = self.ticket_index(epic).ok_or("No such ticket.")?;
        if self.tickets[k].kind != IssueType::Epic {
            return Err("Only an epic can drive a task on the schedule.".into());
        }
        if let Some(u) = task_uid {
            let row = self.index_of_uid(u).ok_or("No such task.")?;
            if self.is_summary(row) {
                return Err("An epic can't drive a summary task; pick one of its subtasks.".into());
            }
            if self.tasks[row].is_sprint {
                return Err("A sprint's progress already comes from its tickets.".into());
            }
        }
        self.tickets[k].task = task_uid;
        Ok(())
    }

    /// Add a ticket of a given type under `parent` (sub-tasks inherit the
    /// parent's sprint; epics never sit in a sprint).
    pub fn add_ticket_of(&mut self, title: &str, sprint: Option<u32>, kind: IssueType, parent: Option<u32>) -> OpResult<u32> {
        let sprint = if kind == IssueType::Epic { None } else { sprint };
        let id = self.add_ticket(title, sprint)?;
        if let Err(e) = self.set_ticket_type(id, kind, parent) {
            let _ = self.delete_ticket(id);
            return Err(e);
        }
        Ok(id)
    }

    /// Distinct labels in use, sorted.
    pub fn all_labels(&self) -> Vec<String> {
        let mut v: Vec<String> = self.tickets.iter().flat_map(|t| t.labels.iter().cloned()).collect();
        v.sort_by_key(|a| a.to_lowercase());
        v.dedup();
        v
    }

    /// New ticket at the bottom of the first column of `sprint`.
    pub fn add_ticket(&mut self, title: &str, sprint: Option<u32>) -> OpResult<u32> {
        let title = title.trim();
        if title.is_empty() {
            return Err("A ticket needs a title.".into());
        }
        if let Some(s) = sprint {
            if !self.tasks.iter().any(|t| t.uid == s && t.is_sprint) {
                return Err("That task isn't a sprint.".into());
            }
        }
        let id = self.next_ticket_id.max(self.tickets.iter().map(|t| t.id + 1).max().unwrap_or(1));
        self.next_ticket_id = id + 1;
        self.tickets.push(Ticket { id, title: title.to_string(), sprint, created: Some(Self::now()), ..Default::default() });
        Ok(id)
    }

    /// Delete a ticket. Its sub-tasks go with it (as in Jira); an epic's
    /// stories stay, without an epic; links to it are removed.
    pub fn delete_ticket(&mut self, id: u32) -> OpResult {
        let k = self.ticket_index(id).ok_or("No such ticket.")?;
        let t = self.tickets.remove(k);
        let mut dead = vec![id];
        if t.kind.is_standard() {
            dead.extend(self.tickets.iter().filter(|x| x.parent == Some(id) && x.kind == IssueType::Subtask).map(|x| x.id));
            self.tickets.retain(|x| !(x.parent == Some(id) && x.kind == IssueType::Subtask));
        }
        for x in self.tickets.iter_mut() {
            if x.parent.is_some_and(|p| dead.contains(&p)) {
                x.parent = None;
            }
            x.links.retain(|l| !dead.contains(&l.to));
        }
        for task in self.tasks.iter_mut() {
            task.board_deps.retain(|d| !dead.contains(&d.ticket));
        }
        Ok(())
    }

    /// Move a ticket to `column` of `sprint`, placed before ticket `before`
    /// (or at the end of that column). Entering the done column stamps
    /// `done_at` (for the burndown); leaving it clears it.
    pub fn move_ticket(&mut self, id: u32, sprint: Option<u32>, column: usize, before: Option<u32>) -> OpResult {
        let k = self.ticket_index(id).ok_or("No such ticket.")?;
        if let Some(s) = sprint {
            if !self.tasks.iter().any(|t| t.uid == s && t.is_sprint) {
                return Err("That task isn't a sprint.".into());
            }
        }
        let kind = self.tickets[k].kind;
        if kind == IssueType::Epic && sprint.is_some() {
            return Err("An epic spans sprints; plan its stories into the sprint instead.".into());
        }
        if kind == IssueType::Subtask {
            let ps = self.tickets[k].parent.and_then(|p| self.ticket(p)).map(|p| p.sprint);
            if ps.is_some_and(|ps| ps != sprint) {
                return Err("A sub-task goes with its parent's sprint; move the parent instead.".into());
            }
        }
        let column = column.min(self.board.done_column());
        let done_col = self.board.done_column();
        let old_sprint = self.tickets[k].sprint;
        if old_sprint != sprint && kind.is_standard() {
            // sub-tasks travel with their parent
            for x in self.tickets.iter_mut().filter(|x| x.parent == Some(id) && x.kind == IssueType::Subtask) {
                x.sprint = sprint;
            }
        }
        let k = self.ticket_index(id).ok_or("No such ticket.")?;
        let mut t = self.tickets.remove(k);
        let was_done = t.column >= done_col;
        if t.column != column {
            t.history.push((Self::now(), column));
            // a long-lived ticket keeps its last moves
            if t.history.len() > HISTORY_CAP {
                t.history.remove(0);
            }
        }
        t.sprint = sprint;
        t.column = column;
        if column >= done_col && !was_done {
            t.done_at = Some(Self::now());
        } else if column < done_col {
            t.done_at = None;
        }
        // position: before `before` if it's in the same place, else after the
        // last ticket already in that column/sprint
        let at = before
            .filter(|&b| b != id)
            .and_then(|b| self.ticket_index(b))
            .filter(|&bi| self.tickets[bi].sprint == sprint && self.tickets[bi].column == column)
            .unwrap_or_else(|| {
                self.tickets
                    .iter()
                    .rposition(|x| x.sprint == sprint && x.column == column)
                    .map(|p| p + 1)
                    .unwrap_or(self.tickets.len())
            });
        self.tickets.insert(at, t);
        Ok(())
    }

    /// Mark a task as a sprint (or not). Unmarking sends its tickets back to
    /// the backlog. Sprints are timeboxes: they become Fixed Duration.
    pub fn set_sprint(&mut self, row: usize, on: bool) -> OpResult {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        if on && self.is_summary(row) {
            return Err("A summary task can't be a sprint; make one of its subtasks a sprint.".into());
        }
        let uid = self.tasks[row].uid;
        self.tasks[row].is_sprint = on;
        if on {
            self.tasks[row].task_type = TaskType::FixedDuration;
            self.tasks[row].effort_driven = false;
            // a sprint's progress comes from its own tickets, not an epic
            for t in self.tickets.iter_mut().filter(|t| t.task == Some(uid)) {
                t.task = None;
            }
            for r in self.board.releases.iter_mut().filter(|r| r.task == Some(uid)) {
                r.task = None;
            }
        } else {
            for t in self.tickets.iter_mut().filter(|t| t.sprint == Some(uid)) {
                t.sprint = None;
            }
        }
        Ok(())
    }

    /// Add the next sprint: a Fixed Duration task of `sprint_days`, placed
    /// right after the last sprint (same outline level) and linked
    /// finish-to-start from it, or at the end of the project if it's the first.
    pub fn new_sprint(&mut self) -> usize {
        let last = (0..self.tasks.len()).filter(|&i| self.tasks[i].is_sprint).max_by_key(|&i| (self.tasks[i].calc.start, i));
        let n = self.tasks.iter().filter(|t| t.is_sprint).count() + 1;
        let mut name = format!("Sprint {n}");
        let mut k = n;
        while self.tasks.iter().any(|t| t.name == name) {
            k += 1;
            name = format!("Sprint {k}");
        }
        let (at, level) = match last {
            Some(l) => (self.subtree_end(l), self.tasks[l].outline_level),
            None => (self.tasks.len(), 1),
        };
        let row = self.insert_task(at, &name);
        self.tasks[row].outline_level = level;
        self.normalize_outline();
        let _ = self.set_field_text(row, crate::ops::Field::Duration, &format!("{}d", self.board.sprint_days.max(1)));
        let _ = self.set_sprint(row, true);
        if let Some(l) = last {
            let _ = self.add_link(l, row, LinkType::FS, Default::default());
        }
        row
    }

    /// Remaining work at each working-day boundary of the sprint.
    pub fn burndown(&self, sprint_uid: u32, now: NaiveDateTime) -> Option<Burndown> {
        let row = self.index_of_uid(sprint_uid)?;
        let t = &self.tasks[row];
        let (s, f) = (t.calc.start?, t.calc.finish?);
        let cal = &self.calendar;
        let mut days = vec![];
        let mut d = s.date();
        while d <= f.date() && days.len() < 400 {
            if cal.working_minutes_on(d) > 0 {
                days.push(d);
            }
            d = d.succ_opt()?;
        }
        if days.is_empty() {
            days.push(s.date());
        }
        let mine: Vec<&Ticket> = self.tickets.iter().filter(|k| k.sprint == Some(sprint_uid) && k.kind.is_standard()).collect();
        let total: f64 = mine.iter().map(|k| self.ticket_units(k)).sum();
        let n = days.len();
        let ideal = (0..=n).map(|k| total * (1.0 - k as f64 / n as f64)).collect();
        let done_by = |end: NaiveDateTime| -> f64 {
            mine.iter()
                .filter(|k| self.is_done(k) && k.done_at.is_some_and(|x| x < end))
                .map(|k| self.ticket_units(k))
                .sum()
        };
        let mut actual = vec![];
        // boundary 0 = start of the sprint's first day
        let first = days[0].and_hms_opt(0, 0, 0)?;
        actual.push(if now >= first { Some(total - done_by(first)) } else { None });
        for day in &days {
            let end = day.succ_opt()?.and_hms_opt(0, 0, 0)?;
            // today's boundary shows the live remaining work
            let shown = if now >= end { Some(end) } else if now >= day.and_hms_opt(0, 0, 0)? { Some(now) } else { None };
            actual.push(shown.map(|e| total - done_by(e.max(first) + chrono::Duration::seconds(1))));
        }
        Some(Burndown { days, total, ideal, actual })
    }

    /// Which column a ticket was in at `at`: from its recorded moves, else
    /// To Do from its creation and Done from when it was done. At or after
    /// `now`, the column it's in.
    pub fn column_at(&self, t: &Ticket, at: NaiveDateTime, now: NaiveDateTime) -> Option<usize> {
        if at >= now {
            return Some(t.column);
        }
        // it existed by the time anything happened to it, whatever its
        // Created date says (an import stamps the import time)
        let first_event = t.history.first().map(|h| h.0).into_iter().chain(t.done_at).min();
        let created = match (t.created, first_event) {
            (Some(c), Some(e)) => Some(c.min(e)),
            (c, _) => c,
        };
        if created.is_some_and(|c| c > at) {
            return None;
        }
        let mut col = 0;
        let mut seen = false;
        for &(when, c) in &t.history {
            if when <= at {
                col = c;
                seen = true;
            }
        }
        if !seen && self.is_done(t) && t.done_at.is_some_and(|d| d <= at) && t.history.is_empty() {
            col = self.board.done_column();
        }
        Some(col.min(self.board.done_column()))
    }

    /// Cumulative flow for a sprint's tickets (stories, tasks, bugs).
    pub fn cumulative_flow(&self, sprint_uid: u32, now: NaiveDateTime) -> Option<CumulativeFlow> {
        let b = self.burndown(sprint_uid, now)?;
        let mine: Vec<&Ticket> = self.tickets.iter().filter(|k| k.sprint == Some(sprint_uid) && k.kind.is_standard()).collect();
        let ncol = self.board.columns.len();
        let counts = b
            .days
            .iter()
            .map(|d| {
                let start = d.and_hms_opt(0, 0, 0).unwrap();
                if now < start {
                    return None;
                }
                let end = d.succ_opt().unwrap_or(*d).and_hms_opt(0, 0, 0).unwrap();
                let at = if now < end { now } else { end - chrono::Duration::seconds(1) };
                let mut c = vec![0; ncol];
                for t in &mine {
                    if let Some(k) = self.column_at(t, at, now) {
                        c[k.min(ncol - 1)] += 1;
                    }
                }
                Some(c)
            })
            .collect();
        Some(CumulativeFlow { days: b.days, columns: self.board.columns.clone(), counts })
    }

    /// Planned vs completed for every sprint, earliest first.
    pub fn velocity(&self) -> Vec<VelocityRow> {
        self.sprint_rows()
            .into_iter()
            .map(|row| {
                let t = &self.tasks[row];
                let st = self.sprint_stats(Some(t.uid));
                // a started sprint's plan is what was committed; a completed
                // one's result is what was done when it closed
                let m = self.board.sprint_meta(t.uid);
                VelocityRow {
                    sprint_uid: t.uid,
                    name: t.name.clone(),
                    start: t.calc.start,
                    finish: t.calc.finish,
                    planned: m.and_then(|m| m.committed).unwrap_or(st.total),
                    completed: m.and_then(|m| m.completed_value).unwrap_or(st.done),
                    tickets: m.filter(|m| m.started.is_some()).map(|m| m.committed_tickets).unwrap_or(st.tickets),
                    done_tickets: m.filter(|m| m.completed.is_some()).map(|m| m.completed_tickets).unwrap_or(st.done_tickets),
                }
            })
            .collect()
    }

    /// Average completed per sprint over the last (up to) three sprints that
    /// have ended by `now`; `None` before any sprint has ended.
    pub fn average_velocity(&self, now: NaiveDateTime) -> Option<f64> {
        let ended: Vec<f64> = self.velocity().into_iter().filter(|v| v.finish.is_some_and(|f| f <= now)).map(|v| v.completed).collect();
        if ended.is_empty() {
            return None;
        }
        let last: Vec<f64> = ended.iter().rev().take(3).copied().collect();
        Some(last.iter().sum::<f64>() / last.len() as f64)
    }

    /// The sprint running at `now`, else the next one, else the last one.
    pub fn current_sprint(&self, now: NaiveDateTime) -> Option<u32> {
        let rows = self.sprint_rows();
        let t = |i: usize| &self.tasks[i];
        if let Some(&i) = rows.iter().find(|&&i| t(i).calc.start.is_some_and(|s| s <= now) && t(i).calc.finish.is_some_and(|f| now <= f)) {
            return Some(t(i).uid);
        }
        // between sprints (a weekend, say): the one that ended stays current
        // until it's completed or the next one starts, as Jira's active sprint
        let last_started = rows.iter().filter(|&&i| t(i).calc.start.is_some_and(|s| s <= now)).max_by_key(|&&i| t(i).calc.start);
        if let Some(&i) = last_started {
            let completed = self.board.sprint_meta(t(i).uid).is_some_and(|m| m.completed.is_some());
            if !completed {
                return Some(t(i).uid);
            }
        }
        rows.iter()
            .filter(|&&i| t(i).calc.start.is_some_and(|s| s > now))
            .min_by_key(|&&i| t(i).calc.start)
            .or(rows.last())
            .map(|&i| t(i).uid)
    }

    /// Make a loaded board consistent (files can be hand-edited or come from
    /// older versions): unique ids, valid parents and sprints, sub-tasks in
    /// their parent's sprint, no dangling or self links, columns in range.
    /// Returns how many things were fixed.
    pub fn repair_board(&mut self) -> usize {
        let mut fixed = 0;
        let mut seen = std::collections::HashSet::new();
        let before = self.tickets.len();
        self.tickets.retain(|t| seen.insert(t.id));
        fixed += before - self.tickets.len();
        let done = self.board.done_column();
        let sprints: std::collections::HashSet<u32> = self.tasks.iter().filter(|t| t.is_sprint).map(|t| t.uid).collect();
        let kinds: std::collections::HashMap<u32, IssueType> = self.tickets.iter().map(|t| (t.id, t.kind)).collect();
        for t in self.tickets.iter_mut() {
            if t.column > done {
                t.column = done;
                fixed += 1;
            }
            if t.sprint.is_some_and(|s| !sprints.contains(&s)) {
                t.sprint = None;
                fixed += 1;
            }
            let ok_parent = match (t.kind, t.parent) {
                (_, None) => true,
                (_, Some(p)) if p == t.id => false,
                (IssueType::Epic, Some(_)) => false,
                (IssueType::Subtask, Some(p)) => kinds.get(&p).is_some_and(|k| k.is_standard()),
                (_, Some(p)) => kinds.get(&p) == Some(&IssueType::Epic),
            };
            if !ok_parent {
                t.parent = None;
                fixed += 1;
            }
            if t.kind == IssueType::Subtask && t.parent.is_none() {
                t.kind = IssueType::Task;
                fixed += 1;
            }
            if t.kind == IssueType::Epic && t.sprint.is_some() {
                t.sprint = None;
                fixed += 1;
            }
            if t.kind != IssueType::Epic && t.task.is_some() {
                t.task = None;
                fixed += 1;
            }
            let n = t.links.len();
            let id = t.id;
            let mut seen_links = std::collections::HashSet::new();
            t.links.retain(|l| l.to != id && kinds.contains_key(&l.to) && seen_links.insert((l.kind, l.to)));
            fixed += n - t.links.len();
        }
        // sub-tasks share their parent's sprint
        let sprint_of: std::collections::HashMap<u32, Option<u32>> = self.tickets.iter().map(|t| (t.id, t.sprint)).collect();
        for t in self.tickets.iter_mut().filter(|t| t.kind == IssueType::Subtask) {
            let s = t.parent.and_then(|p| sprint_of.get(&p).copied()).flatten();
            if t.sprint != s {
                t.sprint = s;
                fixed += 1;
            }
        }
        // an epic driving a missing, summary or sprint task stops driving it
        for k in 0..self.tickets.len() {
            if let Some(u) = self.tickets[k].task {
                let ok = self.index_of_uid(u).is_some_and(|r| !self.is_summary(r) && !self.tasks[r].is_sprint);
                if !ok {
                    self.tickets[k].task = None;
                    fixed += 1;
                }
            }
        }
        if self.board.columns.len() < 2 {
            self.board.columns = BoardSettings::default().columns;
            fixed += 1;
        }
        let n_meta = self.board.sprints.len();
        let sprint_uids: std::collections::HashSet<u32> = self.tasks.iter().filter(|t| t.is_sprint).map(|t| t.uid).collect();
        self.board.sprints.retain(|m| sprint_uids.contains(&m.uid));
        fixed += n_meta - self.board.sprints.len();
        // dependencies on things that no longer exist
        let ids: std::collections::HashSet<u32> = self.tickets.iter().map(|t| t.id).collect();
        let uids: std::collections::HashSet<u32> = self.tasks.iter().map(|t| t.uid).collect();
        for task in self.tasks.iter_mut() {
            let n = task.board_deps.len();
            task.board_deps.retain(|d| ids.contains(&d.ticket));
            fixed += n - task.board_deps.len();
        }
        for t in self.tickets.iter_mut() {
            let n = t.after_tasks.len();
            t.after_tasks.retain(|u| uids.contains(u));
            fixed += n - t.after_tasks.len();
        }
        // releases: stories only, of releases that exist, tied to real tasks
        let rels: std::collections::HashSet<u32> = self.board.releases.iter().map(|r| r.id).collect();
        for t in self.tickets.iter_mut() {
            if t.release.is_some_and(|r| !rels.contains(&r) || !t.kind.is_standard()) {
                t.release = None;
                fixed += 1;
            }
        }
        let mut tied = std::collections::HashSet::new();
        for k in 0..self.board.releases.len() {
            if let Some(u) = self.board.releases[k].task {
                let ok = self.index_of_uid(u).is_some_and(|r| !self.is_summary(r) && !self.tasks[r].is_sprint) && tied.insert(u);
                if !ok {
                    self.board.releases[k].task = None;
                    fixed += 1;
                }
            }
        }
        let mut names = std::collections::HashSet::new();
        let mut seen_ids = std::collections::HashSet::new();
        let n_rel = self.board.releases.len();
        self.board.releases.retain(|r| seen_ids.insert(r.id));
        fixed += n_rel - self.board.releases.len();
        for r in self.board.releases.iter_mut() {
            let mut name = r.name.trim().to_string();
            if name.is_empty() {
                name = format!("Release {}", r.id);
            }
            let base = name.clone();
            let mut n = 2;
            while !names.insert(name.to_lowercase()) {
                name = format!("{base} ({n})");
                n += 1;
            }
            if name != r.name {
                r.name = name;
                fixed += 1;
            }
        }
        let max_rel = self.board.releases.iter().map(|r| r.id + 1).max().unwrap_or(1);
        if self.board.next_release_id < max_rel {
            self.board.next_release_id = max_rel;
        }
        let max = self.tickets.iter().map(|t| t.id + 1).max().unwrap_or(1);
        if self.next_ticket_id < max {
            self.next_ticket_id = max;
        }
        fixed
    }


    // ------------------------------------------------ board ↔ Gantt dependencies

    /// What a task waiting on board item `ticket` waits for. A story (or
    /// task, bug, sub-task) that's done: the moment it was done. Not done
    /// but planned: the end of its sprint (the team's commitment). Not
    /// planned: nothing to schedule against — it's reported instead. An
    /// epic: all of its stories.
    pub fn resolve_board_dep(&self, ticket: u32) -> Option<DepTarget> {
        let t = self.ticket(ticket)?;
        let mut out = DepTarget::default();
        let add = |x: &Ticket, out: &mut DepTarget| self.add_dep_item(x, out);
        if t.kind == IssueType::Epic {
            let kids: Vec<&Ticket> = self.tickets.iter().filter(|x| x.parent == Some(ticket) && x.kind.is_standard()).collect();
            if kids.is_empty() {
                add(t, &mut out);
                // an epic with no stories has no sprint to wait for
                if !self.is_done(t) {
                    out.sprints.clear();
                    out.by_sprint.clear();
                }
            }
            for k in kids {
                add(k, &mut out);
            }
        } else {
            add(t, &mut out);
        }
        Some(out)
    }

    /// Add one board item to what a dependency waits for.
    pub(crate) fn add_dep_item(&self, x: &Ticket, out: &mut DepTarget) {
        if self.is_done(x) {
            if let Some(d) = x.done_at {
                out.done.push((d, x.sprint, x.id));
            }
        } else {
            out.waiting.push(x.id);
            match x.sprint {
                Some(sp) if self.task_by_uid(sp).is_some_and(|s| s.is_sprint) => {
                    if !out.sprints.contains(&sp) {
                        out.sprints.push(sp);
                        out.by_sprint.push((sp, x.id));
                    }
                }
                _ => out.unplanned.push(x.id),
            }
        }
    }

    /// A ticket and everything waiting on it means waiting on: itself, its
    /// epic, its parent story, or (for an epic) its stories.
    fn dep_family(&self, ticket: u32) -> Vec<u32> {
        let mut v = vec![ticket];
        if let Some(t) = self.ticket(ticket) {
            if let Some(e) = self.epic_of(t) {
                v.push(e);
            }
            if t.kind == IssueType::Subtask {
                v.extend(t.parent);
            }
            if t.kind == IssueType::Epic {
                v.extend(self.tickets.iter().filter(|x| x.parent == Some(ticket) && x.kind.is_standard()).map(|x| x.id));
            }
        }
        v
    }

    /// Add (or update the lag of) a task's dependency on a board item.
    pub fn add_board_dep(&mut self, row: usize, ticket: u32, lag: crate::duration::Lag) -> OpResult {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        if self.ticket(ticket).is_none() {
            return Err(format!("{} doesn't exist on the board.", self.ticket_key(ticket)));
        }
        let uid0 = self.tasks[row].uid;
        if self.dep_family(ticket).iter().any(|&x| self.ticket(x).is_some_and(|t| t.after_tasks.contains(&uid0))) {
            return Err(format!("{} (or its epic or stories) already waits on \"{}\"; they can't wait on each other.", self.ticket_key(ticket), self.tasks[row].name));
        }
        let uid = self.tasks[row].uid;
        if self.ticket(ticket).is_some_and(|t| t.sprint == Some(uid)) && self.tasks[row].is_sprint {
            return Err("A sprint can't wait on one of its own tickets.".into());
        }
        let deps = &mut self.tasks[row].board_deps;
        match deps.iter_mut().find(|d| d.ticket == ticket) {
            Some(d) => d.lag = lag,
            None => deps.push(crate::model::BoardDep { ticket, lag }),
        }
        Ok(())
    }

    pub fn remove_board_dep(&mut self, row: usize, ticket: u32) -> OpResult {
        let deps = &mut self.tasks.get_mut(row).ok_or("No such task.")?.board_deps;
        let n = deps.len();
        deps.retain(|d| d.ticket != ticket);
        if deps.len() == n {
            return Err("The task doesn't wait on that item.".into());
        }
        Ok(())
    }

    /// Rows of the tasks that wait on a ticket (directly, or on its epic).
    pub fn tasks_waiting_on(&self, ticket: u32) -> Vec<usize> {
        let t = self.ticket(ticket);
        let epic = t.and_then(|t| self.epic_of(t));
        // a sub-task is part of its story: waiting on the story waits on it
        let parent = t.filter(|t| t.kind == IssueType::Subtask).and_then(|t| t.parent);
        (0..self.tasks.len())
            .filter(|&r| self.tasks[r].board_deps.iter().any(|d| d.ticket == ticket || Some(d.ticket) == epic || Some(d.ticket) == parent))
            .collect()
    }

    /// A ticket can't start before a Gantt task is finished.
    pub fn add_ticket_task_dep(&mut self, ticket: u32, task_uid: u32) -> OpResult {
        let row = self.index_of_uid(task_uid).ok_or("No such task.")?;
        let k = self.ticket_index(ticket).ok_or("No such ticket.")?;
        if self.tasks[row].is_sprint {
            return Err("Plan the ticket into a later sprint instead of waiting on a sprint.".into());
        }
        let fam = self.dep_family(ticket);
        if self.tasks[row].board_deps.iter().any(|d| fam.contains(&d.ticket)) || self.tasks_waiting_on(ticket).contains(&row) {
            return Err(format!("\"{}\" already waits on {}; they can't wait on each other.", self.tasks[row].name, self.ticket_key(ticket)));
        }
        let t = &mut self.tickets[k];
        if !t.after_tasks.contains(&task_uid) {
            t.after_tasks.push(task_uid);
        }
        Ok(())
    }

    pub fn remove_ticket_task_dep(&mut self, ticket: u32, task_uid: u32) -> OpResult {
        let k = self.ticket_index(ticket).ok_or("No such ticket.")?;
        let n = self.tickets[k].after_tasks.len();
        self.tickets[k].after_tasks.retain(|&u| u != task_uid);
        if self.tickets[k].after_tasks.len() == n {
            return Err("The ticket doesn't wait on that task.".into());
        }
        Ok(())
    }

    /// Gantt tasks a ticket waits on that aren't finished (uids).
    pub fn waiting_on_tasks(&self, ticket: u32) -> Vec<u32> {
        let Some(t) = self.ticket(ticket) else { return vec![] };
        // a sub-task also waits on what its parent waits on
        let parent = t.parent.and_then(|p| self.ticket(p)).filter(|_| t.kind == IssueType::Subtask);
        t.after_tasks
            .iter()
            .chain(parent.map(|p| p.after_tasks.iter()).into_iter().flatten())
            .copied()
            // an inactive task won't be done; it doesn't hold anyone up
            .filter(|&u| self.task_by_uid(u).is_some_and(|x| x.active && x.calc.percent_complete < 100.0))
            .collect()
    }

    /// Planning problems with a ticket's dependencies (Jira Plans calls
    /// them dependency conflicts): it's in a sprint that starts before a
    /// task it waits on finishes, or before a ticket that blocks it is
    /// planned to be done.
    pub fn dependency_conflicts(&self, ticket: u32) -> Vec<String> {
        let Some(t) = self.ticket(ticket) else { return vec![] };
        if self.is_done(t) || t.kind == IssueType::Epic {
            return vec![];
        }
        let mut out = vec![];
        let sprint = t.sprint.and_then(|u| self.task_by_uid(u));
        let sprint_start = sprint.and_then(|s| s.calc.start);
        for u in self.waiting_on_tasks(ticket) {
            let Some(task) = self.task_by_uid(u) else { continue };
            if let (Some(ss), Some(tf)) = (sprint_start, task.calc.finish) {
                if ss < tf {
                    out.push(format!("{} starts {} but \"{}\" finishes {}", sprint.unwrap().name, crate::model::fmt_date(ss), task.name, crate::model::fmt_date(tf)));
                }
            }
        }
        for b in self.blockers(ticket) {
            let Some(bt) = self.ticket(b) else { continue };
            let key = self.ticket_key(b);
            match (t.sprint, bt.sprint) {
                (Some(_), None) => out.push(format!("{key} blocks it but isn't planned into a sprint")),
                (Some(_), Some(bs)) => {
                    let bsp = self.task_by_uid(bs);
                    if let (Some(ss), Some(bss)) = (sprint_start, bsp.and_then(|x| x.calc.start)) {
                        if bss > ss {
                            out.push(format!("{key} blocks it but is planned later ({})", bsp.unwrap().name));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The first sprint (from the current one on) that starts after
    /// everything a ticket waits on: unfinished tasks it waits on, and the
    /// sprints of tickets that block it.
    pub fn first_fitting_sprint(&self, ticket: u32) -> Option<u32> {
        let mut after: Option<NaiveDateTime> = None;
        let mut bump = |d: Option<NaiveDateTime>| {
            if let Some(d) = d {
                after = Some(after.map_or(d, |a: NaiveDateTime| a.max(d)));
            }
        };
        for u in self.waiting_on_tasks(ticket) {
            bump(self.task_by_uid(u).and_then(|t| t.calc.finish));
        }
        for b in self.blockers(ticket) {
            match self.ticket(b).and_then(|x| x.sprint) {
                Some(s) => bump(self.task_by_uid(s).and_then(|s| s.calc.finish)),
                // a blocker that isn't planned: no sprint is known to fit
                None => return None,
            }
        }
        // never suggest a sprint that's over (completed, or already ended)
        let now = self.status_or_now();
        self.sprint_rows()
            .into_iter()
            .map(|r| &self.tasks[r])
            .filter(|s| self.board.sprint_meta(s.uid).is_none_or(|m| m.completed.is_none()))
            .filter(|s| s.calc.finish.is_some_and(|f| f > now))
            .find(|s| match (after, s.calc.start) {
                (Some(a), Some(st)) => st >= a,
                (None, _) => true,
                _ => false,
            })
            .map(|s| s.uid)
    }

    // ------------------------------------------------ sprint lifecycle

    /// Board ▸ Start Sprint: record the goal and the committed scope.
    pub fn start_sprint(&mut self, uid: u32, goal: &str, now: NaiveDateTime) -> OpResult {
        if !self.task_by_uid(uid).is_some_and(|t| t.is_sprint) {
            return Err("That task isn't a sprint.".into());
        }
        if self.board.sprint_meta(uid).is_some_and(|m| m.started.is_some()) {
            return Err("This sprint has already been started.".into());
        }
        let st = self.sprint_stats(Some(uid));
        let m = self.board.sprint_meta_mut(uid);
        m.goal = goal.trim().to_string();
        m.started = Some(now);
        m.committed = Some(st.total);
        m.committed_tickets = st.tickets;
        Ok(())
    }

    pub fn set_sprint_goal(&mut self, uid: u32, goal: &str) -> OpResult {
        if !self.task_by_uid(uid).is_some_and(|t| t.is_sprint) {
            return Err("That task isn't a sprint.".into());
        }
        self.board.sprint_meta_mut(uid).goal = goal.trim().to_string();
        Ok(())
    }

    /// The sprint after `uid` on the schedule, if any.
    pub fn next_sprint_after(&self, uid: u32) -> Option<u32> {
        let rows = self.sprint_rows();
        let i = rows.iter().position(|&r| self.tasks[r].uid == uid)?;
        rows.get(i + 1).map(|&r| self.tasks[r].uid)
    }

    /// Board ▸ Complete Sprint: freeze what was done, and move unfinished
    /// tickets (with their sub-tasks) to `dest` (another sprint) or the
    /// backlog. Returns how many tickets moved.
    pub fn complete_sprint(&mut self, uid: u32, dest: Option<u32>, now: NaiveDateTime) -> OpResult<usize> {
        if !self.task_by_uid(uid).is_some_and(|t| t.is_sprint) {
            return Err("That task isn't a sprint.".into());
        }
        if self.board.sprint_meta(uid).is_some_and(|m| m.completed.is_some()) {
            return Err("This sprint is already complete.".into());
        }
        if dest == Some(uid) {
            return Err("Move unfinished tickets to another sprint or the backlog.".into());
        }
        if let Some(d) = dest {
            if !self.task_by_uid(d).is_some_and(|t| t.is_sprint) {
                return Err("The destination isn't a sprint.".into());
            }
            if self.board.sprint_meta(d).is_some_and(|m| m.completed.is_some()) {
                return Err("The destination sprint is already complete.".into());
            }
        }
        // like Jira: a done story can't leave unfinished sub-tasks behind
        if let Some(sub) = self.tickets.iter().find(|t| {
            t.kind == IssueType::Subtask && t.sprint == Some(uid) && !self.is_done(t) && t.parent.and_then(|p| self.ticket(p)).is_some_and(|p| self.is_done(p))
        }) {
            return Err(format!("{} is done but its sub-task {} isn't. Finish the sub-task, or move the story out of Done, first.", self.ticket_key(sub.parent.unwrap()), self.ticket_key(sub.id)));
        }
        let st = self.sprint_stats(Some(uid));
        if self.board.sprint_meta(uid).is_none_or(|m| m.started.is_none()) {
            // completing an unstarted sprint: its scope now is the commitment
            let m = self.board.sprint_meta_mut(uid);
            m.started = Some(now);
            m.committed = Some(st.total);
            m.committed_tickets = st.tickets;
        }
        let open: Vec<u32> = self.tickets.iter().filter(|t| t.sprint == Some(uid) && t.kind.is_standard() && !self.is_done(t)).map(|t| t.id).collect();
        for &id in &open {
            let col = self.ticket(id).map(|t| t.column).unwrap_or(0);
            self.move_ticket(id, dest, if dest.is_some() { col } else { 0 }, None)?;
        }
        let m = self.board.sprint_meta_mut(uid);
        m.completed = Some(now);
        m.completed_value = Some(st.done);
        m.completed_tickets = st.done_tickets;
        m.carried_over = open.len();
        Ok(open.len())
    }

    /// Reopen a completed sprint (undoes the completion record, not the moves).
    pub fn reopen_sprint(&mut self, uid: u32) -> OpResult {
        let m = self.board.sprints.iter_mut().find(|m| m.uid == uid).ok_or("This sprint hasn't been completed.")?;
        if m.completed.is_none() {
            return Err("This sprint hasn't been completed.".into());
        }
        m.completed = None;
        m.completed_value = None;
        m.completed_tickets = 0;
        m.carried_over = 0;
        Ok(())
    }

    /// Board ▸ Columns: rename, add, remove and reorder the board's
    /// columns. Each entry is (name, the old column it was, WIP limit).
    /// Tickets in a removed column move to the nearest kept column before
    /// it; the last column is always Done, and done stamps follow it.
    pub fn set_board_columns(&mut self, cols: &[(String, Option<usize>, Option<u32>)], now: NaiveDateTime) -> OpResult {
        if cols.len() < 2 {
            return Err("A board needs at least two columns (one for work, one for Done).".into());
        }
        if cols.iter().any(|c| c.0.trim().is_empty()) {
            return Err("Every column needs a name.".into());
        }
        let old_n = self.board.columns.len().max(1);
        let old_done = self.board.done_column();
        // old index → new index
        let mut map: Vec<Option<usize>> = vec![None; old_n];
        for (new_i, c) in cols.iter().enumerate() {
            if let Some(o) = c.1.filter(|&o| o < old_n) {
                map[o] = Some(new_i);
            }
        }
        let resolve = |o: usize| -> usize { (0..=o.min(old_n - 1)).rev().find_map(|k| map.get(k).copied().flatten()).unwrap_or(0) };
        let new_done = cols.len() - 1;
        for t in self.tickets.iter_mut() {
            let was_done = t.column >= old_done;
            t.column = resolve(t.column.min(old_n.saturating_sub(1)));
            for h in t.history.iter_mut() {
                h.1 = resolve(h.1.min(old_n.saturating_sub(1)));
            }
            let now_done = t.column >= new_done;
            if now_done && !was_done {
                t.done_at = t.done_at.or(Some(now));
            } else if !now_done {
                t.done_at = None;
            }
        }
        self.board.columns = cols.iter().map(|c| c.0.trim().to_string()).collect();
        self.board.wip = cols.iter().map(|c| c.2.filter(|&n| n > 0)).collect();
        Ok(())
    }

}
