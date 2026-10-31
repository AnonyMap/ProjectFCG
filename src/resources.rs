//! Resources and assignments: Work = Duration × Units under Project's task
//! types, effort-driven scheduling, the `Name[50%]` notation, resource
//! calendars, costs, overallocation and time-phased work.
//!
//! Model: each work assignment has `units` and `work`; its span (working
//! time it occupies) is `work / units` on the resource's calendar. A task's
//! Duration is the longest span of its work assignments. Edits follow
//! Project's rules for which quantity stays fixed:
//!
//! | change    | Fixed Units      | Fixed Duration   | Fixed Work       |
//! |-----------|------------------|------------------|------------------|
//! | Duration  | work = D × units | work = D × units | units = work / D |
//! | Units     | span changes     | work = D × units | span changes     |
//! | Work      | span changes     | units = work / D | span changes     |
//!
//! With effort-driven scheduling on, adding or removing a resource keeps
//! the task's total work constant (Fixed Units/Work: duration changes;
//! Fixed Duration: units change).

use crate::calendar::Calendar;
use crate::model::{Assignment, Project, Resource, ResourceType, TaskType};
use chrono::NaiveDateTime;

pub type OpResult<T = ()> = Result<T, String>;

impl Resource {
    /// The resource's working time: the project calendar with this
    /// resource's own work week and exceptions (vacation, part time) on top.
    pub fn effective_calendar(&self, base: &Calendar) -> Calendar {
        let mut c = base.clone();
        c.name = format!("{} ({})", self.name, base.name);
        if let Some(w) = &self.week {
            c.week = w.clone();
        }
        c.exceptions.extend(self.exceptions.iter().cloned());
        c
    }
    pub fn has_own_calendar(&self) -> bool {
        self.week.is_some() || !self.exceptions.is_empty()
    }
}

impl Project {
    pub fn alloc_res_uid(&mut self) -> u32 {
        let max = self.resources.iter().map(|r| r.uid).max().unwrap_or(0);
        if self.next_res_uid <= max {
            self.next_res_uid = max + 1;
        }
        let u = self.next_res_uid;
        self.next_res_uid += 1;
        u
    }

    pub fn resource_index(&self, uid: u32) -> Option<usize> {
        self.resources.iter().position(|r| r.uid == uid)
    }

    pub fn resource_by_name(&self, name: &str) -> Option<usize> {
        let n = name.trim();
        self.resources.iter().position(|r| r.name.eq_ignore_ascii_case(n))
    }

