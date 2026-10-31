//! Editing operations, written to behave like the equivalent commands in
//! Microsoft Project. Every operation leaves the project unscheduled; the
//! caller (UI or test) runs `schedule()` afterwards.

use crate::duration::{Duration, Lag};
use crate::model::{
    parse_date, ConstraintType, Link, LinkType, Project, Task, TaskMode,
};
use crate::schedule::{check_links, validate_link, Structure};
use chrono::NaiveDateTime;

pub type OpResult<T = ()> = Result<T, String>;

/// Text columns of the default Gantt "Entry" table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Duration,
    Start,
    Finish,
    Predecessors,
    PercentComplete,
    Mode,
    /// Resource Names in Project's notation, e.g. `Alice,Bob[50%]`.
    ResourceNames,
    /// Total work of the task's work assignments ("32h", "4d").
    Work,
}

impl Project {
    pub(crate) fn new_task(&mut self, name: &str) -> Task {
        let uid = self.alloc_uid();
        let mode = if self.settings.new_tasks_auto {
            TaskMode::Auto
        } else {
            TaskMode::Manual
        };
        let mut t = Task::new(uid, name, mode, &self.settings.durations);
        t.task_type = self.settings.default_task_type;
        t.effort_driven = self.settings.new_tasks_effort_driven;
        t
    }

    /// Insert a new task above row `at` (Project's Insert key). The new task
    /// takes the outline level of the row it is inserted above. Returns the
    /// new row index.
    pub fn insert_task(&mut self, at: usize, name: &str) -> usize {
        let at = at.min(self.tasks.len());
        let mut t = self.new_task(name);
        t.outline_level = if at < self.tasks.len() {
            self.tasks[at].outline_level
        } else if let Some(last) = self.tasks.last() {
            // Appending after a summary's last child keeps the child's level.
            last.outline_level
        } else {
            1
        };
        if t.mode == TaskMode::Manual {
            t.manual_start = Some(self.start);
        }
        self.tasks.insert(at, t);
        self.normalize_outline();
        at
    }

    pub fn append_task(&mut self, name: &str) -> usize {
        let at = self.tasks.len();
        self.insert_task(at, name)
    }

    /// Delete rows (and the subtrees of any summary rows). Links to deleted
    /// tasks are removed. Successors are *not* re-linked to predecessors —
    /// Project only does that with the "autolink" option, which it applies to
    /// inserts and deletes inside a chain; we implement the common case below.
    pub fn delete_tasks(&mut self, rows: &[usize]) {
        let mut doomed = vec![false; self.tasks.len()];
        for &r in rows {
            if r < self.tasks.len() {
                let end = self.subtree_end(r);
                for d in doomed.iter_mut().take(end).skip(r) {
                    *d = true;
                }
            }
        }
        let dead: std::collections::HashSet<u32> = self
            .tasks
            .iter()
            .zip(&doomed)
            .filter(|(_, d)| **d)
            .map(|(t, _)| t.uid)
            .collect();
        let mut i = 0;
        self.tasks.retain(|_| {
            let keep = !doomed[i];
            i += 1;
            keep
        });
        for t in self.tasks.iter_mut() {
            t.predecessors.retain(|l| !dead.contains(&l.pred));
        }
        self.assignments.retain(|a| !dead.contains(&a.task));
        // a deleted sprint's tickets go back to the backlog
        for t in self.tickets.iter_mut() {
            t.after_tasks.retain(|u| !dead.contains(u));
            if t.sprint.is_some_and(|s| dead.contains(&s)) {
                t.sprint = None;
            }
            if t.task.is_some_and(|s| dead.contains(&s)) {
                t.task = None;
            }
        }
        for r in self.board.releases.iter_mut() {
            if r.task.is_some_and(|u| dead.contains(&u)) {
                r.task = None;
            }
        }
        self.normalize_outline();
    }

