//! View dialogs: Date Range and Using Resource filters, the filter
//! definition (New / Edit Filter), and a custom Group By field.

use super::columns::ColumnId;
use super::commands::{Command, Filter};
use super::dialogs::{date_field, modal, ok_cancel};
use super::filters::GroupBy;
use super::theme::*;
use super::ProjectFcgApp;
use crate::model::Project;
use crate::views::{Condition, CustomFilter, Test};
use egui::RichText;

#[derive(Clone, Debug)]
pub struct DateRangeDlg {
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}

impl DateRangeDlg {
    pub fn new(app: &ProjectFcgApp) -> Self {
        let (s, f) = match app.filter {
            Filter::DateRange(a, b) => (a, b),
            _ => {
                let (s, f) = app.project_span();
                (s.date(), f.date())
            }
        };
        let df = |d: chrono::NaiveDate| crate::model::fmt_date(d.and_time(chrono::NaiveTime::MIN));
        DateRangeDlg { from: df(s), to: df(f), error: None }
    }
}

#[derive(Clone, Debug)]
pub struct UsingResourceDlg {
    pub resource: Option<u32>,
}

impl UsingResourceDlg {
    pub fn new(app: &ProjectFcgApp) -> Self {
        let resource = match app.filter {
            Filter::UsingResource(r) => Some(r),
            _ => app.project.resources.first().map(|r| r.uid),
        };
        UsingResourceDlg { resource }
    }
}

#[derive(Clone, Debug)]
pub struct FilterDefDlg {
    /// Which saved filter is being edited (`None` = a new one).
    pub index: Option<usize>,
    pub filter: CustomFilter,
    pub error: Option<String>,
}

impl FilterDefDlg {
    pub fn new(index: Option<usize>, p: &Project) -> Self {
        let filter = match index.and_then(|k| p.views.filters.get(k)) {
            Some(f) => f.clone(),
            None => {
                let mut n = p.views.filters.len() + 1;
                while p.views.filters.iter().any(|f| f.name == format!("Filter {n}")) {
                    n += 1;
                }
                CustomFilter { name: format!("Filter {n}"), conditions: vec![Condition::default()], related_summaries: false }
            }
        };
        FilterDefDlg { index, filter, error: None }
    }
}

#[derive(Clone, Debug)]
pub struct CustomGroupDlg {
    pub field: ColumnId,
}

impl CustomGroupDlg {
    pub fn new(g: GroupBy) -> Self {
        CustomGroupDlg { field: if let GroupBy::Field(c) = g { c } else { ColumnId::ResourceNames } }
    }
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    date_range(app, ctx);
    using_resource(app, ctx);
    filter_def(app, ctx);
    custom_group(app, ctx);
}

fn field_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, col: &mut ColumnId, width: f32) {
    egui::ComboBox::from_id_salt(id).selected_text(col.title()).width(width).height(360.0).show_ui(ui, |ui| {
        for c in ColumnId::filterable() {
            ui.selectable_value(col, c, c.title());
        }
    });
}

fn date_range(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.date_range.take() else { return };
    let fallback = app.project.start.date();
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "date_range", "Date Range", 380.0, |ui| {
        ui.label("Show tasks under way at any time between:");
        ui.add_space(4.0);
        egui::Grid::new("dr").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("From:");
            date_field(ui, &mut st.from, "dr_from", fallback);
            ui.end_row();
            ui.label("To:");
            date_field(ui, &mut st.to, "dr_to", fallback);
            ui.end_row();
        });
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.date_range = Some(st);
        return;
    }
    let t0 = chrono::NaiveTime::MIN;
    match (crate::model::parse_date(&st.from, t0), crate::model::parse_date(&st.to, t0)) {
        (Ok(a), Ok(b)) if a.date() <= b.date() => app.execute(Command::SetFilter(Filter::DateRange(a.date(), b.date())), ctx),
        (Ok(_), Ok(_)) => {
            st.error = Some("The range ends before it starts.".into());
            app.dialogs.date_range = Some(st);
        }
        _ => {
            st.error = Some("Enter two valid dates.".into());
            app.dialogs.date_range = Some(st);
        }
    }
}

