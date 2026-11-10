//! The system diagram (experimental): elements, interfaces, nesting,
//! measures, the work rolled up into each part, repair of damaged files,
//! and the view (gated by the Options switch) driven with the mouse and
//! keyboard. Also AutoSave.
use chrono::NaiveDate;
use egui::{vec2, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use projectfcg::app::commands::Command;
use projectfcg::app::system_view::Sel;
use projectfcg::app::{sample, MainView, ProjectFcgApp};
use projectfcg::model::Project;
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;
use projectfcg::system::{Element, ElementKind as K, Health, InterfaceKind, Measure, MeasureStatus, SystemModel, WorkLink};

fn el(kind: K, pos: [f32; 2], size: [f32; 2]) -> Element {
    Element { kind, pos, size, ..Default::default() }
}

#[test]
fn elements_and_interfaces() {
    let mut s = SystemModel::default();
    let a = s.add_element(Element { kind: K::Component, ..Default::default() });
    let b = s.add_element(Element { kind: K::Component, ..Default::default() });
    assert_ne!(a, b);
    assert_eq!(s.element(a).unwrap().name, "Component 1", "named by kind");
    assert_eq!(s.element(b).unwrap().name, "Component 2");
    assert_eq!(s.element(a).unwrap().size, K::Component.default_size());
    let f = s.connect(a, b, InterfaceKind::Power).unwrap();
    assert!(s.connect(a, a, InterfaceKind::Data).is_err(), "not to itself");
    assert!(s.connect(a, 999, InterfaceKind::Data).is_err());
    assert_ne!(f, a, "one id space");
    s.delete_element(b);
    assert!(s.interfaces.is_empty(), "its interfaces go with it");
    assert!(s.element(b).is_none());
}

#[test]
fn elements_nest_by_where_they_sit() {
    let mut s = SystemModel::default();
    let sys = s.add_element(el(K::System, [0.0, 0.0], [600.0, 400.0]));
    let sub = s.add_element(el(K::Subsystem, [20.0, 40.0], [300.0, 200.0]));
    let part = s.add_element(el(K::Component, [40.0, 80.0], [100.0, 50.0]));
    let out = s.add_element(el(K::External, [700.0, 0.0], [100.0, 50.0]));
    // the smallest container that holds it
    assert_eq!(s.element(sub).unwrap().parent, Some(sys));
    assert_eq!(s.element(part).unwrap().parent, Some(sub));
    assert_eq!(s.element(out).unwrap().parent, None);
    assert_eq!(s.descendants(sys).len(), 2);
    // containers are drawn first (behind what they hold)
    let order: Vec<u32> = s.elements.iter().map(|e| e.id).collect();
    assert!(order.iter().position(|x| *x == sys) < order.iter().position(|x| *x == part));
    // moving a container moves what's inside
    s.move_by(sys, [10.0, 5.0]);
    assert_eq!(s.element(part).unwrap().pos, [50.0, 85.0]);
    assert_eq!(s.element(out).unwrap().pos, [700.0, 0.0]);
    // dragged out, it no longer belongs
    s.move_by(part, [1000.0, 0.0]);
    s.reparent(part);
    assert_eq!(s.element(part).unwrap().parent, None);
    // a container is never inside its own contents
    s.move_by(part, [-1000.0, 0.0]);
    s.reparent(part);
    s.reparent(sub);
    assert_eq!(s.element(sub).unwrap().parent, Some(sys));
    // deleting a container lifts its contents to its parent
    s.delete_element(sub);
    assert_eq!(s.element(part).unwrap().parent, Some(sys));
}

#[test]
fn measures() {
    let m = |current, target, threshold, lower| Measure { current, target, threshold, lower_is_better: lower, ..Default::default() };
    // range: more is better
    assert_eq!(m(Some(120.0), Some(100.0), Some(80.0), false).status(), MeasureStatus::Met);
    assert_eq!(m(Some(90.0), Some(100.0), Some(80.0), false).status(), MeasureStatus::AtRisk);
    assert_eq!(m(Some(70.0), Some(100.0), Some(80.0), false).status(), MeasureStatus::Breach);
    // weight: less is better
    assert_eq!(m(Some(9.0), Some(10.0), Some(12.0), true).status(), MeasureStatus::Met);
    assert_eq!(m(Some(11.0), Some(10.0), Some(12.0), true).status(), MeasureStatus::AtRisk);
    assert_eq!(m(Some(13.0), Some(10.0), Some(12.0), true).status(), MeasureStatus::Breach);
    // partial information
    assert_eq!(m(None, Some(10.0), Some(12.0), true).status(), MeasureStatus::Unknown);
    assert_eq!(m(Some(11.0), Some(10.0), None, true).status(), MeasureStatus::AtRisk);
    assert_eq!(m(Some(11.0), None, Some(12.0), true).status(), MeasureStatus::Met);
    assert_eq!(m(Some(13.0), None, Some(12.0), true).status(), MeasureStatus::Breach);
    assert_eq!(m(Some(1.0), None, None, true).status(), MeasureStatus::Unknown);
}

/// Two 10-day tasks under a summary, baselined, and an element for each.
fn plan() -> (Project, u32, u32, u32) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let s = p.append_task("Phase");
    let a = p.append_task("A");
    let b = p.append_task("B");
    p.set_field_text(a, Field::Duration, "10d").unwrap();
    p.set_field_text(b, Field::Duration, "30d").unwrap();
    p.indent(&[a, b]).unwrap();
    schedule(&mut p);
    p.set_baseline(None);
    let (ua, ub, us) = (p.tasks[a].uid, p.tasks[b].uid, p.tasks[s].uid);
    let sys = p.system.add_element(el(K::System, [0.0, 0.0], [600.0, 400.0]));
    let ea = p.system.add_element(Element { work: vec![WorkLink::Task(ua)], ..el(K::Component, [20.0, 40.0], [100.0, 50.0]) });
    let eb = p.system.add_element(Element { work: vec![WorkLink::Task(ub), WorkLink::Task(us)], ..el(K::Component, [200.0, 40.0], [100.0, 50.0]) });
    schedule(&mut p);
    let _ = (ea, eb);
    (p, sys, ea, eb)
}

