//! Risk register: standards and scoring, links, repair, files, and the
//! Risk Register view (selection, editing, undo).
use chrono::NaiveDate;
use egui::{vec2, Event, Key, Modifiers, Pos2, RawInput, Rect};
use projectfcg::app::commands::Command;
use projectfcg::app::{sample, MainView, ProjectFcgApp};
use projectfcg::model::Project;
use projectfcg::risks::{Dimension, Rating, Risk, RiskKind, RiskLevel, RiskLink, RiskStandard, RiskStatus};

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

fn risk(std: Option<RiskStandard>, kind: RiskKind, l: u8, c: u8) -> Risk {
    Risk { standard: std, kind, rating: Rating::new(l, c), ..Default::default() }
}

fn level(p: &Project, std: RiskStandard, l: u8, c: u8) -> RiskLevel {
    p.risk_level(&risk(Some(std), RiskKind::Threat, l, c))
}

#[test]
fn each_standard_rates_the_same_cell_its_own_way() {
    let p = Project::new(d(1, 5));
    use RiskLevel::*;
    // DoD reporting matrix: near-certain but minimal is still low, a
    // critical consequence is never low, the top-right corner is high
    assert_eq!(level(&p, RiskStandard::Dod, 5, 1), Low);
    assert_eq!(level(&p, RiskStandard::Dod, 1, 5), Moderate);
    assert_eq!(level(&p, RiskStandard::Dod, 3, 3), Moderate);
    assert_eq!(level(&p, RiskStandard::Dod, 4, 4), High);
    assert_eq!(level(&p, RiskStandard::Dod, 1, 1), Low);
    // PMI P × I: 0.5 × 0.4 = 0.20 ≥ 0.18 high; 0.3 × 0.2 = 0.06 moderate
    assert_eq!(level(&p, RiskStandard::Pmi, 3, 4), High);
    assert_eq!(level(&p, RiskStandard::Pmi, 2, 3), Moderate);
    assert_eq!(level(&p, RiskStandard::Pmi, 1, 2), Low);
    assert!((p.risk_score(&risk(Some(RiskStandard::Pmi), RiskKind::Threat, 3, 4)) - 0.2).abs() < 1e-9);
    // no standard: L × C, 15 and up high, 5 and up medium
    assert_eq!(level(&p, RiskStandard::None, 3, 5), High);
    assert_eq!(level(&p, RiskStandard::None, 2, 3), Moderate);
    assert_eq!(level(&p, RiskStandard::None, 2, 2), Low);
    assert_eq!(RiskLevel::Moderate.label(RiskStandard::None), "Medium");
    // not rated yet: no level, no score
    assert_eq!(level(&p, RiskStandard::Dod, 0, 4), None);
    assert_eq!(p.risk_score(&risk(Option::None, RiskKind::Threat, 3, 0)), 0.0);
    // issues have happened: the consequence alone sets the level
    let issue = |c| p.risk_level(&risk(Some(RiskStandard::Dod), RiskKind::Issue, 0, c));
    assert_eq!((issue(2), issue(3), issue(4)), (Low, Moderate, High));
}

#[test]
fn the_worst_dimension_counts_and_a_risk_may_follow_its_own_standard() {
    let mut p = Project::new(d(1, 5));
    p.risk_settings.standard = RiskStandard::Dod;
    let mut r = Risk { rating: Rating { likelihood: 4, consequence: vec![] }, ..Default::default() };
    r.rating.set(Dimension::Cost, 2);
    r.rating.set(Dimension::Schedule, 4);
    r.rating.set(Dimension::Performance, 1);
    assert_eq!(r.rating.worst(), 4);
    assert_eq!(p.risk_level(&r), RiskLevel::High, "DoD 4 × 4");
    r.rating.set(Dimension::Schedule, 0);
    assert_eq!(r.rating.consequence.len(), 2, "a dimension set to 0 is dropped");
    assert_eq!(p.risk_level(&r), RiskLevel::Moderate, "now 4 × 2");
    // the same rating under PMI: 0.7 × 0.1 = 0.07, moderate; under no
    // standard 8, medium; the register's standard is the default
    r.standard = Some(RiskStandard::Pmi);
    assert_eq!(p.risk_standard(&r), RiskStandard::Pmi);
    assert!((p.risk_score(&r) - 0.07).abs() < 1e-9);
    r.standard = None;
    assert_eq!(p.risk_standard(&r), RiskStandard::Dod);
}

