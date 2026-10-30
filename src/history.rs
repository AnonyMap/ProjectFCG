//! Undo/redo. Project supports multi-level undo (default 20 levels,
//! configurable); we keep 100.
//!
//! Each step stores a *delta*, not a copy of the project: the previous
//! versions of only the tasks the edit changed, the task order if it
//! changed, and the rest of the project (calendar, resources, tickets …)
//! only if that changed. Applying a delta returns its inverse, so redo is
//! free. Computed values (`calc`) are ignored when looking for changes;
//! the app recalculates after undo/redo.

use crate::model::{Project, Task};
use std::collections::HashMap;

/// What turns one project state into another.
#[derive(Clone, Debug, Default)]
pub struct Delta {
    /// Task uids in the target order, if the order or set of tasks differs.
    order: Option<Vec<u32>>,
    /// Target versions of tasks that differ (or are new).
    tasks: Vec<Task>,
    /// Target assignments / tickets, if they differ (edited far more often
    /// than the rest, so kept apart from it).
    assignments: Option<Vec<crate::model::Assignment>>,
    tickets: Option<Vec<crate::board::Ticket>>,
    /// Target values of everything else, if any of it differs (`tasks`,
    /// `assignments`, `tickets` empty; `calc` default).
    rest: Option<Box<Project>>,
}

impl Delta {
    /// Rough heap size, for tests and diagnostics.
    pub fn approx_bytes(&self) -> usize {
        let task = |t: &Task| std::mem::size_of::<Task>() + t.name.len() + t.notes.len() + t.predecessors.len() * 32;
        self.order.as_ref().map_or(0, |o| o.len() * 4)
            + self.tasks.iter().map(task).sum::<usize>()
            + self.assignments.as_ref().map_or(0, |a| a.len() * std::mem::size_of::<crate::model::Assignment>())
            + self.tickets.as_ref().map_or(0, |v| v.iter().map(|t| std::mem::size_of::<crate::board::Ticket>() + t.title.len() + t.description.len()).sum())
            + self.rest.as_ref().map_or(0, |r| std::mem::size_of::<Project>() + r.resources.len() * 160)
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_none() && self.tasks.is_empty() && self.assignments.is_none() && self.tickets.is_none() && self.rest.is_none()
    }
}

/// Do two tasks have the same *inputs*? (`calc` is ignored.) Exhaustive
/// destructuring: adding a field to `Task` without listing it here is a
/// compile error, so undo can't silently miss a new field.
pub fn same_task(a: &Task, b: &Task) -> bool {
    let Task {
        uid,
        name,
        outline_level,
        mode,
        duration,
        manual_start,
        constraint,
        constraint_date,
        deadline,
        milestone,
        percent_complete,
        notes,
        predecessors,
        collapsed,
        on_timeline,
        bar_color,
        baseline,
        task_type,
        effort_driven,
        fixed_cost,
        priority,
        leveling_delay,
        is_sprint,
        board_driven,
        board_deps,
        recurrence,
        active,
        actual_start,
        actual_finish,
        baselines,
        calc: _,
    } = a;
    *uid == b.uid
        && *name == b.name
        && *outline_level == b.outline_level
        && *mode == b.mode
        && *duration == b.duration
        && *manual_start == b.manual_start
        && *constraint == b.constraint
        && *constraint_date == b.constraint_date
        && *deadline == b.deadline
        && *milestone == b.milestone
        && percent_complete.to_bits() == b.percent_complete.to_bits()
        && *notes == b.notes
        && *predecessors == b.predecessors
        && *collapsed == b.collapsed
        && *on_timeline == b.on_timeline
        && *bar_color == b.bar_color
        && *baseline == b.baseline
        && *task_type == b.task_type
        && *effort_driven == b.effort_driven
        && fixed_cost.to_bits() == b.fixed_cost.to_bits()
        && *priority == b.priority
        && *leveling_delay == b.leveling_delay
        && *is_sprint == b.is_sprint
        && *board_driven == b.board_driven
        && *board_deps == b.board_deps
        && *recurrence == b.recurrence
        && *active == b.active
        && *actual_start == b.actual_start
        && *actual_finish == b.actual_finish
        && *baselines == b.baselines
}

