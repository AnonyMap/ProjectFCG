//! End-to-end UI tests: drive the real `ProjectFcgApp` through egui's input
//! pipeline (keyboard, text, mouse) without a window, and check the
//! resulting project state.

use chrono::NaiveDate;
use egui::{pos2, vec2, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use projectfcg::app::theme::{header_h, row_h, row_header_w};
use projectfcg::app::{ProjectFcgApp, RowRef};
use projectfcg::model::{ConstraintType, LinkType, Project};

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}

const NONE: Modifiers = Modifiers::NONE;
fn ctrl() -> Modifiers {
    Modifiers { ctrl: true, command: true, ..Default::default() }
}
fn shift() -> Modifiers {
    Modifiers { shift: true, ..Default::default() }
}
fn alt_shift() -> Modifiers {
    Modifiers { alt: true, shift: true, ..Default::default() }
}
fn ctrl_shift() -> Modifiers {
    Modifiers { ctrl: true, command: true, shift: true, ..Default::default() }
}

impl H {
    fn new(p: Project) -> Self {
        let mut app = ProjectFcgApp::new(p);
        app.view.timeline = false;
        app.use_portal = false;
        let mut h = H { ctx: egui::Context::default(), app, t: 0.0 };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn blank() -> Self {
        Self::new(Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()))
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360.0, 820.0))),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.frame(ui));
        out.drop_without_applying_deltas();
    }
    fn key(&mut self, key: Key, m: Modifiers) {
        self.frame(vec![
            Event::ModifiersChanged(m),
            Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: m },
        ]);
        self.frame(vec![
            Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: m },
            Event::ModifiersChanged(NONE),
        ]);
    }
    fn text(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.to_string())]);
    }
    /// Type a cell value the way a user does: first char starts editing,
    /// rest goes into the editor, Enter commits.
    fn type_and_enter(&mut self, s: &str) {
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            self.text(&c.to_string());
        }
        let rest: String = chars.collect();
        if !rest.is_empty() {
            self.text(&rest);
        }
        self.key(Key::Enter, NONE);
    }
    fn click_m(&mut self, p: Pos2, m: Modifiers) {
        self.frame(vec![Event::ModifiersChanged(m), Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m }]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m }]);
        self.frame(vec![Event::ModifiersChanged(NONE)]);
    }
    fn click(&mut self, p: Pos2) {
        self.click_m(p, NONE);
    }
    fn double_click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        for _ in 0..2 {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: NONE }]);
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: NONE }]);
        }
        self.frame(vec![]);
    }
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![Event::PointerMoved(from)]);
        self.frame(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: NONE }]);
        let steps = 8;
        for k in 1..=steps {
            let p = from + (to - from) * (k as f32 / steps as f32);
            self.frame(vec![Event::PointerMoved(p)]);
        }
        self.frame(vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: NONE }]);
        self.frame(vec![]);
    }
    /// Centre of a table cell in display row `d`, column index `col`.
    fn cell(&self, d: usize, col: usize) -> Pos2 {
        let r = self.app.last_table_rect;
        let mut x = r.left() + row_header_w() - self.app.table_scroll_x;
        for c in &self.app.columns[..col] {
            x += c.width;
        }
        x += self.app.columns[col].width / 2.0;
        pos2(x, r.top() + header_h() + d as f32 * row_h() + row_h() / 2.0 - self.app.scroll_y)
    }
    fn row_header(&self, d: usize) -> Pos2 {
        let r = self.app.last_table_rect;
        pos2(r.left() + row_header_w() / 2.0, r.top() + header_h() + d as f32 * row_h() + row_h() / 2.0 - self.app.scroll_y)
    }
    fn col(&self, id: projectfcg::app::columns::ColumnId) -> usize {
        self.app.columns.iter().position(|c| c.id == id).unwrap()
    }
    /// x of a datetime in the gantt chart.
    fn gantt_x(&self, dt: chrono::NaiveDateTime) -> f32 {
        let g = self.app.last_gantt_rect;
        g.left() + ((projectfcg::app::to_days(dt) - self.app.gantt.left_days) * self.app.gantt.ppd as f64) as f32
    }
    fn gantt_row_y(&self, d: usize) -> f32 {
        let g = self.app.last_gantt_rect;
        g.top() + header_h() + d as f32 * row_h() + row_h() / 2.0 - self.app.scroll_y
    }
    /// Click row `d`'s Name cell, then arrow across to column `col` the way
    /// a keyboard user would (the table scrolls it into view).
    fn goto(&mut self, d: usize, col: usize) {
        let name = self.col(projectfcg::app::columns::ColumnId::Name);
        self.click(self.cell(d, name));
        for _ in name..col {
            self.key(Key::ArrowRight, NONE);
        }
        self.frame(vec![]);
    }
    fn names(&self) -> Vec<String> {
        self.app.project.tasks.iter().map(|t| t.name.clone()).collect()
    }
}

use projectfcg::app::columns::ColumnId;

fn dt(m: u32, d: u32, h: u32) -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

/// Build "Design, Build, Test" by typing into the blank row.
fn three_tasks() -> H {
    let mut h = H::blank();
    for n in ["Design", "Build", "Test"] {
        h.type_and_enter(n);
    }
    h
}

#[test]
fn typing_in_blank_row_creates_tasks() {
    let h = three_tasks();
    assert_eq!(h.names(), vec!["Design", "Build", "Test"]);
    assert_eq!(h.app.active_row, RowRef::Blank, "Enter moves to the next (blank) row");
    assert!(h.app.dirty);
}

#[test]
fn type_duration_and_navigate_with_arrows() {
    let mut h = three_tasks();
    // go to Design's Duration cell: Ctrl+Up to first row, Right to Duration
    h.key(Key::ArrowUp, ctrl());
    assert_eq!(h.app.active_index(), Some(0));
    h.key(Key::ArrowRight, NONE);
    assert_eq!(h.app.columns[h.app.active_col].id, ColumnId::Duration);
    h.type_and_enter("5d");
    // Enter moved down; type Build's duration
    assert_eq!(h.app.active_index(), Some(1));
    h.type_and_enter("2w");
    let p = &h.app.project;
    assert_eq!(p.tasks[0].duration.minutes, 2400);
    assert_eq!(p.tasks[1].duration.minutes, 4800);
    assert!(!p.tasks[0].duration.estimated);
}

#[test]
fn invalid_duration_shows_message_and_keeps_value() {
    let mut h = three_tasks();
    h.key(Key::ArrowUp, ctrl());
    h.key(Key::ArrowRight, NONE);
    h.type_and_enter("banana");
    assert!(h.app.dialogs.message.is_some());
    assert_eq!(h.app.project.tasks[0].duration.minutes, 480);
    // OK closes the message (Enter)
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.message.is_none());
}

#[test]
fn click_shift_click_then_ctrl_f2_links() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    assert_eq!(h.app.active_index(), Some(0));
    h.click_m(h.cell(2, name), shift());
    assert_eq!(h.app.selected.len(), 3);
    h.key(Key::F2, ctrl());
    assert_eq!(h.app.project.predecessors_text(1), "1");
    assert_eq!(h.app.project.predecessors_text(2), "2");
    assert_eq!(h.app.project.tasks[2].calc.start, Some(dt(1, 7, 8)));
    // Ctrl+Shift+F2 unlinks the selection
    h.key(Key::F2, ctrl_shift());
    assert!(h.app.project.tasks.iter().all(|t| t.predecessors.is_empty()));
}

#[test]
fn ctrl_click_order_determines_link_direction() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(2, name));
    h.click_m(h.cell(0, name), ctrl());
    h.key(Key::F2, ctrl());
    assert_eq!(h.app.project.predecessors_text(0), "3");
}

#[test]
fn indent_outdent_shortcuts() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::ArrowRight, alt_shift());
    assert_eq!(h.app.project.tasks[1].outline_level, 2);
    assert!(h.app.project.tasks[0].calc.is_summary);
    h.key(Key::ArrowLeft, alt_shift());
    assert_eq!(h.app.project.tasks[1].outline_level, 1);
    // indenting the first task is refused with a message
    h.click(h.cell(0, name));
    h.key(Key::ArrowRight, alt_shift());
    assert!(h.app.dialogs.message.is_some());
}

#[test]
fn insert_delete_undo_redo() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::Insert, NONE);
    // Insert starts editing the new task's name
    assert!(h.app.edit.is_some());
    h.text("Review");
    h.key(Key::Enter, NONE);
    assert_eq!(h.names(), vec!["Design", "Review", "Build", "Test"]);
    // Delete removes the selected task (Name column)
    h.click(h.cell(1, name));
    h.key(Key::Delete, NONE);
    assert_eq!(h.names(), vec!["Design", "Build", "Test"]);
    h.key(Key::Z, ctrl());
    assert_eq!(h.names(), vec!["Design", "Review", "Build", "Test"]);
    h.key(Key::Y, ctrl());
    assert_eq!(h.names(), vec!["Design", "Build", "Test"]);
}

#[test]
fn escape_cancels_edit() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.key(Key::F2, NONE);
    assert!(h.app.edit.is_some());
    h.text("XYZ");
    h.key(Key::Escape, NONE);
    assert!(h.app.edit.is_none());
    assert_eq!(h.app.project.tasks[0].name, "Design");
}

#[test]
fn predecessors_typed_in_cell() {
    let mut h = three_tasks();
    let pred = h.col(ColumnId::Predecessors);
    h.click(h.cell(2, pred));
    h.type_and_enter("1SS+1d");
    assert_eq!(h.app.project.predecessors_text(2), "1SS+1 day");
    assert_eq!(h.app.project.tasks[2].calc.start, Some(dt(1, 6, 8)));
    // circular link is refused with a message, nothing changes
    h.click(h.cell(0, pred));
    h.type_and_enter("3");
    assert!(h.app.dialogs.message.is_some());
    assert!(h.app.project.tasks[0].predecessors.is_empty());
}

