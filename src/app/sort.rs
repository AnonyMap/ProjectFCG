//! View ▸ Sort, as in Project: by up to three fields, ascending or
//! descending, keeping the outline structure (subtasks sorted under their
//! summary) or not, and optionally renumbering the tasks permanently.
//!
//! Sorting without renumbering only changes the order rows are shown in;
//! IDs stay. Sort by ID puts them back.

use super::columns::ColumnId;
use super::filters::{field_value, Value};
use crate::model::Project;
use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub struct SortSpec {
    /// (field, descending), most significant first; empty = by ID.
    pub keys: Vec<(ColumnId, bool)>,
    /// Sort subtasks under their summaries (Project's default).
    pub keep_outline: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        SortSpec { keys: vec![], keep_outline: true }
    }
}

/// View ▸ Sort's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortPreset {
    Id,
    StartDate,
    FinishDate,
    Priority,
    Cost,
}

impl SortPreset {
    pub const ALL: [SortPreset; 5] = [SortPreset::StartDate, SortPreset::FinishDate, SortPreset::Priority, SortPreset::Cost, SortPreset::Id];
    pub fn label(self) -> &'static str {
        match self {
            SortPreset::Id => "by ID",
            SortPreset::StartDate => "by Start Date",
            SortPreset::FinishDate => "by Finish Date",
            SortPreset::Priority => "by Priority",
            SortPreset::Cost => "by Cost",
        }
    }
    /// Project's built-in sorts: dates ascending, priority and cost
    /// highest first.
    pub fn spec(self) -> SortSpec {
        let keys = match self {
            SortPreset::Id => vec![],
            SortPreset::StartDate => vec![(ColumnId::Start, false)],
            SortPreset::FinishDate => vec![(ColumnId::Finish, false)],
            SortPreset::Priority => vec![(ColumnId::Priority, true)],
            SortPreset::Cost => vec![(ColumnId::Cost, true)],
        };
        SortSpec { keys, keep_outline: true }
    }
}

impl SortSpec {
    pub fn is_by_id(&self) -> bool {
        self.keys.is_empty()
    }

    /// "by Start Date", "by Cost, Name"…
    pub fn label(&self, p: &Project) -> String {
        if self.keys.is_empty() {
            return "by ID".into();
        }
        for pre in SortPreset::ALL {
            if pre.spec().keys == self.keys {
                return pre.label().into();
            }
        }
        format!("by {}", self.keys.iter().map(|(c, _)| c.title_in(p)).collect::<Vec<_>>().join(", "))
    }
}