/// Same for everything in a project that isn't a task (or computed).
fn same_rest(a: &Project, b: &Project) -> bool {
    let Project {
        title,
        manager,
        start,
        status_date,
        calendar,
        settings,
        tasks: _,
        next_uid,
        format,
        resources,
        assignments: _,
        next_res_uid,
        tickets: _,
        next_ticket_id,
        board,
        baseline_saved,
        calc: _,
    } = a;
    *title == b.title
        && *manager == b.manager
        && *start == b.start
        && *status_date == b.status_date
        && *calendar == b.calendar
        && *settings == b.settings
        && *next_uid == b.next_uid
        && *format == b.format
        && *resources == b.resources
        && *next_res_uid == b.next_res_uid
        && *next_ticket_id == b.next_ticket_id
        && *board == b.board
        && *baseline_saved == b.baseline_saved
}

fn rest_of(p: &Project) -> Box<Project> {
    Box::new(Project {
        title: p.title.clone(),
        manager: p.manager.clone(),
        start: p.start,
        status_date: p.status_date,
        calendar: p.calendar.clone(),
        settings: p.settings.clone(),
        tasks: vec![],
        next_uid: p.next_uid,
        format: p.format.clone(),
        resources: p.resources.clone(),
        assignments: vec![],
        next_res_uid: p.next_res_uid,
        tickets: vec![],
        next_ticket_id: p.next_ticket_id,
        board: p.board.clone(),
        baseline_saved: p.baseline_saved,
        calc: Default::default(),
    })
}

/// Move `src`'s "rest" fields into `dst`, returning `dst`'s old ones.
fn swap_rest(dst: &mut Project, mut src: Box<Project>) -> Box<Project> {
    // these travel separately (or not at all): keep dst's
    std::mem::swap(&mut dst.tasks, &mut src.tasks);
    std::mem::swap(&mut dst.assignments, &mut src.assignments);
    std::mem::swap(&mut dst.tickets, &mut src.tickets);
    std::mem::swap(dst, &mut src);
    std::mem::swap(&mut dst.calc, &mut src.calc); // computed values stay put
    src
}

/// The delta that turns `base` into `target`.
pub fn diff(base: &Project, target: &Project) -> Delta {
    let same_order = base.tasks.len() == target.tasks.len() && base.tasks.iter().zip(&target.tasks).all(|(a, b)| a.uid == b.uid);
    let order = (!same_order).then(|| target.tasks.iter().map(|t| t.uid).collect());
    let tasks = if same_order {
        base.tasks.iter().zip(&target.tasks).filter(|(a, b)| !same_task(a, b)).map(|(_, b)| b.clone()).collect()
    } else {
        let by_uid: HashMap<u32, &Task> = base.tasks.iter().map(|t| (t.uid, t)).collect();
        target.tasks.iter().filter(|t| by_uid.get(&t.uid).is_none_or(|a| !same_task(a, t))).cloned().collect()
    };
    let assignments = (base.assignments != target.assignments).then(|| target.assignments.clone());
    let tickets = (base.tickets != target.tickets).then(|| target.tickets.clone());
    let rest = (!same_rest(base, target)).then(|| rest_of(target));
    Delta { order, tasks, assignments, tickets, rest }
}

/// Apply `d` to `p` in place; returns the inverse delta.
pub fn apply(p: &mut Project, d: Delta) -> Delta {
    let Delta { order, tasks, assignments, tickets, rest } = d;
    let inv_rest = rest.map(|r| swap_rest(p, r));
    let inv_assignments = assignments.map(|a| std::mem::replace(&mut p.assignments, a));
    let inv_tickets = tickets.map(|t| std::mem::replace(&mut p.tickets, t));
    let old = std::mem::take(&mut p.tasks);
    let old_order: Vec<u32> = old.iter().map(|t| t.uid).collect();
    let mut inv_tasks = vec![];
    let new_tasks = match order {
        None => {
            // same tasks in the same order: replace in place
            let mut repl: HashMap<u32, Task> = tasks.into_iter().map(|t| (t.uid, t)).collect();
            let mut v = old;
            for slot in v.iter_mut() {
                if let Some(t) = repl.remove(&slot.uid) {
                    inv_tasks.push(std::mem::replace(slot, t));
                }
            }
            v
        }
        Some(ref target) => {
            let mut have: HashMap<u32, Task> = old.into_iter().map(|t| (t.uid, t)).collect();
            let mut repl: HashMap<u32, Task> = tasks.into_iter().map(|t| (t.uid, t)).collect();
            let mut v = Vec::with_capacity(target.len());
            for uid in target {
                match repl.remove(uid) {
                    Some(t) => {
                        if let Some(o) = have.remove(uid) {
                            inv_tasks.push(o);
                        }
                        v.push(t);
                    }
                    None => v.push(have.remove(uid).expect("undo: task missing from the base state")),
                }
            }
            // tasks this delta removes come back on the inverse
            inv_tasks.extend(have.into_values());
            v
        }
    };
    p.tasks = new_tasks;
    Delta { order: order.map(|_| old_order), tasks: inv_tasks, assignments: inv_assignments, tickets: inv_tickets, rest: inv_rest }
}

