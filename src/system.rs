//! The system diagram (experimental): what the project is building, as a
//! freeform map of elements (systems, subsystems, components, software,
//! external systems, functions, notes and groups) and the interfaces
//! between them.
//!
//! Each element can carry:
//! * **the work that delivers it:** Gantt tasks, Board tickets (stories,
//!   epics…) and risks, rolled up into progress, lateness and the worst
//!   open risk;
//! * **measures:** technical performance measures (TPMs) with a target and
//!   a threshold, so the diagram shows whether the thing works, not only
//!   whether it is on time and on budget;
//! * **a state:** where it is in its life (concept to fielded).
//!
//! Elements nest: an element dropped inside a larger one (a system, a
//! group) belongs to it and moves with it, and its work rolls up into it.

use crate::model::Project;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemModel {
    pub elements: Vec<Element>,
    pub interfaces: Vec<Interface>,
    pub next_id: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementKind {
    System,
    Subsystem,
    #[default]
    Component,
    Software,
    /// Outside the project: a user, another program's system, the
    /// environment.
    External,
    /// Something the system has to do.
    Function,
    /// A frame that holds other elements.
    Group,
    /// Free text on the canvas.
    Note,
}

impl ElementKind {
    pub const ALL: [ElementKind; 8] = [
        ElementKind::System,
        ElementKind::Subsystem,
        ElementKind::Component,
        ElementKind::Software,
        ElementKind::External,
        ElementKind::Function,
        ElementKind::Group,
        ElementKind::Note,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::System => "System",
            ElementKind::Subsystem => "Subsystem",
            ElementKind::Component => "Component",
            ElementKind::Software => "Software",
            ElementKind::External => "External",
            ElementKind::Function => "Function",
            ElementKind::Group => "Group",
            ElementKind::Note => "Note",
        }
    }
    /// A new element's size.
    pub fn default_size(self) -> [f32; 2] {
        match self {
            ElementKind::System => [420.0, 280.0],
            ElementKind::Group => [360.0, 240.0],
            ElementKind::Subsystem => [220.0, 120.0],
            ElementKind::Note => [180.0, 90.0],
            _ => [170.0, 76.0],
        }
    }
    /// Kinds that are drawn as frames and can hold other elements.
    pub fn is_container(self) -> bool {
        matches!(self, ElementKind::System | ElementKind::Subsystem | ElementKind::Group)
    }
}

/// Where an element is in its life.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ElementState {
    #[default]
    Concept,
    Design,
    Build,
    Integrated,
    Verified,
    Fielded,
}

impl ElementState {
    pub const ALL: [ElementState; 6] = [ElementState::Concept, ElementState::Design, ElementState::Build, ElementState::Integrated, ElementState::Verified, ElementState::Fielded];
    pub fn label(self) -> &'static str {
        match self {
            ElementState::Concept => "Concept",
            ElementState::Design => "Design",
            ElementState::Build => "Build",
            ElementState::Integrated => "Integrated",
            ElementState::Verified => "Verified",
            ElementState::Fielded => "Fielded",
        }
    }
}

/// Work tied to an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkLink {
    /// A Gantt task, by unique ID.
    Task(u32),
    /// A Board ticket (story, epic, bug…), by ID.
    Ticket(u32),
    /// A risk, by ID.
    Risk(u32),
}

/// A technical performance measure: what the element must achieve.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Measure {
    pub name: String,
    pub unit: String,
    /// The best estimate or test result so far.
    pub current: Option<f64>,
    /// The goal (PMI: the target; DoD: the objective).
    pub target: Option<f64>,
    /// The least that's acceptable (DoD: the threshold).
    pub threshold: Option<f64>,
    /// More is better (range, throughput) or less is (weight, latency).
    pub lower_is_better: bool,
}

/// How a measure stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeasureStatus {
    /// No current value (or nothing to compare it with).
    #[default]
    Unknown,
    /// At or better than the target.
    Met,
    /// Between the threshold and the target.
    AtRisk,
    /// Worse than the threshold.
    Breach,
}

impl MeasureStatus {
    pub fn label(self) -> &'static str {
        match self {
            MeasureStatus::Unknown => "No data",
            MeasureStatus::Met => "Meets target",
            MeasureStatus::AtRisk => "Below target",
            MeasureStatus::Breach => "Breaches threshold",
        }
    }
}

