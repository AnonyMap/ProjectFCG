//! Search (Alt+Q): what people type finds the right action; every ribbon
//! button can be found; every action runs; the box works from the keyboard.
use egui::{vec2, Event, Key, Modifiers, Pos2, RawInput, Rect};
use projectfcg::app::tell_me::{catalog, search, tokens};
use projectfcg::app::{sample, ProjectFcgApp};

fn top(q: &str, n: usize) -> Vec<&'static str> {
    let cat = catalog();
    search(&cat, q, &[]).into_iter().take(n).map(|i| cat[i].id).collect()
}

#[track_caller]
fn first(q: &str, id: &str) {
    let got = top(q, 3);
    assert_eq!(got.first().copied(), Some(id), "{q:?} → {got:?}");
}

#[track_caller]
fn among(q: &str, id: &str, n: usize) {
    let got = top(q, n);
    assert!(got.contains(&id), "{q:?} → {got:?} (wanted {id} in the first {n})");
}

#[test]
fn what_people_type_finds_the_action() {
    // the names themselves
    first("insert task", "insert_task");
    first("link", "link");
    first("set baseline", "set_baseline");
    first("level all", "level_all");
    first("risk register", "view_risks");
    first("network diagram", "view_network");
    first("print", "print");
    // other words for the same thing
    first("remove task", "delete_task");
    first("delete", "delete_task");
    first("add task", "insert_task");
    first("dependency", "link");
    first("add dependencies", "link");
    first("holiday", "working_time");
    first("vacation", "working_time");
    among("wbs", "outline_number", 2);
    among("overbooked", "level_all", 3);
    among("pdf", "save_pdf", 2);
    among("excel", "export_csv", 3);
    first("jira import", "import_jira");
    among("kanban", "view_board", 2);
    among("night mode", "dark_mode", 2);
    among("screenshot", "copy_picture", 1);
    first("why does this task start", "inspect");
    among("story", "new_ticket", 2);
    among("evm", "table_ev", 1);
    // sentences: the filler words don't count
    first("how do I add a milestone", "insert_milestone");
    first("I want to make a task recurring", "insert_recurring");
    // typos and plurals
    first("baselne", "set_baseline");
    first("milstone", "insert_milestone");
    among("critcal path", "show_cp", 3);
    among("gant", "view_gantt", 2);
    among("resources sheet", "view_res_sheet", 1);
    among("calender", "view_calendar", 2);
    // nothing for nonsense; every word must match something
    assert!(top("xyzzy", 5).is_empty());
    assert!(top("link xyzzy", 5).is_empty());
    assert_eq!(tokens("how do I add a task"), vec!["add", "task"]);
}

#[test]
fn recently_used_actions_rank_higher() {
    let cat = catalog();
    let plain: Vec<&str> = search(&cat, "level", &[]).into_iter().map(|i| cat[i].id).collect();
    let pos = |v: &[&str], id: &str| v.iter().position(|x| *x == id).unwrap();
    let recent = vec!["clear_leveling".to_string()];
    let boosted: Vec<&str> = search(&cat, "level", &recent).into_iter().map(|i| cat[i].id).collect();
    assert!(pos(&boosted, "clear_leveling") < pos(&plain, "clear_leveling") || pos(&plain, "clear_leveling") == 0, "{plain:?} / {boosted:?}");
}

#[test]
fn every_ribbon_button_can_be_found() {
    let cat = catalog();
    let src = include_str!("../src/app/ribbon.rs");
    let mut missing = vec![];
    let mut seen = 0;
    for pat in ["big(ui, \"", "small(ui, \"", "check(ui, "] {
        for (i, _) in src.match_indices(pat) {
            let rest = &src[i + pat.len()..];
            // big/small: glyph, caption; check: state, caption
            // check(ui, state, "Caption", …); big/small(ui, "glyph", "Caption", …)
            // (a caption computed at run time isn't a literal: skip it)
            let caption = if pat.starts_with("check") {
                rest.split('"').nth(1)
            } else {
                let after_glyph = rest.split_once("\", ").map(|x| x.1).unwrap_or("");
                if after_glyph.starts_with('"') { rest.split('"').nth(2) } else { None }
            };
            let Some(caption) = caption else { continue };
            let caption = caption.replace("\\n", " ").replace('…', "");
            if caption.is_empty() || (cfg!(target_arch = "wasm32") && caption == "Exit") {
                continue;
            }
            seen += 1;
            let c = caption.trim().to_lowercase();
            let hits = search(&cat, &caption, &[]);
            // listed under that name, or the best match is plainly it
            // ("Import CSV" on the Board is Import Jira CSV)
            let shares = |l: &str| c.split_whitespace().filter(|w| l.to_lowercase().split_whitespace().any(|x| x == *w)).count() >= c.split_whitespace().count().min(2);
            let found = cat.iter().any(|a| a.path.to_lowercase().ends_with(&c) || a.label.to_lowercase().contains(&c) || a.path.to_lowercase().contains(&format!("▸ {c}")))
                || hits.first().is_some_and(|&i| shares(cat[i].label));
            let searchable = !hits.is_empty();
            if !found || !searchable {
                missing.push(caption.to_string());
            }
        }
    }
    assert!(seen > 80, "parsed {seen} ribbon buttons");
    assert!(missing.is_empty(), "not in Search: {missing:?}");
}