#[test]
fn typing_start_date_sets_constraint_indicator() {
    let mut h = three_tasks();
    let start = h.col(ColumnId::Start);
    h.click(h.cell(0, start));
    h.type_and_enter("1/14/26");
    let t = &h.app.project.tasks[0];
    assert_eq!(t.constraint, ConstraintType::SNET);
    assert_eq!(t.calc.start, Some(dt(1, 14, 8)));
}

#[test]
fn shift_f2_opens_task_information() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::F2, shift());
    let ti = h.app.dialogs.task_info.as_ref().expect("Task Information open");
    assert_eq!(ti.name, "Build");
    assert_eq!(ti.duration, "1 day");
    assert!(ti.estimated);
    // keyboard shortcuts are suspended while a modal is open
    h.key(Key::Insert, NONE);
    assert_eq!(h.app.project.tasks.len(), 3);
    h.key(Key::Escape, NONE);
    assert!(h.app.dialogs.task_info.is_none());
}

#[test]
fn task_info_enter_is_ok_and_applies_all_tabs() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::F2, shift());
    {
        let ti = h.app.dialogs.task_info.as_mut().unwrap();
        ti.name = "Build phase".into();
        ti.duration = "3d".into();
        ti.percent = 50.0;
        ti.preds.push(projectfcg::app::dialogs::PredRow { id_text: "1".into(), kind: LinkType::SS, lag_text: "+1d".into() });
        ti.deadline = "1/6/26".into();
        ti.on_timeline = true;
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.task_info.is_none(), "Enter = OK");
    let t = &h.app.project.tasks[1];
    assert_eq!(t.name, "Build phase");
    assert_eq!(t.duration.minutes, 3 * 480);
    assert_eq!(t.percent_complete, 50.0);
    assert!(t.on_timeline);
    assert!(t.calc.deadline_missed);
    assert_eq!(h.app.project.predecessors_text(1), "1SS+1 day");
    assert_eq!(t.calc.start, Some(dt(1, 6, 8)));
    // one undo step reverts the whole dialog
    h.key(Key::Z, ctrl());
    assert_eq!(h.app.project.tasks[1].name, "Build");
    assert!(h.app.project.tasks[1].predecessors.is_empty());
}

#[test]
fn task_info_invalid_value_keeps_dialog_open() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.key(Key::F2, shift());
    h.app.dialogs.task_info.as_mut().unwrap().duration = "lots".into();
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.task_info.is_some(), "dialog stays open for correction");
    assert!(h.app.dialogs.message.is_some());
    h.key(Key::Enter, NONE); // dismiss message
    h.key(Key::Escape, NONE); // cancel dialog
    assert!(!h.app.dialogs.any_modal_open());
    assert_eq!(h.app.project.tasks[0].duration.minutes, 480);
}

#[test]
fn double_click_bar_opens_task_info() {
    let mut h = three_tasks();
    h.app.scroll_gantt_to(dt(1, 5, 8));
    h.frame(vec![]);
    let s = h.app.project.tasks[1].calc.start.unwrap();
    let f = h.app.project.tasks[1].calc.finish.unwrap();
    let p = pos2((h.gantt_x(s) + h.gantt_x(f)) / 2.0, h.gantt_row_y(1));
    h.double_click(p);
    let ti = h.app.dialogs.task_info.as_ref().expect("dialog open");
    assert_eq!(ti.name, "Build");
}

#[test]
fn drag_bar_moves_task_with_snet() {
    let mut h = three_tasks();
    h.app.scroll_gantt_to(dt(1, 5, 8));
    h.frame(vec![]);
    let t = &h.app.project.tasks[0];
    let (s, f) = (t.calc.start.unwrap(), t.calc.finish.unwrap());
    let y = h.gantt_row_y(0);
    let from = pos2((h.gantt_x(s) + h.gantt_x(f)) / 2.0, y);
    let to = from + vec2(2.0 * h.app.gantt.ppd, 0.0); // two days right
    h.drag(from, to);
    let t = &h.app.project.tasks[0];
    assert_eq!(t.constraint, ConstraintType::SNET);
    assert_eq!(t.calc.start, Some(dt(1, 7, 8)));
}

#[test]
fn drag_right_edge_changes_duration() {
    let mut h = three_tasks();
    h.app.scroll_gantt_to(dt(1, 5, 8));
    h.frame(vec![]);
    let f = h.app.project.tasks[0].calc.finish.unwrap();
    let y = h.gantt_row_y(0);
    let from = pos2(h.gantt_x(f) - 1.0, y);
    let to = from + vec2(2.0 * h.app.gantt.ppd, 0.0);
    h.drag(from, to);
    assert_eq!(h.app.project.tasks[0].duration.minutes, 3 * 480);
}

#[test]
fn drag_between_bars_creates_link() {
    let mut h = three_tasks();
    h.app.scroll_gantt_to(dt(1, 5, 8));
    h.frame(vec![]);
    let s = h.app.project.tasks[0].calc.start.unwrap();
    let f = h.app.project.tasks[0].calc.finish.unwrap();
    let from = pos2((h.gantt_x(s) + h.gantt_x(f)) / 2.0, h.gantt_row_y(0));
    let to = pos2(from.x, h.gantt_row_y(2));
    h.drag(from, to);
    assert_eq!(h.app.project.predecessors_text(2), "1");
    assert_eq!(h.app.project.tasks[2].predecessors[0].kind, LinkType::FS);
}

#[test]
fn clicking_triangle_collapses_summary() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.click_m(h.cell(2, name), shift());
    h.key(Key::ArrowRight, alt_shift());
    assert!(h.app.project.tasks[0].calc.is_summary);
    // triangle sits at the left of the Name cell
    let mut p = h.cell(0, name);
    p.x = h.app.last_table_rect.left() + row_header_w()
        + h.app.columns[..name].iter().map(|c| c.width).sum::<f32>()
        + 9.0;
    h.click(p);
    assert!(h.app.project.tasks[0].collapsed);
    assert_eq!(h.app.display_rows().len(), 1);
    // Alt+Shift+= shows subtasks again
    h.click(h.row_header(0));
    h.key(Key::Equals, alt_shift());
    assert!(!h.app.project.tasks[0].collapsed);
}

#[test]
fn copy_paste_preserves_internal_links() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.click_m(h.cell(1, name), shift());
    h.key(Key::F2, ctrl()); // link 1→2
    h.key(Key::C, ctrl());
    h.click(h.cell(3, name)); // blank row
    h.key(Key::V, ctrl());
    assert_eq!(h.names(), vec!["Design", "Build", "Test", "Design", "Build"]);
    assert_eq!(h.app.project.predecessors_text(4), "4");
}

#[test]
fn auto_calc_off_defers_rescheduling_until_f9() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.click_m(h.cell(1, name), shift());
    h.key(Key::F2, ctrl());
    assert_eq!(h.app.project.tasks[1].calc.start, Some(dt(1, 6, 8)));
    h.key(Key::F9, ctrl()); // automatic calculation off
    assert!(!h.app.auto_calc);
    let dur = h.col(ColumnId::Duration);
    h.click(h.cell(0, dur));
    h.type_and_enter("3d");
    assert_eq!(h.app.project.tasks[1].calc.start, Some(dt(1, 6, 8)), "not recalculated yet");
    assert!(h.app.needs_calc);
    h.key(Key::F9, NONE);
    assert_eq!(h.app.project.tasks[1].calc.start, Some(dt(1, 8, 8)));
}

#[test]
fn manual_and_auto_shortcuts() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.key(Key::M, ctrl_shift());
    assert_eq!(h.app.project.tasks[0].mode, projectfcg::model::TaskMode::Manual);
    h.key(Key::A, Modifiers { ctrl: true, command: true, alt: true, shift: true, mac_cmd: false });
    assert_eq!(h.app.project.tasks[0].mode, projectfcg::model::TaskMode::Auto);
}

#[test]
fn zoom_and_timescale_shortcuts() {
    let mut h = three_tasks();
    let before = h.app.gantt.ppd;
    h.key(Key::Slash, ctrl());
    assert!(h.app.gantt.ppd > before);
    // Ctrl+* (Shift+8 on US layouts) zooms back out
    h.frame(vec![
        Event::Key { key: Key::Num8, physical_key: None, pressed: true, repeat: false, modifiers: ctrl_shift() },
    ]);
    assert!((h.app.gantt.ppd - before).abs() < 1e-3, "zoomed back out");
    let left = h.app.gantt.left_days;
    h.key(Key::ArrowRight, Modifiers { alt: true, ..Default::default() });
    assert!(h.app.gantt.left_days > left);
}

#[test]
fn find_and_goto_dialogs() {
    let mut h = three_tasks();
    h.key(Key::F, ctrl());
    assert!(h.app.dialogs.find_open);
    h.text("tes");
    h.key(Key::Enter, NONE);
    assert_eq!(h.app.active_index(), Some(2));
    h.key(Key::Escape, NONE);
    assert!(!h.app.dialogs.find_open);
    h.key(Key::F5, NONE);
    assert!(h.app.dialogs.goto_open);
    h.text("2");
    h.key(Key::Enter, NONE);
    assert!(!h.app.dialogs.goto_open);
    assert_eq!(h.app.active_index(), Some(1));
}

