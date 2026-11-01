//! Releases (Jira's fix versions): a set of stories that ship together.
//!
//! The board knows which sprint each story is planned in, so a release's
//! projected date is the end of the latest sprint still holding unfinished
//! work, and it's *at risk* when that's after the release date. A release
//! can be tied to a Gantt milestone: the milestone then waits on all the
//! release's unfinished stories (exactly as a task waiting on an epic
//! does), takes the release date as its deadline, and anything linked to
//! the milestone follows the release.

use crate::board::{DepTarget, IssueType, OpResult, Ticket};
use crate::model::Project;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Release {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub start: Option<NaiveDate>,
    /// The release date (target).
    pub date: Option<NaiveDate>,
    /// When it was marked released.
    pub released: Option<NaiveDateTime>,
    /// The Gantt milestone (task uid) that stands for this release.
    pub task: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseStatus {
    Released,
    /// Every story done; not marked released yet.
    Ready,
    OnTrack,
    /// Projected after the release date.
    AtRisk,
    /// Unfinished stories that aren't planned into a sprint.
    Unplanned,
    NoStories,
}

impl ReleaseStatus {
    pub fn label(self) -> &'static str {
        match self {
            ReleaseStatus::Released => "Released",
            ReleaseStatus::Ready => "Ready to release",
            ReleaseStatus::OnTrack => "On track",
            ReleaseStatus::AtRisk => "At risk",
            ReleaseStatus::Unplanned => "Unplanned work",
            ReleaseStatus::NoStories => "No stories",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseStats {
    pub tickets: usize,
    pub done_tickets: usize,
    /// In the board's estimate unit.
    pub total: f64,
    pub done: f64,
    pub unestimated: usize,
    /// Unfinished stories not planned into a sprint.
    pub unplanned: usize,
    /// When the work is expected to be done: the milestone's finish when
    /// one is linked, else the end of the latest sprint with unfinished
    /// stories (or the last done date when all are done). `None` while
    /// any work is unplanned.
    pub projected: Option<NaiveDateTime>,
    pub status: ReleaseStatus,
}

impl ReleaseStats {
    pub fn percent(&self) -> f64 {
        if self.total > 0.0 {
            (self.done / self.total * 100.0).clamp(0.0, 100.0)
        } else if self.tickets > 0 {
            self.done_tickets as f64 / self.tickets as f64 * 100.0
        } else {
            0.0
        }
    }
}

/// Release names are unique ignoring case (Unicode case, as `repair_board`).
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Only stories, tasks and bugs carry a release: an epic's size is its
/// stories', and sub-tasks ship with their parent.
pub fn carries_release(t: &Ticket) -> bool {
    t.kind.is_standard()
}

impl Project {
    pub fn release(&self, id: u32) -> Option<&Release> {
        self.board.releases.iter().find(|r| r.id == id)
    }

    fn release_mut(&mut self, id: u32) -> Option<&mut Release> {
        self.board.releases.iter_mut().find(|r| r.id == id)
    }

    pub fn release_by_name(&self, name: &str) -> Option<u32> {
        self.board.releases.iter().find(|r| same_name(&r.name, name)).map(|r| r.id)
    }

    /// Add a release (names are unique, ignoring case). Returns its id.
    pub fn add_release(&mut self, name: &str, date: Option<NaiveDate>) -> OpResult<u32> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A release needs a name.".into());
        }
        if self.release_by_name(name).is_some() {
            return Err(format!("There's already a release called \"{name}\"."));
        }
        let id = self.board.next_release_id.max(1);
        self.board.next_release_id = id + 1;
        self.board.releases.push(Release { id, name: name.to_string(), date, ..Default::default() });
        Ok(id)
    }

    /// Change a release's name, dates and description (and its milestone's
    /// deadline with it).
    pub fn update_release(&mut self, id: u32, name: &str, description: &str, start: Option<NaiveDate>, date: Option<NaiveDate>) -> OpResult {
        let name = name.trim();
        if name.is_empty() {
            return Err("A release needs a name.".into());
        }
        if self.board.releases.iter().any(|r| r.id != id && same_name(&r.name, name)) {
            return Err(format!("There's already a release called \"{name}\"."));
        }
        if let (Some(s), Some(d)) = (start, date) {
            if d < s {
                return Err("The release date is before its start.".into());
            }
        }
        let r = self.release_mut(id).ok_or("No such release.")?;
        let (task, old_date) = (r.task, r.date);
        r.name = name.to_string();
        r.description = description.to_string();
        r.start = start;
        r.date = date;
        if date.is_none() {
            self.clear_release_deadline(task, old_date);
        }
        self.sync_release_deadline(id);
        Ok(())
    }

    /// Delete a release; its stories keep everything else.
    pub fn delete_release(&mut self, id: u32) -> OpResult {
        let r = self.release(id).ok_or("No such release.")?;
        let (task, date) = (r.task, r.date);
        self.clear_release_deadline(task, date);
        self.board.releases.retain(|r| r.id != id);
        for t in self.tickets.iter_mut() {
            if t.release == Some(id) {
                t.release = None;
            }
        }
        Ok(())
    }

    /// Put a story in a release (or take it out with `None`).
    pub fn set_ticket_release(&mut self, ticket: u32, release: Option<u32>) -> OpResult {
        let k = self.tickets.iter().position(|t| t.id == ticket).ok_or("No such ticket.")?;
        if let Some(r) = release {
            let rel = self.release(r).ok_or("No such release.")?;
            // a shipped release takes no new work (what's in it stays)
            if rel.released.is_some() && self.tickets[k].release != Some(r) {
                return Err(format!("{} is already released; put the work in a later release.", rel.name));
            }
        }
        if release.is_some() && !carries_release(&self.tickets[k]) {
            return Err(match self.tickets[k].kind {
                IssueType::Epic => "Epics don't go in a release; put their stories in it.".into(),
                _ => "Sub-tasks ship with their parent; put the parent in the release.".into(),
            });
        }
        self.tickets[k].release = release;
        Ok(())
    }

    /// The stories (tasks, bugs) in a release.
    pub fn release_tickets(&self, id: u32) -> impl Iterator<Item = &Ticket> {
        self.tickets.iter().filter(move |t| t.release == Some(id) && carries_release(t))
    }

    /// What a milestone standing for this release waits on: all its stories.
    pub fn resolve_release(&self, id: u32) -> DepTarget {
        let mut out = DepTarget::default();
        for t in self.release_tickets(id) {
            self.add_dep_item(t, &mut out);
        }
        out
    }

    pub fn release_stats(&self, id: u32) -> ReleaseStats {
        let mut s = ReleaseStats { tickets: 0, done_tickets: 0, total: 0.0, done: 0.0, unestimated: 0, unplanned: 0, projected: None, status: ReleaseStatus::NoStories };
        let Some(r) = self.release(id) else { return s };
        let mut latest: Option<NaiveDateTime> = None;
        let mut last_done: Option<NaiveDateTime> = None;
        for t in self.release_tickets(id) {
            let u = self.ticket_units(t);
            s.tickets += 1;
            s.total += u;
            if self.board.estimate == crate::board::Estimate::StoryPoints && t.points.is_none() {
                s.unestimated += 1;
            }
            if self.is_done(t) {
                s.done_tickets += 1;
                s.done += u;
                last_done = last_done.max(t.done_at);
            } else {
                match t.sprint.and_then(|sp| self.task_by_uid(sp)).filter(|sp| sp.is_sprint) {
                    Some(sp) => latest = latest.max(sp.calc.finish),
                    None => s.unplanned += 1,
                }
            }
        }
        let milestone = r.task.and_then(|u| self.task_by_uid(u)).and_then(|t| t.calc.finish);
        s.projected = if s.unplanned > 0 {
            None
        } else if s.done_tickets == s.tickets {
            last_done.or(milestone)
        } else {
            milestone.or(latest)
        };
        s.status = if r.released.is_some() {
            ReleaseStatus::Released
        } else if s.tickets == 0 {
            ReleaseStatus::NoStories
        } else if s.done_tickets == s.tickets {
            ReleaseStatus::Ready
        } else if s.unplanned > 0 {
            ReleaseStatus::Unplanned
        } else if matches!((s.projected, r.date), (Some(p), Some(d)) if p.date() > d) {
            ReleaseStatus::AtRisk
        } else {
            ReleaseStatus::OnTrack
        };
        s
    }

    /// Tie a release to a Gantt task (normally a milestone), or untie it.
    /// A task stands for one release at most.
    pub fn set_release_task(&mut self, id: u32, task: Option<u32>) -> OpResult {
        if let Some(u) = task {
            let row = self.index_of_uid(u).ok_or("No such task.")?;
            let t = &self.tasks[row];
            if t.calc.is_summary || self.tasks.get(row + 1).is_some_and(|n| n.outline_level > t.outline_level) {
                return Err("A summary task can't stand for a release; use a milestone.".into());
            }
            if t.is_sprint {
                return Err("A sprint can't stand for a release; use a milestone after it.".into());
            }
            if let Some(other) = self.board.releases.iter().find(|r| r.id != id && r.task == Some(u)) {
                return Err(format!("\"{}\" already stands for release {}.", t.name, other.name));
            }
        }
        let r = self.release_mut(id).ok_or("No such release.")?;
        let (old_task, date) = (r.task, r.date);
        r.task = task;
        if old_task != task {
            self.clear_release_deadline(old_task, date);
        }
        self.sync_release_deadline(id);
        Ok(())
    }

    /// Take a release's date off a milestone it no longer stands for (only
    /// if the deadline is still the one the release put there).
    fn clear_release_deadline(&mut self, task: Option<u32>, date: Option<NaiveDate>) {
        let (Some(u), Some(d)) = (task, date) else { return };
        let at = crate::calendar::at_minute(d, self.settings.default_finish_minute);
        if let Some(row) = self.index_of_uid(u) {
            if self.tasks[row].deadline == Some(at) {
                self.tasks[row].deadline = None;
            }
        }
    }

    /// Add a milestone for the release at the end of the plan and tie it to
    /// the release. Returns the milestone's row.
    pub fn add_release_milestone(&mut self, id: u32) -> OpResult<usize> {
        let name = self.release(id).ok_or("No such release.")?.name.clone();
        let row = self.append_task(&format!("Release {name}"));
        let t = &mut self.tasks[row];
        t.outline_level = 1;
        t.duration.minutes = 0;
        t.duration.estimated = false;
        t.milestone = true;
        let uid = t.uid;
        self.set_release_task(id, Some(uid))?;
        Ok(row)
    }

    /// The release date is the milestone's deadline, so the Gantt shows it
    /// and flags a milestone that lands after it.
    fn sync_release_deadline(&mut self, id: u32) {
        let Some(r) = self.release(id) else { return };
        let (Some(u), Some(d)) = (r.task, r.date) else { return };
        let at = crate::calendar::at_minute(d, self.settings.default_finish_minute);
        if let Some(row) = self.index_of_uid(u) {
            self.tasks[row].deadline = Some(at);
        }
    }

    /// The release a task stands for.
    pub fn release_of_task(&self, uid: u32) -> Option<&Release> {
        self.board.releases.iter().find(|r| r.task == Some(uid))
    }

    /// Mark a release released. Unfinished stories move to `move_to` (another
    /// release) or leave the release (`None`). Returns how many moved.
    pub fn release_version(&mut self, id: u32, at: NaiveDateTime, move_to: Option<u32>) -> OpResult<usize> {
        if self.release(id).ok_or("No such release.")?.released.is_some() {
            return Err("That release is already released.".into());
        }
        if move_to == Some(id) {
            return Err("Unfinished stories can't move to the release being released.".into());
        }
        if let Some(m) = move_to {
            let target = self.release(m).ok_or("No such release.")?;
            if target.released.is_some() {
                return Err(format!("{} is already released.", target.name));
            }
        }
        let unfinished: Vec<u32> = self.release_tickets(id).filter(|t| !self.is_done(t)).map(|t| t.id).collect();
        for t in self.tickets.iter_mut().filter(|t| unfinished.contains(&t.id)) {
            t.release = move_to;
        }
        self.release_mut(id).unwrap().released = Some(at);
        Ok(unfinished.len())
    }

    pub fn unrelease_version(&mut self, id: u32) -> OpResult {
        let r = self.release_mut(id).ok_or("No such release.")?;
        if r.released.take().is_none() {
            return Err("That release isn't released.".into());
        }
        Ok(())
    }
}
