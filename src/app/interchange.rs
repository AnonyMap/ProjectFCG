//! Import/export glue: Jira CSV (board) and Microsoft Project XML.

use super::theme::*;
use super::ProjectFcgApp;
use egui::RichText;
use std::path::PathBuf;

/// Jira CSV import options, shown after a file is picked.
#[derive(Clone, Debug)]
pub struct JiraImportDlg {
    pub path: PathBuf,
    pub text: String,
    pub issues: usize,
    pub epics: usize,
    pub sprints: Vec<String>,
    pub create_sprints: bool,
    pub create_people: bool,
}

impl ProjectFcgApp {
    pub fn start_jira_import(&mut self, path: PathBuf) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return self.error(format!("Couldn't open {}: {e}", path.display())),
        };
        // Jira writes UTF-8; Excel round-trips may leave Windows-1252
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
        };
        // a quick look so the dialog can say what's coming
        let rows = crate::jira::parse_csv(&text);
        let Some(head) = rows.first() else {
            return self.error("The file is empty.");
        };
        let idx = |names: &[&str]| -> Vec<usize> {
            head.iter().enumerate().filter(|(_, h)| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n))).map(|(i, _)| i).collect()
        };
        if idx(&["Summary", "Title"]).is_empty() {
            return self.error("This doesn't look like a Jira CSV export: there's no \"Summary\" column.");
        }
        let types = idx(&["Issue Type", "Work type", "Type"]);
        let sprint_cols = idx(&["Sprint"]);
        let body = &rows[1..];
        let epics = body.iter().filter(|r| types.iter().any(|&i| r.get(i).is_some_and(|v| v.trim().eq_ignore_ascii_case("epic")))).count();
        let mut sprints: Vec<String> = vec![];
        for r in body {
            if let Some(v) = sprint_cols.iter().rev().filter_map(|&i| r.get(i)).find(|v| !v.trim().is_empty()) {
                let v = v.trim().to_string();
                if !sprints.contains(&v) {
                    sprints.push(v);
                }
            }
        }
        self.dialogs.jira_import = Some(JiraImportDlg { path, text, issues: body.len(), epics, sprints, create_sprints: true, create_people: false });
    }

    pub fn open_xml(&mut self, path: PathBuf) {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return self.error(format!("Couldn't open {}: {e}", path.display())),
        };
        match crate::mspdi::from_xml(&text) {
            Ok((p, warnings)) => {
                // an imported file isn't a .pfcg: Save asks for a name
                self.replace_project(p, None);
                self.dirty = true;
                self.set_status(format!("Imported {}", path.display()));
                if !warnings.is_empty() {
                    let list: String = warnings.iter().map(|w| format!("• {w}\n")).collect();
                    self.dialogs.message = Some(("Imported from Project XML".into(), format!("The project was imported. Some things didn't carry over exactly:\n\n{list}")));
                }
            }
            Err(e) => self.error(e),
        }
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    jira_import(app, ctx);
}

fn jira_import(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.jira_import.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let existing: Vec<String> = st.sprints.iter().filter(|s| app.project.tasks.iter().any(|t| t.is_sprint && t.name.trim().eq_ignore_ascii_case(s.trim()))).cloned().collect();
    let missing = st.sprints.len() - existing.len();
    let keys = super::dialogs::modal(ctx, "jira_import", "Import Jira CSV", 470.0, |ui| {
        let name = st.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.label(RichText::new(name).strong());
        ui.label(format!(
            "{} issue{}{} — added to this project's board (Undo removes them).",
            st.issues,
            if st.issues == 1 { "" } else { "s" },
            if st.epics > 0 { format!(", {} epic{}", st.epics, if st.epics == 1 { "" } else { "s" }) } else { String::new() }
        ));
        ui.add_space(6.0);
        if !st.sprints.is_empty() {
            ui.label(format!("Sprints in the file: {}. {} already on the schedule.", st.sprints.len(), existing.len()));
            ui.add_enabled(missing > 0, egui::Checkbox::new(&mut st.create_sprints, format!("Add the {missing} missing sprint{} to the schedule", if missing == 1 { "" } else { "s" })))
                .on_hover_text("New sprints follow the last one, with the length set under Board ▸ New sprints. Unchecked, their issues go to the backlog.");
        }
        ui.checkbox(&mut st.create_people, "Add assignees missing from the Resource Sheet")
            .on_hover_text("Otherwise only assignees whose name or e-mail matches a resource are kept.");
        ui.add_space(4.0);
        ui.label(
            RichText::new("Columns read: Summary, Issue key/id, Issue Type, Status, Priority, Assignee, Labels, Sprint, Story Points / Story point estimate, Parent, Epic Link, Description, Created, Resolved and issue links (Blocks, Relates, Duplicate, Cloners).")
                .size(11.0)
                .color(pal().text_dim),
        );
        let (o, c) = super::dialogs::ok_cancel(ui);
        ok = o;
        cancel = c;
    });
    if keys.esc || cancel {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.jira_import = Some(st);
        return;
    }
    let opts = crate::jira::ImportOptions { create_sprints: st.create_sprints, create_people: st.create_people };
    let text = std::mem::take(&mut st.text);
    if let Some(rep) = app.edit_project("Import Jira CSV", |p| crate::jira::import_csv(p, &text, &opts)) {
        app.view.main = super::MainView::Board;
        app.tab = super::RibbonTab::Board;
        app.set_status(rep.summary());
        let mut msg = rep.summary() + ".";
        if !rep.sprints_created.is_empty() {
            msg += &format!("\n\nNew sprints on the schedule: {}.", rep.sprints_created.join(", "));
        }
        if !rep.people_created.is_empty() {
            msg += &format!("\n\nNew resources: {}.", rep.people_created.join(", "));
        }
        if !rep.warnings.is_empty() {
            msg += "\n";
            for w in &rep.warnings {
                msg += &format!("\n• {w}");
            }
        }
        app.dialogs.message = Some(("Jira CSV imported".into(), msg));
    }
}