#[test]
fn save_and_reopen_via_app() {
    let mut h = three_tasks();
    let dir = std::env::temp_dir().join(format!("projectfcg-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.projectfcg");
    assert!(h.app.save_to(path.clone()));
    assert!(!h.app.dirty);
    let mut h2 = H::blank();
    h2.app.open_path(path.clone());
    assert_eq!(h2.names(), vec!["Design", "Build", "Test"]);
    assert_eq!(h2.app.path.as_deref(), Some(path.as_path()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_with_unsaved_changes_prompts() {
    let mut h = three_tasks();
    h.key(Key::N, ctrl());
    assert!(h.app.dialogs.save_prompt);
    assert_eq!(h.app.project.tasks.len(), 3);
}

#[test]
fn sample_project_renders_many_frames() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    h.app.view.timeline = true;
    h.app.project.format.show_critical = true;
    h.app.project.format.show_slack = true;
    h.app.view.project_summary = true;
    h.app.view.outline_number = true;
    for t in projectfcg::app::commands::Timescale::ALL {
        h.app.execute(projectfcg::app::commands::Command::SetTimescale(t), &h.ctx.clone());
        h.frame(vec![]);
    }
    h.app.execute(projectfcg::app::commands::Command::ZoomEntireProject, &h.ctx.clone());
    h.frame(vec![]);
    for &c in projectfcg::app::columns::ColumnId::ALL {
        if !h.app.columns.iter().any(|s| s.id == c) {
            h.app.columns.push(projectfcg::app::ColumnState { id: c, width: c.default_width() });
        }
    }
    h.frame(vec![]);
    for tab in [
        projectfcg::app::RibbonTab::File,
        projectfcg::app::RibbonTab::Project,
        projectfcg::app::RibbonTab::View,
        projectfcg::app::RibbonTab::Format,
        projectfcg::app::RibbonTab::Help,
    ] {
        h.app.tab = tab;
        h.frame(vec![]);
    }
    // open every dialog once
    h.app.select_index(5);
    let ctx = h.ctx.clone();
    for c in [
        projectfcg::app::commands::Command::TaskInfo,
        projectfcg::app::commands::Command::ProjectInfo,
        projectfcg::app::commands::Command::WorkingTime,
        projectfcg::app::commands::Command::Options,
        projectfcg::app::commands::Command::Shortcuts,
        projectfcg::app::commands::Command::About,
    ] {
        h.app.execute(c, &ctx);
        h.frame(vec![]);
        h.key(Key::Escape, NONE);
        assert!(!h.app.dialogs.any_modal_open(), "{c:?} closed by Esc");
    }
}

#[test]
fn shortcut_help_lists_core_keys() {
    let keys: Vec<&str> = projectfcg::app::commands::SHORTCUT_HELP.iter().map(|(k, _)| *k).collect();
    for k in ["Ctrl+F2", "Shift+F2", "Insert", "F5", "Alt+Shift+→ / ←"] {
        assert!(keys.contains(&k), "{k}");
    }
}

// ---------------------------------------------------------------- iteration 2

use projectfcg::app::commands::Command;
use projectfcg::format::{BarCategory, GanttViewKind, LinkStyle, NonworkingDraw, Rgb, TsUnit};

fn ctrl_alt() -> Modifiers {
    Modifiers { ctrl: true, command: true, alt: true, ..Default::default() }
}

#[test]
fn text_size_shortcuts_scale_rows_and_columns() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    let w0 = h.app.columns[name].width;
    let r0 = row_h();
    for _ in 0..3 {
        h.key(Key::Period, ctrl_shift());
    }
    assert_eq!(h.app.prefs.text_size, 16.0);
    assert!(row_h() > r0, "rows grow with the text");
    assert!(h.app.columns[name].width > w0, "columns grow with the text");
    // clicking still hits the right cell at the new size
    h.click(h.cell(2, name));
    assert_eq!(h.app.active_index(), Some(2));
    for _ in 0..40 {
        h.key(Key::Comma, ctrl_shift());
    }
    assert_eq!(h.app.prefs.text_size, projectfcg::app::theme::MIN_TEXT_SIZE);
    h.key(Key::Num0, ctrl());
    assert_eq!(h.app.prefs.text_size, projectfcg::app::theme::DEFAULT_TEXT_SIZE);
    assert!((h.app.columns[name].width - w0).abs() <= 3.0, "widths return to normal");
}

#[test]
fn ctrl_minus_deletes_and_never_zooms_the_ui() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.key(Key::Minus, ctrl());
    assert_eq!(h.names(), vec!["Build", "Test"]);
    assert!((h.ctx.zoom_factor() - 1.0).abs() < 1e-6, "egui keyboard zoom is disabled");
    h.key(Key::Equals, ctrl_alt());
    assert!(h.app.prefs.ui_zoom > 1.0);
    h.frame(vec![]);
    assert!((h.ctx.zoom_factor() - h.app.prefs.ui_zoom).abs() < 1e-6);
    h.key(Key::Minus, ctrl_alt());
    h.frame(vec![]);
    assert!((h.app.prefs.ui_zoom - 1.0).abs() < 1e-6);
}

#[test]
fn critical_toggle_is_undoable_and_saved_with_file() {
    let mut h = three_tasks();
    let ctx = h.ctx.clone();
    h.app.execute(Command::ToggleCritical, &ctx);
    assert!(h.app.project.format.show_critical);
    let json = projectfcg::io::to_json(&h.app.project).unwrap();
    assert!(projectfcg::io::from_json(&json).unwrap().format.show_critical);
    h.key(Key::Z, ctrl());
    assert!(!h.app.project.format.show_critical);
}

#[test]
fn task_path_roles_follow_selection() {
    // Design → Build → Test, plus a parallel "Docs" feeding Test with slack
    let mut h = three_tasks();
    h.type_and_enter("Docs");
    let pred = h.col(ColumnId::Predecessors);
    let dur = h.col(ColumnId::Duration);
    h.click(h.cell(0, dur));
    h.type_and_enter("5d");
    h.click(h.cell(1, pred));
    h.type_and_enter("1");
    h.click(h.cell(2, pred));
    h.type_and_enter("2,4");
    let ctx = h.ctx.clone();
    h.app.execute(Command::TaskPath(projectfcg::app::bars::PathRole::DrivingPredecessor), &ctx);
    h.app.execute(Command::TaskPath(projectfcg::app::bars::PathRole::Predecessor), &ctx);
    h.click(h.cell(2, h.col(ColumnId::Name)));
    let roles = h.app.path_roles();
    use projectfcg::app::bars::PathRole::*;
    assert_eq!(roles.get(&0), Some(&DrivingPredecessor));
    assert_eq!(roles.get(&1), Some(&DrivingPredecessor));
    assert_eq!(roles.get(&3), Some(&Predecessor), "Docs has slack: predecessor but not driving");
    h.frame(vec![]); // renders highlighted bars
    h.app.execute(Command::ClearTaskPath, &ctx);
    assert!(h.app.path_roles().is_empty());
}

#[test]
fn format_bar_color_and_style_preset() {
    let mut h = three_tasks();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetBarColor(Some(Rgb(255, 0, 0))), &ctx);
    assert_eq!(h.app.project.tasks[1].bar_color, Some(Rgb(255, 0, 0)));
    let (_, st) = projectfcg::app::bars::style_for(&h.app, 1, None);
    assert_eq!(st.fill, Rgb(255, 0, 0));
    h.app.execute(Command::ApplyStylePreset(2), &ctx);
    assert_eq!(h.app.project.format.styles.get(BarCategory::Task).fill, projectfcg::format::STYLE_PRESETS[2].task);
    // per-task colour still wins over the preset
    let (_, st) = projectfcg::app::bars::style_for(&h.app, 1, None);
    assert_eq!(st.fill, Rgb(255, 0, 0));
    h.key(Key::Z, ctrl());
    h.key(Key::Z, ctrl());
    assert_eq!(h.app.project.tasks[1].bar_color, None);
}

#[test]
fn bar_styles_dialog_enter_applies() {
    let mut h = three_tasks();
    let ctx = h.ctx.clone();
    h.app.execute(Command::BarStylesDlg, &ctx);
    h.frame(vec![]);
    {
        let d = h.app.dialogs.bar_styles.as_mut().unwrap();
        let mut s = d.styles.get(BarCategory::Task);
        s.fill = Rgb(10, 20, 30);
        s.text.inside = projectfcg::format::BarField::Duration;
        d.styles.set(BarCategory::Task, s);
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.bar_styles.is_none());
    assert_eq!(h.app.project.format.styles.get(BarCategory::Task).fill, Rgb(10, 20, 30));
    // Esc cancels without changes
    h.app.execute(Command::BarStylesDlg, &ctx);
    h.frame(vec![]);
    h.app.dialogs.bar_styles.as_mut().unwrap().styles.reset();
    h.key(Key::Escape, NONE);
    assert_eq!(h.app.project.format.styles.get(BarCategory::Task).fill, Rgb(10, 20, 30));
}

#[test]
fn baseline_dialog_and_tracking_view() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetBaseline, &ctx);
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.project.has_baseline());
    // slip a task and check the variance column
    h.app.edit_project("slip", |p| p.set_field_text(5, projectfcg::ops::Field::Duration, "12d"));
    assert_eq!(projectfcg::app::columns::cell_text(&h.app.project, 5, ColumnId::FinishVariance), "2 days");
    h.app.execute(Command::SetGanttView(GanttViewKind::Tracking), &ctx);
    h.frame(vec![]);
    assert!(h.app.project.format.baseline_on());
    h.app.execute(Command::SetFilter(projectfcg::app::commands::Filter::Slipping), &ctx);
    assert!(h.app.display_rows().len() >= 2, "slipping tasks and their successors");
}