#[test]
fn custom_matrix_colours_apply_to_the_registers_standard_only() {
    let mut p = Project::new(d(1, 5));
    p.risk_settings.standard = RiskStandard::Dod;
    let mut m = [[0u8; 5]; 5];
    m[0][0] = 2; // L1 C1 high
    p.risk_settings.matrix = Some(m);
    assert_eq!(level(&p, RiskStandard::Dod, 1, 1), RiskLevel::High);
    assert_eq!(level(&p, RiskStandard::Dod, 5, 5), RiskLevel::Low, "the whole matrix is the custom one");
    assert_eq!(level(&p, RiskStandard::Pmi, 5, 5), RiskLevel::High, "a PMI risk keeps PMI's thresholds");
}

#[test]
fn statements_and_expected_value() {
    let p = Project::new(d(1, 5));
    let mut r = Risk { event: "the vendor slips".into(), consequence: "integration starts late".into(), cause: "a single supplier".into(), ..Default::default() };
    assert_eq!(r.statement(RiskStandard::Dod), "If the vendor slips, then integration starts late.");
    assert_eq!(r.statement(RiskStandard::Pmi), "Because of a single supplier, the vendor slips may occur, which would lead to integration starts late.");
    r.description = "free text".into();
    assert_eq!(r.statement(RiskStandard::None), "free text");
    // EMV: probability × cost, negative for threats
    r.standard = Some(RiskStandard::Pmi);
    r.rating = Rating::new(4, 3);
    r.cost_impact = 10_000.0;
    assert!((p.risk_emv(&r) + 7_000.0).abs() < 1e-6);
    r.kind = RiskKind::Opportunity;
    assert!((p.risk_emv(&r) - 7_000.0).abs() < 1e-6);
    // a typed probability wins (no standard)
    r.standard = Some(RiskStandard::None);
    r.probability_pct = Some(25.0);
    assert!((p.risk_emv(&r) - 2_500.0).abs() < 1e-6);
    // keys by kind
    let k = |kind, id| Risk { kind, id, ..Default::default() }.key();
    assert_eq!((k(RiskKind::Threat, 7), k(RiskKind::Opportunity, 12), k(RiskKind::Issue, 3)), ("R-007".to_string(), "O-012".to_string(), "I-003".to_string()));
}

fn plan_with_risks() -> Project {
    let mut p = Project::new(d(1, 5));
    let a = p.append_task("Design");
    p.append_task("Build");
    let res = p.add_resource("Ana", projectfcg::model::ResourceType::Work);
    let (tu, ru) = (p.tasks[a].uid, p.resources[res].uid);
    p.add_risk(Risk { title: "Late spec".into(), links: vec![RiskLink::Task(tu), RiskLink::Resource(ru)], rating: Rating::new(3, 3), ..Default::default() });
    p.add_risk(Risk { title: "Budget, \"tight\"".into(), cost_impact: 500.0, ..Default::default() });
    projectfcg::schedule::schedule(&mut p);
    p
}

