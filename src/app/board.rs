//! Board view (Kanban): sprint selector, filter bar, columns of ticket
//! cards, the backlog/epics panel, and burndown/velocity charts. Cards are
//! dragged between columns, into or out of the backlog, and onto an epic;
//! the keyboard does the same (arrows select, Ctrl+arrows move, Enter
//! edits, Insert adds).

use super::theme::*;
use super::ProjectFcgApp;
use crate::board::{Estimate, IssueType, LinkKind, LinkView, Priority, Ticket};
use crate::model::{fmt_date, Project};
use egui::{pos2, vec2, Align2, Color32, FontId, Id, LayerId, Order, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::sync::Arc;

const TOOLBAR_H: f32 = 40.0;
const FILTER_H: f32 = 34.0;
const BACKLOG_W: f32 = 280.0;
const CHARTS_W: f32 = 360.0;
const GAP: f32 = 10.0;
const CARD_PAD: f32 = 8.0;
const BADGE: f32 = 16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidePanel {
    #[default]
    Backlog,
    Epics,
    Releases,
}

/// Jira's swimlanes: rows across the columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Swimlanes {
    #[default]
    None,
    Epic,
    Assignee,
}

impl Swimlanes {
    pub const ALL: [Swimlanes; 3] = [Swimlanes::None, Swimlanes::Epic, Swimlanes::Assignee];
    pub fn label(self) -> &'static str {
        match self {
            Swimlanes::None => "No swimlanes",
            Swimlanes::Epic => "Swimlanes by epic",
            Swimlanes::Assignee => "Swimlanes by assignee",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EpicFilter {
    #[default]
    Any,
    NoEpic,
    Epic(u32),
}

/// Quick filters (not saved with the project, like Jira's board filters).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardFilter {
    pub text: String,
    pub epic: EpicFilter,
    /// `Some(None)` = in no release.
    pub release: Option<Option<u32>>,
    pub kind: Option<IssueType>,
    pub label: Option<String>,
    /// `Some(None)` = unassigned.
    pub assignee: Option<Option<u32>>,
}

impl BoardFilter {
    pub fn active(&self) -> bool {
        *self != BoardFilter::default()
    }

    pub fn matches(&self, p: &Project, t: &Ticket) -> bool {
        match self.epic {
            EpicFilter::Any => {}
            EpicFilter::NoEpic => {
                if p.epic_of(t).is_some() {
                    return false;
                }
            }
            EpicFilter::Epic(e) => {
                if p.epic_of(t) != Some(e) {
                    return false;
                }
            }
        }
        if self.kind.is_some_and(|k| k != t.kind) {
            return false;
        }
        if let Some(r) = self.release {
            // sub-tasks show with their parent's release
            let rel = if t.kind == IssueType::Subtask { t.parent.and_then(|x| p.ticket(x)).and_then(|x| x.release) } else { t.release };
            if rel != r {
                return false;
            }
        }
        if self.label.as_ref().is_some_and(|l| !t.labels.iter().any(|x| x.eq_ignore_ascii_case(l))) {
            return false;
        }
        if self.assignee.is_some_and(|a| a != t.assignee) {
            return false;
        }
        let q = self.text.trim().to_lowercase();
        if !q.is_empty() {
            let key = p.ticket_key(t.id).to_lowercase();
            let hit = t.title.to_lowercase().contains(&q)
                || t.description.to_lowercase().contains(&q)
                || key == q
                || key.trim_start_matches('#') == q.trim_start_matches('#')
                || t.labels.iter().any(|l| l.to_lowercase().contains(&q));
            if !hit {
                return false;
            }
        }
        true
    }
}

pub struct BoardState {
    /// Sprint shown on the board (task uid).
    pub sprint: Option<u32>,
    /// Selected ticket id.
    pub selected: Option<u32>,
    pub drag: Option<Drag>,
    pub show_charts: bool,
    /// Top chart: cumulative flow instead of the burndown.
    pub flow: bool,
    pub backlog_open: bool,
    pub side: SidePanel,
    pub filter: BoardFilter,
    pub lanes: Swimlanes,
    pub quick_add: String,
    pub quick_backlog: String,
    pub quick_epic: String,
    pub quick_release: String,
    pub scroll_y: f32,
    pub backlog_scroll: f32,
    pub epics_scroll: f32,
    pub releases_scroll: f32,
    /// Where each card was drawn last frame (hit-testing, tests).
    pub card_rects: Vec<(u32, Rect)>,
    /// Column body rects of the sprint board, left to right.
    pub column_rects: Vec<Rect>,
    pub backlog_rect: Rect,
    /// Epic rows of the Epics panel (drop targets, tests).
    pub epic_rects: Vec<(u32, Rect)>,
    /// Release rows of the Releases panel (drop targets, tests).
    pub release_rects: Vec<(u32, Rect)>,
    /// Focus the filter box next frame (Ctrl+F).
    pub focus_filter: bool,
    /// Ticket id → index into `Project::tickets`, rebuilt each frame (the
    /// board looks tickets up by id for every card).
    pub idx: std::collections::HashMap<u32, usize>,
}

impl Default for BoardState {
    fn default() -> Self {
        BoardState {
            sprint: None,
            selected: None,
            drag: None,
            show_charts: true,
            flow: false,
            backlog_open: true,
            side: SidePanel::Backlog,
            filter: BoardFilter::default(),
            lanes: Swimlanes::None,
            quick_add: String::new(),
            quick_backlog: String::new(),
            quick_epic: String::new(),
            quick_release: String::new(),
            scroll_y: 0.0,
            backlog_scroll: 0.0,
            epics_scroll: 0.0,
            releases_scroll: 0.0,
            card_rects: vec![],
            column_rects: vec![],
            backlog_rect: Rect::NOTHING,
            epic_rects: vec![],
            release_rects: vec![],
            focus_filter: false,
            idx: Default::default(),
        }
    }
}

pub struct Drag {
    pub id: u32,
    pub grab: Vec2,
    pub size: Vec2,
}

/// Where a dragged card would land.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Column(usize),
    Backlog,
    Epic(u32),
    Release(u32),
}

/// A link row in the ticket dialog: as seen from this ticket.
#[derive(Clone, Debug, PartialEq)]
pub struct DlgLink {
    pub kind: LinkKind,
    pub outward: bool,
    pub other: u32,
}