impl Measure {
    pub fn status(&self) -> MeasureStatus {
        let Some(c) = self.current else { return MeasureStatus::Unknown };
        let better = |a: f64, b: f64| if self.lower_is_better { a <= b } else { a >= b };
        match (self.target, self.threshold) {
            (Some(t), _) if better(c, t) => MeasureStatus::Met,
            (_, Some(th)) if !better(c, th) => MeasureStatus::Breach,
            (Some(_), _) => MeasureStatus::AtRisk,
            (None, Some(_)) => MeasureStatus::Met,
            (None, None) => MeasureStatus::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Element {
    pub id: u32,
    pub name: String,
    pub kind: ElementKind,
    pub state: ElementState,
    pub owner: String,
    pub notes: String,
    /// Top-left corner and size on the canvas.
    pub pos: [f32; 2],
    pub size: [f32; 2],
    /// A colour of its own (otherwise its kind's).
    pub color: Option<[u8; 3]>,
    /// The element it sits inside.
    pub parent: Option<u32>,
    pub work: Vec<WorkLink>,
    pub measures: Vec<Measure>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceKind {
    #[default]
    Data,
    Signal,
    Power,
    Mechanical,
    Fluid,
    Thermal,
    /// People, process, an agreement.
    Human,
    Other,
}

impl InterfaceKind {
    pub const ALL: [InterfaceKind; 8] = [
        InterfaceKind::Data,
        InterfaceKind::Signal,
        InterfaceKind::Power,
        InterfaceKind::Mechanical,
        InterfaceKind::Fluid,
        InterfaceKind::Thermal,
        InterfaceKind::Human,
        InterfaceKind::Other,
    ];
    pub fn label(self) -> &'static str {
        match self {
            InterfaceKind::Data => "Data",
            InterfaceKind::Signal => "Signal",
            InterfaceKind::Power => "Power",
            InterfaceKind::Mechanical => "Mechanical",
            InterfaceKind::Fluid => "Fluid",
            InterfaceKind::Thermal => "Thermal",
            InterfaceKind::Human => "Human",
            InterfaceKind::Other => "Other",
        }
    }
}

/// Which way an interface flows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flow {
    #[default]
    Forward,
    Both,
    /// A connection with no direction.
    None,
}

impl Flow {
    pub const ALL: [Flow; 3] = [Flow::Forward, Flow::Both, Flow::None];
    pub fn label(self) -> &'static str {
        match self {
            Flow::Forward => "From → to",
            Flow::Both => "Both ways",
            Flow::None => "No direction",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Interface {
    pub id: u32,
    pub from: u32,
    pub to: u32,
    pub label: String,
    pub kind: InterfaceKind,
    pub flow: Flow,
    pub notes: String,
    /// Interfaces have work and risks too (an ICD, integration testing).
    pub work: Vec<WorkLink>,
}

/// What an element's work and measures add up to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rollup {
    /// Linked tasks (and those of nested elements), counted once each.
    pub tasks: usize,
    /// % complete of those tasks, weighted by duration.
    pub percent_complete: Option<f64>,
    /// Tasks finishing after their baseline finish or deadline.
    pub late: usize,
    /// Tasks on the critical path.
    pub critical: usize,
    /// Tickets: done / total (story points when estimated, else count).
    pub tickets: usize,
    pub tickets_done: usize,
    pub points: f64,
    pub points_done: f64,
    /// Active risks, and the worst level among them.
    pub risks: usize,
    pub worst_risk: Option<crate::risks::RiskLevel>,
    /// The worst measure (of this element and nested ones).
    pub worst_measure: MeasureStatus,
    pub measures: usize,
}

/// One word for how an element is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    /// No work, measures or risks to judge by.
    Unknown,
    Good,
    Watch,
    Trouble,
}

impl Health {
    pub fn label(self) -> &'static str {
        match self {
            Health::Unknown => "Not tracked",
            Health::Good => "On track",
            Health::Watch => "Watch",
            Health::Trouble => "In trouble",
        }
    }
}

impl Rollup {
    pub fn health(&self) -> Health {
        use crate::risks::RiskLevel;
        let trouble = self.worst_measure == MeasureStatus::Breach || self.worst_risk == Some(RiskLevel::High) || (self.tasks > 0 && self.late * 3 >= self.tasks && self.late > 0);
        let watch = self.worst_measure == MeasureStatus::AtRisk || self.worst_risk == Some(RiskLevel::Moderate) || self.late > 0;
        if trouble {
            Health::Trouble
        } else if watch {
            Health::Watch
        } else if self.tasks + self.tickets + self.risks + self.measures == 0 {
            Health::Unknown
        } else {
            Health::Good
        }
    }
    /// Progress (0–100) by tasks, else by tickets.
    pub fn progress(&self) -> Option<f64> {
        self.percent_complete.or_else(|| {
            if self.points > 0.0 {
                Some(self.points_done / self.points * 100.0)
            } else if self.tickets > 0 {
                Some(self.tickets_done as f64 / self.tickets as f64 * 100.0)
            } else {
                None
            }
        })
    }
}

impl SystemModel {
    pub fn element(&self, id: u32) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
    pub fn element_mut(&mut self, id: u32) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }
    pub fn interface(&self, id: u32) -> Option<&Interface> {
        self.interfaces.iter().find(|e| e.id == id)
    }
    pub fn interface_mut(&mut self, id: u32) -> Option<&mut Interface> {
        self.interfaces.iter_mut().find(|e| e.id == id)
    }
    fn take_id(&mut self) -> u32 {
        let max = self.elements.iter().map(|e| e.id).chain(self.interfaces.iter().map(|i| i.id)).max().unwrap_or(0);
        self.next_id = self.next_id.max(max + 1).max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Add an element (its id is assigned); returns the id.
    pub fn add_element(&mut self, mut e: Element) -> u32 {
        e.id = self.take_id();
        if e.size[0] <= 0.0 || e.size[1] <= 0.0 {
            e.size = e.kind.default_size();
        }
        if e.name.is_empty() {
            let n = self.elements.iter().filter(|x| x.kind == e.kind).count() + 1;
            e.name = format!("{} {n}", e.kind.label());
        }
        let id = e.id;
        // containers go behind what they hold: first in the list
        if e.kind.is_container() {
            self.elements.insert(0, e);
        } else {
            self.elements.push(e);
        }
        self.reparent(id);
        id
    }

    /// Connect two elements; returns the interface's id, or an error for a
    /// missing element or an element to itself.
    pub fn connect(&mut self, from: u32, to: u32, kind: InterfaceKind) -> Result<u32, String> {
        if from == to {
            return Err("An interface joins two different elements.".into());
        }
        if self.element(from).is_none() || self.element(to).is_none() {
            return Err("That element no longer exists.".into());
        }
        let id = self.take_id();
        self.interfaces.push(Interface { id, from, to, kind, ..Default::default() });
        Ok(id)
    }

    /// Delete an element: what it held moves up to its parent, and its
    /// interfaces go with it.
    pub fn delete_element(&mut self, id: u32) {
        let Some(i) = self.index_of(id) else { return };
        let parent = self.elements[i].parent;
        self.elements.remove(i);
        for e in self.elements.iter_mut() {
            if e.parent == Some(id) {
                e.parent = parent;
            }
        }
        self.interfaces.retain(|f| f.from != id && f.to != id);
    }

    pub fn delete_interface(&mut self, id: u32) {
        self.interfaces.retain(|f| f.id != id);
    }

    /// The elements inside `id`, at any depth.
    pub fn descendants(&self, id: u32) -> Vec<u32> {
        let mut out = vec![];
        let mut stack = vec![id];
        while let Some(p) = stack.pop() {
            for e in &self.elements {
                if e.parent == Some(p) && !out.contains(&e.id) && e.id != id {
                    out.push(e.id);
                    stack.push(e.id);
                }
            }
        }
        out
    }

    /// Move an element (and everything inside it) by `d`.
    pub fn move_by(&mut self, id: u32, d: [f32; 2]) {
        let mut ids = self.descendants(id);
        ids.push(id);
        for e in self.elements.iter_mut().filter(|e| ids.contains(&e.id)) {
            e.pos[0] += d[0];
            e.pos[1] += d[1];
        }
    }

    /// Set an element's parent from where it sits: the smallest container
    /// that wholly holds it (not itself or one of its own descendants).
    pub fn reparent(&mut self, id: u32) {
        let Some(e) = self.element(id).cloned() else { return };
        let mine = self.descendants(id);
        let inside = |c: &Element| {
            c.pos[0] <= e.pos[0] && c.pos[1] <= e.pos[1] && c.pos[0] + c.size[0] >= e.pos[0] + e.size[0] && c.pos[1] + c.size[1] >= e.pos[1] + e.size[1]
        };
        let parent = self
            .elements
            .iter()
            .filter(|c| c.id != id && c.kind.is_container() && !mine.contains(&c.id) && inside(c) && c.size[0] * c.size[1] > e.size[0] * e.size[1])
            .min_by(|a, b| (a.size[0] * a.size[1]).total_cmp(&(b.size[0] * b.size[1])))
            .map(|c| c.id);
        if let Some(x) = self.element_mut(id) {
            x.parent = parent;
        }
        // keep containers behind their contents: a parent before its children
        if let (Some(pi), Some(ci)) = (parent.and_then(|p| self.index_of(p)), self.index_of(id)) {
            if pi > ci {
                let el = self.elements.remove(pi);
                self.elements.insert(ci, el);
            }
        }
    }

    /// The canvas area everything takes up.
    pub fn bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        let mut it = self.elements.iter();
        let first = it.next()?;
        let mut lo = first.pos;
        let mut hi = [first.pos[0] + first.size[0], first.pos[1] + first.size[1]];
        for e in it {
            lo = [lo[0].min(e.pos[0]), lo[1].min(e.pos[1])];
            hi = [hi[0].max(e.pos[0] + e.size[0]), hi[1].max(e.pos[1] + e.size[1])];
        }
        Some((lo, hi))
    }
}