#[test]
fn every_format_option_renders() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    let ctx = h.ctx.clone();
    h.app.project.status_date = Some(dt(12, 10, 17));
    h.app.recalc();
    h.app.view.inspector = true;
    h.app.view.highlight = projectfcg::app::commands::Filter::Critical;
    h.app.project.format.show_late = true;
    for v in GanttViewKind::ALL {
        h.app.execute(Command::SetGanttView(v), &ctx);
        for links in [LinkStyle::Elbow, LinkStyle::Straight, LinkStyle::Hidden] {
            h.app.project.format.layout.links = links;
            h.frame(vec![]);
        }
    }
    let g = &mut h.app.project.format.gridlines;
    g.gantt_rows = true;
    g.major_columns = true;
    g.minor_columns = true;
    g.project_finish = true;
    for nw in [NonworkingDraw::Behind, NonworkingDraw::InFront, NonworkingDraw::Hidden] {
        h.app.project.format.timescale.nonworking = nw;
        h.frame(vec![]);
    }
    let ts = &mut h.app.project.format.timescale;
    ts.auto = false;
    for tiers in 1..=3u8 {
        h.app.project.format.timescale.tiers = tiers;
        for u in TsUnit::ALL {
            h.app.project.format.timescale.bottom.unit = u;
            h.frame(vec![]);
        }
    }
    for k in 0..projectfcg::format::STYLE_PRESETS.len() {
        h.app.execute(Command::ApplyStylePreset(k), &ctx);
    }
    for c in BarCategory::ALL {
        let mut s = h.app.project.format.styles.get(c);
        s.pattern = projectfcg::format::Pattern::Hatched;
        s.start_shape = projectfcg::format::EndShape::Circle;
        s.text = projectfcg::format::BarText {
            left: projectfcg::format::BarField::Id,
            right: projectfcg::format::BarField::Name,
            top: projectfcg::format::BarField::Start,
            bottom: projectfcg::format::BarField::Finish,
            inside: projectfcg::format::BarField::PercentComplete,
        };
        h.app.project.format.styles.set(c, s);
    }
    for (k, _) in h.app.project.tasks.clone().iter().enumerate() {
        h.app.select_index(k);
        h.frame(vec![]);
    }
    // all format dialogs open and close
    for c in [
        Command::BarStylesDlg,
        Command::TextStylesDlg,
        Command::GridlinesDlg,
        Command::LayoutDlg,
        Command::TimescaleDlg,
        Command::FormatBarDlg,
        Command::SetBaseline,
    ] {
        h.app.execute(c, &ctx);
        h.frame(vec![]);
        h.key(Key::Escape, NONE);
        assert!(!h.app.dialogs.any_modal_open(), "{c:?}");
    }
}

#[test]
fn text_styles_and_highlight_affect_rows() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    h.app.project.format.text.critical = projectfcg::format::TextStyle { color: Some(Rgb(200, 0, 0)), bold: true };
    let crit = (0..h.app.project.tasks.len())
        .find(|&i| h.app.project.tasks[i].calc.critical && !h.app.project.tasks[i].calc.is_summary)
        .unwrap();
    let (col, bold) = projectfcg::app::table::row_style(&h.app, crit);
    assert_eq!(col, egui::Color32::from_rgb(200, 0, 0));
    assert!(bold);
    let (_, bold) = projectfcg::app::table::row_style(&h.app, 0);
    assert!(bold, "summary rows are bold by default");
    let ctx = h.ctx.clone();
    let n = h.app.display_rows().len();
    h.app.execute(Command::SetHighlight(projectfcg::app::commands::Filter::Critical), &ctx);
    assert_eq!(h.app.display_rows().len(), n, "highlight shades rows but hides none");
}

// ---------------------------------------------------------------- iteration 3: resources

use projectfcg::app::MainView;

#[test]
fn typing_resource_names_creates_resources_and_work() {
    let mut h = three_tasks();
    let rn = h.col(ColumnId::ResourceNames);
    let dur = h.col(ColumnId::Duration);
    h.click(h.cell(0, dur));
    h.type_and_enter("4d");
    h.goto(0, rn);
    assert!(h.app.table_scroll_x > 0.0, "Resource Names is off to the right and must scroll into view");
    assert!(h.cell(0, rn).x < h.app.last_table_rect.right());
    h.type_and_enter("Alice[50%], Bob");
    let p = &h.app.project;
    assert_eq!(p.resources.len(), 2);
    assert_eq!(p.resource_names_text(0), "Alice[50%],Bob");
    assert_eq!(p.tasks[0].calc.work, 4 * 480 / 2 + 4 * 480);
    assert_eq!(projectfcg::app::columns::cell_text(p, 0, ColumnId::Work), "48 hrs");
    // undo removes both resources and the assignment in one step
    h.key(Key::Z, ctrl());
    assert!(h.app.project.resources.is_empty());
    assert!(h.app.project.assignments.is_empty());
}

#[test]
fn resource_sheet_editing_with_keyboard() {
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.execute(Command::ShowView(MainView::ResourceSheet), &ctx);
    h.frame(vec![]);
    // type in the blank row → new resource
    h.type_and_enter("Alice");
    h.type_and_enter("Bob");
    assert_eq!(h.app.project.resources.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["Alice", "Bob"]);
    // go to Alice's Max Units: up twice, right to Max.
    h.key(Key::ArrowUp, NONE);
    h.key(Key::ArrowUp, NONE);
    let max_col = h.app.sheet.columns.iter().position(|c| c.0 == projectfcg::app::resource_sheet::ResCol::MaxUnits).unwrap();
    for _ in h.app.sheet.col..max_col {
        h.key(Key::ArrowRight, NONE);
    }
    h.type_and_enter("50%");
    assert!((h.app.project.resources[0].max_units - 0.5).abs() < 1e-9);
    // Std. Rate accepts per-day rates
    h.key(Key::ArrowUp, NONE);
    h.key(Key::ArrowRight, NONE);
    h.type_and_enter("400/d");
    assert!((h.app.project.resources[0].std_rate - 50.0).abs() < 1e-9);
    // Insert adds a resource; Delete removes the selected one
    h.key(Key::Escape, NONE);
    h.key(Key::Insert, NONE);
    h.key(Key::Escape, NONE);
    assert_eq!(h.app.project.resources.len(), 3);
    h.key(Key::Delete, NONE);
    assert_eq!(h.app.project.resources.len(), 2);
    // Shift+F2 opens Resource Information; Enter applies
    h.key(Key::ArrowUp, ctrl());
    h.key(Key::F2, shift());
    h.app.dialogs.resource_info.as_mut().expect("resource info").name = "Alice Smith".into();
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert_eq!(h.app.project.resources[0].name, "Alice Smith");
}

#[test]
fn resource_info_vacation_moves_tasks() {
    let mut h = three_tasks();
    let rn = h.col(ColumnId::ResourceNames);
    h.goto(0, rn);
    h.type_and_enter("Alice");
    let ctx = h.ctx.clone();
    h.app.execute(Command::ShowView(MainView::ResourceSheet), &ctx);
    h.frame(vec![]);
    h.app.execute(Command::ResourceInfo, &ctx);
    {
        let d = h.app.dialogs.resource_info.as_mut().unwrap();
        d.exceptions.push(projectfcg::calendar::CalendarException {
            name: "Vacation".into(),
            start: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            shifts: vec![],
        });
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert_eq!(h.app.project.tasks[0].calc.start, Some(dt(1, 6, 8)), "Alice's task waits for her");
}

#[test]
fn assign_window_is_non_modal() {
    let mut h = three_tasks();
    h.key(Key::F10, Modifiers { alt: true, ..Default::default() });
    assert!(h.app.dialogs.assign.is_some());
    assert!(!h.app.dialogs.any_modal_open());
    // the task table still takes keystrokes while it is open
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::Insert, NONE);
    h.text("Extra");
    h.key(Key::Enter, NONE);
    assert_eq!(h.names(), vec!["Design", "Extra", "Build", "Test"]);
    h.key(Key::F10, Modifiers { alt: true, ..Default::default() });
    assert!(h.app.dialogs.assign.is_none());
}

#[test]
fn task_information_resources_tab() {
    let mut h = three_tasks();
    h.app.edit_project("add", |p| {
        p.add_resource("Alice", projectfcg::model::ResourceType::Work);
        Ok(())
    });
    let uid = h.app.project.resources[0].uid;
    let name = h.col(ColumnId::Name);
    h.click(h.cell(1, name));
    h.key(Key::F2, shift());
    {
        let ti = h.app.dialogs.task_info.as_mut().unwrap();
        ti.assigns.push(projectfcg::app::dialogs::AssignRow { res_uid: uid, units: "25%".into(), units0: String::new() });
        ti.task_type = projectfcg::model::TaskType::FixedWork;
        ti.priority = 800;
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    let p = &h.app.project;
    assert_eq!(p.resource_names_text(1), "Alice[25%]");
    assert_eq!(p.tasks[1].task_type, projectfcg::model::TaskType::FixedWork);
    assert_eq!(p.tasks[1].priority, 800);
}

#[test]
fn level_all_on_sample_and_undo() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    assert!(h.app.project.calc.resources.iter().any(|r| r.overallocated));
    let ctx = h.ctx.clone();
    h.app.execute(Command::NextOverallocation, &ctx);
    let first = h.app.active_index().unwrap();
    assert!(h.app.project.tasks[first].calc.overallocated);
    h.app.execute(Command::LevelAll, &ctx);
    assert!(h.app.project.calc.resources.iter().all(|r| !r.overallocated));
    assert!(h.app.project.tasks.iter().any(|t| t.leveling_delay > 0));
    h.app.execute(Command::SetGanttView(GanttViewKind::Leveling), &ctx);
    h.frame(vec![]);
    h.key(Key::Z, ctrl()); // undo view change
    h.key(Key::Z, ctrl()); // undo leveling
    assert!(h.app.project.calc.resources.iter().any(|r| r.overallocated));
}

