//! ProjectFCG desktop entry point.
//!
//! Usage:
//!   projectfcg [FILE.pfcg]      open a project
//!   projectfcg --sample           open the sample project
//!   projectfcg --sample-agile     open the sprints + board sample
//!   projectfcg --self-test        run a headless scheduling check and exit
//!   projectfcg --version

// No console window behind the app on Windows (release builds).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

fn self_test() -> i32 {
    let p = projectfcg::app::sample::software_release();
    let ok = p.calc.errors.is_empty() && p.tasks.iter().all(|t| t.calc.start.is_some());
    println!(
        "ProjectFCG {} self-test: {} tasks scheduled, finish {}, {}",
        env!("CARGO_PKG_VERSION"),
        p.tasks.len(),
        p.calc.finish.map(projectfcg::model::fmt_datetime).unwrap_or_default(),
        if ok { "OK" } else { "FAILED" }
    );
    if ok {
        0
    } else {
        1
    }
}

fn main() {
    let mut open: Option<PathBuf> = None;
    let mut sample: Option<bool> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--self-test" => std::process::exit(self_test()),
            "--version" | "-V" => {
                println!("projectfcg {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                println!("Usage: projectfcg [--sample | --sample-agile] [FILE.pfcg]\n       projectfcg --self-test | --version");
                return;
            }
            "--sample" => sample = Some(false),
            "--sample-agile" => sample = Some(true),
            other if !other.starts_with("--") => open = Some(PathBuf::from(other)),
            _ => {}
        }
    }
    if let Err(e) = projectfcg::app::run(open, sample) {
        eprintln!("projectfcg: failed to start the window: {e}");
        eprintln!("If no display is available, try `projectfcg --self-test` to verify the scheduler.");
        std::process::exit(1);
    }
}