    /// Insert a resource at `at` in the Resource Sheet. Returns its row.
    pub fn insert_resource(&mut self, at: usize, name: &str, rtype: ResourceType) -> usize {
        let uid = self.alloc_res_uid();
        let initials = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_string()).unwrap_or_default();
        let r = Resource {
            uid,
            name: name.to_string(),
            rtype,
            initials,
            max_units: if rtype == ResourceType::Work { 1.0 } else { 0.0 },
            ..Default::default()
        };
        let at = at.min(self.resources.len());
        self.resources.insert(at, r);
        at
    }

    pub fn add_resource(&mut self, name: &str, rtype: ResourceType) -> usize {
        let at = self.resources.len();
        self.insert_resource(at, name, rtype)
    }

    /// Delete resources (and their assignments).
    pub fn delete_resources(&mut self, rows: &[usize]) {
        let dead: Vec<u32> = rows.iter().filter_map(|&r| self.resources.get(r).map(|x| x.uid)).collect();
        self.assignments.retain(|a| !dead.contains(&a.resource));
        self.resources.retain(|r| !dead.contains(&r.uid));
    }

    /// Indices into `assignments` for a task row.
    pub fn assignments_of_task(&self, row: usize) -> Vec<usize> {
        let u = self.tasks[row].uid;
        (0..self.assignments.len()).filter(|&i| self.assignments[i].task == u).collect()
    }

    pub fn assignments_of_resource(&self, res_uid: u32) -> Vec<usize> {
        (0..self.assignments.len()).filter(|&i| self.assignments[i].resource == res_uid).collect()
    }

    fn rtype_of(&self, res_uid: u32) -> ResourceType {
        self.resources
            .iter()
            .find(|r| r.uid == res_uid)
            .map(|r| r.rtype)
            .unwrap_or(ResourceType::Work)
    }

    pub fn rtype_is_work(&self, res_uid: u32) -> bool {
        self.rtype_of(res_uid) == ResourceType::Work
    }

    /// Work-resource assignments of a task.
    pub fn work_assignments(&self, row: usize) -> Vec<usize> {
        self.assignments_of_task(row)
            .into_iter()
            .filter(|&i| self.rtype_of(self.assignments[i].resource) == ResourceType::Work)
            .collect()
    }

    pub fn task_work(&self, row: usize) -> i64 {
        self.work_assignments(row).iter().map(|&i| self.assignments[i].work).sum()
    }

    /// Working time an assignment occupies (work / units).
    pub fn assignment_span(&self, ai: usize) -> i64 {
        let a = &self.assignments[ai];
        if a.units > 1e-9 {
            (a.work as f64 / a.units).round() as i64
        } else {
            self.index_of_uid(a.task).map(|r| self.tasks[r].duration.minutes).unwrap_or(0)
        }
    }

    /// Keep Duration equal to the longest assignment span.
    pub fn sync_task_duration(&mut self, row: usize) {
        let wa = self.work_assignments(row);
        let spans: Vec<i64> = wa
            .iter()
            .filter(|&&i| self.assignments[i].units > 1e-9)
            .map(|&i| self.assignment_span(i))
            .collect();
        if let Some(&m) = spans.iter().max() {
            let d = &mut self.tasks[row].duration;
            if !d.elapsed {
                d.minutes = m;
                d.estimated = false;
            }
        }
    }

    /// Duration edit on a task with resources.
    pub fn apply_duration_with_resources(&mut self, row: usize, minutes: i64) {
        let tt = self.tasks[row].task_type;
        for ai in self.work_assignments(row) {
            let a = &mut self.assignments[ai];
            match tt {
                TaskType::FixedUnits | TaskType::FixedDuration => {
                    a.work = (minutes as f64 * a.units).round() as i64;
                }
                TaskType::FixedWork => {
                    if minutes > 0 {
                        a.units = a.work as f64 / minutes as f64;
                    } else {
                        a.work = 0;
                    }
                }
            }
        }
    }

    /// Edit the task's total Work. Distributed over its work assignments in
    /// proportion to their units.
    pub fn set_task_work(&mut self, row: usize, total: i64) -> OpResult {
        if self.is_summary(row) {
            return Err("The work of a summary task is rolled up from its subtasks.".into());
        }
        let wa = self.work_assignments(row);
        if wa.is_empty() {
            return Err("Assign a work resource to this task before entering work.".into());
        }
        if total < 0 {
            return Err("Work can't be negative.".into());
        }
        let su: f64 = wa.iter().map(|&i| self.assignments[i].units).sum();
        if su <= 1e-9 {
            return Err("The task's assignments have 0% units.".into());
        }
        let tt = self.tasks[row].task_type;
        let d = self.tasks[row].duration.minutes;
        match tt {
            TaskType::FixedUnits | TaskType::FixedWork => {
                let nd = total as f64 / su;
                for &i in &wa {
                    let a = &mut self.assignments[i];
                    a.work = (nd * a.units).round() as i64;
                }
            }
            TaskType::FixedDuration => {
                if d <= 0 {
                    return Err("A fixed-duration task with zero duration can't have work.".into());
                }
                let k = total as f64 / (d as f64 * su);
                for &i in &wa {
                    let a = &mut self.assignments[i];
                    a.units *= k;
                    a.work = (d as f64 * a.units).round() as i64;
                }
            }
        }
        self.sync_task_duration(row);
        Ok(())
    }

    /// Change an assignment's units.
    pub fn set_assignment_units(&mut self, ai: usize, units: f64) -> OpResult {
        if units < 0.0 || !units.is_finite() {
            return Err("Units can't be negative.".into());
        }
        let Some(row) = self.index_of_uid(self.assignments[ai].task) else {
            return Ok(());
        };
        let rtype = self.rtype_of(self.assignments[ai].resource);
        let d = self.tasks[row].duration.minutes;
        let tt = self.tasks[row].task_type;
        let a = &mut self.assignments[ai];
        if rtype != ResourceType::Work {
            a.units = units;
            return Ok(());
        }
        let keep_work = matches!(tt, TaskType::FixedUnits | TaskType::FixedWork) && a.work > 0 && units > 1e-9;
        a.units = units;
        if !keep_work {
            a.work = (d as f64 * units).round() as i64;
        }
        self.sync_task_duration(row);
        Ok(())
    }

    /// Change an assignment's work.
    pub fn set_assignment_work(&mut self, ai: usize, work: i64) -> OpResult {
        if work < 0 {
            return Err("Work can't be negative.".into());
        }
        let Some(row) = self.index_of_uid(self.assignments[ai].task) else {
            return Ok(());
        };
        let d = self.tasks[row].duration.minutes;
        let tt = self.tasks[row].task_type;
        let a = &mut self.assignments[ai];
        a.work = work;
        if tt == TaskType::FixedDuration && d > 0 {
            a.units = work as f64 / d as f64;
        }
        self.sync_task_duration(row);
        Ok(())
    }

    /// Assign a resource to a task (or update the units if already
    /// assigned). Returns the assignment index.
    pub fn assign(&mut self, row: usize, res_uid: u32, units: f64) -> OpResult<usize> {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        if self.is_summary(row) {
            return Err("ProjectFCG assigns resources to subtasks, not summary tasks.".into());
        }
        let Some(ri) = self.resource_index(res_uid) else {
            return Err("No such resource.".into());
        };
        let rtype = self.resources[ri].rtype;
        let tuid = self.tasks[row].uid;
        if let Some(ai) = self.assignments.iter().position(|a| a.task == tuid && a.resource == res_uid) {
            self.set_assignment_units(ai, units)?;
            return Ok(ai);
        }
        match rtype {
            ResourceType::Work => {
                if units < 0.0 || !units.is_finite() {
                    return Err("Units can't be negative.".into());
                }
                let existing = self.work_assignments(row);
                let effort = self.tasks[row].effort_driven && !existing.is_empty();
                let d = self.tasks[row].duration.minutes;
                let total_work = self.task_work(row);
                self.assignments.push(Assignment {
                    task: tuid,
                    resource: res_uid,
                    units,
                    work: (d as f64 * units).round() as i64,
                    cost: 0.0,
                });
                if effort {
                    self.redistribute_work(row, total_work);
                }
                self.sync_task_duration(row);
            }
            ResourceType::Material => self.assignments.push(Assignment {
                task: tuid,
                resource: res_uid,
                units,
                work: 0,
                cost: 0.0,
            }),
            ResourceType::Cost => self.assignments.push(Assignment {
                task: tuid,
                resource: res_uid,
                units: 0.0,
                work: 0,
                cost: units.max(0.0),
            }),
        }
        Ok(self.assignments.len() - 1)
    }

    /// Effort-driven: keep `total` work across the task's work assignments.
    fn redistribute_work(&mut self, row: usize, total: i64) {
        let wa = self.work_assignments(row);
        let su: f64 = wa.iter().map(|&i| self.assignments[i].units).sum();
        if su <= 1e-9 {
            return;
        }
        match self.tasks[row].task_type {
            TaskType::FixedUnits | TaskType::FixedWork => {
                let nd = total as f64 / su;
                for &i in &wa {
                    let a = &mut self.assignments[i];
                    a.work = (nd * a.units).round() as i64;
                }
            }
            TaskType::FixedDuration => {
                let d = self.tasks[row].duration.minutes as f64;
                if d > 0.0 {
                    let k = total as f64 / (d * su);
                    for &i in &wa {
                        let a = &mut self.assignments[i];
                        a.units *= k;
                        a.work = (d * a.units).round() as i64;
                    }
                }
            }
        }
    }

    pub fn unassign(&mut self, row: usize, res_uid: u32) {
        let tuid = self.tasks[row].uid;
        let was_work = self.rtype_of(res_uid) == ResourceType::Work;
        let total = self.task_work(row);
        self.assignments.retain(|a| !(a.task == tuid && a.resource == res_uid));
        if was_work && self.tasks[row].effort_driven && !self.work_assignments(row).is_empty() {
            self.redistribute_work(row, total);
        }
        self.sync_task_duration(row);
    }

    /// Format a task's assignments in Project's Resource Names notation:
    /// `Alice,Bob[50%],Cement[10 tons],Travel[$500.00]`.
    pub fn resource_names_text(&self, row: usize) -> String {
        self.resource_names_of(&self.assignments_of_task(row))
    }

    /// Resource Names text for some assignments (indices) of one task.
    pub fn resource_names_of(&self, ais: &[usize]) -> String {
        ais.iter()
            .filter_map(|&ai| {
                let a = &self.assignments[ai];
                let r = self.resources.iter().find(|r| r.uid == a.resource)?;
                Some(match r.rtype {
                    ResourceType::Work if (a.units - 1.0).abs() < 1e-9 => r.name.clone(),
                    ResourceType::Work => format!("{}[{}]", r.name, fmt_units(a.units)),
                    ResourceType::Material => {
                        let q = crate::duration::fmt_num(a.units);
                        if r.material_label.is_empty() {
                            format!("{}[{}]", r.name, q)
                        } else {
                            format!("{}[{} {}]", r.name, q, r.material_label)
                        }
                    }
                    ResourceType::Cost => format!("{}[{}]", r.name, fmt_money(a.cost, &self.settings.currency_symbol)),
                })
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Initials for the Resource Initials field / bar text.
    pub fn resource_initials_text(&self, row: usize) -> String {
        self.assignments_of_task(row)
            .iter()
            .filter_map(|&ai| {
                let a = &self.assignments[ai];
                self.resources
                    .iter()
                    .find(|r| r.uid == a.resource)
                    .map(|r| if r.initials.is_empty() { r.name.clone() } else { r.initials.clone() })
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Replace a task's assignments from Resource Names text. Unknown names
    /// become new resources when "automatically add new resources" is on.
    pub fn set_resource_names_text(&mut self, row: usize, text: &str) -> OpResult {
        if self.is_summary(row) && !text.trim().is_empty() {
            return Err("ProjectFCG assigns resources to subtasks, not summary tasks.".into());
        }
        let items = parse_resource_names(text, &self.settings.currency_symbol)?;
        // resolve / create resources
        let mut wanted: Vec<(u32, ParsedAmount)> = vec![];
        for (name, amt) in items {
            let idx = match self.resource_by_name(&name) {
                Some(i) => i,
                None => {
                    if !self.settings.auto_add_resources {
                        return Err(format!("\"{name}\" is not in the resource list."));
                    }
                    let rtype = match amt {
                        ParsedAmount::Money(_) => ResourceType::Cost,
                        ParsedAmount::Quantity(_, Some(_)) => ResourceType::Material,
                        _ => ResourceType::Work,
                    };
                    let i = self.add_resource(&name, rtype);
                    if let ParsedAmount::Quantity(_, Some(label)) = &amt {
                        self.resources[i].material_label = label.clone();
                    }
                    i
                }
            };
            let uid = self.resources[idx].uid;
            if wanted.iter().any(|(u, _)| *u == uid) {
                return Err(format!("{} is listed twice.", self.resources[idx].name));
            }
            wanted.push((uid, amt));
        }
        // remove assignments no longer listed
        let tuid = self.tasks[row].uid;
        let current: Vec<u32> = self
            .assignments
            .iter()
            .filter(|a| a.task == tuid)
            .map(|a| a.resource)
            .collect();
        for r in current {
            if !wanted.iter().any(|(u, _)| *u == r) {
                self.unassign(row, r);
            }
        }
        for (uid, amt) in wanted {
            let rtype = self.rtype_of(uid);
            let val = match (rtype, &amt) {
                (ResourceType::Work, ParsedAmount::Default) => 1.0,
                (ResourceType::Work, ParsedAmount::Percent(p)) => p / 100.0,
                (ResourceType::Work, ParsedAmount::Quantity(q, _)) => q / 100.0,
                (ResourceType::Material, ParsedAmount::Default) => 1.0,
                (ResourceType::Material, ParsedAmount::Quantity(q, _)) => *q,
                (ResourceType::Material, ParsedAmount::Percent(p)) => p / 100.0,
                (ResourceType::Cost, ParsedAmount::Money(m)) => *m,
                (ResourceType::Cost, ParsedAmount::Quantity(q, _)) => *q,
                (ResourceType::Cost, ParsedAmount::Default) => 0.0,
                (_, ParsedAmount::Money(_)) => {
                    let name = self.resource_index(uid).map(|i| self.resources[i].name.clone()).unwrap_or_default();
                    return Err(format!(
                        "{name} is a {} resource; a money amount only applies to cost resources.",
                        rtype.label().to_lowercase()
                    ));
                }
                (ResourceType::Cost, ParsedAmount::Percent(_)) => {
                    return Err("Cost resources take an amount, e.g. Travel[$500].".into())
                }
            };
            let tuid = self.tasks[row].uid;
            match self.assignments.iter().position(|a| a.task == tuid && a.resource == uid) {
                Some(ai) => {
                    if rtype == ResourceType::Cost {
                        self.assignments[ai].cost = val;
                    } else if (self.assignments[ai].units - val).abs() > 1e-9 {
                        self.set_assignment_units(ai, val)?;
                    }
                }
                None => {
                    self.assign(row, uid, val)?;
                }
            }
        }
        Ok(())
    }

    /// Effective calendar of every resource (computed on demand; the
    /// scheduler caches these in `calc.resource_calendars`).
    pub fn build_resource_calendars(&self) -> Vec<Calendar> {
        self.resources.iter().map(|r| r.effective_calendar(&self.calendar)).collect()
    }

    /// Work of assignment `ai` falling between `a` and `b` (working
    /// minutes), assuming work is spread evenly over the assignment's
    /// working time (Project's flat contour). Requires a scheduled project.
    pub fn assignment_work_between(&self, ai: usize, a: NaiveDateTime, b: NaiveDateTime) -> f64 {
        let asg = &self.assignments[ai];
        let Some(c) = self.calc.assignments.get(ai) else { return 0.0 };
        let (Some(s), Some(f)) = (c.start, c.finish) else { return 0.0 };
        let lo = s.max(a);
        let hi = f.min(b);
        if hi <= lo || asg.work <= 0 {
            return 0.0;
        }
        let cal = self
            .resource_index(asg.resource)
            .and_then(|i| self.calc.resource_calendars.get(i))
            .unwrap_or(&self.calendar);
        let full = cal.work_between(s, f);
        if full <= 0 {
            return 0.0;
        }
        asg.work as f64 * cal.work_between(lo, hi) as f64 / full as f64
    }
}

/// After scheduling: assignment dates on resource calendars, costs,
/// overallocation, and work/cost rollups.
/// When assignment `ai` (of task row `ti`, resource index `ri`) works: work
/// resources run their span on their own calendar from the task's start;
/// material and cost resources span the task.
pub(crate) fn assignment_dates(p: &Project, cals: &[Calendar], ai: usize, ti: usize, ri: usize) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let a = &p.assignments[ai];
    let t = &p.tasks[ti];
    let (Some(s), Some(f)) = (t.calc.start, t.calc.finish) else { return None };
    let r = &p.resources[ri];
    Some(if r.rtype == ResourceType::Work && a.units > 1e-9 && !t.duration.elapsed {
        let span = p.assignment_span(ai);
        if span == 0 {
            (s, s)
        } else {
            let st = cals[ri].next_working_start(s);
            (st, cals[ri].add_work(st, span))
        }
    } else {
        (s, f)
    })
}

/// Refresh only assignment start/finish from the tasks' current dates
/// (leveling's cheap reschedule); costs and overallocation are untouched.
pub(crate) fn update_assignment_dates(p: &mut Project) {
    let idx: std::collections::HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    let ridx: std::collections::HashMap<u32, usize> = p.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
    let cals = std::mem::take(&mut p.calc.resource_calendars);
    for ai in 0..p.assignments.len().min(p.calc.assignments.len()) {
        let a = p.assignments[ai];
        let (Some(&ti), Some(&ri)) = (idx.get(&a.task), ridx.get(&a.resource)) else { continue };
        if p.tasks[ti].calc.is_summary {
            continue;
        }
        if let Some((s, f)) = assignment_dates(p, &cals, ai, ti, ri) {
            p.calc.assignments[ai].start = Some(s);
            p.calc.assignments[ai].finish = Some(f);
        }
    }
    p.calc.resource_calendars = cals;
}

pub fn compute_resources(p: &mut Project) {
    use crate::model::{AssignmentCalc, ResourceCalc};
    let cals = p.build_resource_calendars();
    let idx: std::collections::HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    let ridx: std::collections::HashMap<u32, usize> = p.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
    let mut acalc: Vec<AssignmentCalc> = vec![AssignmentCalc::default(); p.assignments.len()];
    for ai in 0..p.assignments.len() {
        let a = p.assignments[ai];
        let (Some(&ti), Some(&ri)) = (idx.get(&a.task), ridx.get(&a.resource)) else { continue };
        let t = &p.tasks[ti];
        if t.calc.is_summary {
            continue;
        }
        let Some((as_, af)) = assignment_dates(p, &cals, ai, ti, ri) else { continue };
        let r = &p.resources[ri];
        let cost = match r.rtype {
            ResourceType::Work => a.work as f64 / 60.0 * r.std_rate + r.cost_per_use,
            ResourceType::Material => a.units * r.std_rate + r.cost_per_use,
            ResourceType::Cost => a.cost,
        };
        acalc[ai] = AssignmentCalc { start: Some(as_), finish: Some(af), cost, overallocated: false };
    }
    // overallocation: sweep assigned units over time per work resource
    let mut rcalc: Vec<ResourceCalc> = vec![ResourceCalc::default(); p.resources.len()];
    // inactive tasks keep their assignments but don't book anyone
    let inactive = p.inactive_mask();
    let off = |ai: usize| idx.get(&p.assignments[ai].task).is_some_and(|&ti| inactive[ti]);
    for (ri, r) in p.resources.iter().enumerate() {
        let mine: Vec<usize> = (0..p.assignments.len()).filter(|&i| p.assignments[i].resource == r.uid && !off(i)).collect();
        rcalc[ri].work = mine.iter().map(|&i| p.assignments[i].work).sum();
        rcalc[ri].cost = mine.iter().map(|&i| acalc[i].cost).sum();
        if r.rtype != ResourceType::Work {
            continue;
        }
        let mut ev: Vec<(NaiveDateTime, f64)> = vec![];
        for &i in &mine {
            if let (Some(s), Some(f)) = (acalc[i].start, acalc[i].finish) {
                if f > s && p.assignments[i].units > 1e-9 {
                    ev.push((s, p.assignments[i].units));
                    ev.push((f, -p.assignments[i].units));
                }
            }
        }
        // removals before additions at the same instant
        ev.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap()));
        let mut cur = 0.0;
        let mut over: Vec<(NaiveDateTime, NaiveDateTime)> = vec![];
        for k in 0..ev.len() {
            cur += ev[k].1;
            if cur > rcalc[ri].peak_units {
                rcalc[ri].peak_units = cur;
            }
            if k + 1 < ev.len() && cur > r.max_units + 1e-6 && ev[k + 1].0 > ev[k].0 {
                let (a, b) = (ev[k].0, ev[k + 1].0);
                // only count periods that contain working time
                if cals[ri].work_between(a, b) > 0 {
                    match over.last_mut() {
                        Some(last) if last.1 == a => last.1 = b,
                        _ => over.push((a, b)),
                    }
                }
            }
        }
        rcalc[ri].overallocated = !over.is_empty();
        for &i in &mine {
            if let (Some(s), Some(f)) = (acalc[i].start, acalc[i].finish) {
                acalc[i].overallocated = over.iter().any(|(a, b)| *a < f && s < *b);
            }
        }
        rcalc[ri].over_intervals = over;
    }
    // task rollups (bottom-up so summaries include nested summaries)
    let n = p.tasks.len();
    let mut work = vec![0i64; n];
    let mut cost = vec![0f64; n];
    let mut over = vec![false; n];
    for (ai, a) in p.assignments.iter().enumerate() {
        if let (Some(&ti), Some(&ri)) = (idx.get(&a.task), ridx.get(&a.resource)) {
            if p.tasks[ti].calc.is_summary {
                continue;
            }
            if p.resources[ri].rtype == ResourceType::Work {
                work[ti] += a.work;
            }
            cost[ti] += acalc[ai].cost;
            over[ti] |= acalc[ai].overallocated;
        }
    }
    for i in 0..n {
        cost[i] += p.tasks[i].fixed_cost;
    }
    let parents: Vec<Option<usize>> = (0..n).map(|i| p.parent_of(i)).collect();
    for i in (0..n).rev() {
        if let Some(par) = parents[i] {
            if inactive[i] && !inactive[par] {
                continue; // doesn't roll up into an active summary
            }
            work[par] += work[i];
            cost[par] += cost[i];
            over[par] |= over[i];
        }
    }
    let mut total_cost = 0.0;
    let mut total_work = 0;
    for i in 0..n {
        let c = &mut p.tasks[i].calc;
        c.work = work[i];
        c.cost = cost[i];
        c.overallocated = over[i];
        if parents[i].is_none() && !inactive[i] {
            total_cost += cost[i];
            total_work += work[i];
        }
    }
    let acost: Vec<f64> = acalc.iter().map(|a| a.cost).collect();
    p.calc.assignments = acalc;
    p.calc.resources = rcalc;
    p.calc.resource_calendars = cals;
    p.calc.total_cost = total_cost;
    p.calc.total_work = total_work;
    crate::tracking::compute_tracking(p, &acost, &parents);
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParsedAmount {
    Default,
    Percent(f64),
    Money(f64),
    /// Number with an optional unit label ("10 tons").
    Quantity(f64, Option<String>),
}

/// Parse `Alice, Bob[50%], Cement[10 tons], Travel[$500]`.
pub fn parse_resource_names(text: &str, currency: &str) -> Result<Vec<(String, ParsedAmount)>, String> {
    let mut out = vec![];
    // split on commas that are not inside brackets
    let mut items = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    for ch in text.chars() {
        match ch {
            '[' => {
                depth += 1;
                cur.push(ch)
            }
            ']' => {
                depth -= 1;
                cur.push(ch)
            }
            ',' | ';' if depth == 0 => items.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    items.push(cur);
    for raw in items {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let (name, amt) = match item.find('[') {
            Some(i) => {
                let close = item.rfind(']').ok_or_else(|| format!("\"{item}\" is missing a closing ]"))?;
                if close < i {
                    return Err(format!("\"{item}\" is not valid"));
                }
                (item[..i].trim().to_string(), parse_amount(item[i + 1..close].trim(), currency)?)
            }
            None => (item.to_string(), ParsedAmount::Default),
        };
        if name.is_empty() {
            return Err(format!("\"{item}\" has no resource name"));
        }
        out.push((name, amt));
    }
    Ok(out)
}

fn parse_amount(s: &str, currency: &str) -> Result<ParsedAmount, String> {
    let bad = || format!("\"{s}\" is not a valid amount");
    if s.is_empty() {
        return Ok(ParsedAmount::Default);
    }
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().replace(',', "").parse::<f64>().map(ParsedAmount::Percent).map_err(|_| bad());
    }
    if let Some(m) = s.strip_prefix(currency).or_else(|| s.strip_prefix('$')) {
        return m.trim().replace(',', "").parse::<f64>().map(ParsedAmount::Money).map_err(|_| bad());
    }
    let num_end = s
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == ','))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let q: f64 = s[..num_end].replace(',', "").parse().map_err(|_| bad())?;
    let label = s[num_end..].trim();
    Ok(ParsedAmount::Quantity(q, if label.is_empty() { None } else { Some(label.to_string()) }))
}

/// "100%", "50%", "33.33%".
pub fn fmt_units(u: f64) -> String {
    format!("{}%", crate::duration::fmt_num(u * 100.0))
}

/// Parse Max Units / Units text: "50%", "1.5" (=150%? no: bare numbers are
/// percentages, as Project displays units as percentages by default).
pub fn parse_units(s: &str) -> Result<f64, String> {
    let t = s.trim().trim_end_matches('%').trim();
    let v: f64 = t.parse().map_err(|_| format!("\"{s}\" is not a valid percentage"))?;
    if v < 0.0 {
        return Err("Units can't be negative.".into());
    }
    Ok(v / 100.0)
}

/// "$1,234.50".
pub fn fmt_money(v: f64, sym: &str) -> String {
    let neg = v < 0.0;
    let cents = (v.abs() * 100.0).round() as i64;
    let whole = cents / 100;
    let frac = cents % 100;
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (k, ch) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}{}{}.{:02}", if neg { "-" } else { "" }, sym, grouped, frac)
}

pub fn parse_money(s: &str, sym: &str) -> Result<f64, String> {
    let t = s.trim();
    let t = t.strip_prefix(sym).or_else(|| t.strip_prefix('$')).unwrap_or(t);
    t.trim().replace(',', "").parse::<f64>().map_err(|_| format!("\"{s}\" is not a valid amount"))
}

/// Rates: "$50.00/hr". `per_hour` is the stored value.
pub fn fmt_rate(per_hour: f64, sym: &str, material: bool) -> String {
    if material {
        fmt_money(per_hour, sym)
    } else {
        format!("{}/hr", fmt_money(per_hour, sym))
    }
}

/// Parse "$50/hr", "400/d", "2000/w", "50" (per hour), "0.5/min".
pub fn parse_rate(s: &str, sym: &str, minutes_per_day: i64, minutes_per_week: i64) -> Result<f64, String> {
    let t = s.trim().to_ascii_lowercase();
    let (num, per) = match t.split_once('/') {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (t.clone(), "h".to_string()),
    };
    let v = parse_money(&num, sym)?;
    let minutes = match per.as_str() {
        "m" | "min" | "mins" | "minute" => 1.0,
        "h" | "hr" | "hrs" | "hour" => 60.0,
        "d" | "day" | "days" => minutes_per_day as f64,
        "w" | "wk" | "wks" | "week" => minutes_per_week as f64,
        "mo" | "mon" | "month" => minutes_per_day as f64 * 20.0,
        "y" | "yr" | "year" => minutes_per_day as f64 * 20.0 * 12.0,
        _ => return Err(format!("\"{s}\" is not a valid rate (use /hr, /d, /w)")),
    };
    Ok(v * 60.0 / minutes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notation_parse() {
        let v = parse_resource_names("Alice, Bob[50%],Cement[10 tons], Travel[$1,500]", "$").unwrap();
        assert_eq!(v[0], ("Alice".into(), ParsedAmount::Default));
        assert_eq!(v[1], ("Bob".into(), ParsedAmount::Percent(50.0)));
        assert_eq!(v[2], ("Cement".into(), ParsedAmount::Quantity(10.0, Some("tons".into()))));
        assert_eq!(v[3], ("Travel".into(), ParsedAmount::Money(1500.0)));
        assert!(parse_resource_names("Bob[50%", "$").is_err());
        assert!(parse_resource_names("[50%]", "$").is_err());
        assert!(parse_resource_names("", "$").unwrap().is_empty());
    }
    #[test]
    fn money_and_rates() {
        assert_eq!(fmt_money(1234.5, "$"), "$1,234.50");
        assert_eq!(fmt_money(0.0, "$"), "$0.00");
        assert_eq!(fmt_money(-12.345, "€"), "-€12.35");
        assert_eq!(parse_money("$1,000.25", "$").unwrap(), 1000.25);
        assert_eq!(parse_rate("$50/hr", "$", 480, 2400).unwrap(), 50.0);
        assert_eq!(parse_rate("400/d", "$", 480, 2400).unwrap(), 50.0);
        assert_eq!(parse_rate("2000/w", "$", 480, 2400).unwrap(), 50.0);
        assert_eq!(parse_rate("50", "$", 480, 2400).unwrap(), 50.0);
        assert!(parse_rate("50/fortnight", "$", 480, 2400).is_err());
        assert_eq!(fmt_units(0.5), "50%");
        assert_eq!(parse_units("150%").unwrap(), 1.5);
    }
}
