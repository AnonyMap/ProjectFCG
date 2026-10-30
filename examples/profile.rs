//! Profiling driver: `cargo run --release --example profile -- schedule|level [tasks]`
#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "schedule".into());
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let mut p = common::random_project(7, &common::Shape { tasks: n, resources: n / 50, ..Default::default() });
    let t0 = std::time::Instant::now();
    match what.as_str() {
        "schedule" => {
            for _ in 0..5 {
                projectfcg::schedule::schedule(&mut p);
            }
        }
        "level" => {
            let r = projectfcg::leveling::level(&mut p, None);
            eprintln!("delayed {} unresolved {}", r.delayed.len(), r.unresolved.len());
        }
        "taskusage" | "gantt" | "resusage" => {
            let mut app = projectfcg::app::ProjectFcgApp::new(p.clone());
            app.use_portal = false;
            app.view.main = match what.as_str() {
                "taskusage" => projectfcg::app::MainView::TaskUsage,
                "resusage" => projectfcg::app::MainView::ResourceUsage,
                _ => projectfcg::app::MainView::Gantt,
            };
            let ctx = egui::Context::default();
            for k in 0..20 {
                app.scroll_y = k as f32 * 30.0;
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
                    time: Some(k as f64 * 0.05),
                    ..Default::default()
                };
                ctx.run_ui(raw, |ui| app.frame(ui)).drop_without_applying_deltas();
            }
        }
        "board" | "xml" | "jira" => {
            // n tickets over the agile sample's sprints, with epics, sub-tasks and links
            let mut q = projectfcg::app::sample::agile_release();
            let sprints: Vec<u32> = q.sprint_rows().into_iter().map(|r| q.tasks[r].uid).collect();
            let epics: Vec<u32> = q.epics().into_iter().map(|k| q.tickets[k].id).collect();
            for k in 0..n {
                let s = if k % 3 == 0 { None } else { Some(sprints[k % sprints.len()]) };
                let id = q.add_ticket_of(&format!("Ticket {k}"), s, projectfcg::board::IssueType::Story, Some(epics[k % epics.len()])).unwrap();
                let i = q.ticket_index(id).unwrap();
                q.tickets[i].points = Some((k % 8) as f64);
                q.tickets[i].column = k % 4;
                if k % 5 == 0 {
                    q.add_ticket_of(&format!("Sub {k}"), None, projectfcg::board::IssueType::Subtask, Some(id)).unwrap();
                }
                if k > 10 && k % 7 == 0 {
                    let _ = q.add_ticket_link(id, projectfcg::board::LinkKind::Blocks, id - 3);
                }
            }
            projectfcg::schedule::schedule(&mut q);
            let t1 = std::time::Instant::now();
            match what.as_str() {
                "xml" => {
                    let x = projectfcg::mspdi::to_xml(&p);
                    let t2 = std::time::Instant::now();
                    let (r, _) = projectfcg::mspdi::from_xml(&x).unwrap();
                    eprintln!("xml {} tasks: write {:?}, read+schedule {:?}, {:.1} MB", r.tasks.len(), t2 - t1, t2.elapsed(), x.len() as f64 / 1e6);
                }
                "jira" => {
                    let c = projectfcg::jira::export_csv(&q);
                    let t2 = std::time::Instant::now();
                    let mut r = projectfcg::model::Project::new(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
                    let rep = projectfcg::jira::import_csv(&mut r, &c, &projectfcg::jira::ImportOptions { create_sprints: true, create_people: true }).unwrap();
                    eprintln!("jira {} tickets: export {:?}, import {:?}", rep.tickets, t2 - t1, t2.elapsed());
                }
                _ => {
                    let mut app = projectfcg::app::ProjectFcgApp::new(q);
                    app.use_portal = false;
                    app.view.main = projectfcg::app::MainView::Board;
                    let ctx = egui::Context::default();
                    let mut times = vec![];
                    for k in 0..20 {
                        let f0 = std::time::Instant::now();
                        app.board.scroll_y = k as f32 * 40.0;
                        let raw = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
                            time: Some(k as f64 * 0.05),
                            ..Default::default()
                        };
                        ctx.run_ui(raw, |ui| app.frame(ui)).drop_without_applying_deltas();
                        times.push(f0.elapsed());
                    }
                    times.sort();
                    eprintln!("board {} tickets: median frame {:?}", app.project.tickets.len(), times[10]);
                }
            }
        }
        "mem" => {
            let json = projectfcg::io::to_json(&p).unwrap();
            let rss = || std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmRSS")).unwrap().to_string();
            eprintln!("json {:.1} MB; before {}", json.len() as f64 / 1e6, rss());
            let mut h = projectfcg::history::History::default();
            let mut q = p.clone();
            let leaves: Vec<usize> = (0..q.tasks.len()).filter(|&i| !q.tasks[i].calc.is_summary).collect();
            for k in 0..100 {
                let before = q.clone();
                let row = leaves[k * 7 % leaves.len()];
                q.set_field_text(row, projectfcg::ops::Field::Duration, &format!("{}d", 1 + k % 9)).unwrap();
                projectfcg::schedule::schedule(&mut q);
                h.checkpoint("edit", &before, &q);
            }
            eprintln!("after 100 edits: history ~{:.2} MB, {}", h.approx_bytes() as f64 / 1e6, rss());
        }
        _ => panic!("schedule|level|taskusage|resusage|gantt|mem"),
    }
    eprintln!("{what}: {:?}", t0.elapsed());
}