#[test]
fn links_ids_and_repair() {
    let mut p = plan_with_risks();
    assert_eq!((p.risks[0].id, p.risks[1].id), (1, 2));
    assert_eq!(p.risk_links_text(&p.risks[0]), "Task 1: Design; Resource: Ana");
    assert_eq!(p.risk_links_text(&p.risks[1]), "Project");
    assert_eq!(p.risks_of_task(p.tasks[0].uid).len(), 1);
    assert!(p.risks_of_task(p.tasks[1].uid).is_empty());
    // IDs are never reused, even after a delete
    p.delete_risk(2).unwrap();
    assert!(p.delete_risk(2).is_err());
    assert_eq!(p.add_risk(Risk::default()), 3);
    // closed risks don't show on the task
    p.risks[0].status = RiskStatus::Closed;
    assert!(p.risks_of_task(p.tasks[0].uid).is_empty());
    // repair: dangling links, duplicate IDs, out-of-range levels
    p.risks[0].links.push(RiskLink::Task(9999));
    p.risks[1].id = 1;
    p.risks[1].rating = Rating { likelihood: 9, consequence: vec![(Dimension::Overall, 7)] };
    p.risks[1].cost_impact = f64::NAN;
    assert!(p.repair_risks() >= 4);
    assert_eq!(p.risks[0].links.len(), 2);
    assert_ne!(p.risks[0].id, p.risks[1].id);
    assert_eq!((p.risks[1].rating.likelihood, p.risks[1].rating.worst()), (5, 5));
    assert_eq!(p.risks[1].cost_impact, 0.0);
    let next = p.add_risk(Risk::default());
    assert!(p.risks.iter().filter(|r| r.id == next).count() == 1);
}

#[test]
fn the_register_survives_saving_and_old_files_load() {
    let p = plan_with_risks();
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.risks, p.risks);
    assert_eq!(q.risk_settings.next_id, p.risk_settings.next_id);
    // a file from before the register: no risks, the default standard
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let o = v["project"].as_object_mut().unwrap();
    o.remove("risks");
    o.remove("risk_settings");
    let old = projectfcg::io::from_json(&v.to_string()).unwrap();
    assert!(old.risks.is_empty());
    assert_eq!(old.risk_settings.standard, RiskStandard::None);
}

#[test]
fn csv_has_one_row_per_risk_and_quotes() {
    let p = plan_with_risks();
    let csv = p.risks_csv();
    let mut all = projectfcg::jira::parse_csv(&csv);
    all.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    let head = all.remove(0);
    let rows = all;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.len() == head.len()), "every row has every column");
    let col = |name: &str| head.iter().position(|h| h == name).unwrap_or_else(|| panic!("no column {name}"));
    assert_eq!(&rows[0][col("ID")], "R-001");
    assert_eq!(&rows[0][col("Linked to")], "Task 1: Design; Resource: Ana");
    assert_eq!(&rows[1][col("Title")], "Budget, \"tight\"");
    assert_eq!(&rows[0][col("Level")], "Medium", "3 × 3 under no standard");
}

#[test]
fn the_sample_register_is_valid() {
    let p = sample::software_release();
    assert_eq!(p.risk_settings.standard, RiskStandard::Dod);
    assert!(p.risks.len() >= 5);
    assert!(p.risks.iter().any(|r| r.kind == RiskKind::Opportunity) && p.risks.iter().any(|r| r.kind == RiskKind::Issue));
    let mut q = p.clone();
    assert_eq!(q.repair_risks(), 0, "nothing dangling");
    for r in &p.risks {
        assert!(r.rating.is_rated(r.kind), "{} rated", r.key());
        assert!(projectfcg::risks::responses(RiskStandard::Dod, r.kind).contains(&r.response.as_str()), "{}: {} is a DoD response", r.key(), r.response);
        assert!(RiskStandard::Dod.categories().contains(&r.category.as_str()) || r.category.is_empty());
    }
}

// ------------------------------------------------------------------ view

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
        out.textures_delta.clear(); // epaint panics on dropped deltas in debug builds
    }
    fn exec(&mut self, c: Command) {
        let ctx = self.ctx.clone();
        self.app.execute(c, &ctx);
        self.frame(vec![]);
    }
}

