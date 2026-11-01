//! Renders the README screenshots in a real window (run under a display,
//! or Xvfb) and saves them as PNGs.
//!
//!   cargo run --release --example screens -- docs        # every scene
//!   cargo run --release --example screens -- docs board  # one scene

use projectfcg::app::bars::PathRole;
use projectfcg::app::board::SidePanel;
use projectfcg::app::columns::ColumnId;
use projectfcg::app::commands::{Command, Filter};
use projectfcg::app::filters::GroupBy;
use projectfcg::app::theme::ThemeMode;
use projectfcg::app::{sample, MainView, ProjectFcgApp, RibbonTab};
use projectfcg::format::GanttViewKind;
use projectfcg::views::{Condition, CustomFilter, Test};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const W: f32 = 1440.0;
const H: f32 = 880.0;

type Setup = fn(&mut ProjectFcgApp, &egui::Context);

fn row(app: &ProjectFcgApp, name: &str) -> usize {
    app.project.tasks.iter().position(|t| t.name == name).unwrap_or_else(|| panic!("no task {name}"))
}

/// Detail Gantt with Task Path and the Task Inspector.
fn gantt(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.tab = RibbonTab::Format;
    app.execute(Command::SetGanttView(GanttViewKind::Detail), ctx);
    let r = row(app, "User acceptance testing");
    app.select_index(r);
    app.execute(Command::TaskPath(PathRole::DrivingPredecessor), ctx);
    app.view.inspector = true;
    // the inspector narrows the chart: pick the scale by hand
    app.gantt.ppd = 5.4;
    let start = app.project.start;
    app.scroll_gantt_to(start);
}

/// The board: issue types, epics, releases, burndown and velocity.
fn board(app: &mut ProjectFcgApp, _ctx: &egui::Context) {
    app.view.main = MainView::Board;
    app.tab = RibbonTab::Board;
    let id = app.project.tickets.iter().find(|t| t.title == "Checkout with card").map(|t| t.id);
    app.board.selected = id;
}

/// Releases panel and the cumulative flow chart.
fn releases(app: &mut ProjectFcgApp, _ctx: &egui::Context) {
    app.view.main = MainView::Board;
    app.tab = RibbonTab::Board;
    app.board.side = SidePanel::Releases;
    app.board.flow = true;
}

/// A task waiting on an epic, and the release milestone waiting on its stories.
fn board_deps(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.tab = RibbonTab::Board;
    let r = row(app, "Payment FAQ & support macros");
    app.select_index(r);
    let s3 = row(app, "Sprint 3");
    let end = app.project.calc.finish.unwrap();
    let start = app.project.tasks[s3].calc.start.unwrap();
    app.fit_range(start, end);
    let _ = ctx;
}

/// Group by resource, with a custom filter.
fn grouping(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.tab = RibbonTab::View;
    let f = CustomFilter {
        id: 0,
        name: "Open work".into(),
        conditions: vec![
            Condition { field: ColumnId::PercentComplete.key(), test: Test::Less, value: "100".into(), ..Default::default() },
            Condition { field: ColumnId::Duration.key(), test: Test::Greater, value: "0d".into(), ..Default::default() },
        ],
        related_summaries: false,
    };
    let id = app.edit_project("New Filter", |p| Ok(p.views.add(f))).unwrap();
    app.execute(Command::SetFilter(Filter::Custom(id)), ctx);
    app.execute(Command::SetGroup(GroupBy::Resource), ctx);
    app.execute(Command::ZoomEntireProject, ctx);
}