/// Ticket edit dialog (new when `id` is None).
#[derive(Clone, Debug)]
pub struct TicketDlg {
    pub id: Option<u32>,
    pub kind: IssueType,
    pub title: String,
    pub description: String,
    pub points: String,
    pub priority: Priority,
    pub column: usize,
    pub sprint: Option<u32>,
    pub assignee: Option<u32>,
    pub labels: String,
    /// Epic (story/task/bug) or parent (sub-task).
    pub parent: Option<u32>,
    /// Epics: the Gantt task it drives.
    pub task: Option<u32>,
    pub links: Vec<DlgLink>,
    pub new_link_kind: (LinkKind, bool),
    pub new_link_to: Option<u32>,
    pub new_subtasks: Vec<String>,
    pub subtask_text: String,
    /// Gantt tasks (uids) it can't start before.
    pub after_tasks: Vec<u32>,
    /// Release (fix version).
    pub release: Option<u32>,
    pub error: Option<String>,
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

fn initials(name: &str, given: &str) -> String {
    if !given.trim().is_empty() {
        return given.trim().chars().take(3).collect();
    }
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

/// Badge colour of an issue type (Jira's hues, darkened for white text).
pub fn type_color(k: IssueType) -> Color32 {
    match k {
        IssueType::Story => Color32::from_rgb(46, 134, 87),
        IssueType::Task => Color32::from_rgb(47, 111, 181),
        IssueType::Bug => Color32::from_rgb(201, 55, 44),
        IssueType::Epic => Color32::from_rgb(123, 63, 196),
        IssueType::Subtask => Color32::from_rgb(52, 132, 168),
    }
}

/// An epic's colour, stable per id.
pub fn epic_color(id: u32) -> Color32 {
    const C: [Color32; 8] = [
        Color32::from_rgb(123, 63, 196),
        Color32::from_rgb(214, 110, 24),
        Color32::from_rgb(28, 140, 140),
        Color32::from_rgb(196, 50, 110),
        Color32::from_rgb(70, 110, 200),
        Color32::from_rgb(120, 150, 30),
        Color32::from_rgb(160, 90, 50),
        Color32::from_rgb(90, 90, 170),
    ];
    C[(id as usize).wrapping_mul(5) % C.len()]
}

fn chip_fill(c: Color32) -> Color32 {
    mix(c, pal().bg, if pal().dark { 0.62 } else { 0.8 })
}

fn priority_color(pr: Priority) -> Color32 {
    match pr {
        Priority::Highest | Priority::High => Color32::from_rgb(214, 60, 40),
        Priority::Medium => Color32::from_rgb(222, 150, 20),
        Priority::Low | Priority::Lowest => Color32::from_rgb(60, 130, 210),
    }
}

/// Jira-style priority chevrons centred at `c`.
pub fn paint_priority(p: &egui::Painter, c: egui::Pos2, pr: Priority) {
    let col = priority_color(pr);
    let s = Stroke::new(1.8, col);
    let w = 4.5;
    let chevron = |y: f32, up: bool| {
        let d = if up { -3.0 } else { 3.0 };
        p.line_segment([pos2(c.x - w, y - d / 2.0), pos2(c.x, y + d / 2.0)], s);
        p.line_segment([pos2(c.x, y + d / 2.0), pos2(c.x + w, y - d / 2.0)], s);
    };
    match pr {
        Priority::Highest => {
            chevron(c.y - 2.5, true);
            chevron(c.y + 2.0, true);
        }
        Priority::High => chevron(c.y, true),
        Priority::Medium => {
            p.line_segment([pos2(c.x - w, c.y - 1.8), pos2(c.x + w, c.y - 1.8)], s);
            p.line_segment([pos2(c.x - w, c.y + 1.8), pos2(c.x + w, c.y + 1.8)], s);
        }
        Priority::Low => chevron(c.y, false),
        Priority::Lowest => {
            chevron(c.y - 2.0, false);
            chevron(c.y + 2.5, false);
        }
    }
}

/// Small rounded square with the type's letter.
pub fn paint_type_badge(p: &egui::Painter, r: Rect, k: IssueType) {
    p.rect_filled(r, 3.0, type_color(k));
    p.text(r.center(), Align2::CENTER_CENTER, k.letter(), FontId::proportional(r.height() - 4.0), Color32::WHITE);
}

// ---------------------------------------------------------------- app glue

impl ProjectFcgApp {
    /// The sprint the board shows, falling back to the current sprint.
    pub fn board_sprint(&self) -> Option<u32> {
        let valid = |u: u32| self.project.tasks.iter().any(|t| t.uid == u && t.is_sprint);
        self.board.sprint.filter(|&u| valid(u)).or_else(|| self.project.current_sprint(now()))
    }

    pub fn open_ticket_dialog(&mut self, id: Option<u32>) {
        let sprint = self.board_sprint();
        let p = &self.project;
        let d = match id.and_then(|i| p.ticket(i)) {
            Some(t) => TicketDlg {
                id: Some(t.id),
                kind: t.kind,
                title: t.title.clone(),
                description: t.description.clone(),
                points: t.points.map(crate::duration::fmt_num).unwrap_or_default(),
                priority: t.priority,
                column: t.column,
                sprint: t.sprint,
                assignee: t.assignee,
                labels: t.labels.join(", "),
                parent: t.parent,
                task: t.task,
                links: p.ticket_links(t.id).into_iter().map(|l| DlgLink { kind: l.kind, outward: l.outward, other: l.other }).collect(),
                new_link_kind: (LinkKind::Blocks, true),
                new_link_to: None,
                new_subtasks: vec![],
                subtask_text: String::new(),
                after_tasks: t.after_tasks.clone(),
                release: t.release,
                error: None,
            },
            None => {
                // a new ticket inherits the epic being filtered on
                let parent = match self.board.filter.epic {
                    EpicFilter::Epic(e) => Some(e),
                    _ => None,
                };
                TicketDlg {
                    id: None,
                    kind: IssueType::Story,
                    title: String::new(),
                    description: String::new(),
                    points: String::new(),
                    priority: Priority::Medium,
                    column: 0,
                    sprint,
                    assignee: None,
                    labels: String::new(),
                    parent,
                    task: None,
                    links: vec![],
                    new_link_kind: (LinkKind::Blocks, true),
                    new_link_to: None,
                    new_subtasks: vec![],
                    subtask_text: String::new(),
                    after_tasks: vec![],
                    release: match self.board.filter.release {
                        Some(Some(r)) => Some(r),
                        _ => None,
                    },
                    error: None,
                }
            }
        };
        self.dialogs.ticket = Some(d);
    }

    /// New ticket dialog for a given type (and parent: an epic, or the
    /// story of a sub-task).
    pub fn open_new_ticket(&mut self, kind: IssueType, parent: Option<u32>) {
        self.open_ticket_dialog(None);
        if let Some(d) = self.dialogs.ticket.as_mut() {
            d.kind = kind;
            d.parent = parent;
            if kind == IssueType::Epic {
                d.sprint = None;
                d.parent = None;
            }
            if kind == IssueType::Subtask {
                d.sprint = parent.and_then(|x| self.project.ticket(x)).and_then(|x| x.sprint);
            }
        }
    }

    pub fn board_new_ticket(&mut self) {
        self.open_ticket_dialog(None);
    }

    pub fn board_new_epic(&mut self) {
        self.board.side = SidePanel::Epics;
        self.board.backlog_open = true;
        self.open_new_ticket(IssueType::Epic, None);
    }

    pub fn board_delete_selected(&mut self) {
        let Some(id) = self.board.selected else { return };
        // select a neighbour afterwards
        let next = self.board_neighbour(id, 1).or_else(|| self.board_neighbour(id, -1));
        if self.edit_project("Delete Ticket", |p| p.delete_ticket(id)).is_some() {
            self.board.selected = next;
            if self.board.filter.epic == EpicFilter::Epic(id) {
                self.board.filter.epic = EpicFilter::Any;
            }
        }
    }

    /// Ticket ids shown in a place (after the filter), in order. Epics
    /// aren't cards; they live in the Epics panel.
    fn board_list(&self, sprint: Option<u32>, column: Option<usize>) -> Vec<u32> {
        let f = &self.board.filter;
        self.project
            .tickets
            .iter()
            .filter(|t| t.kind != IssueType::Epic && t.sprint == sprint && column.is_none_or(|c| t.column == c) && f.matches(&self.project, t))
            .map(|t| t.id)
            .collect()
    }

    /// The ticket `delta` places above/below `id` in its own list (epics:
    /// in the Epics panel).
    fn board_neighbour(&self, id: u32, delta: i32) -> Option<u32> {
        let t = self.project.ticket(id)?;
        let list = if t.kind == IssueType::Epic {
            self.project.epics().into_iter().map(|k| self.project.tickets[k].id).collect()
        } else if t.sprint.is_none() {
            self.board_list(None, None)
        } else {
            self.board_list(t.sprint, Some(t.column))
        };
        let i = list.iter().position(|&x| x == id)? as i32 + delta;
        (i >= 0).then(|| list.get(i as usize).copied()).flatten()
    }

    pub fn board_send_to_backlog(&mut self) {
        let Some(id) = self.board.selected else { return };
        self.edit_project("Move to Backlog", |p| p.move_ticket(id, None, 0, None));
    }

    /// Move the selected ticket one column left/right (Ctrl+←/→).
    pub fn board_shift_column(&mut self, delta: i32) {
        let Some(id) = self.board.selected else { return };
        let Some(t) = self.project.ticket(id).cloned() else { return };
        if t.kind == IssueType::Epic {
            return;
        }
        if t.sprint.is_none() {
            // from the backlog, Ctrl+→ plans it into the shown sprint
            if delta > 0 {
                if let Some(s) = self.board_sprint() {
                    self.edit_project("Move to Sprint", |p| p.move_ticket(id, Some(s), 0, None));
                }
            }
            return;
        }
        let n = self.project.board.columns.len() as i32;
        let c = t.column as i32 + delta;
        if c < 0 {
            return;
        }
        if c < n {
            let name = self.project.board.columns[c as usize].clone();
            self.edit_project(&format!("Move to {name}"), |p| p.move_ticket(id, t.sprint, c as usize, None));
        }
    }

    /// Reorder the selected ticket within its column (Ctrl+↑/↓).
    pub fn board_shift_order(&mut self, delta: i32) {
        let Some(id) = self.board.selected else { return };
        let Some(t) = self.project.ticket(id).cloned() else { return };
        if t.kind == IssueType::Epic {
            return;
        }
        let list = if t.sprint.is_none() { self.board_list(None, None) } else { self.board_list(t.sprint, Some(t.column)) };
        let Some(i) = list.iter().position(|&x| x == id) else { return };
        let j = i as i32 + delta;
        if j < 0 || j as usize >= list.len() {
            return;
        }
        // "before" is the ticket now at the target slot (moving down: the one after it)
        let before = if delta < 0 { Some(list[j as usize]) } else { list.get(j as usize + 1).copied() };
        self.edit_project("Reorder Ticket", |p| p.move_ticket(id, t.sprint, t.column, before));
    }

    /// Arrow-key selection: up/down within the list, left/right across
    /// columns (and between the backlog and the first column).
    pub fn board_select_step(&mut self, dx: i32, dy: i32) {
        let sprint = self.board_sprint();
        let ncols = self.project.board.columns.len();
        let Some(id) = self.board.selected.filter(|&i| self.project.ticket(i).is_some()) else {
            let first = sprint.and_then(|s| (0..ncols).find_map(|c| self.board_list(Some(s), Some(c)).first().copied()));
            self.board.selected = first.or_else(|| self.board_list(None, None).first().copied());
            return;
        };
        if dy != 0 {
            if let Some(n) = self.board_neighbour(id, dy) {
                self.board.selected = Some(n);
            }
            return;
        }
        let t = self.project.ticket(id).cloned().unwrap();
        let in_side = t.sprint.is_none() || t.kind == IssueType::Epic;
        let (place, idx): (Option<usize>, usize) = if in_side {
            (None, self.board_list(None, None).iter().position(|&x| x == id).unwrap_or(0))
        } else {
            (Some(t.column), self.board_list(t.sprint, Some(t.column)).iter().position(|&x| x == id).unwrap_or(0))
        };
        // places: backlog (if open) = None, then columns 0..n
        let mut c: i32 = place.map_or(-1, |c| c as i32);
        loop {
            c += dx;
            if c < -1 || c >= ncols as i32 || (c == -1 && !self.board.backlog_open) {
                return;
            }
            let list = if c == -1 {
                match self.board.side {
                    SidePanel::Epics => self.project.epics().into_iter().map(|k| self.project.tickets[k].id).collect(),
                    SidePanel::Releases => vec![],
                    SidePanel::Backlog => self.board_list(None, None),
                }
            } else {
                sprint.map(|s| self.board_list(Some(s), Some(c as usize))).unwrap_or_default()
            };
            if !list.is_empty() {
                self.board.selected = Some(list[idx.min(list.len() - 1)]);
                return;
            }
        }
    }

    /// Show the board for a sprint task (Gantt ▸ Open on Board).
    pub fn open_sprint_on_board(&mut self, uid: u32) {
        self.board.sprint = Some(uid);
        self.board.selected = None;
        self.view.main = super::MainView::Board;
    }

    /// Show only an epic's work (or everything again).
    pub fn board_filter_epic(&mut self, epic: Option<u32>) {
        self.board.filter.epic = match epic {
            Some(e) if self.board.filter.epic != EpicFilter::Epic(e) => EpicFilter::Epic(e),
            _ => EpicFilter::Any,
        };
    }
}

pub fn key(app: &mut ProjectFcgApp, key: egui::Key, m: egui::Modifiers) {
    use egui::Key::*;
    let ctrl = m.ctrl || m.command;
    match key {
        ArrowUp if ctrl => app.board_shift_order(-1),
        ArrowDown if ctrl => app.board_shift_order(1),
        ArrowLeft if ctrl => app.board_shift_column(-1),
        ArrowRight if ctrl => app.board_shift_column(1),
        ArrowUp => app.board_select_step(0, -1),
        ArrowDown => app.board_select_step(0, 1),
        ArrowLeft => app.board_select_step(-1, 0),
        ArrowRight => app.board_select_step(1, 0),
        Enter => {
            if let Some(id) = app.board.selected {
                app.open_ticket_dialog(Some(id));
            }
        }
        F if ctrl => app.board.focus_filter = true,
        Escape => {
            if app.board.selected.is_none() && app.board.filter.active() {
                app.board.filter = BoardFilter::default();
            }
            app.board.selected = None;
            app.board.drag = None;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- drawing

/// Fast id lookup through this frame's index (falls back to a scan).
fn tk<'a>(idx: &std::collections::HashMap<u32, usize>, p: &'a Project, id: u32) -> Option<&'a Ticket> {
    match idx.get(&id).and_then(|&k| p.tickets.get(k)) {
        Some(t) if t.id == id => Some(t),
        _ => p.ticket(id),
    }
}

/// `Project::epic_of` through the index.
fn epic_fast<'a>(idx: &std::collections::HashMap<u32, usize>, p: &'a Project, t: &Ticket) -> Option<&'a Ticket> {
    let e = match t.kind {
        IssueType::Epic => None,
        IssueType::Subtask => t.parent.and_then(|x| tk(idx, p, x)).and_then(|x| x.parent),
        _ => t.parent,
    }?;
    tk(idx, p, e).filter(|x| x.kind == IssueType::Epic)
}

struct Chip {
    text: String,
    fill: Color32,
    dot: Option<Color32>,
}

struct CardLayout {
    title: Arc<egui::Galley>,
    chips: Vec<Chip>,
    h: f32,
}

fn chip_h() -> f32 {
    small_size() + 5.0
}

fn layout_card(ui: &egui::Ui, p: &Project, idx: &std::collections::HashMap<u32, usize>, t: &Ticket, w: f32, done: bool) -> CardLayout {
    let mut job = egui::text::LayoutJob::single_section(
        t.title.clone(),
        egui::TextFormat {
            font_id: FontId::proportional(text_size()),
            color: if done { pal().text_dim } else { pal().text },
            strikethrough: if done { Stroke::new(1.0, pal().text_dim) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping { max_width: (w - 2.0 * CARD_PAD - BADGE - 6.0).max(20.0), max_rows: 3, break_anywhere: false, overflow_character: Some('…') };
    let title = ui.fonts_mut(|f| f.layout_job(job));
    let mut chips = vec![];
    if t.kind == IssueType::Subtask {
        if let Some(par) = t.parent {
            chips.push(Chip { text: format!("↳ {}", p.ticket_key(par)), fill: pal().header_bg, dot: None });
        }
    }
    if let Some(e) = epic_fast(idx, p, t) {
        let name: String = if e.title.chars().count() > 22 { e.title.chars().take(21).collect::<String>() + "…" } else { e.title.clone() };
        chips.push(Chip { text: name, fill: chip_fill(epic_color(e.id)), dot: Some(epic_color(e.id)) });
    }
    // (through the frame's index: this runs for every card)
    let mut waits: Vec<u32> = t.after_tasks.clone();
    if t.kind == IssueType::Subtask {
        if let Some(par) = t.parent.and_then(|x| tk(idx, p, x)) {
            waits.extend(par.after_tasks.iter().copied());
        }
    }
    for u in waits {
        if let Some(task) = p.task_by_uid(u).filter(|x| x.calc.percent_complete < 100.0) {
            let name: String = if task.name.chars().count() > 20 { task.name.chars().take(19).collect::<String>() + "…" } else { task.name.clone() };
            chips.push(Chip { text: format!("waits: {name}"), fill: chip_fill(pal().warn), dot: Some(pal().warn) });
        }
    }
    if let Some(r) = t.release.and_then(|r| p.release(r)) {
        chips.push(Chip { text: format!("⚑ {}", r.name), fill: pal().header_bg, dot: None });
    }
    for l in &t.labels {
        chips.push(Chip { text: l.clone(), fill: pal().tint, dot: None });
    }
    let mut h = CARD_PAD + title.size().y.max(BADGE) + 6.0;
    if !chips.is_empty() {
        h += chip_h() + 5.0;
    }
    h += small_size() + 6.0 + CARD_PAD - 2.0;
    CardLayout { title, chips, h }
}

fn paint_card(app: &ProjectFcgApp, p: &egui::Painter, r: Rect, t: &Ticket, lay: &CardLayout, selected: bool, ghost: bool) {
    let pr = &app.project;
    let th = pal();
    let fill = if ghost { th.bg.gamma_multiply(0.92) } else { th.bg };
    if ghost {
        p.rect_filled(r.translate(vec2(3.0, 4.0)), 5.0, Color32::from_black_alpha(50));
    }
    p.rect_filled(r, 5.0, fill);
    let blocked = !pr.is_done(t) && (!pr.blockers(t.id).is_empty() || !pr.waiting_on_tasks(t.id).is_empty());
    let conflict = !pr.dependency_conflicts(t.id).is_empty();
    if blocked {
        // red edge for a ticket waiting on another
        let edge = Rect::from_min_max(r.min, pos2(r.left() + 4.0, r.bottom()));
        p.rect_filled(edge, egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, th.warn);
    }
    let (bw, bc) = if selected { (2.0, th.active_cell) } else { (1.0, th.header_border) };
    p.rect_stroke(r, 5.0, Stroke::new(bw, bc), StrokeKind::Inside);
    let badge = Rect::from_min_size(pos2(r.left() + CARD_PAD, r.top() + CARD_PAD + 1.0), vec2(BADGE, BADGE));
    paint_type_badge(p, badge, t.kind);
    p.galley(pos2(badge.right() + 6.0, r.top() + CARD_PAD), lay.title.clone(), th.text);
    let sf = FontId::proportional(small_size());
    // chips row
    if !lay.chips.is_empty() {
        let cy = r.top() + CARD_PAD + lay.title.size().y.max(BADGE) + 6.0;
        let mut x = r.left() + CARD_PAD;
        let right = r.right() - CARD_PAD;
        let cf = FontId::proportional(small_size() - 1.0);
        for (k, c) in lay.chips.iter().enumerate() {
            let g = p.layout_no_wrap(c.text.clone(), cf.clone(), th.text);
            let dot_w = if c.dot.is_some() { 10.0 } else { 0.0 };
            let w = g.size().x + 10.0 + dot_w;
            if x + w > right {
                let more = lay.chips.len() - k;
                p.text(pos2(x, cy + chip_h() / 2.0), Align2::LEFT_CENTER, format!("+{more}"), cf.clone(), th.text_dim);
                break;
            }
            let cr = Rect::from_min_size(pos2(x, cy), vec2(w, chip_h()));
            p.rect_filled(cr, 3.0, c.fill);
            if let Some(d) = c.dot {
                p.circle_filled(pos2(cr.left() + 7.0, cr.center().y), 3.0, d);
            }
            p.galley(pos2(cr.left() + 5.0 + dot_w, cr.center().y - g.size().y / 2.0), g, th.text);
            x += w + 4.0;
        }
    }
    // footer: key, priority, blocked, sub-tasks | points, assignee
    let fy = r.bottom() - CARD_PAD - small_size() / 2.0 - 1.0;
    let done = pr.is_done(t);
    let key = pr.ticket_key(t.id);
    let id_text = if done { format!("✔ {key}") } else { key };
    let g = p.layout_no_wrap(id_text, sf.clone(), if done { th.accent } else { th.text_dim });
    let mut lx = r.left() + CARD_PAD + g.size().x + 8.0;
    p.galley(pos2(r.left() + CARD_PAD, fy - g.size().y / 2.0), g, th.text);
    paint_priority(p, pos2(lx + 4.0, fy), t.priority);
    lx += 16.0;
    if blocked || conflict {
        let g = p.layout_no_wrap(if conflict { "Planned too early" } else { "Blocked" }.into(), FontId::proportional(small_size() - 1.0), th.warn);
        p.galley(pos2(lx, fy - g.size().y / 2.0), g.clone(), th.warn);
        lx += g.size().x + 8.0;
    }
    let subs: Vec<&Ticket> = if t.kind.is_standard() { pr.tickets.iter().filter(|x| x.parent == Some(t.id) && x.kind == IssueType::Subtask).collect() } else { vec![] };
    if !subs.is_empty() {
        let d = subs.iter().filter(|x| pr.is_done(x)).count();
        let sq = Rect::from_center_size(pos2(lx + 4.0, fy), vec2(8.0, 8.0));
        p.rect_stroke(sq, 1.5, Stroke::new(1.2, th.text_dim), StrokeKind::Inside);
        if d == subs.len() {
            p.rect_filled(sq.shrink(2.0), 0.5, th.accent);
        }
        p.text(pos2(lx + 11.0, fy), Align2::LEFT_CENTER, format!("{d}/{}", subs.len()), FontId::proportional(small_size() - 1.0), th.text_dim);
    }
    let mut x = r.right() - CARD_PAD;
    if let Some(a) = t.assignee.and_then(|u| pr.resources.iter().find(|r| r.uid == u)) {
        let ini = initials(&a.name, &a.initials);
        let rad = small_size() * 0.85;
        let c = pos2(x - rad, fy);
        p.circle_filled(c, rad, th.band);
        p.text(c, Align2::CENTER_CENTER, ini, FontId::proportional(small_size() - 2.0), th.on_band);
        x -= rad * 2.0 + 6.0;
    }
    let pts = match t.points {
        Some(v) => crate::duration::fmt_num(v),
        None if pr.board.estimate == Estimate::StoryPoints && t.kind.is_standard() => "?".into(),
        None => String::new(),
    };
    if !pts.is_empty() {
        // sub-task points are informational (not counted): dimmed
        let col = if t.kind.is_standard() { th.heading } else { th.text_dim };
        let g = p.layout_no_wrap(pts, sf, col);
        let br = Rect::from_min_size(pos2(x - g.size().x - 10.0, fy - g.size().y / 2.0 - 1.0), g.size() + vec2(10.0, 2.0));
        p.rect_filled(br, 8.0, th.tint);
        p.galley(br.min + vec2(5.0, 1.0), g, col);
    }
}

fn hover_text(p: &Project, t: &Ticket) -> String {
    let mut s = format!("{} {} · {}\n{}", t.kind.label(), p.ticket_key(t.id), t.priority.label(), t.title);
    if !t.description.is_empty() {
        s += &format!("\n\n{}", t.description);
    }
    for u in &t.after_tasks {
        if let Some(task) = p.task_by_uid(*u) {
            s += &format!("\nwaits on task: {} ({})", task.name, if task.calc.percent_complete >= 100.0 { "done".to_string() } else { format!("finishes {}", task.calc.finish.map(fmt_date).unwrap_or_default()) });
        }
    }
    let waiting: Vec<String> = p.tasks_waiting_on(t.id).into_iter().map(|r| p.tasks[r].name.clone()).collect();
    if !waiting.is_empty() {
        s += &format!("\ntasks waiting on it: {}", waiting.join(", "));
    }
    if let Some(r) = t.release.and_then(|r| p.release(r)) {
        s += &format!("\nrelease: {}", r.name);
    }
    for c in p.dependency_conflicts(t.id) {
        s += &format!("\n⚠ {c}");
    }
    let links = p.ticket_links(t.id);
    if !links.is_empty() {
        s += "\n";
        for l in links {
            let other = p.ticket(l.other).map(|o| o.title.clone()).unwrap_or_default();
            s += &format!("\n{} {} {}", l.verb(), p.ticket_key(l.other), other);
        }
    }
    s
}

/// Draw a vertical list of cards in `area` starting at `top`, recording
/// their rects and handling clicks/drags/menus. Returns the content height.
#[allow(clippy::too_many_arguments)]
fn card_list(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect, top: f32, ids: &[u32], painter: &egui::Painter, actions: &mut Vec<Action>) -> f32 {
    let w = area.width();
    let mut y = top;
    for &id in ids {
        let Some(t) = tk(&app.board.idx, &app.project, id).cloned() else { continue };
        let done = app.project.is_done(&t);
        let lay = layout_card(ui, &app.project, &app.board.idx, &t, w, done);
        let r = Rect::from_min_size(pos2(area.left(), y), vec2(w, lay.h));
        y += lay.h + 8.0;
        app.board.card_rects.push((id, r));
        let dragging_me = app.board.drag.as_ref().is_some_and(|d| d.id == id);
        if r.bottom() >= area.top() && r.top() <= area.bottom() {
            if dragging_me {
                // leave a dashed outline where it came from
                painter.rect_stroke(r, 5.0, Stroke::new(1.0, pal().text_dim), StrokeKind::Inside);
            } else {
                paint_card(app, painter, r, &t, &lay, app.board.selected == Some(id), false);
            }
        }
        let vis = r.intersect(area);
        if vis.height() <= 0.0 {
            continue;
        }
        let resp = ui.interact(vis, ui.scope_id().with(("ticket_card", id)), Sense::click_and_drag());
        if resp.clicked() || resp.drag_started() {
            app.board.selected = Some(id);
        }
        if resp.double_clicked() {
            actions.push(Action::Edit(id));
        }
        if resp.drag_started() {
            if let Some(pp) = resp.interact_pointer_pos() {
                app.board.drag = Some(Drag { id, grab: pp - r.min, size: r.size() });
            }
        }
        let resp = resp.on_hover_text_at_pointer(hover_text(&app.project, &t));
        resp.context_menu(|ui| {
            app.board.selected = Some(id);
            card_menu(app, ui, &t, actions);
        });
    }
    y - top
}

fn card_menu(app: &ProjectFcgApp, ui: &mut egui::Ui, t: &Ticket, actions: &mut Vec<Action>) {
    let id = t.id;
    if ui.button("Edit…").clicked() {
        actions.push(Action::Edit(id));
        ui.close();
    }
    if t.kind.is_standard() && ui.button("Add Sub-task…").clicked() {
        actions.push(Action::AddSubtask(id));
        ui.close();
    }
    ui.separator();
    if t.sprint.is_some() {
        for (c, name) in app.project.board.columns.clone().iter().enumerate() {
            if c != t.column && ui.button(format!("Move to {name}")).clicked() {
                actions.push(Action::Move(id, t.sprint, c));
                ui.close();
            }
        }
        if t.kind != IssueType::Subtask && ui.button("Send to Backlog").clicked() {
            actions.push(Action::Move(id, None, 0));
            ui.close();
        }
    }
    if t.kind != IssueType::Subtask && !app.project.dependency_conflicts(id).is_empty() {
        if let Some(fit) = app.project.first_fitting_sprint(id).filter(|&f| Some(f) != t.sprint) {
            let name = app.project.task_by_uid(fit).map(|x| x.name.clone()).unwrap_or_default();
            if ui.button(format!("Move to {name} (after what it waits on)")).clicked() {
                actions.push(Action::Move(id, Some(fit), 0));
                ui.close();
            }
        }
    }
    if t.kind != IssueType::Subtask {
        ui.menu_button("Move to Sprint", |ui| {
            for row in app.project.sprint_rows() {
                let s = &app.project.tasks[row];
                if Some(s.uid) != t.sprint && ui.button(&s.name).clicked() {
                    actions.push(Action::Move(id, Some(s.uid), t.column));
                    ui.close();
                }
            }
        });
    }
    if t.kind.is_standard() {
        ui.menu_button("Epic", |ui| {
            if ui.selectable_label(t.parent.is_none(), "No epic").clicked() {
                actions.push(Action::SetEpic(id, None));
                ui.close();
            }
            for k in app.project.epics() {
                let e = &app.project.tickets[k];
                if ui.selectable_label(t.parent == Some(e.id), format!("{}  {}", app.project.ticket_key(e.id), e.title)).clicked() {
                    actions.push(Action::SetEpic(id, Some(e.id)));
                    ui.close();
                }
            }
        });
    }
    if t.kind.is_standard() && !app.project.board.releases.is_empty() {
        ui.menu_button("Release", |ui| {
            if ui.selectable_label(t.release.is_none(), "No release").clicked() {
                actions.push(Action::SetRelease(id, None));
                ui.close();
            }
            for r in app.project.board.releases.iter().filter(|r| r.released.is_none() || t.release == Some(r.id)) {
                if ui.selectable_label(t.release == Some(r.id), &r.name).clicked() {
                    actions.push(Action::SetRelease(id, Some(r.id)));
                    ui.close();
                }
            }
        });
    }
    ui.menu_button("Priority", |ui| {
        for pr in Priority::ALL {
            if ui.selectable_label(t.priority == pr, pr.label()).clicked() {
                actions.push(Action::SetPriority(id, pr));
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button("Delete").clicked() {
        actions.push(Action::Delete(id));
        ui.close();
    }
}

enum Action {
    Edit(u32),
    Move(u32, Option<u32>, usize),
    Delete(u32),
    Add(Option<u32>, String),
    AddEpic(String),
    AddSubtask(u32),
    SetEpic(u32, Option<u32>),
    SetPriority(u32, Priority),
    SetRelease(u32, Option<u32>),
    AddRelease(String),
    EditRelease(u32),
}

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let th = pal();
    ui.painter().rect_filled(rect, 0.0, th.panel);
    app.board.card_rects.clear();
    app.board.column_rects.clear();
    app.board.epic_rects.clear();
    app.board.release_rects.clear();
    app.board.idx = app.project.tickets.iter().enumerate().map(|(k, t)| (t.id, k)).collect();
    let sprint = app.board_sprint();
    app.board.sprint = sprint;
    if app.board.selected.is_some_and(|id| app.project.ticket(id).is_none()) {
        app.board.selected = None;
    }
    if let EpicFilter::Epic(e) = app.board.filter.epic {
        if app.project.ticket(e).is_none_or(|t| t.kind != IssueType::Epic) {
            app.board.filter.epic = EpicFilter::Any;
        }
    }
    if let Some(Some(r)) = app.board.filter.release {
        if app.project.release(r).is_none() {
            app.board.filter.release = None;
        }
    }
    let mut actions: Vec<Action> = vec![];

    // ---------- toolbar + filter bar ----------
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), TOOLBAR_H));
    ui.painter().rect_filled(bar, 0.0, th.ribbon);
    toolbar(app, ui, bar.shrink2(vec2(10.0, 4.0)), sprint);
    let fbar = Rect::from_min_size(pos2(rect.left(), bar.bottom()), vec2(rect.width(), FILTER_H));
    ui.painter().rect_filled(fbar, 0.0, th.ribbon);
    ui.painter().line_segment([fbar.left_bottom(), fbar.right_bottom()], Stroke::new(1.0, th.header_border));
    filter_bar(app, ui, fbar.shrink2(vec2(10.0, 3.0)));

    // ---------- regions ----------
    let body = Rect::from_min_max(pos2(rect.left(), fbar.bottom()), rect.max);
    let backlog_w = if app.board.backlog_open { BACKLOG_W } else { 34.0 };
    let charts_w = if app.board.show_charts && body.width() > 900.0 { CHARTS_W } else { 0.0 };
    let backlog = Rect::from_min_size(body.min, vec2(backlog_w, body.height()));
    let charts = Rect::from_min_max(pos2(body.right() - charts_w, body.top()), body.max);
    let center = Rect::from_min_max(pos2(backlog.right(), body.top()), pos2(charts.left(), body.bottom()));

    side_panel(app, ui, backlog, &mut actions);
    if charts_w > 0.0 {
        charts_panel(app, ui, charts, sprint);
    }
    match sprint {
        Some(s) => columns(app, ui, center, s, &mut actions),
        None => empty_state(app, ui, center),
    }

    // ---------- drag and drop ----------
    drag_and_drop(app, ui, sprint);

    // ---------- apply ----------
    for a in actions {
        match a {
            Action::Edit(id) => app.open_ticket_dialog(Some(id)),
            Action::Delete(id) => {
                app.board.selected = Some(id);
                app.board_delete_selected();
            }
            Action::Move(id, s, c) => {
                let label = match s {
                    None => "Move to Backlog".to_string(),
                    Some(_) => format!("Move to {}", app.project.board.columns.get(c).cloned().unwrap_or_default()),
                };
                app.edit_project(&label, |p| p.move_ticket(id, s, c, None));
            }
            Action::Add(s, title) => {
                // quick-added tickets join the epic being filtered on
                let epic = match app.board.filter.epic {
                    EpicFilter::Epic(e) => Some(e),
                    _ => None,
                };
                if let Some(id) = app.edit_project("New Ticket", |p| p.add_ticket_of(&title, s, IssueType::Story, epic)) {
                    app.board.selected = Some(id);
                }
            }
            Action::AddEpic(title) => {
                if let Some(id) = app.edit_project("New Epic", |p| p.add_ticket_of(&title, None, IssueType::Epic, None)) {
                    app.board.selected = Some(id);
                }
            }
            Action::AddSubtask(id) => app.open_new_ticket(IssueType::Subtask, Some(id)),
            Action::SetEpic(id, e) => {
                app.edit_project(if e.is_some() { "Set Epic" } else { "Remove from Epic" }, |p| p.set_ticket_parent(id, e));
            }
            Action::SetRelease(id, r) => {
                app.edit_project(if r.is_some() { "Set Release" } else { "Remove from Release" }, |p| p.set_ticket_release(id, r));
            }
            Action::AddRelease(name) => {
                app.edit_project("New Release", |p| p.add_release(&name, None));
            }
            Action::EditRelease(r) => app.open_release_dialog(Some(r)),
            Action::SetPriority(id, pr) => {
                app.edit_project("Set Priority", |p| {
                    let k = p.ticket_index(id).ok_or("No such ticket.")?;
                    p.tickets[k].priority = pr;
                    Ok(())
                });
            }
        }
    }
}

fn toolbar(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, sprint: Option<u32>) {
    let ctx = ui.ctx().clone();
    ui.scope_builder(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        let rows = app.project.sprint_rows();
        let name_of = |u: Option<u32>, app: &ProjectFcgApp| {
            u.and_then(|u| app.project.task_by_uid(u)).map(|t| t.name.clone()).unwrap_or_else(|| "No sprints".into())
        };
        ui.label(RichText::new("Sprint").color(pal().text_dim));
        let cur = name_of(sprint, app);
        let pos = sprint.and_then(|s| rows.iter().position(|&i| app.project.tasks[i].uid == s));
        if ui.add_enabled(pos.is_some_and(|p| p > 0), egui::Button::new("◀").small()).on_hover_text("Previous sprint").clicked() {
            app.board.sprint = Some(app.project.tasks[rows[pos.unwrap() - 1]].uid);
        }
        egui::ComboBox::from_id_salt("board_sprint").selected_text(RichText::new(cur).strong()).width(160.0).show_ui(ui, |ui| {
            for &i in &rows {
                let t = &app.project.tasks[i];
                let label = match (t.calc.start, t.calc.finish) {
                    (Some(s), Some(f)) => format!("{}   {} – {}", t.name, fmt_date(s), fmt_date(f)),
                    _ => t.name.clone(),
                };
                if ui.selectable_label(sprint == Some(t.uid), label).clicked() {
                    app.board.sprint = Some(t.uid);
                }
            }
        });
        if ui.add_enabled(pos.is_some_and(|p| p + 1 < rows.len()), egui::Button::new("▶").small()).on_hover_text("Next sprint").clicked() {
            app.board.sprint = Some(app.project.tasks[rows[pos.unwrap() + 1]].uid);
        }
        if let Some(t) = sprint.and_then(|u| app.project.task_by_uid(u)) {
            if let (Some(s), Some(f)) = (t.calc.start, t.calc.finish) {
                let n = now();
                let mpd = app.project.settings.durations.minutes_per_day.max(1) as f64;
                let left = if n >= f { 0.0 } else { app.project.calendar.work_between(n.max(s), f) as f64 / mpd };
                let when = if n < s {
                    format!("{} – {} · starts in {} days", fmt_date(s), fmt_date(f), (s.date() - n.date()).num_days())
                } else if n > f {
                    format!("{} – {} · ended", fmt_date(s), fmt_date(f))
                } else {
                    format!("{} – {} · {} working days left", fmt_date(s), fmt_date(f), crate::duration::fmt_num(left.ceil()))
                };
                ui.add_space(8.0);
                ui.label(RichText::new(when).color(pal().text_dim));
            }
            let st = app.project.sprint_stats(Some(t.uid));
            let est = app.project.board.estimate;
            ui.add_space(8.0);
            let text = format!("{} of {} done", crate::duration::fmt_num(st.done), est.unit(st.total));
            ui.add(egui::ProgressBar::new((st.percent() / 100.0) as f32).desired_width(170.0).text(text));
            if st.unestimated > 0 {
                ui.label(RichText::new(format!("{} not estimated", st.unestimated)).color(pal().warn))
                    .on_hover_text("Tickets without story points count as 0. Double-click a card to estimate it.");
            }
        }
        if let Some(g) = sprint.and_then(|u| app.project.board.sprint_meta(u)).map(|m| m.goal.clone()).filter(|g| !g.is_empty()) {
            // only in the room the buttons on the right leave
            let room = ui.available_width() - 450.0;
            if room > 90.0 {
                ui.add_space(8.0);
                let r = ui.allocate_ui(vec2(room, 24.0), |ui| {
                    ui.add(egui::Label::new(RichText::new(format!("Goal: {g}")).italics().color(pal().heading)).truncate().sense(Sense::click()))
                });
                if r.inner.on_hover_text(format!("{g}\n\nClick to edit the goal")).clicked() {
                    app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Goal);
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut charts = app.board.show_charts;
            if ui.toggle_value(&mut charts, "📉 Charts").on_hover_text("Burndown and velocity").changed() {
                app.board.show_charts = charts;
            }
            if let Some(u) = sprint {
                let m = app.project.board.sprint_meta(u);
                if m.is_some_and(|m| m.completed.is_some()) {
                    ui.label(RichText::new("✔ Completed").color(pal().accent));
                } else if m.is_some_and(|m| m.started.is_some()) {
                    if ui.button("Complete Sprint").on_hover_text("Freeze its velocity and move unfinished tickets on").clicked() {
                        app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Complete);
                    }
                } else if ui.button("Start Sprint").on_hover_text("Record the goal and the committed scope").clicked() {
                    app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Start);
                }
            }
            if ui.button("📆 New Sprint").on_hover_text("Add a sprint to the schedule after the last one").clicked() {
                app.execute(super::commands::Command::NewSprint, &ctx);
            }
            if ui.button(RichText::new("✚ New Ticket").strong()).on_hover_text("New ticket (Insert)").clicked() {
                app.board_new_ticket();
            }
        });
    });
}

fn filter_bar(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        let f = &mut app.board.filter;
        let id = egui::Id::unique("board_filter_text");
        let te = ui.add(egui::TextEdit::singleline(&mut f.text).id(id).hint_text("🔍 Filter tickets (Ctrl+F)").desired_width(190.0));
        if app.board.focus_filter {
            te.request_focus();
            app.board.focus_filter = false;
        }
        if te.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.board.filter.text.clear();
        }
        let p = &app.project;
        let f = &mut app.board.filter;
        // epic
        let epic_name = match f.epic {
            EpicFilter::Any => "All epics".to_string(),
            EpicFilter::NoEpic => "No epic".to_string(),
            EpicFilter::Epic(e) => p.ticket(e).map(|t| t.title.clone()).unwrap_or_default(),
        };
        egui::ComboBox::from_id_salt("board_f_epic").selected_text(epic_name).width(140.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut f.epic, EpicFilter::Any, "All epics");
            ui.selectable_value(&mut f.epic, EpicFilter::NoEpic, "No epic");
            for k in p.epics() {
                let e = &p.tickets[k];
                ui.selectable_value(&mut f.epic, EpicFilter::Epic(e.id), format!("{}  {}", p.ticket_key(e.id), e.title));
            }
        });
        egui::ComboBox::from_id_salt("board_f_type").selected_text(f.kind.map(|k| k.label()).unwrap_or("All types")).width(90.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut f.kind, None, "All types");
            for k in IssueType::ALL.into_iter().filter(|k| *k != IssueType::Epic) {
                ui.selectable_value(&mut f.kind, Some(k), k.label());
            }
        });
        if !p.board.releases.is_empty() || f.release.is_some() {
            let name = match f.release {
                None => "All releases".to_string(),
                Some(None) => "No release".to_string(),
                Some(Some(r)) => p.release(r).map(|x| x.name.clone()).unwrap_or_default(),
            };
            egui::ComboBox::from_id_salt("board_f_release").selected_text(name).width(100.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut f.release, None, "All releases");
                ui.selectable_value(&mut f.release, Some(None), "No release");
                for r in &p.board.releases {
                    ui.selectable_value(&mut f.release, Some(Some(r.id)), &r.name);
                }
            });
        }
        let labels = p.all_labels();
        if !labels.is_empty() || f.label.is_some() {
            egui::ComboBox::from_id_salt("board_f_label").selected_text(f.label.clone().unwrap_or_else(|| "All labels".into())).width(100.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut f.label, None, "All labels");
                for l in labels {
                    ui.selectable_value(&mut f.label, Some(l.clone()), l);
                }
            });
        }
        let who = match f.assignee {
            None => "Anyone".to_string(),
            Some(None) => "Unassigned".to_string(),
            Some(Some(u)) => p.resources.iter().find(|r| r.uid == u).map(|r| r.name.clone()).unwrap_or_default(),
        };
        egui::ComboBox::from_id_salt("board_f_who").selected_text(who).width(110.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut f.assignee, None, "Anyone");
            ui.selectable_value(&mut f.assignee, Some(None), "Unassigned");
            for r in p.resources.iter().filter(|r| r.rtype == crate::model::ResourceType::Work) {
                ui.selectable_value(&mut f.assignee, Some(Some(r.uid)), &r.name);
            }
        });
        let lanes = &mut app.board.lanes;
        egui::ComboBox::from_id_salt("board_lanes").selected_text(lanes.label()).width(150.0).show_ui(ui, |ui| {
            for l in Swimlanes::ALL {
                ui.selectable_value(lanes, l, l.label());
            }
        });
        let f = &mut app.board.filter;
        if f.active() {
            if ui.button("✖ Clear").on_hover_text("Show every ticket (Esc)").clicked() {
                *f = BoardFilter::default();
            }
            let all = p.tickets.iter().filter(|t| t.kind != IssueType::Epic).count();
            let shown = p.tickets.iter().filter(|t| t.kind != IssueType::Epic && f.matches(p, t)).count();
            ui.label(RichText::new(format!("Showing {shown} of {all}")).color(pal().text_dim));
        }
    });
}