#[test]
fn usage_views_render_and_navigate() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    let ctx = h.ctx.clone();
    for v in [MainView::ResourceUsage, MainView::TaskUsage, MainView::ResourceSheet, MainView::Gantt] {
        h.app.execute(Command::ShowView(v), &ctx);
        for t in projectfcg::app::commands::Timescale::ALL {
            h.app.execute(Command::SetTimescale(t), &ctx);
            h.frame(vec![]);
        }
    }
    h.app.execute(Command::ShowView(MainView::ResourceUsage), &ctx);
    h.frame(vec![]);
    h.key(Key::ArrowDown, NONE);
    h.key(Key::ArrowDown, NONE);
    assert!(h.app.usage.active.is_some());
    // usage rows: every resource followed by its assignments
    let rows = projectfcg::app::usage::rows(&h.app, projectfcg::app::usage::UsageKind::Resource);
    assert_eq!(rows.len(), h.app.project.resources.len() + h.app.project.assignments.len());
    // text size changes still lay out
    h.key(Key::Period, ctrl_shift());
    h.frame(vec![]);
}

#[test]
fn options_turn_off_auto_add_resources() {
    let mut h = three_tasks();
    let ctx = h.ctx.clone();
    h.app.execute(Command::Options, &ctx);
    h.frame(vec![]);
    h.app.dialogs.options.as_mut().expect("options").auto_add = false;
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(!h.app.project.settings.auto_add_resources);
    let rn = h.col(ColumnId::ResourceNames);
    h.goto(0, rn);
    h.type_and_enter("Nobody");
    assert!(h.app.project.resources.is_empty(), "unknown names are refused");
    assert!(h.app.status_error);
}

// ---------------------------------------------------------------- 0.4: themes

use projectfcg::app::theme::{pal, Accent, Palette, ThemeMode};

#[test]
fn default_theme_is_navy_and_follows_system_with_light_fallback() {
    let h = H::blank();
    assert_eq!(h.app.prefs.accent, Accent::Navy);
    assert_eq!(h.app.prefs.theme_mode, ThemeMode::System);
    // no desktop answer in the test harness → light
    assert!(!pal().dark);
    assert_eq!(pal(), Palette::new(Accent::Navy, false));
}

#[test]
fn theme_commands_switch_palette_and_egui_visuals() {
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetThemeMode(ThemeMode::Dark), &ctx);
    h.frame(vec![]);
    assert!(pal().dark);
    assert!(h.ctx.global_style().visuals.dark_mode, "egui widgets follow");
    assert_eq!(h.ctx.global_style().visuals.panel_fill, pal().panel);
    h.app.execute(Command::SetAccent(Accent::Green), &ctx);
    h.frame(vec![]);
    assert_eq!(pal(), Palette::new(Accent::Green, true));
    // Ctrl+Shift+F11 toggles light/dark
    h.key(Key::F11, ctrl_shift());
    assert_eq!(h.app.prefs.theme_mode, ThemeMode::Light);
    assert!(!pal().dark && !h.ctx.global_style().visuals.dark_mode);
    h.key(Key::F11, ctrl_shift());
    assert_eq!(h.app.prefs.theme_mode, ThemeMode::Dark);
    // theme is a per-user preference, not an edit to the project
    assert!(!h.app.history.can_undo());
    assert!(!h.app.dirty);
}

#[test]
fn theme_is_saved_with_preferences() {
    let dir = std::env::temp_dir().join(format!("projectfcg-ui-theme-{}", std::process::id()));
    let path = dir.join("prefs.json");
    let mut h = H::blank();
    h.app.prefs_path = Some(path.clone());
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetAccent(Accent::Teal), &ctx);
    h.app.execute(Command::SetThemeMode(ThemeMode::Dark), &ctx);
    let saved = projectfcg::app::prefs::Prefs::load(&path);
    assert_eq!((saved.accent, saved.theme_mode), (Accent::Teal, ThemeMode::Dark));
    std::fs::remove_dir_all(&dir).ok();
}

/// Every tab, view and dialog draws in every theme without panicking, and
/// switching themes doesn't disturb the project or selection.
#[test]
fn everything_renders_in_every_theme() {
    use projectfcg::app::RibbonTab;
    let mut h = H::new(projectfcg::app::sample::software_release());
    h.app.view.timeline = true;
    h.app.view.inspector = true;
    let before = projectfcg::io::to_json(&h.app.project).unwrap();
    let ctx = h.ctx.clone();
    let uid = h.app.project.tasks[12].uid;
    for (k, accent) in Accent::ALL.into_iter().enumerate() {
        let mode = if k % 2 == 0 { ThemeMode::Dark } else { ThemeMode::Light };
        h.app.execute(Command::SetAccent(accent), &ctx);
        h.app.execute(Command::SetThemeMode(mode), &ctx);
        for tab in [RibbonTab::File, RibbonTab::Task, RibbonTab::Resource, RibbonTab::Project, RibbonTab::View, RibbonTab::Format, RibbonTab::Help] {
            h.app.tab = tab;
            h.frame(vec![]);
        }
        for v in [GanttViewKind::Gantt, GanttViewKind::Tracking, GanttViewKind::Detail, GanttViewKind::Leveling] {
            h.app.execute(Command::SetGanttView(v), &ctx);
            h.frame(vec![]);
        }
        for v in [MainView::TaskUsage, MainView::ResourceSheet, MainView::ResourceUsage, MainView::Gantt] {
            h.app.execute(Command::ShowView(v), &ctx);
            h.frame(vec![]);
        }
        h.app.active_row = RowRef::Task(uid);
        for dlg in [
            Command::TaskInfo, Command::ProjectInfo, Command::WorkingTime, Command::Options, Command::BarStylesDlg,
            Command::FormatBarDlg, Command::TextStylesDlg, Command::GridlinesDlg, Command::LayoutDlg, Command::TimescaleDlg,
            Command::LevelingOptions, Command::Shortcuts, Command::About, Command::GoTo, Command::Find,
        ] {
            h.app.execute(dlg.clone(), &ctx);
            h.frame(vec![]);
            h.frame(vec![]);
            h.key(Key::Escape, NONE);
        }
        assert!(!h.app.dialogs.any_modal_open(), "{accent:?}: a dialog stayed open");
        h.app.execute(Command::AssignResources, &ctx);
        h.frame(vec![]);
        h.app.execute(Command::AssignResources, &ctx);
        h.app.execute(Command::ShowView(MainView::ResourceSheet), &ctx);
        h.app.execute(Command::ResourceInfo, &ctx);
        h.frame(vec![]);
        h.key(Key::Escape, NONE);
        h.app.execute(Command::ShowView(MainView::Gantt), &ctx);
        assert_eq!(pal().dark, mode == ThemeMode::Dark);
    }
    // undo the view switches; the task data itself was never touched
    let after = projectfcg::app::sample::software_release();
    assert_eq!(h.app.project.tasks.len(), after.tasks.len());
    let mut p = h.app.project.clone();
    p.format = after.format.clone();
    let mut q = projectfcg::io::from_json(&before).unwrap();
    q.format = after.format.clone();
    assert_eq!(projectfcg::io::to_json(&p).unwrap(), projectfcg::io::to_json(&q).unwrap());
}

/// Every symbol used in a UI string literal exists in the bundled fonts
/// (a missing one draws as an empty box: 🗓 did, for a whole release).
#[test]
fn every_symbol_in_the_ui_has_a_glyph() {
    let h = H::blank();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app");
    let mut missing: Vec<String> = vec![];
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for (ln, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") || t.starts_with("///") || t.starts_with("//!") {
                continue;
            }
            // characters inside "…" string literals
            let mut in_str = false;
            let mut prev = ' ';
            for ch in line.chars() {
                if ch == '"' && prev != '\\' {
                    in_str = !in_str;
                } else if in_str && (ch as u32) >= 0x2000 && !ch.is_whitespace() {
                    let ok = h.ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(13.0), ch));
                    if !ok {
                        missing.push(format!("{}:{} {ch:?} U+{:04X}", path.file_name().unwrap().to_string_lossy(), ln + 1, ch as u32));
                    }
                }
                prev = ch;
            }
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "symbols with no glyph:\n{}", missing.join("\n"));
}

// ---------------------------------------------------------------- 0.5: board

fn board_h() -> H {
    let mut h = H::new(projectfcg::app::sample::agile_release());
    let ctx = h.ctx.clone();
    h.app.execute(Command::ShowView(MainView::Board), &ctx);
    h.frame(vec![]);
    h.frame(vec![]);
    h
}

fn card_center(h: &H, id: u32) -> Pos2 {
    h.app.board.card_rects.iter().find(|(i, _)| *i == id).map(|(_, r)| r.center()).expect("card on screen")
}

fn col_of(h: &H, id: u32) -> (Option<u32>, usize) {
    let t = h.app.project.ticket(id).unwrap();
    (t.sprint, t.column)
}

#[test]
fn agile_sample_opens_on_the_board() {
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.execute(Command::NewFromAgileSample, &ctx);
    h.frame(vec![]);
    h.frame(vec![]);
    assert_eq!(h.app.view.main, MainView::Board);
    assert_eq!(h.app.tab, projectfcg::app::RibbonTab::Board);
    let s = h.app.board_sprint().expect("a sprint is shown");
    assert_eq!(h.app.project.task_by_uid(s).unwrap().name, "Sprint 3", "the running sprint");
    assert!(!h.app.board.card_rects.is_empty());
    assert_eq!(h.app.board.column_rects.len(), 4);
}

#[test]
fn drag_card_between_columns_and_undo() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let todo = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.column == 0).unwrap().id;
    let before = h.app.project.sprint_stats(Some(s));
    let done_col = h.app.board.column_rects[3];
    let from = card_center(&h, todo);
    h.drag(from, pos2(done_col.center().x, done_col.bottom() - 40.0));
    assert_eq!(col_of(&h, todo), (Some(s), 3), "dropped in Done");
    assert!(h.app.project.ticket(todo).unwrap().done_at.is_some());
    let after = h.app.project.sprint_stats(Some(s));
    assert!(after.done > before.done);
    // the sprint task's % complete followed
    let row = h.app.project.index_of_uid(s).unwrap();
    assert!((h.app.project.tasks[row].percent_complete - after.percent()).abs() < 0.1);
    h.key(Key::Z, ctrl());
    assert_eq!(col_of(&h, todo), (Some(s), 0), "undo puts it back");
}

