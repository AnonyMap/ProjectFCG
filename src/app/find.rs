//! Find (Ctrl+F) and Replace (Ctrl+H), as in Project: look in any field
//! with any test, search down or up, match case; replace in one task or
//! all of them (one undoable step).
//!
//! The search follows the view: the current filter and sort, with
//! collapsed summaries opened when a match is inside them.

use super::columns::{cell_text, edit_text, set_cell, ColumnId, EditKind};
use super::filters::{Compiled, FilterEval};
use super::ProjectFcgApp;
use crate::model::Project;
use crate::views::{Condition, Test};

#[derive(Clone, Debug)]
pub struct FindOpts {
    pub field: ColumnId,
    pub test: Test,
    pub match_case: bool,
    /// Search toward lower IDs.
    pub up: bool,
    /// The dialog shows Replace with / Replace / Replace All.
    pub replace_mode: bool,
    pub replace_text: String,
}

impl Default for FindOpts {
    fn default() -> Self {
        FindOpts { field: ColumnId::Name, test: Test::Contains, match_case: false, up: false, replace_mode: false, replace_text: String::new() }
    }
}

/// The tests Find offers ("is within" needs two values: not here).
pub const FIND_TESTS: [Test; 8] = [Test::Contains, Test::NotContains, Test::Equals, Test::NotEquals, Test::Greater, Test::GreaterOrEqual, Test::Less, Test::LessOrEqual];

/// Does task `row` match? `compiled` is the condition for non-text tests.
fn row_matches(p: &Project, row: usize, needle: &str, o: &FindOpts, compiled: &Option<Compiled>) -> bool {
    match o.test {
        Test::Contains | Test::NotContains => {
            let text = cell_text(p, row, o.field);
            let has = if o.match_case { text.contains(needle) } else { text.to_lowercase().contains(&needle.to_lowercase()) };
            has == (o.test == Test::Contains)
        }
        _ => compiled.as_ref().is_some_and(|c| c.matches(p, row)),
    }
}

/// Replace every occurrence of `needle` in `hay` (ignoring case unless
/// `case`), keeping the rest of the text as it was.
pub fn replace_text(hay: &str, needle: &str, with: &str, case: bool) -> String {
    if needle.is_empty() {
        return hay.to_string();
    }
    if case {
        return hay.replace(needle, with);
    }
    let h: Vec<char> = hay.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    let low = |c: char| c.to_lowercase().collect::<String>();
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    while i < h.len() {
        if i + n.len() <= h.len() && h[i..i + n.len()].iter().zip(&n).all(|(a, b)| low(*a) == low(*b)) {
            out.push_str(with);
            i += n.len();
        } else {
            out.push(h[i]);
            i += 1;
        }
    }
    out
}

impl ProjectFcgApp {
    /// Rows in the order Find walks them: the current filter and sort,
    /// collapsed or not.
    fn find_order(&self) -> Vec<usize> {
        let p = &self.project;
        let eval = FilterEval::new(p, self.filter);
        let mut rows: Vec<usize> = (0..p.tasks.len()).filter(|&i| eval.matches(i)).collect();
        super::sort::sort_rows(p, &mut rows, &self.view.sort);
        rows
    }

    fn find_compiled(&self) -> Option<Compiled> {
        let o = &self.dialogs.find;
        (!matches!(o.test, Test::Contains | Test::NotContains)).then(|| {
            Compiled::new(&self.project, &Condition { field: o.field.key(), test: o.test, value: self.dialogs.find_text.clone(), ..Default::default() })
        })
    }

    /// Does the active task match what Find is looking for?
    fn active_matches(&self) -> bool {
        let Some(row) = self.active_index() else { return false };
        row_matches(&self.project, row, &self.dialogs.find_text, &self.dialogs.find, &self.find_compiled())
    }

    /// Find Next (Shift+F4): the next matching task after the active one.
    pub fn find_next(&mut self) {
        let o = self.dialogs.find.clone();
        if self.dialogs.find_text.is_empty() && matches!(o.test, Test::Contains | Test::Equals) {
            self.dialogs.find_open = true;
            return;
        }
        if !matches!(o.test, Test::Contains | Test::NotContains) {
            let c = Condition { field: o.field.key(), test: o.test, value: self.dialogs.find_text.clone(), ..Default::default() };
            if let Err(e) = super::filters::validate(&self.project, &c) {
                self.error(e);
                return;
            }
        }
        let order = self.find_order();
        let n = order.len();
        if n == 0 {
            self.set_status("No tasks to search");
            return;
        }
        let compiled = self.find_compiled();
        let here = self.active_index().and_then(|r| order.iter().position(|&x| x == r));
        for k in 1..=n {
            let pos = match (here, o.up) {
                (Some(h), false) => (h + k) % n,
                (Some(h), true) => (h + n - k % n) % n,
                (None, false) => (k - 1) % n,
                (None, true) => n - k,
            };
            let i = order[pos];
            if row_matches(&self.project, i, &self.dialogs.find_text, &o, &compiled) {
                for a in self.project.ancestors_of(i) {
                    self.project.tasks[a].collapsed = false;
                }
                self.select_index(i);
                if let Some(c) = self.columns.iter().position(|c| c.id == o.field) {
                    self.active_col = c;
                }
                self.set_status(format!("Found at task {}", i + 1));
                return;
            }
        }
        self.set_status(format!("\"{}\" not found", self.dialogs.find_text));
    }