fn panel_scroll(ui: &egui::Ui, area: Rect, scroll: &mut f32, content: f32) {
    if ui.rect_contains_pointer(area) {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        *scroll -= dy;
    }
    *scroll = scroll.clamp(0.0, (content - area.height() + 40.0).max(0.0));
}

fn side_panel(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, actions: &mut Vec<Action>) {
    let th = pal();
    let p = ui.painter_at(r);
    p.rect_filled(r, 0.0, th.header_bg);
    p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, th.header_border));
    app.board.backlog_rect = r;
    // collapse toggle
    let tr = Rect::from_min_size(pos2(r.right() - 30.0, r.top() + 6.0), vec2(24.0, 24.0));
    let tresp = ui.interact(tr, ui.scope_id().with("backlog_toggle"), Sense::click());
    p.text(tr.center(), Align2::CENTER_CENTER, if app.board.backlog_open { "⏴" } else { "⏵" }, FontId::proportional(14.0), if tresp.hovered() { th.accent } else { th.text_dim });
    if tresp.on_hover_text(if app.board.backlog_open { "Hide the panel" } else { "Show the backlog, epics and releases" }).clicked() {
        app.board.backlog_open = !app.board.backlog_open;
    }
    if !app.board.backlog_open {
        let g = p.layout_no_wrap("BACKLOG · EPICS · RELEASES".into(), FontId::proportional(11.0), th.text_dim);
        p.add(egui::epaint::TextShape::new(pos2(r.center().x - g.size().y / 2.0, r.top() + 50.0 + g.size().x), g, th.text_dim).with_angle(-std::f32::consts::FRAC_PI_2));
        return;
    }
    // tabs
    let mut x = r.left() + 10.0;
    for (tab, name) in [(SidePanel::Backlog, "Backlog"), (SidePanel::Epics, "Epics"), (SidePanel::Releases, "Releases")] {
        let on = app.board.side == tab;
        let g = p.layout_no_wrap(name.into(), FontId::proportional(text_size() + 1.0), if on { th.heading } else { th.text_dim });
        let tr = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(g.size().x + 8.0, 26.0));
        let resp = ui.interact(tr, ui.scope_id().with(("side_tab", name)), Sense::click());
        p.galley(pos2(tr.left() + 4.0, tr.center().y - g.size().y / 2.0), g, th.text);
        if on {
            p.line_segment([pos2(tr.left() + 2.0, tr.bottom()), pos2(tr.right() - 2.0, tr.bottom())], Stroke::new(2.5, th.accent));
        } else if resp.hovered() {
            p.line_segment([pos2(tr.left() + 2.0, tr.bottom()), pos2(tr.right() - 2.0, tr.bottom())], Stroke::new(1.0, th.text_dim));
        }
        if resp.clicked() {
            app.board.side = tab;
        }
        x = tr.right() + 8.0;
    }
    match app.board.side {
        SidePanel::Backlog => backlog_list(app, ui, r, actions),
        SidePanel::Epics => epics_list(app, ui, r, actions),
        SidePanel::Releases => releases_list(app, ui, r, actions),
    }
}