#[test]
fn drag_from_backlog_into_sprint_and_back() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let b = h.app.project.tickets.iter().find(|t| t.sprint.is_none()).unwrap().id;
    let col1 = h.app.board.column_rects[1];
    h.drag(card_center(&h, b), pos2(col1.center().x, col1.bottom() - 30.0));
    assert_eq!(col_of(&h, b), (Some(s), 1));
    h.frame(vec![]);
    let back = h.app.board.backlog_rect;
    h.drag(card_center(&h, b), pos2(back.center().x, back.bottom() - 30.0));
    assert_eq!(col_of(&h, b).0, None, "back in the backlog");
}

#[test]
fn drop_position_orders_cards() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let done: Vec<u32> = h.app.project.tickets.iter().filter(|t| t.sprint == Some(s) && t.column == 3).map(|t| t.id).collect();
    assert!(done.len() >= 2);
    let todo = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.column == 0).unwrap().id;
    // drop just above the first done card
    let first = h.app.board.card_rects.iter().find(|(i, _)| *i == done[0]).unwrap().1;
    h.drag(card_center(&h, todo), pos2(first.center().x, first.top() + 3.0));
    let order: Vec<u32> = h.app.project.tickets.iter().filter(|t| t.sprint == Some(s) && t.column == 3).map(|t| t.id).collect();
    assert_eq!(order[0], todo, "inserted before the card it was dropped on");
}

#[test]
fn keyboard_moves_selects_edits_and_deletes() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    h.key(Key::ArrowDown, NONE); // selects the first card
    let first = h.app.board.selected.expect("selected");
    let (sp, c) = col_of(&h, first);
    // Ctrl+Right moves it a column right
    h.key(Key::ArrowRight, ctrl());
    let (sp2, c2) = col_of(&h, first);
    if sp.is_some() {
        assert_eq!((sp2, c2), (sp, c + 1));
    } else {
        assert_eq!(sp2, Some(s), "from the backlog Ctrl+→ plans it into the sprint");
    }
    // Enter opens the ticket; Esc closes without change
    h.key(Key::Enter, NONE);
    assert_eq!(h.app.dialogs.ticket.as_ref().and_then(|d| d.id), Some(first));
    h.frame(vec![]);
    h.key(Key::Escape, NONE);
    assert!(h.app.dialogs.ticket.is_none());
    // Delete removes it; undo restores
    let n = h.app.project.tickets.len();
    h.key(Key::Delete, NONE);
    assert_eq!(h.app.project.tickets.len(), n - 1);
    h.key(Key::Z, ctrl());
    assert_eq!(h.app.project.tickets.len(), n);
}

#[test]
fn insert_creates_ticket_through_dialog() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let n = h.app.project.tickets_in(Some(s)).len();
    h.key(Key::Insert, NONE);
    assert!(h.app.dialogs.ticket.is_some());
    h.frame(vec![]);
    h.text("Export to CSV");
    {
        let d = h.app.dialogs.ticket.as_mut().unwrap();
        assert_eq!(d.title, "Export to CSV", "title field has focus");
        d.points = "5".into();
        d.description = "Line one\nLine two".into();
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.ticket.is_none());
    assert_eq!(h.app.project.tickets_in(Some(s)).len(), n + 1);
    let t = h.app.project.tickets.iter().find(|t| t.title == "Export to CSV").unwrap();
    assert_eq!((t.points, t.column, t.sprint), (Some(5.0), 0, Some(s)));
    assert_eq!(h.app.board.selected, Some(t.id));
    // bad points are refused with a message
    h.key(Key::Insert, NONE);
    h.frame(vec![]);
    {
        let d = h.app.dialogs.ticket.as_mut().unwrap();
        d.title = "X".into();
        d.points = "lots".into();
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    let d = h.app.dialogs.ticket.as_ref().expect("still open");
    assert!(d.error.as_deref().unwrap_or("").contains("isn't a number"));
}

#[test]
fn quick_add_in_first_column() {
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let col0 = h.app.board.column_rects[0];
    let bottom = h
        .app
        .board
        .card_rects
        .iter()
        .filter(|(_, r)| col0.contains(r.center()))
        .map(|(_, r)| r.bottom())
        .fold(col0.top() + 34.0, f32::max);
    h.click(pos2(col0.center().x, bottom + 8.0 + 12.0));
    h.text("Quick one");
    h.key(Key::Enter, NONE);
    let t = h.app.project.tickets.iter().find(|t| t.title == "Quick one").expect("created");
    assert_eq!((t.sprint, t.column), (Some(s), 0));
}

#[test]
fn new_sprint_from_an_empty_board() {
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.execute(Command::ShowView(MainView::Board), &ctx);
    h.frame(vec![]);
    assert!(h.app.board_sprint().is_none());
    h.app.execute(Command::NewSprint, &ctx);
    h.frame(vec![]);
    let s = h.app.board_sprint().expect("shown");
    let row = h.app.project.index_of_uid(s).unwrap();
    assert!(h.app.project.tasks[row].is_sprint);
    assert_eq!(h.app.project.tasks[row].duration.minutes, 10 * 480);
    assert_eq!(h.app.board.column_rects.len(), 4);
}

#[test]
fn gantt_side_of_sprints() {
    let mut h = H::new(projectfcg::app::sample::agile_release());
    let ctx = h.ctx.clone();
    let row = h.app.project.tasks.iter().position(|t| t.name == "Sprint 3").unwrap();
    let uid = h.app.project.tasks[row].uid;
    // % complete is the board's
    h.app.select_single(RowRef::Task(uid));
    h.app.execute(Command::SetPercent(100), &ctx);
    assert!(h.app.status_error);
    assert!(h.app.project.tasks[row].percent_complete < 100.0);
    let msg = h.app.dialogs.message.as_ref().map(|m| m.1.clone()).unwrap_or_default();
    assert!(msg.contains("comes from the Board"), "{msg}");
    h.frame(vec![]);
    h.key(Key::Escape, NONE);
    assert!(h.app.dialogs.message.is_none());
    // Open on Board shows that sprint
    h.app.execute(Command::OpenOnBoard, &ctx);
    assert_eq!((h.app.view.main, h.app.board.sprint), (MainView::Board, Some(uid)));
    // Mark/unmark from the Gantt; unmarking sends its tickets to the backlog
    h.app.execute(Command::ShowView(MainView::Gantt), &ctx);
    let n_backlog = h.app.project.tickets_in(None).len();
    let n_mine = h.app.project.tickets_in(Some(uid)).len();
    h.app.execute(Command::ToggleSprint, &ctx);
    assert!(!h.app.project.tasks[row].is_sprint);
    assert_eq!(h.app.project.tickets_in(None).len(), n_backlog + n_mine);
    h.key(Key::Z, ctrl());
    assert!(h.app.project.tasks[row].is_sprint);
    // Task Information has the sprint switch
    h.key(Key::F2, shift());
    assert!(h.app.dialogs.task_info.as_ref().unwrap().is_sprint);
    assert!(h.app.dialogs.task_info.as_ref().unwrap().board_progress);
    h.frame(vec![]);
    h.key(Key::Escape, NONE);
}

#[test]
fn estimate_mode_is_undoable_and_changes_progress() {
    let mut h = board_h();
    let ctx = h.ctx.clone();
    let s = h.app.board_sprint().unwrap();
    let row = h.app.project.index_of_uid(s).unwrap();
    let by_points = h.app.project.tasks[row].percent_complete;
    h.app.execute(Command::SetEstimate(projectfcg::board::Estimate::TicketCount), &ctx);
    let by_count = h.app.project.tasks[row].percent_complete;
    let st = h.app.project.sprint_stats(Some(s));
    assert!((by_count - st.done_tickets as f64 / st.tickets as f64 * 100.0).abs() < 0.1);
    assert_ne!(by_points, by_count);
    h.key(Key::Z, ctrl());
    assert_eq!(h.app.project.board.estimate, projectfcg::board::Estimate::StoryPoints);
}

#[test]
fn board_renders_everywhere() {
    let mut h = board_h();
    let ctx = h.ctx.clone();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        h.app.execute(Command::SetThemeMode(mode), &ctx);
        for (charts, backlog) in [(true, true), (false, true), (true, false), (false, false)] {
            h.app.board.show_charts = charts;
            h.app.board.backlog_open = backlog;
            h.frame(vec![]);
        }
        // each sprint, including the future one with no done work
        for row in h.app.project.sprint_rows() {
            h.app.board.sprint = Some(h.app.project.tasks[row].uid);
            h.frame(vec![]);
        }
        h.key(Key::Period, ctrl_shift()); // bigger text still lays out
        h.frame(vec![]);
        h.app.tab = projectfcg::app::RibbonTab::Board;
        h.frame(vec![]);
    }
}

// ---------------------------------------------------------------- 0.6: tracking