    /// Indent (demote) rows and their subtrees. A row can only be indented
    /// if the row above it is at the same level or deeper.
    pub fn indent(&mut self, rows: &[usize]) -> OpResult<usize> {
        let mut rows: Vec<usize> = rows.to_vec();
        rows.sort();
        rows.dedup();
        let mut changed = 0;
        let mut done_until = 0usize;
        for r in rows {
            if r < done_until || r == 0 || r >= self.tasks.len() {
                continue;
            }
            if self.tasks[r - 1].outline_level < self.tasks[r].outline_level {
                continue;
            }
            let end = self.subtree_end(r);
            for t in &mut self.tasks[r..end] {
                t.outline_level += 1;
            }
            done_until = end;
            changed += 1;
        }
        let removed = self.sanitize_links();
        if changed == 0 {
            return Err("Can't indent: the task above must be at the same outline level.".into());
        }
        let _ = removed;
        Ok(changed)
    }

    /// Outdent (promote) rows and their subtrees.
    pub fn outdent(&mut self, rows: &[usize]) -> OpResult<usize> {
        let mut rows: Vec<usize> = rows.to_vec();
        rows.sort();
        rows.dedup();
        let mut changed = 0;
        let mut done_until = 0usize;
        for r in rows {
            if r < done_until || r >= self.tasks.len() || self.tasks[r].outline_level <= 1 {
                continue;
            }
            let end = self.subtree_end(r);
            for t in &mut self.tasks[r..end] {
                t.outline_level -= 1;
            }
            done_until = end;
            changed += 1;
        }
        self.normalize_outline();
        self.sanitize_links();
        if changed == 0 {
            return Err("Can't outdent a top-level task.".into());
        }
        Ok(changed)
    }

    /// Remove links made invalid by an outline change (e.g. a task indented
    /// beneath its own predecessor). Returns how many were removed.
    pub fn sanitize_links(&mut self) -> usize {
        let st = Structure::build(self);
        let idx: std::collections::HashMap<u32, usize> =
            self.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        let mut removed = 0;
        for succ in 0..self.tasks.len() {
            let before = self.tasks[succ].predecessors.len();
            let keep: Vec<Link> = self.tasks[succ]
                .predecessors
                .iter()
                .filter(|l| match idx.get(&l.pred) {
                    Some(&p) => validate_link(&st, p, succ, l.kind).is_ok(),
                    None => false,
                })
                .copied()
                .collect();
            removed += before - keep.len();
            self.tasks[succ].predecessors = keep;
        }
        removed
    }

    /// Move a row (with its subtree) up past the previous sibling block.
    pub fn move_up(&mut self, row: usize) -> OpResult<usize> {
        if row == 0 || row >= self.tasks.len() {
            return Err("Can't move further up.".into());
        }
        let lvl = self.tasks[row].outline_level;
        let end = self.subtree_end(row);
        // find previous sibling start
        let mut j = row - 1;
        while self.tasks[j].outline_level > lvl {
            if j == 0 {
                return Err("Can't move further up.".into());
            }
            j -= 1;
        }
        if self.tasks[j].outline_level < lvl {
            return Err("Can't move a task above its summary task.".into());
        }
        let block: Vec<Task> = self.tasks.drain(row..end).collect();
        for (k, t) in block.into_iter().enumerate() {
            self.tasks.insert(j + k, t);
        }
        Ok(j)
    }

    pub fn move_down(&mut self, row: usize) -> OpResult<usize> {
        if row >= self.tasks.len() {
            return Err("Can't move further down.".into());
        }
        let lvl = self.tasks[row].outline_level;
        let end = self.subtree_end(row);
        if end >= self.tasks.len() || self.tasks[end].outline_level != lvl {
            return Err("Can't move further down.".into());
        }
        let next_end = self.subtree_end(end);
        let block: Vec<Task> = self.tasks.drain(end..next_end).collect();
        let n = block.len();
        for (k, t) in block.into_iter().enumerate() {
            self.tasks.insert(row + k, t);
        }
        Ok(row + n)
    }