fn backlog_list(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, actions: &mut Vec<Action>) {
    let th = pal();
    let p = ui.painter_at(r);
    let st = app.project.sprint_stats(None);
    p.text(
        pos2(r.left() + 12.0, r.top() + 44.0),
        Align2::LEFT_CENTER,
        format!("{} · {}", if st.tickets == 1 { "1 ticket".into() } else { format!("{} tickets", st.tickets) }, app.project.board.estimate.unit(st.total)),
        FontId::proportional(small_size()),
        th.text_dim,
    );
    // quick add
    let qa = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 56.0), vec2(r.width() - 20.0, 24.0));
    let te = ui.put(qa, egui::TextEdit::singleline(&mut app.board.quick_backlog).hint_text("+ Add to backlog…"));
    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.board.quick_backlog.trim().is_empty() {
        actions.push(Action::Add(None, std::mem::take(&mut app.board.quick_backlog)));
        te.request_focus();
    }
    let list = Rect::from_min_max(pos2(r.left() + 10.0, qa.bottom() + 10.0), pos2(r.right() - 10.0, r.bottom() - 6.0));
    let ids = app.board_list(None, None);
    let lp = ui.painter_at(list.expand2(vec2(4.0, 0.0)));
    let top = list.top() - app.board.backlog_scroll;
    let h = card_list(app, ui, list, top, &ids, &lp, actions);
    if ids.is_empty() {
        let msg = if app.board.filter.active() { "No backlog tickets match the filter" } else { "Nothing in the backlog" };
        lp.text(list.center_top() + vec2(0.0, 30.0), Align2::CENTER_CENTER, msg, FontId::proportional(small_size()), th.text_dim);
    }
    let mut s = app.board.backlog_scroll;
    panel_scroll(ui, list, &mut s, h);
    app.board.backlog_scroll = s;
}

fn epics_list(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, actions: &mut Vec<Action>) {
    let th = pal();
    let p = ui.painter_at(r);
    let n = app.project.epics().len();
    p.text(
        pos2(r.left() + 12.0, r.top() + 44.0),
        Align2::LEFT_CENTER,
        format!("{} · click to filter, drop a card to add it", if n == 1 { "1 epic".to_string() } else { format!("{n} epics") }),
        FontId::proportional(small_size()),
        th.text_dim,
    );
    let qa = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 56.0), vec2(r.width() - 20.0, 24.0));
    let te = ui.put(qa, egui::TextEdit::singleline(&mut app.board.quick_epic).hint_text("+ New epic…"));
    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.board.quick_epic.trim().is_empty() {
        actions.push(Action::AddEpic(std::mem::take(&mut app.board.quick_epic)));
        te.request_focus();
    }
    let list = Rect::from_min_max(pos2(r.left() + 10.0, qa.bottom() + 10.0), pos2(r.right() - 10.0, r.bottom() - 6.0));
    let lp = ui.painter_at(list.expand2(vec2(4.0, 0.0)));
    let mut y = list.top() - app.board.epics_scroll;
    let est = app.project.board.estimate;
    let drop_hover = app.board.drag.as_ref().map(|d| d.id);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    for k in app.project.epics() {
        let e = app.project.tickets[k].clone();
        let st = app.project.epic_stats(e.id);
        let task = e.task.and_then(|u| app.project.task_by_uid(u)).map(|t| t.name.clone());
        let h = 66.0 + if task.is_some() { small_size() + 4.0 } else { 0.0 };
        let er = Rect::from_min_size(pos2(list.left(), y), vec2(list.width(), h));
        y += h + 8.0;
        app.board.epic_rects.push((e.id, er));
        if er.bottom() < list.top() || er.top() > list.bottom() {
            continue;
        }
        let filtered = app.board.filter.epic == EpicFilter::Epic(e.id);
        let selected = app.board.selected == Some(e.id);
        let hot = drop_hover.is_some() && pointer.is_some_and(|pp| er.contains(pp));
        let c = epic_color(e.id);
        lp.rect_filled(er, 5.0, if hot { chip_fill(c) } else { th.bg });
        lp.rect_filled(Rect::from_min_max(er.min, pos2(er.left() + 5.0, er.bottom())), egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, c);
        let (bw, bc) = if selected || filtered { (2.0, th.active_cell) } else { (1.0, th.header_border) };
        lp.rect_stroke(er, 5.0, Stroke::new(bw, bc), StrokeKind::Inside);
        let tx = er.left() + 14.0;
        let done = app.project.is_done(&e);
        let title: String = if e.title.chars().count() > 30 { e.title.chars().take(29).collect::<String>() + "…" } else { e.title.clone() };
        lp.text(pos2(tx, er.top() + 14.0), Align2::LEFT_CENTER, title, FontId::proportional(text_size()), if done { th.text_dim } else { th.text });
        let key = app.project.ticket_key(e.id);
        lp.text(pos2(er.right() - 8.0, er.top() + 14.0), Align2::RIGHT_CENTER, if done { format!("✔ {key}") } else { key }, FontId::proportional(small_size()), th.text_dim);
        // progress
        let bar = Rect::from_min_size(pos2(tx, er.top() + 30.0), vec2(er.right() - 8.0 - tx, 7.0));
        lp.rect_filled(bar, 3.5, th.header_bg);
        let frac = (st.percent() / 100.0) as f32;
        if frac > 0.0 {
            lp.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 3.5, c);
        }
        let text = if st.tickets == 0 {
            "No stories yet".to_string()
        } else {
            format!("{} of {} done · {:.0}%", crate::duration::fmt_num(st.done), est.unit(st.total), st.percent())
        };
        lp.text(pos2(tx, er.top() + 50.0), Align2::LEFT_CENTER, text, FontId::proportional(small_size()), th.text_dim);
        if let Some(tn) = task {
            lp.text(pos2(tx, er.top() + 50.0 + small_size() + 4.0), Align2::LEFT_CENTER, format!("▶ drives \"{tn}\""), FontId::proportional(small_size() - 1.0), th.heading);
        }
        let vis = er.intersect(list);
        let resp = ui.interact(vis, ui.scope_id().with(("epic_row", e.id)), Sense::click());
        if resp.clicked() {
            app.board.selected = Some(e.id);
            app.board_filter_epic(Some(e.id));
        }
        if resp.double_clicked() {
            actions.push(Action::Edit(e.id));
        }
        let resp = resp.on_hover_text_at_pointer(format!("{}\n\nClick to show only this epic's tickets; double-click to edit.", hover_text(&app.project, &e)));
        resp.context_menu(|ui| {
            app.board.selected = Some(e.id);
            if ui.button("Edit…").clicked() {
                actions.push(Action::Edit(e.id));
                ui.close();
            }
            if ui.button(if filtered { "Show All Tickets" } else { "Show Only This Epic" }).clicked() {
                app.board_filter_epic(Some(e.id));
                ui.close();
            }
            ui.separator();
            if ui.button("Delete Epic").on_hover_text("Its stories stay, without an epic").clicked() {
                actions.push(Action::Delete(e.id));
                ui.close();
            }
        });
    }
    if n == 0 {
        lp.text(list.center_top() + vec2(0.0, 30.0), Align2::CENTER_CENTER, "No epics yet", FontId::proportional(small_size()), th.text_dim);
    }
    let content = y + app.board.epics_scroll - list.top();
    let mut s = app.board.epics_scroll;
    panel_scroll(ui, list, &mut s, content);
    app.board.epics_scroll = s;
}