#[test]
fn tables_switch_columns_and_edit_actuals() {
    let mut h = three_tasks();
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetTable(projectfcg::app::columns::Table::Tracking), &ctx);
    h.frame(vec![]);
    let ids: Vec<ColumnId> = h.app.columns.iter().map(|c| c.id).collect();
    assert!(ids.contains(&ColumnId::ActualStart) && ids.contains(&ColumnId::RemainingDuration));
    assert!(!ids.contains(&ColumnId::Predecessors));
    // type an actual start into the cell
    let col = h.col(ColumnId::ActualStart);
    h.goto(1, col);
    h.type_and_enter("1/6/26");
    let t = &h.app.project.tasks[1];
    assert_eq!(t.actual_start, Some(dt(1, 6, 8)));
    assert_eq!(t.calc.start, Some(dt(1, 6, 8)));
    // remaining duration
    let col = h.col(ColumnId::RemainingDuration);
    h.goto(1, col);
    h.type_and_enter("3d");
    assert_eq!(h.app.project.tasks[1].duration.minutes, 3 * 480);
    // back to Entry
    h.app.execute(Command::SetTable(projectfcg::app::columns::Table::Entry), &ctx);
    assert!(h.app.columns.iter().any(|c| c.id == ColumnId::Predecessors));
    // every table draws
    for t in projectfcg::app::columns::Table::ALL {
        h.app.execute(Command::SetTable(t), &ctx);
        h.frame(vec![]);
    }
}

#[test]
fn update_tasks_dialog() {
    let mut h = three_tasks();
    let ctx = h.ctx.clone();
    let name = h.col(ColumnId::Name);
    h.click(h.cell(0, name));
    h.app.execute(Command::UpdateTasks, &ctx);
    h.frame(vec![]);
    h.app.dialogs.update_tasks.as_mut().unwrap().percent = 100.0;
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.update_tasks.is_none());
    let t = &h.app.project.tasks[0];
    assert_eq!(t.percent_complete, 100.0);
    assert!(t.actual_start.is_some() && t.actual_finish.is_some());
    // bad date keeps the dialog open with the message
    h.app.execute(Command::UpdateTasks, &ctx);
    h.frame(vec![]);
    h.app.dialogs.update_tasks.as_mut().unwrap().actual_start = "not a date".into();
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.update_tasks.as_ref().is_some_and(|d| d.error.is_some()));
    h.key(Key::Escape, NONE);
    // Mark on Track at a status date
    h.app.project.status_date = Some(dt(1, 5, 17));
    h.click(h.cell(1, name));
    h.app.execute(Command::MarkOnTrack, &ctx);
    assert_eq!(h.app.project.tasks[1].percent_complete, 100.0, "1-day task due by the status date");
}

#[test]
fn update_project_dialog_and_statistics() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    let ctx = h.ctx.clone();
    h.app.execute(Command::SetBaseline, &ctx);
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.project.has_baseline_n(0));
    h.app.execute(Command::UpdateProject, &ctx);
    h.frame(vec![]);
    h.app.dialogs.update_project.as_mut().unwrap().through = "11/20/26 5:00 PM".into();
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.update_project.is_none());
    assert!(h.app.status.starts_with("Update Project:"), "{}", h.app.status);
    let p = &h.app.project;
    assert!(p.tasks.iter().any(|t| !t.calc.is_summary && t.percent_complete > 0.0 && t.percent_complete < 100.0));
    // earned value exists now
    h.app.project.status_date = Some(dt(11, 20, 17));
    h.app.recalc();
    let ev = h.app.project.project_earned_value();
    assert!(ev.bac > 0.0 && ev.bcwp > 0.0);
    h.app.execute(Command::ProjectStatistics, &ctx);
    h.frame(vec![]);
    h.frame(vec![]);
    h.key(Key::Escape, NONE);
    assert!(!h.app.dialogs.statistics);
    // one undo removes the whole update
    let before = h.app.history.undo_label().map(str::to_string);
    assert_eq!(before.as_deref(), Some("Update Project"));
}

#[test]
fn baselines_and_progress_lines_draw() {
    let mut h = H::new(projectfcg::app::sample::software_release());
    let ctx = h.ctx.clone();
    // Baseline 2 through the dialog
    h.app.execute(Command::SetBaseline, &ctx);
    h.frame(vec![]);
    h.app.dialogs.baseline.as_mut().unwrap().n = 2;
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.project.has_baseline_n(2) && !h.app.project.has_baseline_n(0));
    assert!(h.app.project.baseline_saved[2].is_some());
    h.app.execute(Command::SetShownBaseline(2), &ctx);
    h.app.execute(Command::ToggleProgressLines, &ctx);
    assert!(h.app.project.format.gridlines.progress_lines);
    for v in [GanttViewKind::Tracking, GanttViewKind::Gantt] {
        h.app.execute(Command::SetGanttView(v), &ctx);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            h.app.execute(Command::SetThemeMode(mode), &ctx);
            h.frame(vec![]);
        }
    }
    for tab in [projectfcg::app::RibbonTab::Task, projectfcg::app::RibbonTab::Project, projectfcg::app::RibbonTab::Format, projectfcg::app::RibbonTab::View] {
        h.app.tab = tab;
        h.frame(vec![]);
    }
}

// ---------------------------------------------------------------- 0.6: epics, filters, Jira, XML

#[test]
fn board_filter_and_epics_panel() {
    use projectfcg::app::board::{EpicFilter, SidePanel};
    use projectfcg::board::IssueType;
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let all = h.app.board.card_rects.len();
    // type in the filter box (Ctrl+F focuses it)
    h.key(Key::F, ctrl());
    h.frame(vec![]);
    h.text("checkout");
    h.frame(vec![]);
    let shown: Vec<u32> = h.app.board.card_rects.iter().map(|(i, _)| *i).collect();
    assert!(!shown.is_empty() && shown.len() < all, "{} of {all}", shown.len());
    assert!(shown.iter().all(|&i| h.app.project.ticket(i).unwrap().title.to_lowercase().contains("checkout")));
    h.key(Key::Escape, NONE);
    h.frame(vec![]);
    assert!(h.app.board.filter.text.is_empty());
    // epics tab: click an epic to filter by it
    h.app.board.side = SidePanel::Epics;
    h.frame(vec![]);
    let checkout = h.app.project.tickets.iter().find(|t| t.kind == IssueType::Epic && t.title == "Checkout").unwrap().id;
    let r = h.app.board.epic_rects.iter().find(|(e, _)| *e == checkout).unwrap().1;
    h.click(r.center());
    assert_eq!(h.app.board.filter.epic, EpicFilter::Epic(checkout));
    h.frame(vec![]);
    for (id, _) in &h.app.board.card_rects {
        let t = h.app.project.ticket(*id).unwrap();
        assert_eq!(h.app.project.epic_of(t), Some(checkout), "{}", t.title);
    }
    // a quick-added ticket joins the filtered epic
    h.app.board.filter.epic = EpicFilter::Any;
    h.frame(vec![]);
    // drag a card with no epic onto the Checkout epic
    let loose = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.kind.is_standard() && h.app.project.epic_of(t).is_none());
    let loose = match loose {
        Some(t) => t.id,
        None => {
            let id = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.title == "Push notifications").unwrap().id;
            id
        }
    };
    h.frame(vec![]);
    let from = card_center(&h, loose);
    let to = h.app.board.epic_rects.iter().find(|(e, _)| *e == checkout).unwrap().1.center();
    h.drag(from, to);
    assert_eq!(h.app.project.ticket(loose).unwrap().parent, Some(checkout));
    h.key(Key::Z, ctrl());
    assert_ne!(h.app.project.ticket(loose).unwrap().parent, Some(checkout), "undo");
}

#[test]
fn ticket_dialog_types_subtasks_and_links() {
    use projectfcg::board::{IssueType, LinkKind};
    let mut h = board_h();
    let ctx = h.ctx.clone();
    let s = h.app.board_sprint().unwrap();
    let story = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.title == "Order history").unwrap().id;
    let target = h.app.project.tickets.iter().find(|t| t.title == "Dark mode").unwrap().id;
    h.app.board.selected = Some(story);
    h.app.execute(Command::EditTicket, &ctx);
    h.frame(vec![]);
    {
        let d = h.app.dialogs.ticket.as_mut().expect("dialog");
        d.new_subtasks.push("Pagination".into());
        d.links.push(projectfcg::app::board::DlgLink { kind: LinkKind::Blocks, outward: false, other: target });
        d.labels = "backend, api, backend".into();
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.ticket.is_none(), "{:?}", h.app.dialogs.ticket.as_ref().and_then(|d| d.error.clone()));
    let p = &h.app.project;
    let sub = p.tickets.iter().find(|t| t.title == "Pagination").expect("sub-task created");
    assert_eq!((sub.kind, sub.parent, sub.sprint), (IssueType::Subtask, Some(story), Some(s)));
    assert_eq!(p.blockers(story), vec![target], "Dark mode blocks it now");
    assert_eq!(p.ticket(story).unwrap().labels, vec!["backend", "api"]);
    // one undo step for the whole dialog
    h.key(Key::Z, ctrl());
    assert!(h.app.project.tickets.iter().all(|t| t.title != "Pagination"));
    assert!(h.app.project.blockers(story).is_empty());
    // a new epic from the ribbon command
    h.app.execute(Command::NewEpic, &ctx);
    h.frame(vec![]);
    assert_eq!(h.app.dialogs.ticket.as_ref().unwrap().kind, IssueType::Epic);
    h.text("Growth");
    h.key(Key::Enter, NONE);
    let e = h.app.project.tickets.iter().find(|t| t.title == "Growth").expect("epic");
    assert_eq!((e.kind, e.sprint), (IssueType::Epic, None));
    assert_eq!(h.app.board.side, projectfcg::app::board::SidePanel::Epics);
}