#[test]
fn work_rolls_up() {
    let (mut p, sys, ea, eb) = plan();
    let (a, b) = (1, 2);
    p.set_percent_complete(a, 100.0).unwrap();
    p.set_percent_complete(b, 10.0).unwrap();
    schedule(&mut p);
    let r = p.system_rollup(ea);
    assert_eq!((r.tasks, r.percent_complete), (1, Some(100.0)));
    assert_eq!(r.health(), Health::Good);
    // a summary stands for its subtasks, each counted once
    let r = p.system_rollup(eb);
    assert_eq!(r.tasks, 2);
    // the system holds both: weighted by duration (10 days done, 3 of 30)
    let r = p.system_rollup(sys);
    assert_eq!(r.tasks, 2);
    assert!((r.percent_complete.unwrap() - (10.0 * 100.0 + 30.0 * 10.0) / 40.0).abs() < 1e-6, "{r:?}");
    // B slips past its baseline: late, so the parts holding it need watching
    p.set_field_text(b, Field::Duration, "35d").unwrap();
    schedule(&mut p);
    let r = p.system_rollup(eb);
    assert_eq!(r.late, 1);
    assert_eq!(r.health(), Health::Trouble, "1 of 2 tasks late");
    assert_eq!(p.system_rollup(ea).health(), Health::Good);
    // a measure that breaches its threshold is trouble whatever the schedule
    p.system.element_mut(ea).unwrap().measures.push(Measure { current: Some(5.0), threshold: Some(4.0), lower_is_better: true, ..Default::default() });
    assert_eq!(p.system_rollup(ea).health(), Health::Trouble);
    assert_eq!(p.system_rollup(sys).worst_measure, MeasureStatus::Breach, "rolled up");
    // the column
    assert_eq!(projectfcg::app::columns::cell_text(&p, a, projectfcg::app::columns::ColumnId::SystemElements), "Component 1");
    // nothing tied to it: nothing to judge
    let lone = p.system.add_element(el(K::Function, [900.0, 0.0], [100.0, 50.0]));
    assert_eq!(p.system_rollup(lone).health(), Health::Unknown);
}