fn using_resource(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.using_resource.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let res: Vec<(u32, String)> = app.project.resources.iter().map(|r| (r.uid, r.name.clone())).collect();
    let keys = modal(ctx, "using_resource", "Using Resource", 360.0, |ui| {
        if res.is_empty() {
            ui.label("The project has no resources yet (View ▸ Resource Sheet).");
        } else {
            ui.horizontal(|ui| {
                ui.label("Show tasks using:");
                let cur = st.resource.and_then(|u| res.iter().find(|r| r.0 == u)).map(|r| r.1.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("ur_res").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                    for (u, n) in &res {
                        ui.selectable_value(&mut st.resource, Some(*u), n);
                    }
                });
            });
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.using_resource = Some(st);
        return;
    }
    if let Some(r) = st.resource {
        app.execute(Command::SetFilter(Filter::UsingResource(r)), ctx);
    }
}

fn filter_def(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.filter_def.take() else { return };
    let (mut ok, mut cancel, mut delete) = (false, false, false);
    let title = if st.index.is_some() { "Filter Definition" } else { "New Filter" };
    let keys = modal(ctx, "filter_def", title, 720.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut st.filter.name).desired_width(260.0));
        });
        ui.add_space(6.0);
        let mut remove = None;
        egui::Grid::new("fd_rows").num_columns(6).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("And/Or").strong());
            ui.label(RichText::new("Field Name").strong());
            ui.label(RichText::new("Test").strong());
            ui.label(RichText::new("Value(s)").strong());
            ui.label("");
            ui.label("");
            ui.end_row();
            let n = st.filter.conditions.len();
            for (k, c) in st.filter.conditions.iter_mut().enumerate() {
                if k == 0 {
                    ui.label("");
                } else {
                    egui::ComboBox::from_id_salt(("fd_join", k)).selected_text(if c.or { "Or" } else { "And" }).width(56.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.or, false, "And");
                        ui.selectable_value(&mut c.or, true, "Or");
                    });
                }
                let mut col = ColumnId::from_key(&c.field).unwrap_or(ColumnId::Name);
                field_combo(ui, ("fd_field", k), &mut col, 170.0);
                c.field = col.key();
                egui::ComboBox::from_id_salt(("fd_test", k)).selected_text(c.test.label()).width(170.0).show_ui(ui, |ui| {
                    for t in Test::ALL {
                        ui.selectable_value(&mut c.test, t, t.label());
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut c.value).desired_width(110.0));
                if c.test.two_values() {
                    ui.horizontal(|ui| {
                        ui.label("and");
                        ui.add(egui::TextEdit::singleline(&mut c.value2).desired_width(90.0));
                    });
                } else {
                    ui.label("");
                }
                if n > 1 && ui.small_button("✖").on_hover_text("Remove this condition").clicked() {
                    remove = Some(k);
                }
                ui.end_row();
            }
        });
        if let Some(k) = remove {
            st.filter.conditions.remove(k);
            if let Some(c) = st.filter.conditions.first_mut() {
                c.or = false;
            }
        }
        if ui.button("+ Add condition").clicked() {
            st.filter.conditions.push(Condition::default());
        }
        ui.add_space(4.0);
        ui.checkbox(&mut st.filter.related_summaries, "Show related summary rows");
        ui.label(
            RichText::new("\"And\" is evaluated before \"Or\". Dates compare by day; type NA for no date. Durations take units (5d, 3w, 8h).")
                .size(11.0)
                .color(pal().text_dim),
        );
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        ui.horizontal(|ui| {
            if st.index.is_some() && ui.button("Delete Filter").clicked() {
                delete = true;
            }
        });
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if delete {
        if let Some(k) = st.index {
            app.delete_custom_filter(k);
        }
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.filter_def = Some(st);
        return;
    }
    let name = st.filter.name.trim().to_string();
    let clash = app.project.views.filters.iter().enumerate().any(|(k, f)| Some(k) != st.index && f.name.eq_ignore_ascii_case(&name));
    let err = if name.is_empty() {
        Some("The filter needs a name.".to_string())
    } else if clash {
        Some(format!("There's already a filter called \"{name}\"."))
    } else {
        st.filter.conditions.iter().find_map(|c| super::filters::validate(&app.project, c).err())
    };
    if let Some(e) = err {
        st.error = Some(e);
        app.dialogs.filter_def = Some(st);
        return;
    }
    st.filter.name = name;
    let f = st.filter.clone();
    let index = st.index;
    let label = if index.is_some() { "Edit Filter" } else { "New Filter" };
    let saved = app.edit_project(label, |p| {
        Ok(match index {
            Some(k) if k < p.views.filters.len() => {
                p.views.filters[k] = f;
                k
            }
            _ => {
                p.views.filters.push(f);
                p.views.filters.len() - 1
            }
        })
    });
    if let Some(k) = saved {
        app.execute(Command::SetFilter(Filter::Custom(k)), ctx);
    }
}

fn custom_group(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.custom_group.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "custom_group", "Group By a Field", 380.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Group by:");
            field_combo(ui, "cg_field", &mut st.field, 200.0);
        });
        ui.label(RichText::new("Dates are grouped by week.").size(11.0).color(pal().text_dim));
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.custom_group = Some(st);
        return;
    }
    app.execute(Command::SetGroup(GroupBy::Field(st.field)), ctx);
}

impl ProjectFcgApp {
    /// Remove a saved filter; filters after it shift down one place.
    pub fn delete_custom_filter(&mut self, k: usize) {
        if self.edit_project("Delete Filter", |p| {
            if k >= p.views.filters.len() {
                return Err("No such filter.".into());
            }
            p.views.filters.remove(k);
            Ok(())
        })
        .is_none()
        {
            return;
        }
        let shift = |f: Filter| match f {
            Filter::Custom(j) if j == k => Filter::AllTasks,
            Filter::Custom(j) if j > k => Filter::Custom(j - 1),
            f => f,
        };
        self.filter = shift(self.filter);
        self.view.highlight = shift(self.view.highlight);
    }

    /// A saved filter that was removed (by undo, say) stops applying.
    pub fn check_filters(&mut self) {
        let n = self.project.views.filters.len();
        for f in [&mut self.filter, &mut self.view.highlight] {
            if matches!(*f, Filter::Custom(k) if k >= n) {
                *f = Filter::AllTasks;
            }
            if let Filter::UsingResource(r) = *f {
                if !self.project.resources.iter().any(|x| x.uid == r) {
                    *f = Filter::AllTasks;
                }
            }
        }
    }
}