pub struct History {
    undo: Vec<(String, Delta)>,
    redo: Vec<(String, Delta)>,
    pub limit: usize,
}

impl Default for History {
    fn default() -> Self {
        History { undo: vec![], redo: vec![], limit: 100 }
    }
}

impl History {
    /// Record a change described by `label` from `before` to `after`.
    pub fn checkpoint(&mut self, label: &str, before: &Project, after: &Project) {
        let back = diff(after, before);
        self.undo.push((label.to_string(), back));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Drop the most recent checkpoint (used when an edit turns out to be a
    /// no-op or is rejected).
    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    pub fn undo(&mut self, current: &mut Project) -> Option<String> {
        let (label, d) = self.undo.pop()?;
        let inv = apply(current, d);
        self.redo.push((label.clone(), inv));
        Some(label)
    }

    pub fn redo(&mut self, current: &mut Project) -> Option<String> {
        let (label, d) = self.redo.pop()?;
        let inv = apply(current, d);
        self.undo.push((label.clone(), inv));
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(l, _)| l.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(l, _)| l.as_str())
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    /// Approximate memory held by the history (bytes).
    pub fn approx_bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(|(l, d)| l.len() + d.approx_bytes()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn proj() -> Project {
        let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        for k in 0..6 {
            p.append_task(&format!("T{k}"));
        }
        crate::schedule::schedule(&mut p);
        p
    }

    fn json(p: &Project) -> String {
        crate::io::to_json(p).unwrap()
    }

    #[test]
    fn deltas_are_small_and_exact() {
        let base = proj();
        let mut p = base.clone();
        p.tasks[2].name = "Changed".into();
        let d = diff(&base, &p);
        assert_eq!(d.tasks.len(), 1);
        assert!(d.order.is_none() && d.rest.is_none() && d.assignments.is_none() && d.tickets.is_none());
        // computed values alone don't count as a change
        let mut q = base.clone();
        q.tasks[3].calc.total_slack = 999;
        assert!(diff(&base, &q).is_empty());
        // apply/inverse round trip
        let mut x = base.clone();
        let inv = apply(&mut x, diff(&base, &p));
        assert_eq!(json(&x), json(&p));
        apply(&mut x, inv);
        assert_eq!(json(&x), json(&base));
    }

    #[test]
    fn insert_delete_reorder_and_rest() {
        let base = proj();
        let mut p = base.clone();
        p.delete_tasks(&[1]);
        p.insert_task(3, "New");
        p.tasks.swap(0, 2);
        p.title = "Other".into();
        p.add_ticket("Story", None).unwrap();
        let d = diff(&base, &p);
        assert!(d.order.is_some() && d.rest.is_some() && d.tickets.is_some());
        let mut x = base.clone();
        let inv = apply(&mut x, d);
        assert_eq!(json(&x), json(&p));
        apply(&mut x, inv);
        assert_eq!(json(&x), json(&base));
    }

    #[test]
    fn history_walks_back_and_forth() {
        let mut h = History::default();
        let mut p = proj();
        let states: Vec<String> = {
            let mut v = vec![json(&p)];
            for k in 0..5 {
                let before = p.clone();
                match k {
                    0 => p.tasks[0].name = "A".into(),
                    1 => {
                        p.insert_task(0, "Top");
                    }
                    2 => p.delete_tasks(&[3]),
                    3 => p.board.sprint_days = 7,
                    _ => p.tasks[1].percent_complete = 50.0,
                }
                h.checkpoint(&format!("step {k}"), &before, &p);
                v.push(json(&p));
            }
            v
        };
        for k in (0..5).rev() {
            h.undo(&mut p).unwrap();
            assert_eq!(json(&p), states[k], "undo to state {k}");
        }
        assert!(h.undo(&mut p).is_none());
        for k in 1..=5 {
            h.redo(&mut p).unwrap();
            assert_eq!(json(&p), states[k], "redo to state {k}");
        }
    }
}