#[test]
fn tickets_and_risks_roll_up() {
    let mut p = sample::software_release();
    // the sample's diagram: the database's high risk makes it trouble,
    // and the system holding it too
    let id = |p: &Project, n: &str| p.system.elements.iter().find(|e| e.name == n).unwrap().id;
    let db = id(&p, "Database");
    let r = p.system_rollup(db);
    assert_eq!(r.risks, 1);
    assert_eq!(r.worst_risk, Some(projectfcg::risks::RiskLevel::High));
    assert_eq!(r.health(), Health::Trouble);
    assert_eq!(p.system_rollup(id(&p, "Release 2.0")).health(), Health::Trouble);
    assert_eq!(p.system_rollup(id(&p, "Web app")).worst_measure, MeasureStatus::AtRisk, "page load is below target");
    // tickets: done / total by points
    let t1 = p.add_ticket("Checkout page", None).unwrap();
    let t2 = p.add_ticket("Receipts", None).unwrap();
    for (t, pts) in [(t1, 3.0), (t2, 5.0)] {
        let k = p.ticket_index(t).unwrap();
        p.tickets[k].points = Some(pts);
    }
    let k = p.ticket_index(t1).unwrap();
    p.tickets[k].column = p.board.done_column();
    let web = id(&p, "Web app");
    p.system.element_mut(web).unwrap().work = vec![WorkLink::Ticket(t1), WorkLink::Ticket(t2)];
    let r = p.system_rollup(web);
    assert_eq!((r.tickets, r.tickets_done, r.points, r.points_done), (2, 1, 8.0, 3.0));
    assert_eq!(r.progress(), Some(3.0 / 8.0 * 100.0), "no tasks: progress by points");
    // and a ticket's elements by name
    assert_eq!(p.system_names_of(WorkLink::Ticket(t1)), vec!["Web app".to_string()]);
}

#[test]
fn damaged_files_are_repaired() {
    let (mut p, sys, ea, eb) = plan();
    let s = &mut p.system;
    s.element_mut(ea).unwrap().work.push(WorkLink::Task(9999));
    s.element_mut(ea).unwrap().work.push(WorkLink::Risk(77));
    s.element_mut(eb).unwrap().id = ea; // a duplicate id
    s.element_mut(sys).unwrap().parent = Some(sys); // its own parent
    s.elements[0].size = [f32::NAN, -5.0];
    s.interfaces.push(projectfcg::system::Interface { id: 50, from: ea, to: 4242, ..Default::default() });
    let fixed = p.repair_system();
    assert!(fixed >= 6, "{fixed}");
    let s = &p.system;
    let mut ids: Vec<u32> = s.elements.iter().map(|e| e.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), s.elements.len(), "ids unique");
    assert!(s.interfaces.is_empty(), "the interface to nowhere");
    assert!(s.elements.iter().all(|e| e.size[0] >= 20.0 && e.size[1] >= 20.0));
    assert!(s.elements.iter().all(|e| e.parent != Some(e.id)));
    assert_eq!(s.element(ea).unwrap().work, vec![WorkLink::Task(p.tasks[1].uid)]);
    // saved and opened: the same diagram; an older file has none
    let json = projectfcg::io::to_json(&p).unwrap();
    assert_eq!(projectfcg::io::from_json(&json).unwrap().system, p.system);
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["project"].as_object_mut().unwrap().remove("system");
    assert!(projectfcg::io::from_json(&serde_json::to_string(&v).unwrap()).unwrap().system.elements.is_empty());
}

// ------------------------------------------------------------------ UI

const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}

impl H {
    fn new(enabled: bool) -> Self {
        let mut app = ProjectFcgApp::new(sample::software_release());
        app.prefs_path = None;
        app.prefs.experimental.system_diagram = enabled;
        let mut h = H { ctx: egui::Context::default(), app, t: 0.0 };
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), time: Some(self.t), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
    }
    fn show(&mut self) {
        let ctx = self.ctx.clone();
        self.app.execute(Command::ShowView(MainView::System), &ctx);
        self.frame(vec![]);
        self.frame(vec![]);
    }
    fn button(&mut self, p: Pos2, pressed: bool, m: Modifiers) {
        self.frame(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: m }]);
    }
    fn click(&mut self, p: Pos2) {
        self.button(p, true, Modifiers::NONE);
        self.button(p, false, Modifiers::NONE);
    }
    fn double_click(&mut self, p: Pos2) {
        self.click(p);
        self.click(p);
        self.frame(vec![]);
    }
    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.button(a, true, Modifiers::NONE);
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            self.frame(vec![Event::PointerMoved(a + (b - a) * t)]);
        }
        self.button(b, false, Modifiers::NONE);
        self.frame(vec![]);
    }
    fn key(&mut self, k: Key, m: Modifiers) {
        self.frame(vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
        self.frame(vec![Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m }]);
    }
    /// An element's centre on screen.
    fn at(&self, id: u32) -> Pos2 {
        let st = &self.app.system_view;
        let e = self.app.project.system.element(id).unwrap();
        st.canvas.min + st.offset + vec2(e.pos[0] + e.size[0] / 2.0, e.pos[1] + e.size[1] / 2.0) * st.zoom
    }
    /// A spot on the canvas with nothing on it.
    fn empty(&self) -> Pos2 {
        let c = self.app.system_view.canvas;
        let st = &self.app.system_view;
        let mut p = c.left_bottom() + vec2(30.0, -30.0);
        for _ in 0..40 {
            let w = (p - c.min - st.offset) / st.zoom;
            let hit = self.app.project.system.elements.iter().any(|e| w.x >= e.pos[0] - 10.0 && w.y >= e.pos[1] - 10.0 && w.x <= e.pos[0] + e.size[0] + 10.0 && w.y <= e.pos[1] + e.size[1] + 10.0);
            if !hit {
                return p;
            }
            p.x += 40.0;
        }
        p
    }
    fn id(&self, name: &str) -> u32 {
        self.app.project.system.elements.iter().find(|e| e.name == name).unwrap().id
    }
}

