//! Search the plan (Ctrl+E): tasks, tickets, people and risks by name, ID,
//! key or text; going to and opening what's found.
use egui::{vec2, Event, Key, Modifiers, Pos2, RawInput, Rect};
use projectfcg::app::item_search::{search, Item, Scope};
use projectfcg::app::{sample, MainView, ProjectFcgApp};
use projectfcg::model::Project;

fn agile() -> Project {
    sample::agile_release()
}

fn first(p: &Project, q: &str) -> Option<Item> {
    search(p, q).first().map(|h| h.0)
}

#[test]
fn names_ids_keys_and_text_are_found() {
    let p = sample::software_release();
    let row = |name: &str| p.tasks.iter().position(|t| t.name == name).unwrap();
    assert_eq!(first(&p, "frontend"), Some(Item::Task(row("Frontend UI"))));
    assert_eq!(first(&p, "7"), Some(Item::Task(6)), "an ID is the task with that ID");
    assert_eq!(first(&p, "#7"), Some(Item::Task(6)));
    assert_eq!(first(&p, "db migration").is_none(), true, "every word must appear");
    assert_eq!(first(&p, "database migr"), Some(Item::Task(row("Database migration"))));
    // risks by key and by words in their statement
    assert_eq!(first(&p, "R-001"), Some(Item::Risk(1)));
    assert!(search(&p, "legacy").iter().any(|h| h.0 == Item::Risk(1)), "If/then text counts");
    // people, and their tasks too (assigned names count)
    let lena = p.resources.iter().find(|r| r.name == "Lena Park").unwrap().uid;
    let hits = search(&p, "lena");
    assert_eq!(hits.first().map(|h| h.0), Some(Item::Resource(lena)), "the person first");
    assert!(hits.iter().any(|h| h.0 == Item::Task(row("Frontend UI"))), "then what's assigned to her");
    // tickets by key, title, label and description
    let a = agile();
    let t = &a.tickets[0];
    let key = a.ticket_key(t.id);
    assert_eq!(first(&a, &key), Some(Item::Ticket(t.id)));
    assert_eq!(first(&a, &key.to_lowercase()), Some(Item::Ticket(t.id)));
    let word = t.title.split_whitespace().max_by_key(|w| w.len()).unwrap().to_lowercase();
    assert!(search(&a, &word).iter().any(|h| h.0 == Item::Ticket(t.id)), "{word}");
    assert!(search(&p, "").is_empty());
    assert!(search(&p, "zzzqqq").is_empty());
}

// ------------------------------------------------------------------ UI

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}

impl H {
    fn new(p: Project) -> Self {
        let mut app = ProjectFcgApp::new(p);
        app.prefs_path = None;
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
    fn key(&mut self, k: Key, m: Modifiers) {
        self.frame(vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
        self.frame(vec![Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m }]);
    }
    fn typ(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.into())]);
        self.frame(vec![]);
    }
    fn search(&mut self, q: &str) {
        self.frame(vec![Event::Key { key: Key::E, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }, Event::Text("e".into())]);
        self.frame(vec![]);
        assert!(self.app.item_search.open, "Ctrl+E opens the plan search");
        assert_eq!(self.app.item_search.query, "", "and types nothing");
        self.typ(q);
    }
}

#[test]
fn enter_goes_to_a_task_even_one_hidden_by_a_filter_or_collapsed() {
    let mut h = H::new(sample::software_release());
    let ctx = h.ctx.clone();
    let row = h.app.project.tasks.iter().position(|t| t.name == "Database migration").unwrap();
    let parent = h.app.project.parent_of(row).unwrap();
    h.app.project.tasks[parent].collapsed = true;
    h.app.execute(projectfcg::app::commands::Command::SetFilter(projectfcg::app::commands::Filter::Milestones), &ctx);
    h.app.execute(projectfcg::app::commands::Command::ShowView(MainView::Board), &ctx);
    h.search("database");
    h.key(Key::Enter, Modifiers::NONE);
    assert!(!h.app.item_search.open);
    assert_eq!(h.app.view.main, MainView::Gantt);
    assert_eq!(h.app.active_index(), Some(row));
    assert!(!h.app.project.tasks[parent].collapsed, "its summary opened");
    assert_eq!(h.app.filter, projectfcg::app::commands::Filter::AllTasks, "the filter that hid it went");
}

#[test]
fn shift_enter_opens_the_task() {
    let mut h = H::new(sample::software_release());
    h.search("backend api");
    h.key(Key::Enter, Modifiers::SHIFT);
    assert!(h.app.dialogs.task_info.is_some(), "Task Information");
}

#[test]
fn tickets_open_on_the_board_in_their_sprint() {
    let p = agile();
    // a ticket in a sprint other than the one the board opens on
    let t = p.tickets.iter().filter(|t| t.sprint.is_some() && t.kind != projectfcg::board::IssueType::Epic).last().unwrap().clone();
    let key = p.ticket_key(t.id);
    let mut h = H::new(p);
    h.search(&key);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.view.main, MainView::Board);
    assert_eq!(h.app.board.sprint, t.sprint);
    assert_eq!(h.app.board.selected, Some(t.id));
    // a backlog ticket: the Backlog panel
    let b = h.app.project.tickets.iter().find(|t| t.sprint.is_none() && t.kind != projectfcg::board::IssueType::Epic).cloned();
    if let Some(b) = b {
        let key = h.app.project.ticket_key(b.id);
        h.search(&key);
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(h.app.board.selected, Some(b.id));
        assert!(h.app.board.backlog_open);
    }
}

#[test]
fn people_and_risks_go_to_their_views() {
    let mut h = H::new(sample::software_release());
    h.search("marco");
    h.key(Key::Enter, Modifiers::NONE);
    let marco = h.app.project.resources.iter().find(|r| r.name == "Marco Ruiz").unwrap().uid;
    assert_eq!(h.app.view.main, MainView::ResourceSheet);
    assert_eq!(h.app.sheet.active, Some(marco));
    h.search("R-004");
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.view.main, MainView::Risks);
    assert_eq!(h.app.risk_view.selected, Some(4));
}

#[test]
fn tab_narrows_to_one_kind_and_esc_closes() {
    let mut h = H::new(sample::software_release());
    h.search("a");
    assert_eq!(h.app.item_search.scope, Scope::All);
    h.key(Key::Tab, Modifiers::NONE);
    assert_eq!(h.app.item_search.scope, Scope::Tasks);
    assert!(h.app.item_search.open && h.ctx.memory(|m| m.has_focus(projectfcg::app::item_search::box_id())), "still typing here");
    h.key(Key::Tab, Modifiers::SHIFT);
    assert_eq!(h.app.item_search.scope, Scope::All);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.app.item_search.sel, 1);
    let n = h.app.project.tasks.len();
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.app.item_search.open);
    assert_eq!(h.app.project.tasks.len(), n, "nothing typed reached the table");
}