#[test]
fn ids_are_unique_and_actions_complete() {
    let cat = catalog();
    let mut ids: Vec<&str> = cat.iter().map(|a| a.id).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate ids");
    assert!(n > 180, "{n} actions");
    for a in &cat {
        assert!(!a.label.is_empty() && !a.path.is_empty() && !a.icon.is_empty(), "{}", a.id);
    }
}

// ------------------------------------------------------------------ UI

struct H {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}

impl H {
    fn new() -> Self {
        let mut app = ProjectFcgApp::new(sample::software_release());
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
    fn focused(&self) -> bool {
        self.ctx.memory(|m| m.has_focus(projectfcg::app::tell_me::box_id()))
    }
}

const ALT: Modifiers = Modifiers { alt: true, ctrl: false, shift: false, mac_cmd: false, command: false };

#[test]
fn alt_q_type_enter_runs_the_action() {
    let mut h = H::new();
    h.app.select_index(2);
    // a browser (and some desktops) also sends the chord as typed text
    h.frame(vec![Event::Key { key: Key::Q, physical_key: None, pressed: true, repeat: false, modifiers: ALT }, Event::Text("q".into())]);
    h.frame(vec![]);
    assert!(h.focused() && h.app.tell_me.open, "Alt+Q puts the cursor in Search");
    assert_eq!(h.app.tell_me.query, "", "and types nothing");
    h.typ("task info");
    h.key(Key::Enter, Modifiers::NONE);
    assert!(h.app.dialogs.task_info.is_some(), "Task Information opened");
    assert!(!h.app.tell_me.open && h.app.tell_me.query.is_empty() && !h.focused());
    assert_eq!(h.app.prefs.recent_actions.first().map(String::as_str), Some("task_info"), "remembered");
}

#[test]
fn arrows_pick_and_escape_closes() {
    let mut h = H::new();
    let views = h.app.view.main;
    h.key(Key::Q, ALT);
    h.typ("view");
    let rows_before = h.app.tell_me.sel;
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.app.tell_me.sel, rows_before + 1);
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(h.app.tell_me.sel, rows_before);
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.app.tell_me.open && !h.focused());
    assert_eq!(h.app.view.main, views, "Escape ran nothing");
    // the keys typed into Search never reached the table
    assert_eq!(h.app.project.tasks.len(), sample::software_release().tasks.len());
}

#[test]
fn unavailable_actions_say_why_and_do_nothing() {
    let mut h = H::new();
    h.app.select_index(2);
    let n = h.app.project.tasks.len();
    h.key(Key::Q, ALT);
    h.typ("link tasks"); // needs two tasks selected
    h.key(Key::Enter, Modifiers::NONE);
    assert!(h.app.status.contains("Select two or more tasks"), "{}", h.app.status);
    assert_eq!(h.app.project.tasks.len(), n);
}

#[test]
fn no_action_hands_the_words_to_the_plan_search() {
    let mut h = H::new();
    h.key(Key::Q, ALT);
    h.typ("database migration");
    // no command is called that: the first row searches the plan (Ctrl+E)
    h.key(Key::Enter, Modifiers::NONE);
    assert!(!h.app.tell_me.open);
    assert!(h.app.item_search.open, "the plan search opened");
    assert_eq!(h.app.item_search.query, "database migration", "with the words");
    h.frame(vec![]);
    assert!(h.ctx.memory(|m| m.has_focus(projectfcg::app::item_search::box_id())));
}

#[test]
fn every_action_runs_without_trouble() {
    for act in catalog() {
        let mut h = H::new();
        h.app.select_index(2);
        if act.id == "link" {
            h.app.select_index(1);
        }
        let ctx = h.ctx.clone();
        match act.run {
            projectfcg::app::tell_me::Run::Cmd(c) => h.app.execute(c, &ctx),
            projectfcg::app::tell_me::Run::Do(f) => f(&mut h.app, &ctx),
        }
        h.frame(vec![]);
        h.frame(vec![]);
        // (New starts an empty plan; the rest keep the sample's tasks)
        if !["new", "delete_task", "cut", "agile_sample"].contains(&act.id) {
            assert!(h.app.project.tasks.len() >= 17, "{} left {} tasks", act.id, h.app.project.tasks.len());
        }
    }
}