pub fn release_status_color(s: crate::releases::ReleaseStatus) -> Color32 {
    use crate::releases::ReleaseStatus as R;
    match s {
        R::Released | R::Ready => pal().accent,
        R::OnTrack => Color32::from_rgb(46, 125, 50),
        R::AtRisk | R::Unplanned => pal().warn,
        R::NoStories => pal().text_dim,
    }
}

fn releases_list(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, actions: &mut Vec<Action>) {
    let th = pal();
    let p = ui.painter_at(r);
    let n = app.project.board.releases.len();
    p.text(
        pos2(r.left() + 12.0, r.top() + 44.0),
        Align2::LEFT_CENTER,
        format!("{} · click to filter, drop a card to add it", if n == 1 { "1 release".to_string() } else { format!("{n} releases") }),
        FontId::proportional(small_size()),
        th.text_dim,
    );
    let qa = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 56.0), vec2(r.width() - 20.0, 24.0));
    let te = ui.put(qa, egui::TextEdit::singleline(&mut app.board.quick_release).hint_text("+ New release (e.g. 1.2)…"));
    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.board.quick_release.trim().is_empty() {
        actions.push(Action::AddRelease(std::mem::take(&mut app.board.quick_release)));
        te.request_focus();
    }
    let list = Rect::from_min_max(pos2(r.left() + 10.0, qa.bottom() + 10.0), pos2(r.right() - 10.0, r.bottom() - 6.0));
    let lp = ui.painter_at(list.expand2(vec2(4.0, 0.0)));
    let mut y = list.top() - app.board.releases_scroll;
    let est = app.project.board.estimate;
    let dragging = app.board.drag.is_some();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let sf = FontId::proportional(small_size());
    for rel in app.project.board.releases.clone() {
        let st = app.project.release_stats(rel.id);
        let milestone = rel.task.and_then(|u| app.project.task_by_uid(u)).map(|t| t.name.clone());
        let h = 84.0 + if milestone.is_some() { small_size() + 4.0 } else { 0.0 };
        let er = Rect::from_min_size(pos2(list.left(), y), vec2(list.width(), h));
        y += h + 8.0;
        app.board.release_rects.push((rel.id, er));
        if er.bottom() < list.top() || er.top() > list.bottom() {
            continue;
        }
        let filtered = app.board.filter.release == Some(Some(rel.id));
        let hot = dragging && pointer.is_some_and(|pp| er.contains(pp));
        let sc = release_status_color(st.status);
        lp.rect_filled(er, 5.0, if hot { chip_fill(th.accent) } else { th.bg });
        lp.rect_filled(Rect::from_min_max(er.min, pos2(er.left() + 5.0, er.bottom())), egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, sc);
        let (bw, bc) = if filtered { (2.0, th.active_cell) } else { (1.0, th.header_border) };
        lp.rect_stroke(er, 5.0, Stroke::new(bw, bc), StrokeKind::Inside);
        let tx = er.left() + 14.0;
        let released = rel.released.is_some();
        lp.text(pos2(tx, er.top() + 14.0), Align2::LEFT_CENTER, format!("⚑ {}", rel.name), FontId::proportional(text_size()), if released { th.text_dim } else { th.text });
        // status chip
        let g = lp.layout_no_wrap(st.status.label().to_string(), FontId::proportional(small_size() - 1.0), sc);
        let cr = Rect::from_min_size(pos2(er.right() - 8.0 - g.size().x - 10.0, er.top() + 6.0), g.size() + vec2(10.0, 4.0));
        lp.rect_filled(cr, 3.0, chip_fill(sc));
        lp.galley(cr.min + vec2(5.0, 2.0), g, sc);
        // progress
        let bar = Rect::from_min_size(pos2(tx, er.top() + 30.0), vec2(er.right() - 8.0 - tx, 7.0));
        lp.rect_filled(bar, 3.5, th.header_bg);
        let frac = (st.percent() / 100.0) as f32;
        if frac > 0.0 {
            lp.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 3.5, sc);
        }
        let prog = if st.tickets == 0 {
            "No stories yet".to_string()
        } else {
            format!("{} of {} done · {:.0}%", crate::duration::fmt_num(st.done), est.unit(st.total), st.percent())
        };
        lp.text(pos2(tx, er.top() + 50.0), Align2::LEFT_CENTER, prog, sf.clone(), th.text_dim);
        let dates = match (rel.released, rel.date, st.projected) {
            (Some(at), _, _) => format!("Released {}", fmt_date(at)),
            (None, Some(d), Some(pj)) => format!("Due {} · projected {}", d.format("%-m/%-d/%y"), fmt_date(pj)),
            (None, Some(d), None) => format!("Due {}", d.format("%-m/%-d/%y")),
            (None, None, Some(pj)) => format!("Projected {}", fmt_date(pj)),
            (None, None, None) => "No release date".to_string(),
        };
        let dc = if st.status == crate::releases::ReleaseStatus::AtRisk { th.warn } else { th.text_dim };
        lp.text(pos2(tx, er.top() + 68.0), Align2::LEFT_CENTER, dates, sf.clone(), dc);
        if let Some(m) = &milestone {
            lp.text(pos2(tx, er.top() + 68.0 + small_size() + 4.0), Align2::LEFT_CENTER, format!("◆ milestone \"{m}\""), FontId::proportional(small_size() - 1.0), th.heading);
        }
        let vis = er.intersect(list);
        let resp = ui.interact(vis, ui.scope_id().with(("release_row", rel.id)), Sense::click());
        if resp.clicked() {
            let f = &mut app.board.filter.release;
            *f = if *f == Some(Some(rel.id)) { None } else { Some(Some(rel.id)) };
        }
        if resp.double_clicked() {
            actions.push(Action::EditRelease(rel.id));
        }
        let mut tip = format!("Release {}", rel.name);
        if !rel.description.is_empty() {
            tip += &format!("\n{}", rel.description);
        }
        if st.unplanned > 0 {
            tip += &format!("\n{} unfinished stor{} not planned into a sprint", st.unplanned, if st.unplanned == 1 { "y" } else { "ies" });
        }
        if st.unestimated > 0 {
            tip += &format!("\n{} not estimated", st.unestimated);
        }
        tip += "\n\nClick to show only this release's tickets; double-click to edit.";
        let resp = resp.on_hover_text_at_pointer(tip);
        resp.context_menu(|ui| {
            if ui.button("Edit…").clicked() {
                actions.push(Action::EditRelease(rel.id));
                ui.close();
            }
            if ui.button(if filtered { "Show All Tickets" } else { "Show Only This Release" }).clicked() {
                app.board.filter.release = if filtered { None } else { Some(Some(rel.id)) };
                ui.close();
            }
            ui.separator();
            if released {
                if ui.button("Unrelease").clicked() {
                    app.edit_project("Unrelease", |p| p.unrelease_version(rel.id));
                    ui.close();
                }
            } else if ui.button("Release…").on_hover_text("Mark it released; unfinished stories move to another release").clicked() {
                app.open_release_version_dialog(rel.id);
                ui.close();
            }
            match rel.task.and_then(|u| app.project.index_of_uid(u)) {
                Some(row) => {
                    if ui.button("Go to Milestone").clicked() {
                        app.view.main = super::MainView::Gantt;
                        app.select_index(row);
                        if let Some(s) = app.project.tasks[row].calc.start {
                            app.scroll_gantt_to(s);
                        }
                        ui.close();
                    }
                }
                None => {
                    if ui.button("Add Milestone to Schedule").on_hover_text("A milestone that waits on this release's stories, with the release date as its deadline").clicked() {
                        app.edit_project("Add Release Milestone", |p| p.add_release_milestone(rel.id));
                        ui.close();
                    }
                }
            }
            ui.separator();
            if ui.button("Delete Release").on_hover_text("Its stories stay, without a release").clicked() {
                app.edit_project("Delete Release", |p| p.delete_release(rel.id));
                ui.close();
            }
        });
    }
    if n == 0 {
        lp.text(list.center_top() + vec2(0.0, 30.0), Align2::CENTER_CENTER, "No releases yet", FontId::proportional(small_size()), th.text_dim);
    }
    let content = y + app.board.releases_scroll - list.top();
    let mut s = app.board.releases_scroll;
    panel_scroll(ui, list, &mut s, content);
    app.board.releases_scroll = s;
}

fn columns(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect, sprint: u32, actions: &mut Vec<Action>) {
    let th = pal();
    let cols = app.project.board.columns.clone();
    let n = cols.len().max(1);
    let inner = area.shrink(GAP);
    let col_w = ((inner.width() - GAP * (n as f32 - 1.0)) / n as f32).max(120.0);
    let head_h = 34.0;
    let mut max_h: f32 = 0.0;
    let est = app.project.board.estimate;
    // column backgrounds and headers
    let mut bodies: Vec<Rect> = vec![];
    let mut col_ids: Vec<Vec<u32>> = vec![];
    for (c, name) in cols.iter().enumerate() {
        let x = inner.left() + c as f32 * (col_w + GAP);
        let col = Rect::from_min_size(pos2(x, inner.top()), vec2(col_w, inner.height()));
        let p = ui.painter_at(col);
        p.rect_filled(col, 6.0, th.header_bg);
        let ids = app.board_list(Some(sprint), Some(c));
        let units: f64 = ids.iter().filter_map(|&i| tk(&app.board.idx, &app.project, i)).map(|t| app.project.ticket_units(t)).sum();
        let is_done = c + 1 == cols.len();
        // WIP limits count every card in the column, filter or not
        let in_col = app.project.tickets.iter().filter(|t| t.sprint == Some(sprint) && t.column == c && t.kind != IssueType::Epic).count();
        let wip = app.project.board.wip_limit(c);
        let over = wip.is_some_and(|w| in_col as u32 > w);
        if over {
            p.rect_filled(Rect::from_min_size(col.min, vec2(col.width(), head_h)), egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, chip_fill(th.warn));
        }
        p.text(pos2(col.left() + 12.0, col.top() + head_h / 2.0), Align2::LEFT_CENTER, if is_done { format!("✔ {name}") } else { name.clone() }, FontId::proportional(text_size()), th.heading);
        let n_text = match wip {
            Some(w) => format!("{}/{w}", in_col),
            None => ids.len().to_string(),
        };
        let count = if est == Estimate::StoryPoints { format!("{n_text} · {}", crate::duration::fmt_num(units)) } else { n_text };
        p.text(pos2(col.right() - 12.0, col.top() + head_h / 2.0), Align2::RIGHT_CENTER, count, FontId::proportional(small_size()), if over { th.warn } else { th.text_dim });
        bodies.push(Rect::from_min_max(pos2(col.left() + 8.0, col.top() + head_h), pos2(col.right() - 8.0, col.bottom() - 8.0)));
        app.board.column_rects.push(col);
        col_ids.push(ids);
    }
    let body_top = inner.top() + head_h;
    let quick_add = |app: &mut ProjectFcgApp, ui: &mut egui::Ui, body: Rect, y: f32, actions: &mut Vec<Action>| {
        let qa = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), 24.0));
        if qa.bottom() <= body.bottom() && qa.top() >= body.top() {
            let te = ui.put(qa, egui::TextEdit::singleline(&mut app.board.quick_add).hint_text("+ Add a ticket…"));
            if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.board.quick_add.trim().is_empty() {
                actions.push(Action::Add(Some(sprint), std::mem::take(&mut app.board.quick_add)));
                te.request_focus();
            }
        }
    };
    if app.board.lanes == Swimlanes::None {
        for c in 0..cols.len() {
            let body = bodies[c];
            let bp = ui.painter_at(body.expand2(vec2(4.0, 0.0)));
            let top = body.top() - app.board.scroll_y;
            let ids = col_ids[c].clone();
            let mut h = card_list(app, ui, body, top, &ids, &bp, actions);
            // quick add at the bottom of the first column
            if c == 0 {
                quick_add(app, ui, body, top + h, actions);
                h += 30.0;
            }
            max_h = max_h.max(h + head_h);
        }
    } else {
        // swimlanes: one band per epic or per assignee, across all columns
        let lane_of = |app: &ProjectFcgApp, id: u32| -> Option<u32> {
            let t = tk(&app.board.idx, &app.project, id)?;
            match app.board.lanes {
                Swimlanes::Epic => epic_fast(&app.board.idx, &app.project, t).map(|e| e.id),
                _ => t.assignee,
            }
        };
        let mut lanes: Vec<Option<u32>> = vec![];
        for ids in &col_ids {
            for &id in ids {
                let l = lane_of(app, id);
                if !lanes.contains(&l) {
                    lanes.push(l);
                }
            }
        }
        // named lanes first (by name), the "none" lane last, as in Jira
        let lane_name = |app: &ProjectFcgApp, l: Option<u32>| -> String {
            match (app.board.lanes, l) {
                (Swimlanes::Epic, Some(e)) => app.project.ticket(e).map(|t| t.title.clone()).unwrap_or_default(),
                (Swimlanes::Epic, None) => "No epic".into(),
                (_, Some(r)) => app.project.resources.iter().find(|x| x.uid == r).map(|x| x.name.clone()).unwrap_or_default(),
                (_, None) => "Unassigned".into(),
            }
        };
        lanes.sort_by_key(|l| (l.is_none(), lane_name(app, *l).to_lowercase()));
        let region = Rect::from_min_max(pos2(inner.left(), body_top), pos2(inner.right(), inner.bottom() - 8.0));
        let lp = ui.painter_at(region);
        let mut y = body_top - app.board.scroll_y;
        let lane_h = small_size() + 14.0;
        for l in lanes {
            let name = lane_name(app, l);
            let count: usize = col_ids.iter().map(|ids| ids.iter().filter(|&&id| lane_of(app, id) == l).count()).sum();
            let band = Rect::from_min_size(pos2(inner.left(), y + 2.0), vec2(inner.width(), lane_h - 4.0));
            lp.rect_filled(band, 4.0, th.bg.gamma_multiply(0.9));
            let dot = match (app.board.lanes, l) {
                (Swimlanes::Epic, Some(e)) => epic_color(e),
                _ => th.band,
            };
            lp.circle_filled(pos2(band.left() + 10.0, band.center().y), 4.0, dot);
            lp.text(pos2(band.left() + 20.0, band.center().y), Align2::LEFT_CENTER, format!("{name}  ·  {count}"), FontId::proportional(small_size() + 0.5), th.heading);
            y += lane_h;
            let mut h_max: f32 = 0.0;
            for c in 0..cols.len() {
                let ids: Vec<u32> = col_ids[c].iter().copied().filter(|&id| lane_of(app, id) == l).collect();
                let body = bodies[c];
                let bp = ui.painter_at(body.expand2(vec2(4.0, 0.0)));
                let h = card_list(app, ui, body, y, &ids, &bp, actions);
                h_max = h_max.max(h);
            }
            y += h_max.max(10.0) + 6.0;
        }
        quick_add(app, ui, bodies[0], y, actions);
        y += 30.0;
        max_h = y + app.board.scroll_y - inner.top();
    }
    if app.board.filter.active() && app.board.card_rects.iter().all(|(id, _)| app.project.ticket(*id).is_none_or(|t| t.sprint != Some(sprint))) {
        let name = app.project.task_by_uid(sprint).map(|t| t.name.clone()).unwrap_or_default();
        ui.painter().text(
            pos2(inner.center().x, inner.top() + 90.0),
            Align2::CENTER_CENTER,
            format!("Nothing in {name} matches the filter · Esc clears it"),
            FontId::proportional(text_size()),
            th.text_dim,
        );
    }
    let mut s = app.board.scroll_y;
    panel_scroll(ui, inner, &mut s, max_h);
    app.board.scroll_y = s;
}

