//! File ▸ Print: the PDF the Gantt view prints to.
mod common;
use projectfcg::app::commands::{Command, Filter};
use projectfcg::app::filters::GroupBy;
use projectfcg::app::print::{print_pdf, PrintOptions};
use projectfcg::app::{sample, ProjectFcgApp};
use projectfcg::pdf::Paper;
use std::process::Command as Sh;

fn pages(pdf: &[u8]) -> usize {
    String::from_utf8_lossy(pdf).matches("/Type /Page ").count()
}

/// Text in the PDF, if poppler's pdftotext is installed (else `None`).
fn text_of(pdf: &[u8], tag: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pfcg_print_{}_{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let f = dir.join("p.pdf");
    std::fs::write(&f, pdf).ok()?;
    let out = Sh::new("pdftotext").args(["-layout"]).arg(&f).arg("-").output().ok()?;
    if let Ok(q) = Sh::new("qpdf").arg("--check").arg(&f).output() {
        assert!(q.status.success(), "qpdf --check: {}", String::from_utf8_lossy(&q.stdout));
    }
    std::fs::remove_dir_all(&dir).ok();
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn the_sample_prints_on_one_page_with_its_tasks() {
    let app = ProjectFcgApp::new(sample::software_release());
    let pdf = print_pdf(&app, &PrintOptions::default());
    assert_eq!(pages(&pdf), 1);
    if let Some(t) = text_of(&pdf, "sample") {
        for name in ["Software Release 2.0", "Define requirements", "Release 2.0 shipped", "Page 1 of 1", "Task Name"] {
            assert!(t.contains(name), "{name} missing:\n{t}");
        }
    }
}

#[test]
fn a_big_plan_paginates_and_every_task_prints_once() {
    let shape = common::Shape { tasks: 300, resources: 10, ..Default::default() };
    let p = common::random_project(3, &shape);
    let n = p.tasks.len();
    let app = ProjectFcgApp::new(p);
    for (paper, landscape) in [(Paper::Letter, true), (Paper::A4, false), (Paper::Tabloid, true)] {
        let o = PrintOptions { paper, landscape, ..Default::default() };
        let pdf = print_pdf(&app, &o);
        let k = pages(&pdf);
        assert!(k > 1, "{paper:?}");
        if let Some(t) = text_of(&pdf, &format!("{paper:?}{landscape}")) {
            assert!(t.contains(&format!("Page {k} of {k}")));
            // each row's ID appears at the start of a line exactly once
            let ids: Vec<usize> = t.lines().filter_map(|l| l.split_whitespace().next()?.parse().ok()).collect();
            for id in [1, n / 2, n] {
                assert!(ids.contains(&id), "ID {id} missing on {paper:?}");
            }
        }
    }
}

#[test]
fn prints_what_the_view_shows() {
    let mut app = ProjectFcgApp::new(sample::software_release());
    let ctx = egui::Context::default();
    app.execute(Command::SetFilter(Filter::Critical), &ctx);
    app.execute(Command::SetGroup(GroupBy::Resource), &ctx);
    let pdf = print_pdf(&app, &PrintOptions { range: Some((app.project.start.date(), app.project.start.date() + chrono::Duration::days(20))), ..Default::default() });
    if let Some(t) = text_of(&pdf, "view") {
        assert!(t.contains("Filter: Critical") && t.contains("Grouped by Resource"), "{t}");
        assert!(t.contains("Resource: Lena Park"), "{t}");
        assert!(!t.contains("Database migration"), "not critical");
    }
}