#[test]
fn off_until_turned_on() {
    let mut h = H::new(false);
    h.show();
    assert_eq!(h.app.view.main, MainView::Gantt, "experimental: off by default");
    assert!(h.app.status.contains("Options"), "{}", h.app.status);
    // Options ▸ Experimental features
    let ctx = h.ctx.clone();
    h.app.execute(Command::Options, &ctx);
    h.app.dialogs.options.as_mut().unwrap().system_diagram = true;
    projectfcg::app::dialogs::apply_options(&mut h.app);
    assert!(h.app.prefs.experimental.system_diagram);
    h.show();
    assert_eq!(h.app.view.main, MainView::System);
    // and off again: back to the Gantt chart; the plan's diagram is kept
    h.app.execute(Command::Options, &ctx);
    h.app.dialogs.options.as_mut().unwrap().system_diagram = false;
    projectfcg::app::dialogs::apply_options(&mut h.app);
    assert_eq!(h.app.view.main, MainView::Gantt);
    assert!(!h.app.project.system.elements.is_empty());
}

#[test]
fn add_name_move_connect_delete_with_the_mouse() {
    let mut h = H::new(true);
    h.show();
    let n = h.app.project.system.elements.len();
    // double-click the background: a new element, selected, its name ready
    let p = h.empty();
    h.double_click(p);
    assert_eq!(h.app.project.system.elements.len(), n + 1);
    let Sel::Element(new) = h.app.system_view.sel else { panic!("{:?}", h.app.system_view.sel) };
    h.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: CTRL }]);
    h.frame(vec![Event::Text("Telemetry".into())]);
    h.frame(vec![]);
    // clicking the canvas saves the name (one undo step)
    let gen = h.app.history.generation();
    h.click(h.empty());
    assert_eq!(h.app.project.system.element(new).unwrap().name, "Telemetry");
    assert_ne!(h.app.history.generation(), gen);
    // drag it into the system: it belongs there now
    let sys = h.id("Release 2.0");
    let before = h.app.project.system.element(new).unwrap().pos;
    let target = h.at(h.id("Release & docs")) + vec2(0.0, 0.0);
    let _ = target;
    let sys_rect_centre = h.at(sys);
    let from = h.at(new);
    h.drag(from, sys_rect_centre + vec2(250.0, 60.0) * h.app.system_view.zoom);
    let e = h.app.project.system.element(new).unwrap().clone();
    assert_ne!(e.pos, before, "moved");
    assert_eq!(e.parent, Some(sys), "dropped inside the system");
    // connect it to the database with the handle
    h.click(h.at(new));
    let st = &h.app.system_view;
    let handle = st.canvas.min + st.offset + vec2(e.pos[0] + e.size[0], e.pos[1] + e.size[1] / 2.0) * st.zoom;
    let nif = h.app.project.system.interfaces.len();
    let db = h.at(h.id("Database"));
    h.drag(handle, db);
    assert_eq!(h.app.project.system.interfaces.len(), nif + 1, "connected");
    let f = h.app.project.system.interfaces.last().unwrap();
    assert_eq!((f.from, f.to), (new, h.id("Database")));
    assert!(matches!(h.app.system_view.sel, Sel::Interface(_)));
    // Delete removes the element and its interface; Ctrl+Z brings them back
    h.click(h.at(new));
    h.key(Key::Delete, Modifiers::NONE);
    assert!(h.app.project.system.element(new).is_none());
    assert_eq!(h.app.project.system.interfaces.len(), nif);
    assert!(h.app.project.tasks.len() >= 17, "no task was deleted");
    h.key(Key::Z, CTRL);
    assert!(h.app.project.system.element(new).is_some());
    assert_eq!(h.app.project.system.interfaces.len(), nif + 1);
}