#[test]
fn looking_at_risks_changes_nothing() {
    let mut h = H::new(sample::software_release());
    h.exec(Command::ShowView(MainView::Risks));
    h.app.risk_view.show_target = true;
    let gen = h.app.history.generation();
    let ids: Vec<u32> = h.app.project.risks.iter().map(|r| r.id).collect();
    for id in ids {
        h.app.select_risk(Some(id));
        h.frame(vec![]);
        h.frame(vec![]);
        assert_eq!(h.app.history.generation(), gen, "selecting {} made an edit ({:?})", id, h.app.history.undo_label());
    }
    // every standard, and a risk under another standard than the register's
    for std in RiskStandard::ALL {
        h.app.project.risk_settings.standard = std;
        h.app.select_risk(Some(1));
        h.frame(vec![]);
        h.app.select_risk(Some(2));
        h.frame(vec![]);
    }
    assert_eq!(h.app.history.generation(), gen);
    assert_eq!(h.app.risk_rows().len(), h.app.project.risks.len(), "all open");
}

#[test]
fn edits_save_as_one_undo_step_and_undo_reloads_them() {
    let mut h = H::new(plan_with_risks());
    h.exec(Command::ShowView(MainView::Risks));
    h.app.select_risk(Some(1));
    h.frame(vec![]);
    let draft = h.app.risk_view.draft.as_mut().expect("loaded");
    draft.title = "Spec arrives late".into();
    draft.rating = Rating::new(5, 4);
    h.frame(vec![]); // nobody typing: saved
    assert_eq!(h.app.project.risk(1).unwrap().title, "Spec arrives late");
    assert_eq!(h.app.history.undo_label(), Some("Edit R-001"));
    h.exec(Command::Undo);
    assert_eq!(h.app.project.risk(1).unwrap().title, "Late spec");
    assert_eq!(h.app.risk_view.draft.as_ref().unwrap().title, "Late spec", "the details follow the undo");
    h.exec(Command::Redo);
    assert_eq!(h.app.risk_view.draft.as_ref().unwrap().rating.worst(), 4);
}

#[test]
fn insert_and_delete_act_on_risks_in_the_register() {
    let mut h = H::new(plan_with_risks());
    let tasks = h.app.project.tasks.len();
    h.exec(Command::ShowView(MainView::Risks));
    h.exec(Command::InsertTask);
    assert_eq!(h.app.project.risks.len(), 3);
    let id = h.app.risk_view.selected.expect("the new risk is selected");
    assert_eq!(h.app.project.risk(id).unwrap().identified, Some(chrono::Local::now().date_naive()));
    // the Delete key (nothing focused) deletes the risk, never a task
    h.frame(vec![Event::Key { key: Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    h.frame(vec![]);
    assert_eq!(h.app.project.risks.len(), 2);
    assert_eq!(h.app.project.tasks.len(), tasks);
    assert_eq!(h.app.risk_view.selected, None);
    h.exec(Command::Undo);
    assert_eq!(h.app.project.risks.len(), 3);
}

#[test]
fn add_risk_for_a_task_ties_it_and_opens_the_register() {
    let mut h = H::new(plan_with_risks());
    h.app.select_index(1);
    h.app.add_risk_for_active_task();
    h.frame(vec![]);
    assert_eq!(h.app.view.main, MainView::Risks);
    let id = h.app.risk_view.selected.unwrap();
    let uid = h.app.project.tasks[1].uid;
    assert_eq!(h.app.project.risk(id).unwrap().links, vec![RiskLink::Task(uid)]);
    assert_eq!(h.app.project.risks_of_task(uid).len(), 1);
    // filters: kind, status, search, a matrix cell
    h.app.risk_view.search = "late".into();
    assert_eq!(h.app.risk_rows(), vec![1]);
    h.app.risk_view.search.clear();
    h.app.risk_view.cell = Some((false, 3, 3));
    assert_eq!(h.app.risk_rows(), vec![1]);
    h.app.risk_view.cell = None;
    h.app.risk_view.kind = Some(RiskKind::Issue);
    assert!(h.app.risk_rows().is_empty());
}