/// Tracking Gantt: baseline, progress lines and slippage at a status date.
fn tracking(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.tab = RibbonTab::Format;
    app.project.set_baseline(None);
    // two weeks in: the backend started three days late, so everything after
    // it slipped; some work is behind
    let r = row(app, "Backend API");
    let late = app.project.calendar.add_work(app.project.tasks[r].calc.start.unwrap(), 3 * 480);
    app.project.tasks[r].constraint = projectfcg::model::ConstraintType::SNET;
    app.project.tasks[r].constraint_date = Some(late);
    app.recalc();
    let start = app.project.start;
    let status = app.project.calendar.add_work(start, 10 * 480);
    app.project.status_date = Some(status);
    for (name, pct) in [("Backend API", 20.0), ("Frontend UI", 10.0)] {
        let r = row(app, name);
        app.project.set_percent_complete(r, pct).ok();
    }
    app.project.format.gridlines.progress_lines = true;
    app.project.format.show_slippage = true;
    app.recalc();
    app.execute(Command::SetGanttView(GanttViewKind::Tracking), ctx);
    let t = app.project.tasks.iter().position(|t| t.name == "Frontend UI").unwrap();
    app.select_index(t);
    app.execute(Command::ZoomEntireProject, ctx);
}

/// Resource Usage with an overallocation.
fn usage(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.tab = RibbonTab::Resource;
    app.execute(Command::NextOverallocation, ctx);
    app.execute(Command::ShowView(MainView::ResourceUsage), ctx);
    if let Some(s) = app.active_index().and_then(|r| app.project.tasks[r].calc.start) {
        app.scroll_gantt_to(s - chrono::Duration::days(3));
    }
}

/// The dark theme.
fn dark(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    app.prefs.theme_mode = ThemeMode::Dark;
    app.tab = RibbonTab::Task;
    app.project.format.show_critical = true;
    let r = row(app, "Frontend UI");
    app.select_index(r);
    app.execute(Command::ZoomEntireProject, ctx);
}

struct Scene {
    name: &'static str,
    agile: bool,
    setup: Setup,
}

const SCENES: &[Scene] = &[
    Scene { name: "screenshot", agile: false, setup: gantt },
    Scene { name: "board", agile: true, setup: board },
    Scene { name: "releases", agile: true, setup: releases },
    Scene { name: "board-deps", agile: true, setup: board_deps },
    Scene { name: "grouping", agile: false, setup: grouping },
    Scene { name: "tracking", agile: false, setup: tracking },
    Scene { name: "usage", agile: false, setup: usage },
    Scene { name: "dark", agile: false, setup: dark },
];

struct Shot {
    app: ProjectFcgApp,
    setup: Option<Setup>,
    frames: u32,
    slot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    asked: bool,
    out: PathBuf,
}

impl eframe::App for Shot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.app.frame(ui);
        self.frames += 1;
        ctx.request_repaint();
        // set the scene once the layout has settled (zooming needs the
        // chart's width)
        if self.frames == 4 {
            if let Some(s) = self.setup.take() {
                s(&mut self.app, &ctx);
            }
        }
        if self.frames == 16 && !self.asked {
            self.asked = true;
            let slot = self.slot.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
            });
        }
        let shot = self.slot.lock().unwrap().take();
        if let Some(img) = shot {
            let png = projectfcg::app::encode_png(&img).expect("png");
            std::fs::write(&self.out, png).expect("write");
            eprintln!("wrote {} ({}×{})", self.out.display(), img.size[0], img.size[1]);
            self.app.allow_close = true; // scenes edit the plan: don't ask to save
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "docs".into()));
    let only: Vec<String> = args.collect();
    for sc in SCENES.iter().filter(|s| only.is_empty() || only.iter().any(|o| o == s.name)) {
        let p = if sc.agile { sample::agile_release() } else { sample::software_release() };
        let mut app = ProjectFcgApp::new(p);
        app.prefs.theme_mode = ThemeMode::Light;
        app.use_portal = false;
        let shot = Shot { app, setup: Some(sc.setup), frames: 0, slot: Default::default(), asked: false, out: dir.join(format!("{}.png", sc.name)) };
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_title("ProjectFCG").with_inner_size([W, H]).with_resizable(false),
            ..Default::default()
        };
        eframe::run_native("ProjectFCG", options, Box::new(move |_| Ok(Box::new(shot)))).expect("window");
    }
}