    /// The text a replacement leaves in `row`'s field.
    fn replaced(&self, p: &Project, row: usize) -> String {
        let o = &self.dialogs.find;
        match o.test {
            Test::Contains => replace_text(&edit_text(p, row, o.field), &self.dialogs.find_text, &o.replace_text, o.match_case),
            _ => o.replace_text.clone(),
        }
    }

    fn can_replace(&mut self) -> bool {
        let o = &self.dialogs.find;
        if o.field.edit_kind() == EditKind::ReadOnly {
            let msg = format!("{} can't be edited, so nothing can be replaced in it.", o.field.title_in(&self.project));
            self.error(msg);
            return false;
        }
        if o.test == Test::NotContains {
            self.error("Replace needs something to find: use contains, equals or a comparison.");
            return false;
        }
        true
    }

    /// Replace: in the active task if it matches, then find the next one.
    pub fn replace_one(&mut self) {
        if !self.can_replace() {
            return;
        }
        if self.active_matches() {
            let row = self.active_index().expect("matched");
            let new = self.replaced(&self.project, row);
            let col = self.dialogs.find.field;
            if self.edit_project("Replace", move |p| set_cell(p, row, col, &new)).is_none() {
                return;
            }
        }
        self.find_next();
    }

    /// Replace All: every matching task, as one step. Returns how many.
    pub fn replace_all(&mut self) -> usize {
        if !self.can_replace() {
            return 0;
        }
        let o = self.dialogs.find.clone();
        let compiled = self.find_compiled();
        let rows: Vec<usize> = self.find_order().into_iter().filter(|&i| row_matches(&self.project, i, &self.dialogs.find_text, &o, &compiled)).collect();
        if rows.is_empty() {
            self.set_status(format!("\"{}\" not found", self.dialogs.find_text));
            return 0;
        }
        let texts: Vec<(usize, String)> = rows.iter().map(|&r| (r, self.replaced(&self.project, r))).collect();
        let col = o.field;
        let n = texts.len();
        let done = self.edit_project("Replace All", move |p| {
            for (r, t) in &texts {
                set_cell(p, *r, col, t).map_err(|e| format!("Task {}: {e}", r + 1))?;
            }
            Ok(())
        });
        if done.is_some() {
            self.set_status(format!("Replaced in {n} task(s)"));
            n
        } else {
            0
        }
    }
}

/// The Find / Replace dialog.
pub fn dialog(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    use egui::RichText;
    if !app.dialogs.find_open {
        return;
    }
    let mut text = std::mem::take(&mut app.dialogs.find_text);
    let mut o = app.dialogs.find.clone();
    let (mut next, mut close, mut one, mut all) = (false, false, false, false);
    let title = if o.replace_mode { "Replace" } else { "Find" };
    let keys = super::dialogs::modal(ctx, "find", title, 440.0, |ui| {
        egui::Grid::new("find_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Find what:");
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0));
            if !o.replace_mode {
                r.request_focus();
            }
            ui.end_row();
            if o.replace_mode {
                ui.label("Replace with:");
                ui.add(egui::TextEdit::singleline(&mut o.replace_text).desired_width(280.0));
                ui.end_row();
            }
            ui.label("Look in field:");
            super::view_dialogs::field_combo(ui, "find_field", &mut o.field, 200.0, &app.project);
            ui.end_row();
            ui.label("Test:");
            egui::ComboBox::from_id_salt("find_test").selected_text(o.test.label()).width(200.0).show_ui(ui, |ui| {
                for t in FIND_TESTS {
                    ui.selectable_value(&mut o.test, t, t.label());
                }
            });
            ui.end_row();
            ui.label("Search:");
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.up, false, "Down");
                ui.radio_value(&mut o.up, true, "Up");
                ui.add_space(16.0);
                ui.checkbox(&mut o.match_case, "Match case");
            });
            ui.end_row();
        });
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                close = true;
            }
            if o.replace_mode {
                if ui.button("Replace All").clicked() {
                    all = true;
                }
                if ui.button("Replace").clicked() {
                    one = true;
                }
            } else if ui.button("Replace…").clicked() {
                o.replace_mode = true;
            }
            if ui.button(RichText::new("Find Next").strong()).clicked() {
                next = true;
            }
        });
    });
    app.dialogs.find_text = text;
    app.dialogs.find = o;
    if close || keys.esc {
        app.dialogs.find_open = false;
        return;
    }
    if all {
        app.replace_all();
    } else if one {
        app.replace_one();
    } else if next || keys.enter {
        app.find_next();
    }
}