impl Project {
    /// The work, measures and risks of an element and everything nested in
    /// it.
    pub fn system_rollup(&self, id: u32) -> Rollup {
        let sys = &self.system;
        let mut ids = sys.descendants(id);
        ids.push(id);
        let mut work: Vec<WorkLink> = vec![];
        let mut r = Rollup { worst_measure: MeasureStatus::Unknown, ..Default::default() };
        for e in sys.elements.iter().filter(|e| ids.contains(&e.id)) {
            for w in &e.work {
                if !work.contains(w) {
                    work.push(*w);
                }
            }
            for m in &e.measures {
                r.measures += 1;
                r.worst_measure = r.worst_measure.max(m.status());
            }
        }
        self.add_work(&work, &mut r);
        r
    }

    /// The same for an interface's own work.
    pub fn interface_rollup(&self, id: u32) -> Rollup {
        let mut r = Rollup { worst_measure: MeasureStatus::Unknown, ..Default::default() };
        if let Some(f) = self.system.interface(id) {
            self.add_work(&f.work, &mut r);
        }
        r
    }

    fn add_work(&self, work: &[WorkLink], r: &mut Rollup) {
        let mut weight = 0f64;
        let mut done = 0f64;
        let mut tasks: Vec<usize> = vec![];
        for w in work {
            match *w {
                WorkLink::Task(uid) => {
                    let Some(i) = self.index_of_uid(uid) else { continue };
                    // a summary stands for its subtasks: count each task once
                    let mut rows = vec![i];
                    if self.tasks[i].calc.is_summary {
                        rows = (i + 1..self.subtree_end(i)).filter(|&k| !self.tasks[k].calc.is_summary).collect();
                    }
                    for k in rows {
                        if !tasks.contains(&k) {
                            tasks.push(k);
                        }
                    }
                }
                WorkLink::Ticket(tid) => {
                    let Some(t) = self.ticket_index(tid).map(|k| &self.tickets[k]) else { continue };
                    r.tickets += 1;
                    let d = self.is_done(t);
                    if d {
                        r.tickets_done += 1;
                    }
                    if let Some(p) = t.points {
                        r.points += p;
                        if d {
                            r.points_done += p;
                        }
                    }
                }
                WorkLink::Risk(rid) => {
                    let Some(k) = self.risk(rid) else { continue };
                    if k.status.is_active() && k.kind != crate::risks::RiskKind::Opportunity {
                        r.risks += 1;
                        let lv = self.risk_level(k);
                        r.worst_risk = Some(r.worst_risk.map_or(lv, |w| w.max(lv)));
                    }
                }
            }
        }
        for &k in &tasks {
            let t = &self.tasks[k];
            let w = (t.duration.minutes.max(0) as f64).max(1.0);
            weight += w;
            done += w * t.percent_complete;
            r.tasks += 1;
            if t.calc.critical {
                r.critical += 1;
            }
            let fin = t.calc.finish;
            let late_vs = |d: Option<chrono::NaiveDateTime>| matches!((fin, d), (Some(f), Some(d)) if f > d);
            if t.percent_complete < 100.0 && (late_vs(t.baseline.map(|b| b.finish)) || late_vs(t.deadline)) {
                r.late += 1;
            }
        }
        if weight > 0.0 {
            r.percent_complete = Some(done / weight);
        }
    }