    /// Add one dependency, rejecting circular or invalid links.
    pub fn add_link(&mut self, pred_row: usize, succ_row: usize, kind: LinkType, lag: Lag) -> OpResult {
        if pred_row >= self.tasks.len() || succ_row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        let st = Structure::build(self);
        validate_link(&st, pred_row, succ_row, kind)?;
        let loops_before = self.board_loops();
        let pred_uid = self.tasks[pred_row].uid;
        let backup = self.tasks[succ_row].predecessors.clone();
        let preds = &mut self.tasks[succ_row].predecessors;
        if let Some(existing) = preds.iter_mut().find(|l| l.pred == pred_uid) {
            existing.kind = kind;
            existing.lag = lag;
        } else {
            preds.push(Link {
                pred: pred_uid,
                kind,
                lag,
            });
        }
        if let Err(e) = check_links(self) {
            self.tasks[succ_row].predecessors = backup;
            return Err(e);
        }
        // the link would close a loop through a board dependency
        if let Some(n) = self.board_loops().into_iter().find(|n| !loops_before.contains(n)) {
            self.tasks[succ_row].predecessors = backup;
            return Err(format!("That link would make a loop with a board dependency. {n}"));
        }
        Ok(())
    }

    pub fn remove_link(&mut self, pred_row: usize, succ_row: usize) {
        if pred_row < self.tasks.len() && succ_row < self.tasks.len() {
            let pu = self.tasks[pred_row].uid;
            self.tasks[succ_row].predecessors.retain(|l| l.pred != pu);
        }
    }

