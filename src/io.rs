//! File I/O: native `.pfcg` JSON documents (`.ganttry` and `.gantry` files
//! from before the renames still open) and CSV export.

use crate::model::{fmt_datetime, Project};
use crate::schedule::schedule;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXTENSION: &str = "pfcg";
/// Extensions from before the app was renamed — "Gantry" (0.1–0.2) and
/// "Ganttry" (0.3) — still opened.
pub const LEGACY_EXTENSIONS: [&str; 2] = ["ganttry", "gantry"];
const FORMAT: &str = "projectfcg-project";
const LEGACY_FORMATS: [&str; 2] = ["ganttry-project", "gantry-project"];

/// True if `path` looks like a project file (current or an older name).
pub fn is_project_file(name: &str) -> bool {
    let n = name.to_lowercase();
    std::iter::once(EXTENSION).chain(LEGACY_EXTENSIONS).any(|e| n.ends_with(&format!(".{e}")))
}
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct FileEnvelope {
    format: String,
    version: u32,
    project: Project,
}

pub fn to_json(p: &Project) -> Result<String, String> {
    let env = FileEnvelope {
        format: FORMAT.into(),
        version: VERSION,
        project: p.clone(),
    };
    serde_json::to_string_pretty(&env).map_err(|e| e.to_string())
}

pub fn from_json(text: &str) -> Result<Project, String> {
    let env: FileEnvelope =
        serde_json::from_str(text).map_err(|e| format!("Not a valid ProjectFCG file: {e}"))?;
    if env.format != FORMAT && !LEGACY_FORMATS.contains(&env.format.as_str()) {
        return Err(format!("Unrecognised file format \"{}\"", env.format));
    }
    if env.version > VERSION {
        return Err(format!(
            "This file was saved by a newer version of ProjectFCG (format v{}).",
            env.version
        ));
    }
    let mut p = env.project;
    // Repair next_uid if a file was hand-edited.
    let max_uid = p.tasks.iter().map(|t| t.uid).max().unwrap_or(0);
    if p.next_uid <= max_uid {
        p.next_uid = max_uid + 1;
    }
    p.repair_board();
    p.views.repair();
    schedule(&mut p);
    Ok(p)
}

pub fn save(p: &Project, path: &Path) -> Result<(), String> {
    let json = to_json(p)?;
    // write atomically so a crash mid-save never truncates the user's file
    let tmp = path.with_extension("pfcg.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't save {}: {e}", path.display()))
}

pub fn load(path: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
    from_json(&text)
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export the Entry table (plus slack/critical) as CSV.
pub fn to_csv(p: &Project) -> String {
    let mut out = String::from(
        "ID,Outline,Mode,Name,Duration,Start,Finish,Predecessors,% Complete,Total Slack,Critical\n",
    );
    let ds = &p.settings.durations;
    for (i, t) in p.tasks.iter().enumerate() {
        let dur = if t.calc.is_summary {
            p.format_duration_minutes(t.calc.summary_duration, crate::duration::DurUnit::Day)
        } else {
            t.duration.format(ds)
        };
        let row = [
            (i + 1).to_string(),
            t.calc.outline_number.clone(),
            format!("{:?}", t.mode),
            format!("{}{}", "  ".repeat(t.outline_level as usize - 1), t.name),
            dur,
            t.calc.start.map(fmt_datetime).unwrap_or_default(),
            t.calc.finish.map(fmt_datetime).unwrap_or_default(),
            p.predecessors_text(i),
            format!("{}%", crate::duration::fmt_num(t.calc.percent_complete)),
            p.format_duration_minutes(t.calc.total_slack, crate::duration::DurUnit::Day),
            if t.calc.critical { "Yes" } else { "No" }.to_string(),
        ];
        out.push_str(
            &row.iter()
                .map(|c| csv_escape(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}
