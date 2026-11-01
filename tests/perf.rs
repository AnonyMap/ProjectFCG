//! Timing probes (not run by default):
//!   cargo test --release --test perf -- --ignored --nocapture
mod common;
use common::{random_project, Shape};
use egui::{vec2, Pos2, RawInput, Rect};
use projectfcg::app::{ProjectFcgApp, MainView};
use std::time::Instant;

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t0 = Instant::now();
    let v = f();
    eprintln!("{label:<44} {:>9.2} ms", t0.elapsed().as_secs_f64() * 1000.0);
    v
}

fn frames(app: &mut ProjectFcgApp, ctx: &egui::Context, n: usize, label: &str) {
    let mut t = 0.0;
    // warm-up (fonts, layout caches)
    for _ in 0..3 {
        t += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), time: Some(t), ..Default::default() };
        ctx.run_ui(raw, |ui| app.frame(ui)).drop_without_applying_deltas();
    }
    let t0 = Instant::now();
    let mut worst = 0.0f64;
    for k in 0..n {
        t += 0.05;
        // scroll as we go so each frame draws different rows
        app.scroll_y = (k as f32) * 30.0;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), time: Some(t), ..Default::default() };
        let f0 = Instant::now();
        ctx.run_ui(raw, |ui| app.frame(ui)).drop_without_applying_deltas();
        worst = worst.max(f0.elapsed().as_secs_f64());
    }
    let avg = t0.elapsed().as_secs_f64() / n as f64;
    eprintln!("{label:<44} {:>9.2} ms/frame (worst {:.2})", avg * 1000.0, worst * 1000.0);
}

#[test]
#[ignore]
fn probe() {
    for (tasks, res) in [(500, 20), (2000, 40), (5000, 80)] {
        eprintln!("--- {tasks} tasks, {res} resources");
        let shape = Shape { tasks, resources: res, ..Default::default() };
        let mut p = time("generate", || random_project(7, &shape));
        eprintln!("  ({} assignments, {} links)", p.assignments.len(), p.tasks.iter().map(|t| t.predecessors.len()).sum::<usize>());
        time("schedule", || projectfcg::schedule::schedule(&mut p));
        time("compute_resources", || projectfcg::resources::compute_resources(&mut p));
        let json = time("to_json", || projectfcg::io::to_json(&p).unwrap());
        time("from_json (incl. schedule)", || projectfcg::io::from_json(&json).unwrap());
        let mut app = ProjectFcgApp::new(p.clone());
        app.use_portal = false;
        let ctx = egui::Context::default();
        frames(&mut app, &ctx, 30, "frame: Gantt");
        app.view.main = MainView::ResourceUsage;
        frames(&mut app, &ctx, 30, "frame: Resource Usage");
        app.view.main = MainView::TaskUsage;
        frames(&mut app, &ctx, 30, "frame: Task Usage");
        app.view.main = MainView::ResourceSheet;
        frames(&mut app, &ctx, 30, "frame: Resource Sheet");
        app.view.main = MainView::Gantt;
        // a typical edit: change one duration and reschedule (what the UI does)
        let row = p.tasks.iter().position(|t| !t.calc.is_summary).unwrap();
        time("edit duration + reschedule", || {
            app.edit_project("dur", |p| p.set_field_text(row, projectfcg::ops::Field::Duration, "7d"));
        });
        if tasks <= 2000 {
            let r = time("level all (has ALAP tasks: full reschedules)", || projectfcg::leveling::level(&mut p, None));
            eprintln!("  ({} delayed, {} unresolved)", r.delayed.len(), r.unresolved.len());
        }
        let mut q = random_project(7, &Shape { constraints: false, ..Shape { tasks, resources: res, ..Default::default() } });
        let r = time("level all (no ALAP: lazy forward pass)", || projectfcg::leveling::level(&mut q, None));
        eprintln!("  ({} delayed, {} unresolved)", r.delayed.len(), r.unresolved.len());
    }
}

#[test]
#[ignore]
fn views_probe() {
    use projectfcg::app::columns::ColumnId;
    use projectfcg::app::commands::{Command, Filter};
    use projectfcg::app::filters::GroupBy;
    use projectfcg::views::{Condition, CustomFilter, Test};
    let shape = Shape { tasks: 5000, resources: 80, ..Default::default() };
    let p = random_project(7, &shape);
    let ctx = egui::Context::default();
    let mut app = ProjectFcgApp::new(p);
    app.view.timeline = false;
    frames(&mut app, &ctx, 30, "5000 tasks: plain Gantt");
    let f = CustomFilter {
        id: 0,
        name: "Long or costly".into(),
        conditions: vec![
            Condition { field: ColumnId::Duration.key(), test: Test::Greater, value: "3d".into(), ..Default::default() },
            Condition { or: true, field: ColumnId::Cost.key(), test: Test::Greater, value: "2000".into(), ..Default::default() },
            Condition { field: ColumnId::Name.key(), test: Test::Contains, value: "1".into(), ..Default::default() },
        ],
        related_summaries: true,
    };
    let id = app.project.views.add(f);
    app.execute(Command::SetFilter(Filter::Custom(id)), &ctx);
    time("display_rows, custom filter", || app.display_rows().len());
    frames(&mut app, &ctx, 30, "5000 tasks: custom filter");
    app.execute(Command::SetGroup(GroupBy::Resource), &ctx);
    time("display_rows, filter + group by resource", || app.display_rows().len());
    frames(&mut app, &ctx, 30, "5000 tasks: filter + group");
    app.execute(Command::SetFilter(Filter::AllTasks), &ctx);
    app.execute(Command::SetGroup(GroupBy::Field(ColumnId::Start)), &ctx);
    frames(&mut app, &ctx, 30, "5000 tasks: group by start week");
}