    /// Ctrl+F2: link the selected tasks finish-to-start in selection order.
    pub fn link_tasks(&mut self, rows: &[usize]) -> OpResult<usize> {
        if rows.len() < 2 {
            return Err("Select two or more tasks to link.".into());
        }
        let mut made = 0;
        let mut first_err = None;
        for w in rows.windows(2) {
            let (a, b) = (w[0], w[1]);
            let pu = self.tasks[a].uid;
            if self.tasks[b].predecessors.iter().any(|l| l.pred == pu) {
                continue;
            }
            match self.add_link(a, b, LinkType::FS, Lag::default()) {
                Ok(()) => made += 1,
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match (made, first_err) {
            (0, Some(e)) => Err(e),
            _ => Ok(made),
        }
    }

    /// Ctrl+Shift+F2: with one task selected, remove all of its
    /// predecessor and successor links; with several, remove the links
    /// among them.
    pub fn unlink_tasks(&mut self, rows: &[usize]) {
        if rows.len() == 1 {
            let r = rows[0];
            if r >= self.tasks.len() {
                return;
            }
            let u = self.tasks[r].uid;
            self.tasks[r].predecessors.clear();
            for t in self.tasks.iter_mut() {
                t.predecessors.retain(|l| l.pred != u);
            }
        } else {
            let set: std::collections::HashSet<u32> = rows
                .iter()
                .filter(|&&r| r < self.tasks.len())
                .map(|&r| self.tasks[r].uid)
                .collect();
            for &r in rows {
                if r < self.tasks.len() {
                    self.tasks[r].predecessors.retain(|l| !set.contains(&l.pred));
                }
            }
        }
    }

    pub fn toggle_collapse(&mut self, row: usize) {
        if row < self.tasks.len() && self.is_summary(row) {
            self.tasks[row].collapsed = !self.tasks[row].collapsed;
        }
    }

    /// Show only outline levels up to `level` (Project's Outline ▸ Level N).
    pub fn show_outline_level(&mut self, level: Option<u32>) {
        for i in 0..self.tasks.len() {
            let summary = self.is_summary(i);
            let t = &mut self.tasks[i];
            t.collapsed = match level {
                Some(l) => summary && t.outline_level >= l,
                None => false,
            };
        }
    }

    pub fn set_mode(&mut self, row: usize, mode: TaskMode) {
        let t = &mut self.tasks[row];
        if t.mode == mode {
            return;
        }
        if mode == TaskMode::Manual {
            t.manual_start = t.calc.start.or(Some(self.start));
        }
        t.mode = mode;
    }

    pub fn set_duration(&mut self, row: usize, d: Duration) -> OpResult {
        if self.is_summary(row) {
            return Err("The duration of a summary task is calculated from its subtasks.".into());
        }
        let has_work = !self.work_assignments(row).is_empty();
        let elapsed = d.elapsed;
        self.tasks[row].duration = d;
        if has_work && !elapsed {
            // Work = Duration × Units, per the task type
            self.apply_duration_with_resources(row, d.minutes);
            self.sync_task_duration(row);
            let unit = d.unit;
            self.tasks[row].duration.unit = unit;
        }
        Ok(())
    }

    /// Typing a start date on an auto-scheduled task sets a Start No Earlier
    /// Than constraint (Project's behaviour); on a manual task it simply
    /// moves the task.
    pub fn set_start(&mut self, row: usize, dt: NaiveDateTime) -> OpResult {
        let summary = self.is_summary(row);
        let t = &mut self.tasks[row];
        match t.mode {
            TaskMode::Manual if !summary => t.manual_start = Some(dt),
            _ => {
                t.constraint = ConstraintType::SNET;
                t.constraint_date = Some(dt);
            }
        }
        Ok(())
    }

    /// Typing a finish: auto task → Finish No Earlier Than constraint;
    /// manual task → duration changes to fit.
    pub fn set_finish(&mut self, row: usize, dt: NaiveDateTime) -> OpResult {
        if self.is_summary(row) {
            return Err("The finish of a summary task is calculated from its subtasks.".into());
        }
        let cal = self.calendar.clone();
        let start = self.tasks[row].calc.start;
        let t = &mut self.tasks[row];
        match t.mode {
            TaskMode::Manual => {
                let s = t.manual_start.or(start).unwrap_or(self.start);
                if dt < s {
                    return Err("The finish date can't be earlier than the start date.".into());
                }
                t.duration.minutes = if t.duration.elapsed {
                    (dt - s).num_minutes()
                } else {
                    cal.work_between(s, dt)
                };
                t.duration.estimated = false;
            }
            TaskMode::Auto => {
                t.constraint = ConstraintType::FNET;
                t.constraint_date = Some(dt);
            }
        }
        Ok(())
    }

    pub fn set_constraint(&mut self, row: usize, kind: ConstraintType, date: Option<NaiveDateTime>) -> OpResult {
        if kind.needs_date() && date.is_none() {
            return Err(format!("{} requires a date.", kind.label()));
        }
        if self.is_summary(row)
            && !matches!(
                kind,
                ConstraintType::ASAP | ConstraintType::SNET | ConstraintType::FNLT
            )
        {
            return Err("Summary tasks only support As Soon As Possible, Start No Earlier Than and Finish No Later Than constraints.".into());
        }
        let t = &mut self.tasks[row];
        t.constraint = kind;
        t.constraint_date = if kind.needs_date() { date } else { None };
        Ok(())
    }

    /// Replace a task's predecessors from Project's text notation. Board
    /// items can be listed too, by key: "3, APP-12, #7+2d" makes the task
    /// wait on task 3 and on two stories (finish-to-start).
    pub fn set_predecessors_text(&mut self, row: usize, text: &str) -> OpResult {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        let loops_before = self.board_loops();
        let backup = (self.tasks[row].predecessors.clone(), self.tasks[row].board_deps.clone());
        let res = self.set_predecessors_text_inner(row, text).and_then(|_| {
            // a board item whose sprint already waits on this task would be a loop
            match self.board_loops().into_iter().find(|n| !loops_before.contains(n)) {
                Some(n) => Err(n),
                None => Ok(()),
            }
        });
        if res.is_err() {
            self.tasks[row].predecessors = backup.0;
            self.tasks[row].board_deps = backup.1;
        }
        res
    }

    /// Notes about board dependencies the scheduler had to ignore because
    /// they'd make a loop.
    pub fn board_loops(&self) -> Vec<String> {
        if self.tasks.iter().all(|t| t.board_deps.is_empty()) {
            return vec![];
        }
        let g = crate::schedule::build_graph(self, &Structure::build(self));
        g.board_notes
            .into_iter()
            .enumerate()
            .flat_map(|(i, v)| v.into_iter().map(move |n| (i, n)))
            .filter(|(_, n)| n.contains("loop") || n.contains("contains this task") || n.contains("own sprint"))
            .map(|(i, n)| format!("Task {}: {n}", i + 1))
            .collect()
    }

    fn set_predecessors_text_inner(&mut self, row: usize, text: &str) -> OpResult {
        let mut task_items = vec![];
        let mut board = vec![];
        for raw in text.split([',', ';']) {
            let item = raw.trim();
            if item.is_empty() {
                continue;
            }
            if item.starts_with(|c: char| c.is_ascii_digit()) {
                task_items.push(item);
                continue;
            }
            // a board key: "APP-12", "#12", optionally "FS", then a lag
            let key_len = if let Some(rest) = item.strip_prefix('#') {
                1 + rest.chars().take_while(|c| c.is_ascii_digit()).count()
            } else {
                match item.find('-') {
                    Some(dash) if dash > 0 => dash + 1 + item[dash + 1..].chars().take_while(|c| c.is_ascii_digit()).count(),
                    _ => 0,
                }
            };
            let key = &item[..key_len.min(item.len())];
            let Some(ticket) = self.ticket_by_key(key).filter(|_| key_len > 1 && key.ends_with(|c: char| c.is_ascii_digit())) else {
                return Err(format!("\"{item}\" is neither a task ID nor a ticket on the board."));
            };
            let mut rest = item[key_len..].trim();
            if let Some(head) = rest.get(..2) {
                match head.to_ascii_uppercase().as_str() {
                    "FS" => rest = rest[2..].trim(),
                    "SS" | "FF" | "SF" => return Err(format!("{key}: a task waits for a board item to be done, so only finish-to-start (FS) links are possible.")),
                    _ => {}
                }
            }
            let lag = if rest.is_empty() {
                Lag::default()
            } else if rest.starts_with('+') || rest.starts_with('-') {
                Lag::parse(rest, &self.settings.durations)?
            } else {
                return Err(format!("\"{item}\" is not a valid predecessor."));
            };
            board.retain(|d: &crate::model::BoardDep| d.ticket != ticket);
            board.push(crate::model::BoardDep { ticket, lag });
        }
        let parsed = parse_predecessors(&task_items.join(","), &self.settings.durations)?;
        self.tasks[row].board_deps.clear();
        for d in &board {
            self.add_board_dep(row, d.ticket, d.lag)?;
        }
        let mut links = vec![];
        let st = Structure::build(self);
        for (id, kind, lag) in parsed {
            if id == 0 || id > self.tasks.len() {
                return Err(format!("Task {id} does not exist."));
            }
            validate_link(&st, id - 1, row, kind)?;
            let uid = self.tasks[id - 1].uid;
            links.retain(|l: &Link| l.pred != uid);
            links.push(Link { pred: uid, kind, lag });
        }
        self.tasks[row].predecessors = links;
        check_links(self)
    }

    /// Format a task's predecessors as Project displays them: "2,3SS+1 day".
    pub fn predecessors_text(&self, row: usize) -> String {
        self.tasks[row]
            .predecessors
            .iter()
            .filter_map(|l| {
                let id = self.id_of_uid(l.pred)?;
                Some(if l.kind == LinkType::FS && l.lag.is_zero() {
                    id.to_string()
                } else {
                    format!("{}{}{}", id, l.kind.code(), l.lag.format(&self.settings.durations))
                })
            })
            .chain(self.tasks[row].board_deps.iter().map(|d| {
                let key = self.ticket_key(d.ticket);
                if d.lag.is_zero() {
                    key
                } else {
                    format!("{key}{}", d.lag.format(&self.settings.durations))
                }
            }))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Successor rows of `row` (direct links only).
    pub fn successors_of(&self, row: usize) -> Vec<(usize, Link)> {
        let u = self.tasks[row].uid;
        self.tasks
            .iter()
            .enumerate()
            .flat_map(|(i, t)| {
                t.predecessors
                    .iter()
                    .filter(move |l| l.pred == u)
                    .map(move |l| (i, *l))
            })
            .collect()
    }

    /// Edit a cell from text, the way the Entry table does.
    pub fn set_field_text(&mut self, row: usize, field: Field, text: &str) -> OpResult {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        match field {
            Field::Name => {
                self.tasks[row].name = text.to_string();
                Ok(())
            }
            Field::Duration => {
                let d = self.parse_duration(text)?;
                self.set_duration(row, d)
            }
            Field::Start => {
                let dt = parse_date(text, self.default_start_time())?;
                self.set_start(row, dt)
            }
            Field::Finish => {
                let dt = parse_date(text, self.default_finish_time())?;
                self.set_finish(row, dt)
            }
            Field::Predecessors => self.set_predecessors_text(row, text),
            Field::PercentComplete => {
                let v: f64 = text
                    .trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse()
                    .map_err(|_| format!("\"{text}\" is not a valid percentage"))?;
                self.set_percent_complete(row, v)
            }
            Field::ResourceNames => self.set_resource_names_text(row, text),
            Field::Work => {
                let d = Duration::parse(text, &self.settings.durations, crate::duration::DurUnit::Hour)?;
                self.set_task_work(row, d.minutes)
            }
            Field::Mode => {
                let m = match text.trim().to_ascii_lowercase().as_str() {
                    "auto" | "auto scheduled" | "a" => TaskMode::Auto,
                    "manual" | "manually scheduled" | "m" => TaskMode::Manual,
                    _ => return Err("Task mode must be Auto or Manual.".into()),
                };
                self.set_mode(row, m);
                Ok(())
            }
        }
    }
}

/// Parse Project's predecessor notation: comma/semicolon separated items of
/// the form `ID[type][±lag]`, e.g. `3`, `4SS`, `5FF+2d`, `6SF-50%`.
pub fn parse_predecessors(
    text: &str,
    s: &crate::duration::DurationSettings,
) -> Result<Vec<(usize, LinkType, Lag)>, String> {
    let mut out = vec![];
    for raw in text.split([',', ';']) {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let digits = item.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return Err(format!("\"{item}\" is not a valid predecessor (expected a task ID)."));
        }
        let id: usize = item[..digits].parse().map_err(|_| "Bad task ID".to_string())?;
        let rest = item[digits..].trim();
        let (kind, rest) = if rest.len() >= 2 {
            match rest.get(..2).map(|h| h.to_ascii_uppercase()).unwrap_or_default().as_str() {
                "FS" => (LinkType::FS, &rest[2..]),
                "SS" => (LinkType::SS, &rest[2..]),
                "FF" => (LinkType::FF, &rest[2..]),
                "SF" => (LinkType::SF, &rest[2..]),
                _ => (LinkType::FS, rest),
            }
        } else {
            (LinkType::FS, rest)
        };
        let rest = rest.trim();
        let lag = if rest.is_empty() {
            Lag::default()
        } else if rest.starts_with('+') || rest.starts_with('-') {
            Lag::parse(rest, s)?
        } else {
            return Err(format!("\"{item}\" is not a valid predecessor."));
        };
        out.push((id, kind, lag));
    }
    Ok(out)
}