fn empty_state(app: &mut ProjectFcgApp, ui: &mut egui::Ui, area: Rect) {
    let ctx = ui.ctx().clone();
    let r = Rect::from_center_size(area.center(), vec2(440.0, 170.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Center)), |ui| {
        ui.label(RichText::new("No sprints yet").size(18.0).strong().color(pal().heading));
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "A sprint is a task on the schedule that holds tickets. New Sprint adds a {}-day sprint to the Gantt chart; you can also mark an existing task as a sprint (Board ▸ Mark as Sprint).",
                app.project.board.sprint_days
            ))
            .color(pal().text_dim),
        );
        ui.add_space(10.0);
        if ui.button(RichText::new("📆  New Sprint").strong()).clicked() {
            app.execute(super::commands::Command::NewSprint, &ctx);
        }
    });
}

fn drag_and_drop(app: &mut ProjectFcgApp, ui: &mut egui::Ui, sprint: Option<u32>) {
    let Some(d) = app.board.drag.as_ref() else { return };
    let (id, grab, size) = (d.id, d.grab, d.size);
    let (pos, released, down) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.any_released(), i.pointer.primary_down()));
    let Some(pos) = pos else { return };
    // target place and insertion point
    let target = if app.board.backlog_open && app.board.backlog_rect.contains(pos) {
        match app.board.side {
            SidePanel::Epics => app.board.epic_rects.iter().find(|(_, r)| r.contains(pos)).map(|(e, _)| Target::Epic(*e)),
            SidePanel::Releases => app.board.release_rects.iter().find(|(_, r)| r.contains(pos)).map(|(e, _)| Target::Release(*e)),
            SidePanel::Backlog => Some(Target::Backlog),
        }
    } else if sprint.is_some() {
        app.board.column_rects.iter().position(|r| r.contains(pos)).map(Target::Column)
    } else {
        None
    };
    let list_of = |app: &ProjectFcgApp, tg: Target| match tg {
        Target::Backlog => app.board_list(None, None),
        Target::Column(c) => app.board_list(sprint, Some(c)),
        Target::Epic(_) | Target::Release(_) => vec![],
    };
    let before = target.and_then(|tg| {
        list_of(app, tg)
            .into_iter()
            .filter(|&i| i != id)
            .find(|&i| app.board.card_rects.iter().any(|(ci, r)| *ci == i && r.center().y > pos.y))
    });
    // ghost card + drop indicator on top of everything
    let fg = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::unique("board_drag")));
    if let Some(t) = app.project.ticket(id).cloned() {
        let lay = layout_card(ui, &app.project, &app.board.idx, &t, size.x, app.project.is_done(&t));
        paint_card(app, &fg, Rect::from_min_size(pos - grab, size), &t, &lay, true, true);
    }
    match target {
        Some(Target::Epic(e)) => {
            if let Some((_, r)) = app.board.epic_rects.iter().find(|(x, _)| *x == e) {
                fg.rect_stroke(*r, 5.0, Stroke::new(2.5, pal().accent), StrokeKind::Outside);
            }
        }
        Some(Target::Release(e)) => {
            if let Some((_, r)) = app.board.release_rects.iter().find(|(x, _)| *x == e) {
                fg.rect_stroke(*r, 5.0, Stroke::new(2.5, pal().accent), StrokeKind::Outside);
            }
        }
        Some(tg) => {
            let area = match tg {
                Target::Column(c) => app.board.column_rects[c].shrink2(vec2(8.0, 0.0)),
                _ => app.board.backlog_rect.shrink2(vec2(10.0, 0.0)),
            };
            let y = match before.and_then(|b| app.board.card_rects.iter().find(|(i, _)| *i == b)) {
                Some((_, r)) => r.top() - 4.0,
                None => list_of(app, tg)
                    .iter()
                    .filter(|&&i| i != id)
                    .filter_map(|i| app.board.card_rects.iter().find(|(ci, _)| ci == i).map(|(_, r)| r.bottom() + 4.0))
                    .fold(area.top() + 40.0, f32::max),
            };
            fg.line_segment([pos2(area.left(), y), pos2(area.right(), y)], Stroke::new(3.0, pal().accent));
        }
        None => {}
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if released || !down {
        app.board.drag = None;
        let Some(tg) = target else { return };
        let Some(t) = app.project.ticket(id).cloned() else { return };
        if let Target::Epic(e) = tg {
            if t.parent != Some(e) {
                let name = app.project.ticket(e).map(|x| x.title.clone()).unwrap_or_default();
                app.edit_project(&format!("Add to {name}"), |p| p.set_ticket_parent(id, Some(e)));
            }
            app.board.selected = Some(id);
            return;
        }
        if let Target::Release(r) = tg {
            // a sub-task brings its parent
            let target = if t.kind == IssueType::Subtask { t.parent.unwrap_or(id) } else { id };
            if app.project.ticket(target).is_some_and(|x| x.release != Some(r)) {
                let name = app.project.release(r).map(|x| x.name.clone()).unwrap_or_default();
                app.edit_project(&format!("Add to {name}"), |p| p.set_ticket_release(target, Some(r)));
            }
            app.board.selected = Some(id);
            return;
        }
        let (s, c, label) = match tg {
            Target::Column(c) => (sprint, c, format!("Move to {}", app.project.board.columns[c])),
            _ => (None, 0, "Move to Backlog".to_string()),
        };
        // a drop where it already is (same place, same neighbour) is a no-op
        let unchanged = t.sprint == s && t.column == c && before == app.board_neighbour(id, 1);
        if !unchanged {
            app.edit_project(&label, |p| p.move_ticket(id, s, c, before));
        }
        app.board.selected = Some(id);
    }
}

// ---------------------------------------------------------------- charts

fn charts_panel(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect, sprint: Option<u32>) {
    let th = pal();
    let p = ui.painter_at(r);
    p.rect_filled(r, 0.0, th.bg);
    p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, th.header_border));
    let half = (r.height() / 2.0).max(180.0);
    let top = Rect::from_min_size(r.min + vec2(14.0, 10.0), vec2(r.width() - 28.0, half - 20.0));
    let bottom = Rect::from_min_max(pos2(r.left() + 14.0, top.bottom() + 20.0), r.max - vec2(14.0, 10.0));
    // Burndown | Flow switch
    let mut x = top.left();
    for (flow, name) in [(false, "Burndown"), (true, "Flow")] {
        let on = app.board.flow == flow;
        let g = p.layout_no_wrap(name.into(), FontId::proportional(text_size() + 1.0), if on { th.heading } else { th.text_dim });
        let tr = Rect::from_min_size(pos2(x, top.top() - 2.0), vec2(g.size().x + 6.0, g.size().y + 6.0));
        let resp = ui.interact(tr, ui.scope_id().with(("chart_tab", name)), Sense::click());
        if on {
            p.line_segment([pos2(tr.left(), tr.bottom()), pos2(tr.right(), tr.bottom())], Stroke::new(2.0, th.accent));
        }
        p.galley(pos2(tr.left() + 3.0, tr.top() + 2.0), g, th.text);
        let tip = if flow { "Cumulative flow: tickets in each column, day by day" } else { "Remaining work against the ideal line" };
        if resp.on_hover_text(tip).clicked() {
            app.board.flow = flow;
        }
        x = tr.right() + 12.0;
    }
    if app.board.flow {
        flow_chart(app, &p, top, sprint);
    } else {
        burndown_chart(app, &p, top, sprint);
    }
    velocity_chart(app, &p, bottom, sprint);
}

/// Colour of a column's band: Done in the accent, the others from cool to warm.
fn flow_color(c: usize, n: usize) -> Color32 {
    if c + 1 >= n {
        return pal().accent;
    }
    const BANDS: [Color32; 5] = [
        Color32::from_rgb(158, 158, 170),
        Color32::from_rgb(66, 133, 244),
        Color32::from_rgb(171, 71, 188),
        Color32::from_rgb(255, 167, 38),
        Color32::from_rgb(38, 166, 154),
    ];
    BANDS[c % BANDS.len()]
}

fn flow_chart(app: &ProjectFcgApp, p: &egui::Painter, r: Rect, sprint: Option<u32>) {
    let th = pal();
    let Some(cf) = sprint.and_then(|s| app.project.cumulative_flow(s, now())) else {
        p.text(r.center(), Align2::CENTER_CENTER, "No sprint selected", FontId::proportional(small_size()), th.text_dim);
        return;
    };
    p.text(r.right_top(), Align2::RIGHT_TOP, "tickets", FontId::proportional(small_size()), th.text_dim);
    let plot = Rect::from_min_max(r.min + vec2(34.0, 30.0), r.max - vec2(6.0, 22.0));
    let n = cf.days.len();
    let ncol = cf.columns.len();
    let total = cf.counts.iter().flatten().map(|c| c.iter().sum::<usize>()).max().unwrap_or(0);
    let ymax = axis_max(total as f64);
    // a point per day, at the day's end
    let x = |k: usize| plot.left() + plot.width() * (k as f32 + 1.0) / n.max(1) as f32;
    let y = |v: f64| plot.bottom() - plot.height() * (v / ymax) as f32;
    let sf = FontId::proportional(small_size() - 1.0);
    for k in 0..=4 {
        let v = ymax * k as f64 / 4.0;
        let yy = y(v);
        p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.0, th.grid));
        p.text(pos2(plot.left() - 5.0, yy), Align2::RIGHT_CENTER, crate::duration::fmt_num(v), sf.clone(), th.text_dim);
    }
    let step = ((n as f32 * 34.0 / plot.width()).ceil() as usize).max(1);
    for (k, d) in cf.days.iter().enumerate() {
        if k % step == 0 {
            use chrono::Datelike;
            p.text(pos2(x(k), plot.bottom() + 4.0), Align2::CENTER_TOP, format!("{}/{}", d.month(), d.day()), sf.clone(), th.text_dim);
        }
    }
    // stacked bands, Done at the bottom (as Jira draws it); one quad per day
    // pair keeps every shape convex
    let stack = |c: &Vec<usize>| -> Vec<f64> {
        // stack[j] = tickets in columns ncol-1 down to ncol-1-j (cumulative)
        let mut acc = 0.0;
        (0..ncol).rev().map(|col| {
            acc += c[col] as f64;
            acc
        }).collect()
    };
    let known: Vec<(usize, Vec<f64>)> = cf.counts.iter().enumerate().filter_map(|(k, c)| c.as_ref().map(|c| (k, stack(c)))).collect();
    for w in known.windows(2) {
        let ((k0, s0), (k1, s1)) = (&w[0], &w[1]);
        for j in 0..ncol {
            let col = ncol - 1 - j;
            let lo0 = if j == 0 { 0.0 } else { s0[j - 1] };
            let lo1 = if j == 0 { 0.0 } else { s1[j - 1] };
            let (hi0, hi1) = (s0[j], s1[j]);
            if hi0 <= lo0 && hi1 <= lo1 {
                continue;
            }
            let pts = vec![pos2(x(*k0), y(lo0)), pos2(x(*k1), y(lo1)), pos2(x(*k1), y(hi1)), pos2(x(*k0), y(hi0))];
            p.add(egui::Shape::convex_polygon(pts, flow_color(col, ncol).gamma_multiply(0.85), Stroke::NONE));
        }
    }
    if known.len() == 1 {
        // one day so far: a column of bands
        let (k, s) = &known[0];
        for j in 0..ncol {
            let lo = if j == 0 { 0.0 } else { s[j - 1] };
            let rr = Rect::from_min_max(pos2(x(*k) - 4.0, y(s[j])), pos2(x(*k) + 4.0, y(lo)));
            p.rect_filled(rr, 0.0, flow_color(ncol - 1 - j, ncol));
        }
    }
    // legend
    let ly = r.bottom() - 4.0;
    let mut lx = plot.left();
    for (c, name) in cf.columns.iter().enumerate() {
        p.rect_filled(Rect::from_center_size(pos2(lx + 5.0, ly), vec2(10.0, 8.0)), 1.0, flow_color(c, ncol));
        let g = p.layout_no_wrap(name.clone(), sf.clone(), th.text_dim);
        let w = g.size().x;
        p.galley(pos2(lx + 13.0, ly - g.size().y / 2.0), g, th.text_dim);
        lx += w + 24.0;
        if lx > r.right() - 30.0 {
            break;
        }
    }
}