/// Compare two values for sorting: empty values (no date, NaN) last in
/// either direction, text case-insensitively, dates to the minute.
fn cmp_values(a: &Value, b: &Value, desc: bool) -> Ordering {
    let dir = |o: Ordering| if desc { o.reverse() } else { o };
    match (a, b) {
        (Value::Date(None), Value::Date(None)) => Ordering::Equal,
        (Value::Date(None), Value::Date(Some(_))) => Ordering::Greater,
        (Value::Date(Some(_)), Value::Date(None)) => Ordering::Less,
        (Value::Date(Some(x)), Value::Date(Some(y))) => dir(x.cmp(y)),
        (Value::Num(x), Value::Num(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => dir(x.partial_cmp(y).unwrap_or(Ordering::Equal)),
        },
        (Value::Minutes(x), Value::Minutes(y)) => dir(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => dir(x.cmp(y)),
        (Value::Text(x), Value::Text(y)) => match (x.is_empty(), y.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => dir(x.to_lowercase().cmp(&y.to_lowercase()).then_with(|| x.cmp(y))),
        },
        _ => Ordering::Equal,
    }
}

/// Reorder `rows` (task rows, in outline order) by `spec`. Ties keep ID
/// order. With `keep_outline`, each summary's subtasks are sorted among
/// themselves and stay under it; the tree is rebuilt from outline levels
/// within `rows`, so filtered lists work too.
pub fn sort_rows(p: &Project, rows: &mut Vec<usize>, spec: &SortSpec) {
    if spec.keys.is_empty() || rows.len() < 2 {
        return;
    }
    // each row's keys, computed once
    let keys: std::collections::HashMap<usize, Vec<Value>> = rows.iter().map(|&r| (r, spec.keys.iter().map(|(c, _)| field_value(p, r, *c)).collect())).collect();
    let cmp = |a: &usize, b: &usize| -> Ordering {
        let (ka, kb) = (&keys[a], &keys[b]);
        for (k, (_, desc)) in spec.keys.iter().enumerate() {
            let o = cmp_values(&ka[k], &kb[k], *desc);
            if o != Ordering::Equal {
                return o;
            }
        }
        a.cmp(b)
    };
    if !spec.keep_outline {
        rows.sort_by(cmp);
        return;
    }
    // tree from outline levels: a row's parent is the nearest earlier row
    // with a lower level
    let n = rows.len();
    let mut children: Vec<Vec<usize>> = vec![vec![]; n + 1]; // index n = root
    let mut stack: Vec<usize> = vec![];
    for (k, &r) in rows.iter().enumerate() {
        let lvl = p.tasks[r].outline_level;
        while stack.last().is_some_and(|&s| p.tasks[rows[s]].outline_level >= lvl) {
            stack.pop();
        }
        children[stack.last().copied().unwrap_or(n)].push(k);
        stack.push(k);
    }
    for c in children.iter_mut() {
        c.sort_by(|&a, &b| cmp(&rows[a], &rows[b]));
    }
    let mut out = Vec::with_capacity(n);
    let mut todo: Vec<usize> = children[n].iter().rev().copied().collect();
    while let Some(k) = todo.pop() {
        out.push(rows[k]);
        todo.extend(children[k].iter().rev().copied());
    }
    *rows = out;
}

impl Project {
    /// Project ▸ Sort ▸ Permanently renumber tasks: put the tasks in this
    /// sorted order for good (outline kept: subtasks move with their
    /// summary). IDs change; unique IDs, links and assignments don't.
    pub fn renumber_sorted(&mut self, spec: &SortSpec) -> Result<(), String> {
        if spec.keys.is_empty() {
            return Ok(());
        }
        let mut order: Vec<usize> = (0..self.tasks.len()).collect();
        sort_rows(self, &mut order, &SortSpec { keys: spec.keys.clone(), keep_outline: true });
        let mut old = std::mem::take(&mut self.tasks).into_iter().map(Some).collect::<Vec<_>>();
        self.tasks = order.into_iter().map(|i| old[i].take().expect("a permutation")).collect();
        Ok(())
    }
}

/// Project ▸ Sort ▸ Sort By…
#[derive(Clone, Debug)]
pub struct SortDlg {
    /// Up to three keys; `None` = "(none)".
    pub keys: [(Option<ColumnId>, bool); 3],
    pub keep_outline: bool,
    pub renumber: bool,
}

impl From<&SortSpec> for SortDlg {
    fn from(s: &SortSpec) -> Self {
        let mut keys = [(None, false); 3];
        for (k, (c, d)) in s.keys.iter().take(3).enumerate() {
            keys[k] = (Some(*c), *d);
        }
        if keys[0].0.is_none() {
            keys[0].0 = Some(ColumnId::Start);
        }
        SortDlg { keys, keep_outline: s.keep_outline, renumber: false }
    }
}

impl SortDlg {
    pub fn spec(&self) -> SortSpec {
        SortSpec { keys: self.keys.iter().filter_map(|(c, d)| c.map(|c| (c, *d))).collect(), keep_outline: self.keep_outline || self.renumber }
    }
}

impl super::ProjectFcgApp {
    /// Apply the Sort dialog: sort the view, or renumber the tasks.
    pub fn apply_sort(&mut self, d: &SortDlg) {
        let spec = d.spec();
        if d.renumber {
            let s2 = spec.clone();
            if self.edit_project("Sort (renumber)", move |p| p.renumber_sorted(&s2)).is_some() {
                // the tasks are in that order now: back to ID order
                self.view.sort = SortSpec::default();
                self.set_status(format!("Tasks renumbered {}", spec.label(&self.project)));
            }
        } else {
            self.set_status(format!("Sorted {}", spec.label(&self.project)));
            self.view.sort = spec;
        }
    }
}

pub fn dialog(app: &mut super::ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.sort.take() else { return };
    let (mut ok, mut cancel, mut reset) = (false, false, false);
    let keys = super::dialogs::modal(ctx, "sort", "Sort", 460.0, |ui| {
        for (k, label) in ["Sort by", "Then by", "Then by"].iter().enumerate() {
            ui.label(egui::RichText::new(*label).strong());
            ui.horizontal(|ui| {
                let (col, desc) = &mut st.keys[k];
                let shown = col.map(|c| c.title_in(&app.project)).unwrap_or_else(|| "(none)".into());
                egui::ComboBox::from_id_salt(("sort_key", k)).selected_text(shown).width(220.0).height(360.0).show_ui(ui, |ui| {
                    if k > 0 && ui.selectable_label(col.is_none(), "(none)").clicked() {
                        *col = None;
                    }
                    for c in ColumnId::filterable() {
                        if ui.selectable_label(*col == Some(c), c.title_in(&app.project)).clicked() {
                            *col = Some(c);
                        }
                    }
                });
                ui.radio_value(desc, false, "Ascending");
                ui.radio_value(desc, true, "Descending");
            });
            ui.add_space(4.0);
        }
        ui.separator();
        ui.checkbox(&mut st.renumber, "Permanently renumber tasks");
        ui.add_enabled(!st.renumber, egui::Checkbox::new(&mut st.keep_outline, "Keep outline structure"))
            .on_hover_text("Sort subtasks among themselves, under their summary");
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if ui.button("Reset").on_hover_text("Back to ID order").clicked() {
                    reset = true;
                }
                if ui.button(egui::RichText::new("Sort").strong()).clicked() {
                    ok = true;
                }
            });
        });
    });
    if cancel || keys.esc {
        return;
    }
    if reset {
        app.view.sort = SortSpec::default();
        app.set_status("Sorted by ID");
        return;
    }
    if ok || keys.enter {
        app.apply_sort(&st);
        return;
    }
    app.dialogs.sort = Some(st);
}
