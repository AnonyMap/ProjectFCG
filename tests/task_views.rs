//! Network Diagram and Calendar views.
use chrono::NaiveDate;
use egui::{pos2, vec2, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use projectfcg::app::calendar_view::{add_months, lanes, week_span, weeks_of};
use projectfcg::app::network::{layout, BOX_H, BOX_W};
use projectfcg::app::{MainView, ProjectFcgApp, RowRef};
use projectfcg::model::Project;
use projectfcg::ops::Field;

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

/// Phase (Design → Build → Test, and Docs after Design).
fn plan() -> Project {
    let mut p = Project::new(d(1, 5));
    let ph = p.append_task("Phase");
    for (n, dur) in [("Design", "2d"), ("Build", "3d"), ("Test", "2d"), ("Docs", "1d")] {
        let r = p.append_task(n);
        p.tasks[r].outline_level = 2;
        p.set_field_text(r, Field::Duration, dur).unwrap();
    }
    projectfcg::schedule::refresh_structure(&mut p);
    p.set_field_text(2, Field::Predecessors, "2").unwrap();
    p.set_field_text(3, Field::Predecessors, "3").unwrap();
    p.set_field_text(4, Field::Predecessors, "2").unwrap();
    let _ = ph;
    projectfcg::schedule::schedule(&mut p);
    p
}

#[test]
fn network_columns_follow_the_links() {
    let p = plan();
    let rows: Vec<usize> = (0..p.tasks.len()).collect();
    let l = layout(&p, &rows);
    let col = |r: usize| ((l.pos[&r].x - 24.0) / (BOX_W + 64.0)).round() as i32;
    assert_eq!(col(0), 0, "the summary first");
    assert_eq!(col(1), 1, "then its first subtask");
    assert_eq!((col(2), col(3)), (2, 3), "Build after Design, Test after Build");
    assert_eq!(col(4), 2, "Docs right after Design, beside Build");
    assert_ne!(l.pos[&2].y, l.pos[&4].y, "two boxes in a column don't overlap");
    assert_eq!(l.links.len(), 3);
    assert!(l.links.iter().any(|&(a, b, crit)| (a, b) == (2, 3) && crit), "critical links are marked");
    // a loop doesn't hang the layout
    let mut q = plan();
    let u3 = q.tasks[3].uid;
    q.tasks[1].predecessors.push(projectfcg::model::Link { pred: u3, kind: projectfcg::model::LinkType::FS, lag: Default::default() });
    let _ = layout(&q, &rows);
}

#[test]
fn calendar_weeks_spans_and_lanes() {
    let w = weeks_of(d(3, 1)); // March 2026 starts on a Sunday
    assert_eq!(w.first(), Some(&d(3, 1)));
    assert_eq!(w.len(), 5);
    assert_eq!(weeks_of(d(2, 1)).len(), 4, "Feb 2026: exactly four weeks");
    assert_eq!(add_months(d(12, 1), 1), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    assert_eq!(add_months(d(1, 1), -1), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
    // a task Thu–next Tue shows as Thu–Sat, then Sun–Tue
    assert_eq!(week_span(d(3, 5), d(3, 10), d(3, 1)), Some((4, 6)));
    assert_eq!(week_span(d(3, 5), d(3, 10), d(3, 8)), Some((0, 2)));
    assert_eq!(week_span(d(3, 5), d(3, 10), d(3, 15)), None);
    // overlapping bars take separate lanes; a later one reuses a free lane
    let l = lanes(vec![(1, 0, 2), (2, 1, 3), (3, 3, 5)]);
    let lane = |r| l.iter().find(|x| x.0 == r).unwrap().3;
    assert_eq!((lane(1), lane(2), lane(3)), (0, 1, 0));
}

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}

impl H {
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), time: Some(self.t), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear(); // epaint panics on dropped deltas in debug builds
    }
    fn click(&mut self, at: Pos2, double: bool) {
        self.frame(vec![Event::PointerMoved(at)]);
        let n = if double { 2 } else { 1 };
        for _ in 0..n {
            self.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
            self.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
        }
        self.frame(vec![]);
    }
}

#[test]
fn network_diagram_selects_and_opens_task_information() {
    let mut h = H { ctx: egui::Context::default(), app: ProjectFcgApp::new(plan()), t: 0.0 };
    h.app.prefs_path = None;
    h.app.view.main = MainView::NetworkDiagram;
    h.frame(vec![]);
    h.frame(vec![]);
    let view = h.app.last_view_rect;
    let at = h.app.network.layout.pos[&3];
    let center = view.min + (at.to_vec2() + vec2(BOX_W, BOX_H) / 2.0) * h.app.network.zoom - h.app.network.pan;
    h.click(center, false);
    assert_eq!(h.app.active_index(), Some(3), "clicked Test");
    assert!(h.app.row_selected, "a whole task is selected: Delete deletes it");
    h.click(center, true);
    assert!(h.app.dialogs.task_info.is_some());
    h.app.dialogs.task_info = None;
    // the layout follows edits
    let before = h.app.network.layout.pos[&4];
    h.app.edit_project("link", |p| p.set_field_text(4, Field::Predecessors, "4"));
    h.frame(vec![]);
    assert!(h.app.network.layout.pos[&4].x > before.x, "Docs moved after Test");
    // zoom
    h.app.execute(projectfcg::app::commands::Command::ZoomOut, &h.ctx.clone());
    assert!(h.app.network.zoom < 1.0);
}

#[test]
fn calendar_shows_the_month_and_selects_tasks() {
    let mut h = H { ctx: egui::Context::default(), app: ProjectFcgApp::new(plan()), t: 0.0 };
    h.app.prefs_path = None;
    h.app.view.main = MainView::Calendar;
    h.app.select_single(RowRef::Task(h.app.project.tasks[3].uid)); // Test: Jan 12–13
    h.frame(vec![]);
    h.frame(vec![]);
    assert_eq!(h.app.calendar.month, Some(d(1, 1)), "the selected task's month");
    // click Build's bar: Wed Jan 7 is in the 2nd week row (Jan 4–10), Wednesday column
    let v = h.app.last_view_rect;
    let grid_top = v.top() + 34.0 + 22.0;
    let wh = (v.bottom() - grid_top) / weeks_of(d(1, 1)).len() as f32;
    let cw = v.width() / 7.0;
    let at = pos2(v.left() + 3.5 * cw, grid_top + wh + 20.0 + 9.0);
    h.select_none();
    h.click(at, false);
    assert!(h.app.active_index().is_some(), "a bar was hit");
    let name = h.app.project.tasks[h.app.active_index().unwrap()].name.clone();
    assert!(["Design", "Build", "Docs"].contains(&name.as_str()), "{name}");
    h.click(at, true);
    assert!(h.app.dialogs.task_info.is_some());
}

impl H {
    fn select_none(&mut self) {
        self.app.select_single(RowRef::Blank);
        self.frame(vec![]);
    }
}
