//! Base calendars for tasks (Project ▸ Change Working Time ▸ Create New
//! Calendar; Task Information ▸ Advanced ▸ Calendar).
//!
//! The project calendar stays where it was (`Project::calendar`); tasks
//! without a calendar use it. Other base calendars live in
//! `Project::calendars` and are referred to by uid.

use crate::calendar::Calendar;
use crate::model::{BaseCalendar, Project, ResourceType};
use std::borrow::Cow;

pub type OpResult<T = ()> = Result<T, String>;

/// Templates for a new calendar, as Project offers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// A copy of the project calendar.
    ProjectCalendar,
    Standard,
    TwentyFourHours,
    NightShift,
}

impl Template {
    pub const ALL: [Template; 4] = [Template::ProjectCalendar, Template::Standard, Template::TwentyFourHours, Template::NightShift];
    pub fn label(self) -> &'static str {
        match self {
            Template::ProjectCalendar => "Copy of the project calendar",
            Template::Standard => "Standard (Mon–Fri 8–5)",
            Template::TwentyFourHours => "24 Hours",
            Template::NightShift => "Night Shift (11 PM–8 AM)",
        }
    }
}

impl Project {
    pub fn base_calendar(&self, uid: u32) -> Option<&Calendar> {
        self.calendars.iter().find(|c| c.uid == uid).map(|c| &c.calendar)
    }

    /// The calendar a task's duration is measured on when it has no
    /// resources: its own, or the project's.
    pub fn task_calendar(&self, row: usize) -> &Calendar {
        self.tasks.get(row).and_then(|t| t.calendar).and_then(|u| self.base_calendar(u)).unwrap_or(&self.calendar)
    }