fn axis_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if v <= m * mag {
            return m * mag;
        }
    }
    10.0 * mag
}

fn burndown_chart(app: &ProjectFcgApp, p: &egui::Painter, r: Rect, sprint: Option<u32>) {
    let th = pal();
    let est = app.project.board.estimate;
    let Some(b) = sprint.and_then(|s| app.project.burndown(s, now())) else {
        p.text(r.center(), Align2::CENTER_CENTER, "No sprint selected", FontId::proportional(small_size()), th.text_dim);
        return;
    };
    p.text(r.right_top(), Align2::RIGHT_TOP, format!("remaining {}", if est == Estimate::StoryPoints { "points" } else { "tickets" }), FontId::proportional(small_size()), th.text_dim);
    let plot = Rect::from_min_max(r.min + vec2(34.0, 30.0), r.max - vec2(6.0, 22.0));
    let ymax = axis_max(b.total);
    let n = b.days.len();
    let x = |k: usize| plot.left() + plot.width() * k as f32 / n.max(1) as f32;
    let y = |v: f64| plot.bottom() - plot.height() * (v / ymax) as f32;
    let sf = FontId::proportional(small_size() - 1.0);
    for k in 0..=4 {
        let v = ymax * k as f64 / 4.0;
        let yy = y(v);
        p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.0, th.grid));
        p.text(pos2(plot.left() - 5.0, yy), Align2::RIGHT_CENTER, crate::duration::fmt_num(v), sf.clone(), th.text_dim);
    }
    // day labels (every day if room, else every few)
    let step = ((n as f32 * 34.0 / plot.width()).ceil() as usize).max(1);
    for (k, d) in b.days.iter().enumerate() {
        if k % step == 0 {
            use chrono::Datelike;
            p.text(pos2((x(k) + x(k + 1)) / 2.0, plot.bottom() + 4.0), Align2::CENTER_TOP, format!("{}/{}", d.month(), d.day()), sf.clone(), th.text_dim);
        }
    }
    // ideal (dashed) and actual
    let ideal: Vec<egui::Pos2> = b.ideal.iter().enumerate().map(|(k, v)| pos2(x(k), y(*v))).collect();
    for w in ideal.windows(2) {
        p.add(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.5, th.text_dim), 5.0, 4.0));
    }
    let pts: Vec<egui::Pos2> = b.actual.iter().enumerate().filter_map(|(k, v)| v.map(|v| pos2(x(k), y(v)))).collect();
    if pts.len() >= 2 {
        p.add(egui::Shape::line(pts.clone(), Stroke::new(2.5, th.accent)));
    }
    for q in &pts {
        p.circle_filled(*q, 3.0, th.accent);
    }
    // legend
    let ly = r.bottom() - 4.0;
    p.line_segment([pos2(plot.left(), ly), pos2(plot.left() + 16.0, ly)], Stroke::new(2.5, th.accent));
    p.text(pos2(plot.left() + 20.0, ly), Align2::LEFT_CENTER, "Actual", sf.clone(), th.text_dim);
    p.add(egui::Shape::dashed_line(&[pos2(plot.left() + 80.0, ly), pos2(plot.left() + 96.0, ly)], Stroke::new(1.5, th.text_dim), 4.0, 3.0));
    p.text(pos2(plot.left() + 100.0, ly), Align2::LEFT_CENTER, "Ideal", sf, th.text_dim);
}

fn velocity_chart(app: &ProjectFcgApp, p: &egui::Painter, r: Rect, sprint: Option<u32>) {
    let th = pal();
    let est = app.project.board.estimate;
    p.text(r.left_top(), Align2::LEFT_TOP, "Velocity", FontId::proportional(text_size() + 1.0), th.heading);
    let v = app.project.velocity();
    let avg = app.project.average_velocity(now());
    if let Some(a) = avg {
        p.text(r.right_top(), Align2::RIGHT_TOP, format!("average {}", est.unit((a * 10.0).round() / 10.0)), FontId::proportional(small_size()), th.text_dim);
    }
    if v.is_empty() {
        p.text(r.center(), Align2::CENTER_CENTER, "Velocity appears once you have sprints", FontId::proportional(small_size()), th.text_dim);
        return;
    }
    // show up to the last 8 sprints ending at the selected one (or the last)
    let end = sprint.and_then(|s| v.iter().position(|x| x.sprint_uid == s)).map(|i| i + 1).unwrap_or(v.len());
    let first = end.saturating_sub(8);
    let rows = &v[first..end];
    let plot = Rect::from_min_max(r.min + vec2(34.0, 30.0), r.max - vec2(6.0, 40.0));
    let ymax = axis_max(rows.iter().map(|x| x.planned.max(x.completed)).fold(avg.unwrap_or(0.0), f64::max));
    let y = |val: f64| plot.bottom() - plot.height() * (val / ymax) as f32;
    let sf = FontId::proportional(small_size() - 1.0);
    for k in 0..=4 {
        let val = ymax * k as f64 / 4.0;
        let yy = y(val);
        p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.0, th.grid));
        p.text(pos2(plot.left() - 5.0, yy), Align2::RIGHT_CENTER, crate::duration::fmt_num(val), sf.clone(), th.text_dim);
    }
    let slot = plot.width() / rows.len() as f32;
    let bw = (slot * 0.32).min(22.0);
    for (k, row) in rows.iter().enumerate() {
        let cx = plot.left() + slot * (k as f32 + 0.5);
        let planned = Rect::from_min_max(pos2(cx - bw - 1.0, y(row.planned)), pos2(cx - 1.0, plot.bottom()));
        let done = Rect::from_min_max(pos2(cx + 1.0, y(row.completed)), pos2(cx + bw + 1.0, plot.bottom()));
        p.rect_filled(planned, 2.0, th.tint);
        p.rect_stroke(planned, 2.0, Stroke::new(1.0, th.tint_border), StrokeKind::Inside);
        p.rect_filled(done, 2.0, th.accent);
        let sel = Some(row.sprint_uid) == sprint;
        let label: String = row.name.chars().take(12).collect();
        p.text(pos2(cx, plot.bottom() + 4.0), Align2::CENTER_TOP, label, sf.clone(), if sel { th.heading } else { th.text_dim });
        if row.completed > 0.0 {
            p.text(pos2(done.center().x, done.top() - 2.0), Align2::CENTER_BOTTOM, crate::duration::fmt_num(row.completed), sf.clone(), th.text);
        }
    }
    if let Some(a) = avg {
        let yy = y(a);
        p.add(egui::Shape::dashed_line(&[pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.5, th.today), 6.0, 4.0));
    }
    let ly = r.bottom() - 6.0;
    p.rect_filled(Rect::from_center_size(pos2(plot.left() + 6.0, ly), vec2(10.0, 10.0)), 2.0, th.tint);
    p.rect_stroke(Rect::from_center_size(pos2(plot.left() + 6.0, ly), vec2(10.0, 10.0)), 2.0, Stroke::new(1.0, th.tint_border), StrokeKind::Inside);
    p.text(pos2(plot.left() + 15.0, ly), Align2::LEFT_CENTER, "Planned", sf.clone(), th.text_dim);
    p.rect_filled(Rect::from_center_size(pos2(plot.left() + 80.0, ly), vec2(10.0, 10.0)), 2.0, th.accent);
    p.text(pos2(plot.left() + 89.0, ly), Align2::LEFT_CENTER, "Completed", sf.clone(), th.text_dim);
    if avg.is_some() {
        p.add(egui::Shape::dashed_line(&[pos2(plot.left() + 168.0, ly), pos2(plot.left() + 184.0, ly)], Stroke::new(1.5, th.today), 4.0, 3.0));
        p.text(pos2(plot.left() + 188.0, ly), Align2::LEFT_CENTER, "Avg of last 3 ended", sf, th.text_dim);
    }
}

// ---------------------------------------------------------------- dialog

/// The seven link directions, as offered in the dialog.
const LINK_CHOICES: [(LinkKind, bool); 7] = [
    (LinkKind::Blocks, true),
    (LinkKind::Blocks, false),
    (LinkKind::Relates, true),
    (LinkKind::Duplicates, true),
    (LinkKind::Duplicates, false),
    (LinkKind::Clones, true),
    (LinkKind::Clones, false),
];

fn link_verb(kind: LinkKind, outward: bool) -> &'static str {
    if outward {
        kind.outward()
    } else {
        kind.inward()
    }
}