#[test]
fn panel_edits_and_ties_work() {
    let mut h = H::new(true);
    h.show();
    let api = h.id("API service");
    h.click(h.at(api));
    assert_eq!(h.app.system_view.sel, Sel::Element(api));
    // tie the task selected in the Gantt: the panel offers it
    let row = h.app.project.tasks.iter().position(|t| t.name == "Integration testing").unwrap();
    h.app.select_index(row);
    h.frame(vec![]);
    let gen = h.app.history.generation();
    let ctx = h.ctx.clone();
    let _ = ctx;
    // the button is in the panel; press it through the same code path
    let uid = h.app.project.tasks[row].uid;
    if let Some((_, projectfcg::app::system_view::Draft::Element(e))) = h.app.system_view.draft_mut() {
        e.work.push(WorkLink::Task(uid));
    }
    h.frame(vec![]);
    assert_ne!(h.app.history.generation(), gen, "saved as an edit");
    assert!(h.app.project.system.element(api).unwrap().work.contains(&WorkLink::Task(uid)));
    assert_eq!(h.app.project.system_names_of(WorkLink::Task(uid)), vec!["API service".to_string()]);
    h.key(Key::Z, CTRL);
    assert!(!h.app.project.system.element(api).unwrap().work.contains(&WorkLink::Task(uid)), "one undo step");
    // every colouring and kind draws
    for c in projectfcg::app::system_view::ColorBy::ALL {
        h.app.system_view.color_by = c;
        h.frame(vec![]);
    }
    for k in K::ALL {
        h.app.system_view.new_kind = k;
        h.app.add_system_element_in_view();
        h.frame(vec![]);
    }
    // an empty diagram offers a starter layout
    let mut h = H::new(true);
    h.app.edit_project("Clear", |p| {
        p.system = Default::default();
        Ok(())
    });
    h.show();
    h.app.system_starter();
    h.frame(vec![]);
    assert_eq!(h.app.project.system.elements.len(), 5);
    assert_eq!(h.app.project.system.interfaces.len(), 3);
}

// ------------------------------------------------------------------ AutoSave

#[test]
fn autosave_saves_a_plan_with_a_file() {
    let dir = std::env::temp_dir().join(format!("projectfcg-autosave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.pfcg");
    let mut h = H::new(false);
    h.app.autosave.delay = std::time::Duration::ZERO;
    h.app.prefs.autosave = true;
    // no file yet: nothing happens
    h.app.edit_project("Rename", |p| {
        p.tasks[1].name = "First".into();
        Ok(())
    });
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(h.app.dirty && !path.exists());
    // saved once: from then on every change is saved
    let ctx = h.ctx.clone();
    h.app.path = Some(path.clone());
    h.app.execute(Command::Save, &ctx);
    assert!(!h.app.dirty);
    h.app.edit_project("Rename", |p| {
        p.tasks[1].name = "Second".into();
        Ok(())
    });
    assert!(h.app.dirty);
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(!h.app.dirty, "{}", h.app.status);
    assert!(h.app.status.starts_with("AutoSaved plan.pfcg"), "{}", h.app.status);
    assert_eq!(projectfcg::io::load(&path).unwrap().tasks[1].name, "Second");
    // off: changes wait for Save
    h.app.execute(Command::ToggleAutoSave, &ctx);
    assert!(!h.app.prefs.autosave);
    h.app.edit_project("Rename", |p| {
        p.tasks[1].name = "Third".into();
        Ok(())
    });
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(h.app.dirty);
    assert_eq!(projectfcg::io::load(&path).unwrap().tasks[1].name, "Second");
    // a file that can't be written: AutoSave pauses (no error every few
    // seconds) until the next Save
    h.app.execute(Command::ToggleAutoSave, &ctx);
    h.app.path = Some(dir.join("no such folder").join("plan.pfcg"));
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(h.app.autosave.paused && h.app.dirty, "{}", h.app.status);
    assert!(h.app.status.starts_with("AutoSave paused"), "{}", h.app.status);
    std::fs::remove_dir_all(&dir).ok();
}