    /// The calendar a task's duration is measured on, the way the
    /// scheduler measures it: with a work resource, the first one's
    /// calendar (∩ the task's own, unless the task ignores resource
    /// calendars); otherwise the task's calendar, or the project's.
    /// Computed from the plan as it is now, so it's right mid-edit.
    pub fn duration_calendar(&self, row: usize) -> Cow<'_, Calendar> {
        let Some(t) = self.tasks.get(row) else { return Cow::Borrowed(&self.calendar) };
        let own = t.calendar.and_then(|u| self.base_calendar(u));
        let res = if t.duration.elapsed {
            None
        } else {
            self.assignments
                .iter()
                .filter(|a| a.task == t.uid && a.units > 1e-9)
                .filter_map(|a| self.resource_index(a.resource))
                .find(|&ri| self.resources[ri].rtype == ResourceType::Work)
        };
        match (res, own) {
            (Some(ri), Some(c)) if !t.ignore_resource_calendars => Cow::Owned(self.resources[ri].effective_calendar(&self.calendar).intersect(c)),
            (_, Some(c)) => Cow::Borrowed(c),
            (Some(ri), None) if self.resources[ri].has_own_calendar() => Cow::Owned(self.resources[ri].effective_calendar(&self.calendar)),
            _ => Cow::Borrowed(&self.calendar),
        }
    }

    /// The same, from the last schedule: cheap, for views and bulk
    /// updates that don't change calendars or assignments.
    pub fn scheduled_duration_calendar(&self, row: usize) -> Cow<'_, Calendar> {
        let c = &self.calc;
        match c.task_cal.get(row).filter(|_| c.task_cal.len() == self.tasks.len()).and_then(|&i| c.resource_calendars.get(i)) {
            Some(cal) => Cow::Borrowed(cal),
            None => self.duration_calendar(row),
        }
    }

    /// Every calendar assignments and tasks work on, the scheduler's way:
    /// each resource's (indices 0..resources), the project's, base
    /// calendars, and resource ∩ task calendar intersections. Returns them
    /// with each assignment's and each task's index into them.
    pub fn build_calendar_index(&self) -> (Vec<Calendar>, Vec<usize>, Vec<usize>) {
        use std::collections::HashMap;
        let mut cals = self.build_resource_calendars();
        let project_ci = cals.len();
        cals.push(self.calendar.clone());
        let ridx: HashMap<u32, usize> = self.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
        let tidx: HashMap<u32, usize> = self.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        let mut base: HashMap<u32, usize> = HashMap::new();
        let mut both: HashMap<(usize, u32), usize> = HashMap::new();
        let mut base_ci = |cals: &mut Vec<Calendar>, u: u32| -> Option<usize> {
            if let Some(&c) = base.get(&u) {
                return Some(c);
            }
            let c = self.base_calendar(u)?.clone();
            cals.push(c);
            base.insert(u, cals.len() - 1);
            Some(cals.len() - 1)
        };
        let mut task_ci: Vec<Option<usize>> = vec![None; self.tasks.len()];
        let mut asg_ci = vec![usize::MAX; self.assignments.len()];
        for (ai, a) in self.assignments.iter().enumerate() {
            let (Some(&ri), Some(&ti)) = (ridx.get(&a.resource), tidx.get(&a.task)) else { continue };
            let t = &self.tasks[ti];
            let ci = match t.calendar.and_then(|u| base_ci(&mut cals, u).map(|c| (u, c))) {
                None => ri,
                Some((_, c)) if t.ignore_resource_calendars => c,
                Some((u, c)) => *both.entry((ri, u)).or_insert_with(|| {
                    let x = cals[ri].intersect(&cals[c]);
                    cals.push(x);
                    cals.len() - 1
                }),
            };
            asg_ci[ai] = ci;
            if task_ci[ti].is_none() && !t.duration.elapsed && a.units > 1e-9 && self.resources[ri].rtype == ResourceType::Work {
                task_ci[ti] = Some(ci);
            }
        }
        let task_ci = self
            .tasks
            .iter()
            .zip(task_ci)
            .map(|(t, c)| c.or_else(|| t.calendar.and_then(|u| base_ci(&mut cals, u))).unwrap_or(project_ci))
            .collect();
        (cals, asg_ci, task_ci)
    }

    /// Assignment `ai`'s calendar from the last schedule (resource `ri`'s
    /// own if the schedule is stale).
    pub fn assignment_calendar(&self, ai: usize, ri: usize) -> &Calendar {
        let c = &self.calc;
        let i = c.assignment_cal.get(ai).copied().filter(|&i| i < c.resource_calendars.len() && c.assignment_cal.len() == self.assignments.len()).unwrap_or(ri);
        c.resource_calendars.get(i).unwrap_or(&self.calendar)
    }

    pub fn calendar_by_name(&self, name: &str) -> Option<u32> {
        let n = name.trim().to_lowercase();
        self.calendars.iter().find(|c| c.calendar.name.trim().to_lowercase() == n).map(|c| c.uid)
    }

    fn name_taken(&self, name: &str, except: Option<u32>) -> bool {
        let n = name.trim().to_lowercase();
        self.calendar.name.trim().to_lowercase() == n || self.calendars.iter().any(|c| Some(c.uid) != except && c.calendar.name.trim().to_lowercase() == n)
    }

    /// Add a base calendar. Returns its uid.
    pub fn add_base_calendar(&mut self, name: &str, template: Template) -> OpResult<u32> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A calendar needs a name.".into());
        }
        if self.name_taken(name, None) {
            return Err(format!("There's already a calendar called \"{name}\"."));
        }
        let mut cal = match template {
            Template::ProjectCalendar => self.calendar.clone(),
            Template::Standard => Calendar::standard(),
            Template::TwentyFourHours => Calendar::twenty_four_hours(),
            Template::NightShift => Calendar::night_shift(),
        };
        cal.name = name.to_string();
        // uids from 2 up (1 is the project calendar in Project XML), past
        // any a task still names, so a new calendar never inherits tasks
        let uid = self.calendars.iter().map(|c| c.uid).chain(self.tasks.iter().filter_map(|t| t.calendar)).max().map_or(2, |m| m + 1).max(2);
        self.calendars.push(BaseCalendar { uid, calendar: cal });
        Ok(uid)
    }

    /// Replace a base calendar's working time (and name).
    pub fn update_base_calendar(&mut self, uid: u32, cal: Calendar) -> OpResult {
        if cal.name.trim().is_empty() {
            return Err("A calendar needs a name.".into());
        }
        if self.name_taken(&cal.name, Some(uid)) {
            return Err(format!("There's already a calendar called \"{}\".", cal.name.trim()));
        }
        if !cal.has_any_working_time() {
            return Err("The calendar must have some working time.".into());
        }
        let bc = self.calendars.iter_mut().find(|c| c.uid == uid).ok_or("No such calendar.")?;
        bc.calendar = cal;
        Ok(())
    }

    /// Delete a base calendar; its tasks go back to the project calendar.
    pub fn delete_base_calendar(&mut self, uid: u32) -> OpResult {
        let n = self.calendars.len();
        self.calendars.retain(|c| c.uid != uid);
        if self.calendars.len() == n {
            return Err("No such calendar.".into());
        }
        for t in self.tasks.iter_mut().filter(|t| t.calendar == Some(uid)) {
            t.calendar = None;
            t.ignore_resource_calendars = false;
        }
        Ok(())
    }

    /// Give a task a calendar (`None`: the project calendar).
    pub fn set_task_calendar(&mut self, row: usize, uid: Option<u32>, ignore_resource_calendars: bool) -> OpResult {
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        if let Some(u) = uid {
            if self.base_calendar(u).is_none() {
                return Err("No such calendar.".into());
            }
            if self.is_summary(row) {
                return Err("A summary's dates come from its subtasks; give the subtasks the calendar.".into());
            }
        }
        let t = &mut self.tasks[row];
        t.calendar = uid;
        t.ignore_resource_calendars = uid.is_some() && ignore_resource_calendars;
        Ok(())
    }

    /// Files from elsewhere: tasks pointing at missing calendars use the
    /// project's; uids are unique.
    pub fn repair_calendars(&mut self) -> usize {
        let mut fixed = 0;
        let mut seen = std::collections::HashSet::new();
        let n = self.calendars.len();
        self.calendars.retain(|c| c.uid >= 2 && seen.insert(c.uid));
        fixed += n - self.calendars.len();
        for t in self.tasks.iter_mut() {
            if t.calendar.is_some_and(|u| !seen.contains(&u)) {
                t.calendar = None;
                t.ignore_resource_calendars = false;
                fixed += 1;
            }
        }
        fixed
    }
}
