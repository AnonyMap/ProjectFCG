//! Base calendars for tasks (Project ▸ Change Working Time ▸ Create New
//! Calendar; Task Information ▸ Advanced ▸ Calendar).
//!
//! The project calendar stays where it was (`Project::calendar`); tasks
//! without a calendar use it. Other base calendars live in
//! `Project::calendars` and are referred to by uid.

use crate::calendar::Calendar;
use crate::model::{BaseCalendar, Project};

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
        // uids from 2 up: 1 is the project calendar in Project XML
        let uid = self.calendars.iter().map(|c| c.uid + 1).max().unwrap_or(2).max(2);
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