    /// The elements (and interfaces) a task, ticket or risk is tied to, by
    /// name.
    pub fn system_names_of(&self, w: WorkLink) -> Vec<String> {
        let mut out: Vec<String> = self.system.elements.iter().filter(|e| e.work.contains(&w)).map(|e| e.name.clone()).collect();
        for f in self.system.interfaces.iter().filter(|f| f.work.contains(&w)) {
            let n = |id| self.system.element(id).map(|e| e.name.as_str()).unwrap_or("?");
            out.push(if f.label.is_empty() { format!("{} – {}", n(f.from), n(f.to)) } else { f.label.clone() });
        }
        out
    }

    /// Drop links to work that no longer exists, interfaces to missing
    /// elements, and parents that are gone or circular; fix duplicate ids.
    /// Returns the number of things fixed.
    pub fn repair_system(&mut self) -> usize {
        let tasks: std::collections::HashSet<u32> = self.tasks.iter().map(|t| t.uid).collect();
        let tickets: std::collections::HashSet<u32> = self.tickets.iter().map(|t| t.id).collect();
        let risks: std::collections::HashSet<u32> = self.risks.iter().map(|r| r.id).collect();
        let ok = |w: &WorkLink| match w {
            WorkLink::Task(u) => tasks.contains(u),
            WorkLink::Ticket(u) => tickets.contains(u),
            WorkLink::Risk(u) => risks.contains(u),
        };
        let sys = &mut self.system;
        let mut fixed = 0;
        let mut seen = std::collections::HashSet::new();
        let mut next = sys.elements.iter().map(|e| e.id).chain(sys.interfaces.iter().map(|i| i.id)).max().unwrap_or(0);
        for e in sys.elements.iter_mut() {
            if e.id == 0 || !seen.insert(e.id) {
                next += 1;
                e.id = next;
                seen.insert(next);
                fixed += 1;
            }
            let n = e.work.len();
            e.work.retain(ok);
            fixed += n - e.work.len();
            for v in e.pos.iter_mut() {
                if !v.is_finite() || v.abs() > 1e6 {
                    *v = 0.0;
                    fixed += 1;
                }
            }
            for (k, v) in e.size.iter_mut().enumerate() {
                if !v.is_finite() || *v < 20.0 || *v > 1e5 {
                    *v = e.kind.default_size()[k];
                    fixed += 1;
                }
            }
        }
        let ids: std::collections::HashSet<u32> = sys.elements.iter().map(|e| e.id).collect();
        for k in 0..sys.elements.len() {
            // a parent that's missing, or a loop back to this element
            let id = sys.elements[k].id;
            let mut p = sys.elements[k].parent;
            let mut steps = 0;
            let mut bad = false;
            while let Some(x) = p {
                if !ids.contains(&x) || x == id || steps > sys.elements.len() {
                    bad = true;
                    break;
                }
                p = sys.elements.iter().find(|e| e.id == x).and_then(|e| e.parent);
                steps += 1;
            }
            if bad {
                sys.elements[k].parent = None;
                fixed += 1;
            }
        }
        let n = sys.interfaces.len();
        sys.interfaces.retain(|f| ids.contains(&f.from) && ids.contains(&f.to) && f.from != f.to);
        fixed += n - sys.interfaces.len();
        for f in sys.interfaces.iter_mut() {
            if f.id == 0 || !seen.insert(f.id) {
                next += 1;
                f.id = next;
                seen.insert(next);
                fixed += 1;
            }
            let n = f.work.len();
            f.work.retain(ok);
            fixed += n - f.work.len();
        }
        // (an empty diagram keeps its counter: a saved file reopens unchanged)
        if next > 0 {
            sys.next_id = sys.next_id.max(next + 1);
        }
        fixed
    }
}