#[test]
fn jira_import_dialog_and_export() {
    let dir = std::env::temp_dir().join(format!("pfcg_jira_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("export.csv");
    std::fs::write(&csv, "Summary,Issue key,Issue Type,Status,Sprint,Custom field (Story Points),Parent key\nPlatform,OPS-1,Epic,To Do,,,\nUpgrade DB,OPS-2,Story,In Progress,Ops Sprint 1,5,OPS-1\nRotate keys,OPS-3,Task,Done,Ops Sprint 1,2,OPS-1\n").unwrap();
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.finish_save_dialog(projectfcg::app::filebrowser::Mode::ImportJiraCsv, csv.clone());
    let d = h.app.dialogs.jira_import.as_ref().expect("options dialog");
    assert_eq!((d.issues, d.epics, d.sprints.clone()), (3, 1, vec!["Ops Sprint 1".to_string()]));
    h.frame(vec![]);
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.jira_import.is_none());
    assert_eq!(h.app.project.tickets.len(), 3);
    assert_eq!(h.app.project.sprint_rows().len(), 1, "the sprint was created");
    assert_eq!(h.app.view.main, MainView::Board);
    let report = h.app.dialogs.message.as_ref().map(|m| m.1.clone()).unwrap_or_default();
    assert!(report.contains("Imported 3 issues"), "{report}");
    h.key(Key::Escape, NONE);
    // export writes a CSV Jira reads
    let out = dir.join("board.csv");
    h.app.finish_save_dialog(projectfcg::app::filebrowser::Mode::ExportJiraCsv, out.clone());
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.lines().count() == 4 && text.contains("OPS-2"), "{text}");
    // undo removes the import (tickets and the new sprint)
    h.app.execute(Command::Undo, &ctx);
    assert!(h.app.project.tickets.is_empty() && h.app.project.tasks.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_xml_and_open_xml() {
    let dir = std::env::temp_dir().join(format!("pfcg_xml_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("release.xml");
    let mut h = H::new(projectfcg::app::sample::software_release());
    let finish = h.app.project.calc.finish;
    h.app.finish_save_dialog(projectfcg::app::filebrowser::Mode::ExportXml, path.clone());
    assert!(path.exists());
    assert!(!h.app.status_error, "{}", h.app.status);
    let mut h2 = H::blank();
    h2.app.open_path(path.clone());
    assert_eq!(h2.app.project.title, "Software Release 2.0");
    assert_eq!(h2.app.project.calc.finish, finish);
    assert!(h2.app.path.is_none(), "an imported XML isn't the document's file");
    assert!(h2.app.dialogs.message.is_none(), "no warnings for our own file");
    // a broken file reports an error and keeps the current project
    std::fs::write(dir.join("bad.xml"), "<Project><Tasks>").unwrap();
    h2.app.open_path(dir.join("bad.xml"));
    assert!(h2.app.dialogs.message.is_some());
    assert_eq!(h2.app.project.title, "Software Release 2.0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_subtask_from_a_card() {
    use projectfcg::board::IssueType;
    let mut h = board_h();
    let s = h.app.board_sprint().unwrap();
    let parent = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.title == "Order history").unwrap().id;
    h.app.open_new_ticket(IssueType::Subtask, Some(parent));
    h.frame(vec![]);
    h.text("Empty state");
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.ticket.is_none(), "{:?}", h.app.dialogs.ticket.as_ref().and_then(|d| d.error.clone()));
    let t = h.app.project.tickets.iter().find(|t| t.title == "Empty state").expect("created");
    assert_eq!((t.kind, t.parent, t.sprint), (IssueType::Subtask, Some(parent), Some(s)));
}

// ---------------------------------------------------------------- 0.7: board ↔ Gantt dependencies

#[test]
fn type_a_story_key_into_predecessors() {
    let mut h = H::new(projectfcg::app::sample::agile_release());
    let faq = h.app.project.tasks.iter().position(|t| t.name == "Payment FAQ & support macros").unwrap();
    let api = h.app.project.tickets.iter().find(|t| t.title == "Order history").unwrap().id;
    let key = h.app.project.ticket_key(api);
    let col = h.col(projectfcg::app::columns::ColumnId::Predecessors);
    h.goto(faq, col);
    h.type_and_enter(&format!("{key}, APP-27"));
    let t = &h.app.project.tasks[faq];
    assert_eq!(t.board_deps.len(), 2, "{}", h.app.status);
    assert!(h.app.project.predecessors_text(faq).contains(&key));
    // the Indicators tooltip source and the Gantt link paths know about it
    h.frame(vec![]);
    let uid = h.app.project.tasks[faq].uid;
    assert!(h.app.gantt_state.link_paths.iter().any(|l| l.succ_uid == uid && l.board == Some(api)), "dashed board link drawn");
    // Task Information lists board items as predecessor rows, and keeps them on OK
    let ctx = h.ctx.clone();
    h.app.select_index(faq);
    h.app.execute(Command::TaskInfo, &ctx);
    h.frame(vec![]);
    let rows: Vec<String> = h.app.dialogs.task_info.as_ref().unwrap().preds.iter().map(|r| r.id_text.clone()).collect();
    assert!(rows.contains(&key), "{rows:?}");
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.task_info.is_none());
    assert_eq!(h.app.project.tasks[faq].board_deps.len(), 2);
    // undo takes both away
    h.key(Key::Z, ctrl());
    h.key(Key::Z, ctrl());
    assert!(h.app.project.tasks[faq].board_deps.len() <= 1);
}

#[test]
fn ticket_dialog_waits_on_a_task() {
    let mut h = board_h();
    let ctx = h.ctx.clone();
    let s = h.app.board_sprint().unwrap();
    let story = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.title == "Order history").unwrap().id;
    let contract = h.app.project.tasks.iter().find(|t| t.name == "Payment provider contract").unwrap().uid;
    h.app.board.selected = Some(story);
    h.app.execute(Command::EditTicket, &ctx);
    h.frame(vec![]);
    h.app.dialogs.ticket.as_mut().unwrap().after_tasks.push(contract);
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.ticket.is_none());
    assert_eq!(h.app.project.ticket(story).unwrap().after_tasks, vec![contract]);
    assert_eq!(h.app.project.waiting_on_tasks(story), vec![contract]);
    assert!(!h.app.project.dependency_conflicts(story).is_empty(), "Sprint 3 starts before the contract finishes");
    h.frame(vec![]);
    h.frame(vec![]);
}

#[test]
fn swimlanes_complete_sprint_and_columns_dialogs() {
    use projectfcg::app::board::Swimlanes;
    let mut h = board_h();
    let n = h.app.board.card_rects.len();
    for l in [Swimlanes::Epic, Swimlanes::Assignee] {
        h.app.board.lanes = l;
        h.frame(vec![]);
        assert_eq!(h.app.board.card_rects.len(), n, "every card still drawn once with {l:?}");
    }
    // drag still changes the column in lanes mode
    let s = h.app.board_sprint().unwrap();
    let todo = h.app.project.tickets.iter().find(|t| t.sprint == Some(s) && t.column == 0 && t.kind.is_standard()).unwrap().id;
    let done_col = h.app.board.column_rects[3];
    let from = card_center(&h, todo);
    h.drag(from, pos2(done_col.center().x, done_col.bottom() - 30.0));
    assert_eq!(col_of(&h, todo).1, 3);
    // Complete Sprint 3 → unfinished go to Sprint 4
    h.app.open_sprint_dialog(projectfcg::app::sprint_dialogs::SprintAction::Complete);
    h.frame(vec![]);
    let s4 = h.app.dialogs.sprint.as_ref().unwrap().dest.expect("next sprint offered");
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.sprint.is_none());
    assert!(h.app.project.board.sprint_meta(s).unwrap().completed.is_some());
    assert!(h.app.project.tickets.iter().filter(|t| t.sprint == Some(s) && t.kind.is_standard()).all(|t| h.app.project.is_done(t)));
    assert_eq!(h.app.board_sprint(), Some(s4), "the board follows the carried-over work");
    // one undo step
    h.key(Key::Z, ctrl());
    assert!(h.app.project.board.sprint_meta(s).unwrap().completed.is_none());
    // Columns dialog: add a column before Done with a WIP limit
    h.app.open_columns_dialog();
    h.frame(vec![]);
    {
        let d = h.app.dialogs.columns.as_mut().unwrap();
        let at = d.rows.len() - 1;
        d.rows.insert(at, ("Ready to ship".into(), None, "2".into()));
    }
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.columns.is_none());
    assert_eq!(h.app.project.board.columns.len(), 5);
    assert_eq!(h.app.project.board.wip_limit(3), Some(2));
    h.frame(vec![]);
    assert_eq!(h.app.board.column_rects.len(), 5);
}

#[test]
fn recurring_and_inactive_from_the_ui() {
    let mut h = H::blank();
    let ctx = h.ctx.clone();
    h.app.execute(Command::InsertRecurring, &ctx);
    h.frame(vec![]);
    {
        let d = h.app.dialogs.recurring.as_mut().expect("dialog");
        d.weekdays = [true, false, true, false, false, false, false];
        d.start = "1/5/26".into();
        d.count = 4;
    }
    h.text("Planning meeting");
    h.frame(vec![]);
    h.key(Key::Enter, NONE);
    assert!(h.app.dialogs.recurring.is_none(), "{:?}", h.app.dialogs.recurring.as_ref().and_then(|d| d.error.clone()));
    let p = &h.app.project;
    assert_eq!(p.tasks.len(), 5);
    assert!(p.tasks[0].recurrence.is_some());
    assert_eq!(p.tasks[2].name, "Planning meeting 2");
    // Task Information on the series opens the pattern again
    h.app.select_index(0);
    h.app.execute(Command::TaskInfo, &ctx);
    assert!(h.app.dialogs.recurring.is_some() && h.app.dialogs.task_info.is_none());
    h.frame(vec![]);
    h.key(Key::Escape, NONE);
    // inactivate an occurrence, then undo
    h.app.select_index(3);
    h.app.execute(Command::ToggleActive, &ctx);
    assert!(!h.app.project.tasks[3].active);
    h.frame(vec![]);
    assert!(h.app.inactive_rows[3]);
    h.app.execute(Command::ToggleActive, &ctx);
    assert!(h.app.project.tasks[3].active);
}