pub fn show_dialog(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.ticket.take() else { return };
    let (mut ok, mut cancel, mut delete) = (false, false, false);
    let mut text_focus = false;
    let title = match (st.id, st.kind) {
        (Some(id), k) => format!("{} {}", k.label(), app.project.ticket_key(id)),
        (None, k) => format!("New {}", k.label()),
    };
    let p = &app.project;
    let sprints: Vec<(u32, String)> = p.sprint_rows().into_iter().map(|i| (p.tasks[i].uid, p.tasks[i].name.clone())).collect();
    let people: Vec<(u32, String)> = p.resources.iter().filter(|r| r.rtype == crate::model::ResourceType::Work).map(|r| (r.uid, r.name.clone())).collect();
    // released releases take no new work; the ticket's own stays listed
    let own_release = st.id.and_then(|i| p.ticket(i)).and_then(|t| t.release);
    let releases: Vec<(u32, String)> = p.board.releases.iter().filter(|r| r.released.is_none() || Some(r.id) == own_release).map(|r| (r.id, r.name.clone())).collect();
    let epics: Vec<(u32, String)> = p.epics().into_iter().map(|k| (p.tickets[k].id, format!("{}  {}", p.ticket_key(p.tickets[k].id), p.tickets[k].title))).collect();
    let parents: Vec<(u32, String)> = p
        .tickets
        .iter()
        .filter(|t| t.kind.is_standard() && Some(t.id) != st.id)
        .map(|t| (t.id, format!("{}  {}", p.ticket_key(t.id), t.title)))
        .collect();
    let others: Vec<(u32, String)> = p.tickets.iter().filter(|t| Some(t.id) != st.id).map(|t| (t.id, format!("{}  {}", p.ticket_key(t.id), t.title))).collect();
    let tasks: Vec<(u32, String)> = (0..p.tasks.len())
        .filter(|&i| !p.is_summary(i) && !p.tasks[i].is_sprint)
        .map(|i| (p.tasks[i].uid, format!("{}  {}", i + 1, p.tasks[i].name)))
        .collect();
    let children: Vec<(String, String, String)> = st
        .id
        .map(|id| {
            p.ticket_children(id)
                .into_iter()
                .map(|k| {
                    let t = &p.tickets[k];
                    (p.ticket_key(t.id), t.title.clone(), p.board.columns.get(t.column).cloned().unwrap_or_default())
                })
                .collect()
        })
        .unwrap_or_default();
    let has_children = !children.is_empty();
    let parent_sprint = if st.kind == IssueType::Subtask { st.parent.and_then(|x| p.ticket(x)).map(|x| x.sprint) } else { None };
    let epic_stats = st.id.filter(|_| st.kind == IssueType::Epic).map(|id| p.epic_stats(id));
    let key_of = |id: u32| p.ticket_key(id);
    let title_of = |id: u32| p.ticket(id).map(|t| t.title.clone()).unwrap_or_default();
    let cols = p.board.columns.clone();
    let est = p.board.estimate;
    let kind0 = st.kind;
    // leaf, non-sprint tasks to wait on: (uid, "ID name", finish, % complete)
    let gantt_tasks: Vec<(u32, String, String, f64)> = (0..p.tasks.len())
        .filter(|&i| !p.tasks[i].is_sprint)
        .map(|i| {
            let t = &p.tasks[i];
            (t.uid, format!("{}  {}", i + 1, t.name), t.calc.finish.map(fmt_date).unwrap_or_default(), t.calc.percent_complete)
        })
        .collect();
    let waiting_tasks: Vec<String> = st.id.map(|id| p.tasks_waiting_on(id).into_iter().map(|r| format!("{} {}", r + 1, p.tasks[r].name)).collect()).unwrap_or_default();
    let conflicts: Vec<String> = st.id.map(|id| p.dependency_conflicts(id)).unwrap_or_default();
    let keys = super::dialogs::modal(ctx, "ticket", &title, 540.0, |ui| {
        egui::Grid::new("ticket_grid").num_columns(2).spacing([12.0, 7.0]).show(ui, |ui| {
            ui.label("Type:");
            ui.horizontal(|ui| {
                for k in IssueType::ALL {
                    // the hierarchy limits what an existing ticket can become
                    let allowed = k == kind0
                        || match k {
                            IssueType::Epic | IssueType::Subtask => !has_children,
                            _ => kind0 != IssueType::Epic || !has_children,
                        };
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    paint_type_badge(ui.painter(), r, k);
                    if ui.add_enabled(allowed, egui::RadioButton::new(st.kind == k, k.label())).clicked() && st.kind != k {
                        // an epic and a sub-task have different kinds of parent
                        if k == IssueType::Subtask || st.kind == IssueType::Subtask || k == IssueType::Epic {
                            st.parent = None;
                        }
                        st.kind = k;
                    }
                    ui.add_space(2.0);
                }
            });
            ui.end_row();
            ui.label("Summary:");
            let r = ui.add(egui::TextEdit::singleline(&mut st.title).desired_width(390.0));
            if st.id.is_none() && st.title.is_empty() && !r.has_focus() {
                r.request_focus();
            }
            ui.end_row();
            ui.label("Priority:");
            ui.horizontal(|ui| {
                for pr in Priority::ALL {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                    paint_priority(ui.painter(), r.center(), pr);
                    ui.radio_value(&mut st.priority, pr, pr.label());
                }
            });
            ui.end_row();
            ui.label("Story points:");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.points).desired_width(50.0).hint_text("–"));
                for v in ["1", "2", "3", "5", "8", "13"] {
                    if ui.selectable_label(st.points.trim() == v, v).clicked() {
                        st.points = v.into();
                    }
                }
                if !st.kind.is_standard() {
                    ui.label(RichText::new("(not counted)").size(11.0).color(pal().text_dim)).on_hover_text(
                        "Sprint totals, burndown, velocity and epic progress count stories, tasks and bugs. An epic's size is its stories'; a sub-task is part of its parent's estimate.",
                    );
                }
            });
            ui.end_row();
            ui.label("Status:");
            egui::ComboBox::from_id_salt("ticket_col").selected_text(cols.get(st.column).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                for (c, name) in cols.iter().enumerate() {
                    ui.selectable_value(&mut st.column, c, name);
                }
            });
            ui.end_row();
            match st.kind {
                IssueType::Epic => {
                    ui.label("Drives task:");
                    let cur = st.task.and_then(|u| tasks.iter().find(|x| x.0 == u)).map(|x| x.1.clone()).unwrap_or_else(|| "None".into());
                    egui::ComboBox::from_id_salt("ticket_task").selected_text(cur).width(300.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.task, None, "None");
                        for (u, n) in &tasks {
                            ui.selectable_value(&mut st.task, Some(*u), n);
                        }
                    })
                    .response
                    .on_hover_text("That Gantt task's % complete then follows this epic's stories, like a sprint follows its tickets.");
                    ui.end_row();
                }
                IssueType::Subtask => {
                    ui.label("Parent:");
                    let cur = st.parent.and_then(|u| parents.iter().find(|x| x.0 == u)).map(|x| x.1.clone()).unwrap_or_else(|| "Choose…".into());
                    egui::ComboBox::from_id_salt("ticket_parent").selected_text(cur).width(300.0).show_ui(ui, |ui| {
                        for (u, n) in &parents {
                            ui.selectable_value(&mut st.parent, Some(*u), n);
                        }
                    });
                    ui.end_row();
                    ui.label("Sprint:");
                    let s = parent_sprint.flatten().and_then(|s| sprints.iter().find(|x| x.0 == s)).map(|x| x.1.clone()).unwrap_or_else(|| "Backlog".into());
                    ui.label(RichText::new(format!("{s} (follows the parent)")).color(pal().text_dim));
                    ui.end_row();
                }
                _ => {
                    ui.label("Sprint:");
                    let cur = st.sprint.and_then(|s| sprints.iter().find(|x| x.0 == s)).map(|x| x.1.clone()).unwrap_or_else(|| "Backlog".into());
                    egui::ComboBox::from_id_salt("ticket_sprint").selected_text(cur).show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.sprint, None, "Backlog");
                        for (u, n) in &sprints {
                            ui.selectable_value(&mut st.sprint, Some(*u), n);
                        }
                    });
                    ui.end_row();
                    ui.label("Epic:");
                    let cur = st.parent.and_then(|u| epics.iter().find(|x| x.0 == u)).map(|x| x.1.clone()).unwrap_or_else(|| "None".into());
                    egui::ComboBox::from_id_salt("ticket_epic").selected_text(cur).width(300.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.parent, None, "None");
                        for (u, n) in &epics {
                            ui.selectable_value(&mut st.parent, Some(*u), n);
                        }
                    });
                    ui.end_row();
                    if !releases.is_empty() {
                        ui.label("Release:");
                        let cur = st.release.and_then(|u| releases.iter().find(|x| x.0 == u)).map(|x| x.1.clone()).unwrap_or_else(|| "None".into());
                        egui::ComboBox::from_id_salt("ticket_release").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut st.release, None, "None");
                            for (u, n) in &releases {
                                ui.selectable_value(&mut st.release, Some(*u), n);
                            }
                        });
                        ui.end_row();
                    }
                }
            }
            ui.label("Assignee:");
            let cur = st.assignee.and_then(|a| people.iter().find(|x| x.0 == a)).map(|x| x.1.clone()).unwrap_or_else(|| "Unassigned".into());
            egui::ComboBox::from_id_salt("ticket_who").selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.assignee, None, "Unassigned");
                for (u, n) in &people {
                    ui.selectable_value(&mut st.assignee, Some(*u), n);
                }
            });
            ui.end_row();
            ui.label("Labels:");
            ui.add(egui::TextEdit::singleline(&mut st.labels).desired_width(390.0).hint_text("comma-separated, e.g. backend, perf"));
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.label("Description:");
        let d = ui.add(egui::TextEdit::multiline(&mut st.description).desired_rows(4).desired_width(f32::INFINITY));
        text_focus |= d.has_focus();

        // ---- epic progress / children ----
        if let Some(es) = epic_stats {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Progress: {} of {} done ({:.0}%)", crate::duration::fmt_num(es.done), est.unit(es.total), es.percent())).color(pal().heading));
        }
        if !children.is_empty() || (st.kind.is_standard() && st.id.is_some()) {
            ui.add_space(4.0);
            let head = if st.kind == IssueType::Epic { "Stories in this epic:" } else { "Sub-tasks:" };
            ui.label(RichText::new(head).strong());
            egui::ScrollArea::vertical().id_salt("ticket_children").max_height(90.0).show(ui, |ui| {
                for (k, t, s) in &children {
                    ui.label(format!("{k}  {t}   [{s}]"));
                }
                for t in &st.new_subtasks {
                    ui.label(RichText::new(format!("new  {t}")).italics());
                }
            });
            if st.kind.is_standard() {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut st.subtask_text).hint_text("+ Add a sub-task").desired_width(300.0));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    text_focus |= r.has_focus() || enter;
                    if (ui.button("Add").clicked() || enter) && !st.subtask_text.trim().is_empty() {
                        st.new_subtasks.push(std::mem::take(&mut st.subtask_text).trim().to_string());
                        r.request_focus();
                    }
                });
            }
        }

        // ---- schedule dependencies ----
        if st.kind != IssueType::Epic {
            ui.add_space(4.0);
            ui.label(RichText::new("Waits on tasks:").strong()).on_hover_text("Gantt tasks that must be finished before this ticket can start. It shows as blocked until they're 100% complete, and a sprint that starts too early is flagged.");
            let mut remove: Option<usize> = None;
            for (i, u) in st.after_tasks.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("✖").on_hover_text("Stop waiting on this task").clicked() {
                        remove = Some(i);
                    }
                    let (name, fin, pct) = gantt_tasks.iter().find(|x| x.0 == *u).map(|x| (x.1.clone(), x.2.clone(), x.3)).unwrap_or_default();
                    ui.label(name);
                    ui.label(RichText::new(if pct >= 100.0 { "done".to_string() } else { format!("finishes {fin}") }).color(pal().text_dim));
                });
            }
            if let Some(i) = remove {
                st.after_tasks.remove(i);
            }
            egui::ComboBox::from_id_salt("ticket_after_task").selected_text("+ Wait on a task…").width(300.0).show_ui(ui, |ui| {
                for (u, n, fin, _) in &gantt_tasks {
                    if !st.after_tasks.contains(u) && ui.selectable_label(false, format!("{n}   {fin}")).clicked() {
                        st.after_tasks.push(*u);
                    }
                }
            });
        }
        if !waiting_tasks.is_empty() {
            ui.label(RichText::new(format!("Tasks waiting on this: {}", waiting_tasks.join(", "))).color(pal().text_dim));
        }
        for c in &conflicts {
            ui.colored_label(pal().warn, format!("⚠ {c}"));
        }

        // ---- links ----
        ui.add_space(4.0);
        ui.label(RichText::new("Links:").strong());
        let mut remove: Option<usize> = None;
        for (i, l) in st.links.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("✖").on_hover_text("Remove this link").clicked() {
                    remove = Some(i);
                }
                ui.label(RichText::new(link_verb(l.kind, l.outward)).color(pal().text_dim));
                ui.label(format!("{}  {}", key_of(l.other), title_of(l.other)));
            });
        }
        if let Some(i) = remove {
            st.links.remove(i);
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("ticket_link_kind").selected_text(link_verb(st.new_link_kind.0, st.new_link_kind.1)).width(120.0).show_ui(ui, |ui| {
                for (k, o) in LINK_CHOICES {
                    ui.selectable_value(&mut st.new_link_kind, (k, o), link_verb(k, o));
                }
            });
            let cur = st.new_link_to.and_then(|u| others.iter().find(|x| x.0 == u)).map(|x| x.1.clone()).unwrap_or_else(|| "Choose a ticket…".into());
            egui::ComboBox::from_id_salt("ticket_link_to").selected_text(cur).width(250.0).show_ui(ui, |ui| {
                for (u, n) in &others {
                    ui.selectable_value(&mut st.new_link_to, Some(*u), n);
                }
            });
            if ui.add_enabled(st.new_link_to.is_some(), egui::Button::new("Link")).clicked() {
                let (kind, outward) = st.new_link_kind;
                let l = DlgLink { kind, outward, other: st.new_link_to.take().unwrap() };
                if !st.links.contains(&l) {
                    st.links.push(l);
                }
            }
        });
        ui.label(
            RichText::new(if est == Estimate::StoryPoints {
                "The assignee is a label for the board; the team's workload comes from the resources assigned to the sprint task."
            } else {
                "This project counts tickets, not points (Board ▸ Estimate). The assignee is a label for the board."
            })
            .size(11.0)
            .color(pal().text_dim),
        );
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            if st.id.is_some() && ui.button(RichText::new("Delete").color(pal().warn)).clicked() {
                delete = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("  Cancel  ").clicked() {
                    cancel = true;
                }
                if ui.button(RichText::new("    OK    ").strong()).clicked() {
                    ok = true;
                }
            });
        });
    });
    if keys.enter && !text_focus {
        ok = true;
    }
    if cancel || keys.esc {
        return;
    }
    if delete {
        if let Some(id) = st.id {
            app.board.selected = Some(id);
            app.board_delete_selected();
        }
        return;
    }
    if !ok {
        app.dialogs.ticket = Some(st);
        return;
    }
    let points = match st.points.trim() {
        "" | "-" | "–" | "?" => None,
        s => match s.parse::<f64>() {
            Ok(v) if v >= 0.0 && v.is_finite() => Some(v),
            _ => {
                st.error = Some(format!("\"{s}\" isn't a number of story points."));
                app.dialogs.ticket = Some(st);
                return;
            }
        },
    };
    if st.kind == IssueType::Subtask && st.parent.is_none() {
        st.error = Some("A sub-task needs a parent story, task or bug.".into());
        app.dialogs.ticket = Some(st);
        return;
    }
    let labels: Vec<String> = {
        let mut v: Vec<String> = vec![];
        for l in st.labels.split([',', ' ']).map(str::trim).filter(|l| !l.is_empty()) {
            if !v.iter().any(|x| x.eq_ignore_ascii_case(l)) {
                v.push(l.to_string());
            }
        }
        v
    };
    let s2 = st.clone();
    let label = match (st.id, st.kind) {
        (Some(_), IssueType::Epic) => "Edit Epic",
        (Some(_), _) => "Edit Ticket",
        (None, IssueType::Epic) => "New Epic",
        (None, IssueType::Subtask) => "New Sub-task",
        (None, _) => "New Ticket",
    };
    let res = app.edit_project(label, move |p| apply_ticket_dialog(p, &s2, points, labels));
    match res {
        Some(id) => app.board.selected = Some(id),
        None => {
            st.error = Some(app.status.clone());
            app.dialogs.ticket = Some(st);
        }
    }
}

/// Apply the ticket dialog to the project (one undo step).
fn apply_ticket_dialog(p: &mut Project, s: &TicketDlg, points: Option<f64>, labels: Vec<String>) -> Result<u32, String> {
    if s.title.trim().is_empty() {
        return Err("A ticket needs a summary.".into());
    }
    let id = match s.id {
        Some(id) => {
            p.ticket_index(id).ok_or("The ticket was deleted.")?;
            // type and parent change together, so the hierarchy stays valid
            p.set_ticket_type(id, s.kind, if s.kind == IssueType::Epic { None } else { s.parent })?;
            id
        }
        None => p.add_ticket_of(&s.title, if s.kind.is_standard() { s.sprint } else { None }, s.kind, if s.kind == IssueType::Epic { None } else { s.parent })?,
    };
    {
        let k = p.ticket_index(id).unwrap();
        let t = &mut p.tickets[k];
        t.title = s.title.trim().to_string();
        t.description = s.description.clone();
        t.points = points;
        t.assignee = s.assignee;
        t.priority = s.priority;
        t.labels = labels;
    }
    match s.kind {
        IssueType::Epic => {
            p.set_epic_task(id, s.task)?;
        }
        _ => p.set_ticket_parent(id, s.parent)?,
    }
    // place it: sub-tasks follow their parent's sprint, epics have none
    let t = p.ticket(id).unwrap().clone();
    let sprint = match s.kind {
        IssueType::Epic => None,
        IssueType::Subtask => t.sprint,
        _ => s.sprint,
    };
    if t.sprint != sprint || t.column != s.column {
        p.move_ticket(id, sprint, s.column, None)?;
    }
    // links: remove the ones taken out, add the new ones
    let want: Vec<LinkView> = s.links.iter().map(|l| LinkView { kind: l.kind, outward: l.outward, other: l.other }).collect();
    let mut have: Vec<LinkView> = vec![];
    for l in p.ticket_links(id) {
        if !have.contains(&l) {
            have.push(l);
        }
    }
    for l in have.iter().filter(|l| !want.contains(l)) {
        p.remove_ticket_link(id, l)?;
    }
    for l in want.iter().filter(|l| !have.contains(l)) {
        if l.outward {
            p.add_ticket_link(id, l.kind, l.other)?;
        } else {
            p.add_ticket_link(l.other, l.kind, id)?;
        }
    }
    for title in &s.new_subtasks {
        p.add_ticket_of(title, None, IssueType::Subtask, Some(id))?;
    }
    let have = p.ticket(id).map(|t| t.after_tasks.clone()).unwrap_or_default();
    for u in have.iter().filter(|u| !s.after_tasks.contains(u)) {
        p.remove_ticket_task_dep(id, *u)?;
    }
    for u in s.after_tasks.iter().filter(|u| !have.contains(u)) {
        p.add_ticket_task_dep(id, *u)?;
    }
    let release = if s.kind.is_standard() { s.release } else { None };
    if p.ticket(id).is_some_and(|t| t.release != release) {
        p.set_ticket_release(id, release)?;
    }
    Ok(id)
}
