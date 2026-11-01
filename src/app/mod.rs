//! The desktop application (egui/eframe).

pub mod bars;
pub mod board;
pub mod columns;
pub mod commands;
pub mod datepicker;
pub mod dialogs;
pub mod filebrowser;
pub mod filters;
pub mod format_dialogs;
pub mod gantt;
pub mod inspector;
pub mod interchange;
pub mod sprint_dialogs;
pub mod prefs;
pub mod release_dialogs;
pub mod resource_dialogs;
pub mod resource_sheet;
pub mod usage;
pub mod ribbon;
pub mod sample;
pub mod system_theme;
pub mod table;
pub mod theme;
pub mod timeline;
pub mod tracking_dialogs;
pub mod view_dialogs;

use crate::history::History;
use crate::model::{fmt_date, Project, Task, TaskMode};
use crate::schedule::{refresh_structure, schedule};
use chrono::{NaiveDate, NaiveDateTime};
use columns::ColumnId;
use commands::{Command, Filter, Timescale};
use std::path::PathBuf;

pub const APP_NAME: &str = "ProjectFCG";
/// Height of the ribbon's group row (below the tab strip).
pub const GROUP_ROW_H: f32 = 98.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowRef {
    Task(u32),
    /// The first empty row below the last task (typing here adds a task).
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    ProjectSummary,
    Task(usize),
    /// A Group By header (index into `ProjectFcgApp::groups`).
    Group(usize),
}

#[derive(Clone, Debug)]
pub struct ColumnState {
    pub id: ColumnId,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub struct CellEdit {
    pub row: RowRef,
    pub col: usize,
    pub text: String,
    pub focus_requested: bool,
    pub select_all: bool,
    pub picker_open: bool,
}

/// Copy Picture: the screenshot is asked for, arrives a frame later, and
/// is encoded while the save dialog is open.
#[derive(Clone, Debug, Default)]
pub enum Picture {
    #[default]
    None,
    /// Filled in by the screenshot callback.
    Requested(std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>),
    /// PNG bytes waiting for a file name.
    Ready(Vec<u8>),
}

/// Encode an image as PNG.
pub fn encode_png(img: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    {
        let mut enc = png::Encoder::new(&mut out, img.size[0] as u32, img.size[1] as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        w.write_image_data(&bytes).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonTab {
    File,
    Task,
    Resource,
    Project,
    View,
    Format,
    Board,
    Help,
}

#[derive(Clone, Debug)]
pub struct ViewOptions {
    pub summary_rows: bool,
    pub project_summary: bool,
    pub outline_number: bool,
    pub timeline: bool,
    /// Format ▸ Task Path selections.
    pub path_preds: bool,
    pub path_driving: bool,
    pub path_succs: bool,
    pub path_driven: bool,
    /// View ▸ Highlight (rows matching are shaded in the table).
    pub highlight: Filter,
    /// Task ▸ Inspect: the Task Inspector pane.
    pub inspector: bool,
    pub main: MainView,
    /// View ▸ Filter ▸ Show Related Summary Rows.
    pub related_summaries: bool,
    /// Group By headers the user collapsed (by label).
    pub collapsed_groups: std::collections::HashSet<String>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            summary_rows: true,
            project_summary: false,
            outline_number: false,
            timeline: true,
            path_preds: false,
            path_driving: false,
            path_succs: false,
            path_driven: false,
            highlight: Filter::AllTasks,
            inspector: false,
            main: MainView::Gantt,
            related_summaries: false,
            collapsed_groups: Default::default(),
        }
    }
}

impl ViewOptions {
    pub fn any_path(&self) -> bool {
        self.path_preds || self.path_driving || self.path_succs || self.path_driven
    }
}

/// Does task `i` pass a filter? (For many rows, use `filters::FilterEval`.)
pub fn filter_matches(p: &Project, i: usize, f: Filter) -> bool {
    filters::FilterEval::new(p, f).matches(i)
}

/// The main view (View tab ▸ Task Views / Resource Views).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MainView {
    #[default]
    Gantt,
    TaskUsage,
    ResourceSheet,
    ResourceUsage,
    Board,
}

impl MainView {
    pub fn label(&self) -> &'static str {
        match self {
            MainView::Gantt => "Gantt Chart",
            MainView::TaskUsage => "Task Usage",
            MainView::ResourceSheet => "Resource Sheet",
            MainView::ResourceUsage => "Resource Usage",
            MainView::Board => "Board",
        }
    }
}

/// Horizontal position/zoom of the Gantt chart.
#[derive(Clone, Copy, Debug)]
pub struct GanttView {
    /// Days since EPOCH at the left edge.
    pub left_days: f64,
    pub ppd: f32,
}

pub fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()
}
pub fn to_days(dt: NaiveDateTime) -> f64 {
    (dt - epoch()).num_seconds() as f64 / 86400.0
}
pub fn from_days(d: f64) -> NaiveDateTime {
    epoch() + chrono::Duration::seconds((d * 86400.0).round() as i64)
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    New,
    NewSample,
    NewAgileSample,
    Open,
    OpenPath(PathBuf),
    Quit,
}

pub struct ProjectFcgApp {
    pub project: Project,
    pub history: History,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub auto_calc: bool,
    pub needs_calc: bool,

    pub columns: Vec<ColumnState>,
    pub table_width: f32,
    pub scroll_y: f32,
    pub table_scroll_x: f32,
    pub gantt: GanttView,

    pub active_row: RowRef,
    pub active_col: usize,
    pub selected: Vec<u32>,
    pub anchor: Option<u32>,
    pub edit: Option<CellEdit>,

    pub tab: RibbonTab,
    pub view: ViewOptions,
    /// View ▸ Table: which built-in table the sheet shows.
    pub table: columns::Table,
    pub filter: Filter,
    /// View ▸ Group by.
    pub group: filters::GroupBy,
    /// Group headers of the last `display_rows` (rollups for painting).
    pub groups: std::cell::RefCell<Vec<filters::GroupRow>>,
    pub status: String,
    pub status_error: bool,
    pub dialogs: dialogs::Dialogs,
    pub gantt_state: gantt::GanttState,
    pub clipboard: Vec<Task>,
    pub ensure_visible: Option<usize>,
    /// Column the table last scrolled into view; when `active_col`
    /// differs, the table scrolls horizontally to show it.
    pub shown_col: usize,
    pub last_title: String,
    pub allow_close: bool,
    pub file_browser: Option<filebrowser::FileBrowser>,
    pub pending: Option<PendingAction>,
    /// Action to run at the start of next frame (e.g. quit after Save As).
    pub deferred_action: Option<PendingAction>,
    pub splitter_drag: bool,
    pub use_portal: bool,
    /// Rows area height from last frame (for page up/down and scrolling).
    pub rows_view_h: f32,
    pub gantt_view_w: f32,
    pub themed: bool,
    /// (accent, dark) last pushed into egui's visuals.
    pub applied_theme: Option<(theme::Accent, bool)>,
    /// Layout of the last frame (used by tests and for hit-testing).
    pub last_table_rect: egui::Rect,
    /// Rows that are inactive (themselves or by a summary), for drawing.
    pub inactive_rows: Vec<bool>,
    pub last_gantt_rect: egui::Rect,
    /// The area the main view was drawn in last frame (Copy Picture).
    pub last_view_rect: egui::Rect,
    /// Copy Picture's progress.
    pub picture: Picture,
    pub sheet: resource_sheet::SheetState,
    pub board: board::BoardState,
    pub usage: usage::UsageState,
    pub prefs: prefs::Prefs,
    /// Where prefs are saved (None in tests).
    pub prefs_path: Option<PathBuf>,
}

impl Default for ProjectFcgApp {
    fn default() -> Self {
        Self::new(Project::new(chrono::Local::now().date_naive()))
    }
}

impl ProjectFcgApp {
    pub fn new(mut project: Project) -> Self {
        schedule(&mut project);
        let name_col = ColumnId::default_set()
            .iter()
            .position(|(c, _)| *c == ColumnId::Name)
            .unwrap_or(0);
        let mut app = ProjectFcgApp {
            project,
            history: History::default(),
            path: None,
            dirty: false,
            auto_calc: true,
            needs_calc: false,
            columns: ColumnId::default_set()
                .into_iter()
                .map(|(id, width)| ColumnState { id, width })
                .collect(),
            table_width: 640.0,
            scroll_y: 0.0,
            table_scroll_x: 0.0,
            gantt: GanttView {
                left_days: 0.0,
                ppd: Timescale::Days.ppd(),
            },
            active_row: RowRef::Blank,
            active_col: name_col,
            selected: vec![],
            anchor: None,
            edit: None,
            tab: RibbonTab::Task,
            table: columns::Table::Entry,
            view: ViewOptions::default(),
            filter: Filter::AllTasks,
            group: filters::GroupBy::None,
            groups: Default::default(),
            status: "Ready".into(),
            status_error: false,
            dialogs: dialogs::Dialogs::default(),
            gantt_state: gantt::GanttState::default(),
            clipboard: vec![],
            ensure_visible: None,
            shown_col: name_col,
            last_title: String::new(),
            allow_close: false,
            file_browser: None,
            pending: None,
            deferred_action: None,
            splitter_drag: false,
            use_portal: std::env::var_os("PROJECTFCG_NO_PORTAL").is_none(),
            rows_view_h: 400.0,
            gantt_view_w: 600.0,
            themed: false,
            applied_theme: None,
            last_table_rect: egui::Rect::NOTHING,
            inactive_rows: vec![],
            last_gantt_rect: egui::Rect::NOTHING,
            last_view_rect: egui::Rect::NOTHING,
            picture: Picture::None,
            sheet: resource_sheet::SheetState::default(),
            board: board::BoardState::default(),
            usage: usage::UsageState::default(),
            prefs: prefs::Prefs::default(),
            prefs_path: None,
        };
        app.scroll_gantt_to(app.project.start);
        if let Some(t) = app.project.tasks.first() {
            app.active_row = RowRef::Task(t.uid);
        }
        app
    }

    // ------------------------------------------------------------ helpers

    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.project.title.clone())
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_error = false;
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.status = msg.clone();
        self.status_error = true;
        self.dialogs.message = Some((APP_NAME.to_string(), msg));
    }

    pub fn recalc(&mut self) {
        if self.auto_calc {
            schedule(&mut self.project);
            self.needs_calc = false;
        } else {
            refresh_structure(&mut self.project);
            self.needs_calc = true;
        }
    }

    /// Run an editing operation as one undoable step. On error the project
    /// is restored and the message shown.
    pub fn edit_project<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project) -> Result<T, String>,
    ) -> Option<T> {
        let before = self.project.clone();
        match f(&mut self.project) {
            Ok(v) => {
                self.history.checkpoint(label, &before, &self.project);
                self.dirty = true;
                self.recalc();
                self.set_status(label);
                Some(v)
            }
            Err(e) => {
                self.project = before;
                self.error(e);
                None
            }
        }
    }

    pub fn row_of_uid(&self, uid: u32) -> Option<usize> {
        self.project.index_of_uid(uid)
    }

    pub fn active_index(&self) -> Option<usize> {
        match self.active_row {
            RowRef::Task(u) => self.row_of_uid(u),
            RowRef::Blank => None,
        }
    }

    /// Selected task rows in selection order (falls back to the active row).
    pub fn selected_rows(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected.iter().filter_map(|&u| self.row_of_uid(u)).collect();
        if v.is_empty() {
            if let Some(i) = self.active_index() {
                v.push(i);
            }
        }
        v
    }

    pub fn selected_rows_sorted(&self) -> Vec<usize> {
        let mut v = self.selected_rows();
        v.sort();
        v.dedup();
        v
    }

    pub fn select_single(&mut self, row: RowRef) {
        self.active_row = row;
        self.selected = match row {
            RowRef::Task(u) => vec![u],
            RowRef::Blank => vec![],
        };
        self.anchor = match row {
            RowRef::Task(u) => Some(u),
            RowRef::Blank => None,
        };
        if let RowRef::Task(u) = row {
            self.ensure_visible = self.row_of_uid(u);
        }
    }

    pub fn select_index(&mut self, idx: usize) {
        if let Some(t) = self.project.tasks.get(idx) {
            let u = t.uid;
            self.select_single(RowRef::Task(u));
        } else {
            self.select_single(RowRef::Blank);
        }
    }

    pub fn display_rows(&self) -> Vec<DisplayRow> {
        let p = &self.project;
        let mut out = vec![];
        if self.view.project_summary {
            out.push(DisplayRow::ProjectSummary);
        }
        let eval = filters::FilterEval::new(p, self.filter);
        if self.group != filters::GroupBy::None {
            // grouped: tasks without their outline, under group headers
            let cx = filters::GroupCtx::new(p, self.group);
            // summaries roll up their own subtasks, so they're listed only when
            // the filter picks nothing else (Summary Tasks, say)
            let matching: Vec<usize> = (0..p.tasks.len()).filter(|&i| eval.matches(i)).collect();
            let any_leaf = matching.iter().any(|&i| !p.tasks[i].calc.is_summary);
            let mut keyed: Vec<((i64, String), usize)> = matching
                .into_iter()
                .filter(|&i| !any_leaf || !p.tasks[i].calc.is_summary)
                .map(|i| (filters::group_key(p, i, self.group, &cx), i))
                .collect();
            // stable: tasks keep their ID order inside a group
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            let mut groups = vec![];
            let mut k = 0;
            while k < keyed.len() {
                let mut e = k;
                while e < keyed.len() && keyed[e].0 == keyed[k].0 {
                    e += 1;
                }
                let label = filters::group_label(p, keyed[k].1, self.group, &cx);
                let rows: Vec<usize> = keyed[k..e].iter().map(|x| x.1).collect();
                let collapsed = self.view.collapsed_groups.contains(&label);
                out.push(DisplayRow::Group(groups.len()));
                groups.push(filters::rollup(p, label, &rows, collapsed));
                if !collapsed {
                    out.extend(rows.into_iter().map(DisplayRow::Task));
                }
                k = e;
            }
            *self.groups.borrow_mut() = groups;
            return out;
        }
        let base: Vec<usize> = if self.view.summary_rows {
            p.visible_rows()
        } else {
            (0..p.tasks.len()).filter(|&i| !p.tasks[i].calc.is_summary).collect()
        };
        if eval.related(self.view.related_summaries) && self.view.summary_rows {
            // matching tasks plus the summaries above them, in one pass
            // with the chain of open summaries on a stack
            let mut keep = vec![false; p.tasks.len()];
            let mut stack: Vec<usize> = vec![];
            for i in 0..p.tasks.len() {
                let lvl = p.tasks[i].outline_level;
                while stack.last().is_some_and(|&a| p.tasks[a].outline_level >= lvl) {
                    stack.pop();
                }
                if eval.matches(i) {
                    keep[i] = true;
                    for &a in stack.iter().rev() {
                        if std::mem::replace(&mut keep[a], true) {
                            break;
                        }
                    }
                }
                stack.push(i);
            }
            out.extend(base.into_iter().filter(|&i| keep[i]).map(DisplayRow::Task));
        } else {
            out.extend(base.into_iter().filter(|&i| eval.matches(i)).map(DisplayRow::Task));
        }
        out
    }

    /// The name of a filter, custom ones included.
    pub fn filter_label(&self, f: Filter) -> String {
        match f {
            Filter::Custom(k) => self.project.views.filter(k).map(|c| c.name.clone()).unwrap_or_else(|| "All Tasks".into()),
            Filter::DateRange(a, b) => format!("Date Range ({} – {})", a.format("%-m/%-d/%y"), b.format("%-m/%-d/%y")),
            Filter::UsingResource(r) => match self.project.resources.iter().find(|x| x.uid == r) {
                Some(x) => format!("Using Resource ({})", x.name),
                None => "Using Resource".into(),
            },
            _ => f.label().into(),
        }
    }

    pub fn scroll_gantt_to(&mut self, dt: NaiveDateTime) {
        self.gantt.left_days = to_days(dt) - 2.0 * (28.0 / self.gantt.ppd as f64).max(1.0 / 24.0);
    }

    pub fn project_span(&self) -> (NaiveDateTime, NaiveDateTime) {
        let s = self
            .project
            .tasks
            .iter()
            .filter_map(|t| t.calc.start)
            .min()
            .unwrap_or(self.project.start)
            .min(self.project.start);
        let f = self.project.calc.finish.unwrap_or(s).max(s);
        (s, f)
    }

    pub fn zoom_to(&mut self, ppd: f32, anchor_x_frac: f32) {
        let ppd = ppd.clamp(0.05, 2400.0);
        let w = self.gantt_view_w.max(100.0) as f64;
        let anchor_days = self.gantt.left_days + (anchor_x_frac as f64 * w) / self.gantt.ppd as f64;
        self.gantt.ppd = ppd;
        self.gantt.left_days = anchor_days - (anchor_x_frac as f64 * w) / ppd as f64;
    }

    pub fn fit_range(&mut self, a: NaiveDateTime, b: NaiveDateTime) {
        let span = (to_days(b) - to_days(a)).max(1.0);
        let w = (self.gantt_view_w - 40.0).max(100.0) as f64;
        self.gantt.ppd = (w / (span * 1.08)).clamp(0.05, 2400.0) as f32;
        self.gantt.left_days = to_days(a) - span * 0.04;
    }

    pub fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} - {}",
            if self.dirty { "*" } else { "" },
            self.file_name(),
            APP_NAME
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    // ------------------------------------------------------------ files

    pub fn replace_project(&mut self, p: Project, path: Option<PathBuf>) {
        let mut fresh = ProjectFcgApp::new(p);
        fresh.path = path;
        fresh.view = self.view.clone();
        // filters and groups name this file's tasks, filters and resources
        fresh.view.highlight = Filter::AllTasks;
        fresh.view.collapsed_groups.clear();
        fresh.columns = self.columns.clone();
        fresh.table_width = self.table_width;
        fresh.use_portal = self.use_portal;
        fresh.gantt.ppd = self.gantt.ppd;
        fresh.gantt_view_w = self.gantt_view_w;
        fresh.rows_view_h = self.rows_view_h;
        fresh.themed = self.themed;
        fresh.applied_theme = self.applied_theme;
        fresh.prefs = self.prefs.clone();
        fresh.prefs_path = self.prefs_path.clone();
        fresh.board.show_charts = self.board.show_charts;
        fresh.board.backlog_open = self.board.backlog_open;
        let start = fresh.project_span().0;
        fresh.scroll_gantt_to(start);
        *self = fresh;
    }

    pub fn open_path(&mut self, path: PathBuf) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
            return self.open_xml(path);
        }
        match crate::io::load(&path) {
            Ok(p) => {
                self.replace_project(p, Some(path.clone()));
                self.set_status(format!("Opened {}", path.display()));
            }
            Err(e) => self.error(e),
        }
    }

    pub fn save_to(&mut self, path: PathBuf) -> bool {
        let path = if path.extension().is_none() {
            path.with_extension(crate::io::EXTENSION)
        } else {
            path
        };
        if self.project.title == "Project1" || self.project.title.is_empty() {
            if let Some(stem) = path.file_stem() {
                self.project.title = stem.to_string_lossy().to_string();
            }
        }
        match crate::io::save(&self.project, &path) {
            Ok(()) => {
                self.set_status(format!("Saved {}", path.display()));
                self.path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                self.error(e);
                false
            }
        }
    }

    /// Run an action that would discard the current project, asking to save
    /// first if needed (Project's "Do you want to save changes?" prompt).
    pub fn guarded(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.dirty {
            self.pending = Some(action);
            self.dialogs.save_prompt = true;
        } else {
            self.run_pending(action, ctx);
        }
    }

    pub fn run_pending(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::New => {
                let p = Project::new(chrono::Local::now().date_naive());
                self.replace_project(p, None);
            }
            PendingAction::NewSample => {
                let p = sample::software_release();
                self.replace_project(p, None);
                self.set_status("Loaded sample project");
            }
            PendingAction::NewAgileSample => {
                let p = sample::agile_release();
                self.replace_project(p, None);
                self.view.main = MainView::Board;
                self.tab = RibbonTab::Board;
                self.set_status("Loaded agile sample project");
            }
            PendingAction::Open => self.pick_open(),
            PendingAction::OpenPath(p) => self.open_path(p),
            PendingAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub fn pick_open(&mut self) {
        self.pick_open_for(filebrowser::Mode::Open);
    }

    /// Pick an existing file: a project to open, or a file to import.
    pub fn pick_open_for(&mut self, mode: filebrowser::Mode) {
        if self.use_portal {
            let t0 = std::time::Instant::now();
            let d = rfd::FileDialog::new();
            let d = if mode == filebrowser::Mode::ImportJiraCsv {
                d.set_title("Import Jira CSV").add_filter("CSV", &["csv"])
            } else {
                d.set_title("Open")
                    .add_filter("Projects (ProjectFCG, MS Project XML)", &[crate::io::EXTENSION, crate::io::LEGACY_EXTENSIONS[0], crate::io::LEGACY_EXTENSIONS[1], "xml"])
                    .add_filter("ProjectFCG project", &[crate::io::EXTENSION, crate::io::LEGACY_EXTENSIONS[0], crate::io::LEGACY_EXTENSIONS[1]])
                    .add_filter("MS Project XML", &["xml"])
            };
            let picked = d.add_filter("All files", &["*"]).pick_file();
            match picked {
                Some(p) => {
                    self.finish_save_dialog(mode, p);
                    return;
                }
                // Windows and macOS dialogs are always there: None means cancelled.
                // On Linux an instant None means no portal answered.
                None if !cfg!(target_os = "linux") || t0.elapsed().as_millis() > 300 => return, // user cancelled
                None => {
                    // No portal answered instantly: fall back to the built-in dialog.
                    self.use_portal = false;
                }
            }
        }
        self.file_browser = Some(filebrowser::FileBrowser::new(mode, self.path.clone()));
    }

    pub fn pick_save(&mut self, purpose: filebrowser::Mode) {
        let (ext, filter_name, default_name) = match purpose {
            filebrowser::Mode::ExportCsv => ("csv", "CSV", format!("{}.csv", self.project.title)),
            filebrowser::Mode::ExportJiraCsv => ("csv", "CSV", format!("{} - Jira.csv", self.project.title)),
            filebrowser::Mode::ExportXml => ("xml", "MS Project XML", format!("{}.xml", self.project.title)),
            filebrowser::Mode::ExportPng => ("png", "PNG image", format!("{} - {}.png", self.project.title, self.view.main.label())),
            _ => (
                crate::io::EXTENSION,
                "ProjectFCG project",
                format!("{}.{}", self.project.title, crate::io::EXTENSION),
            ),
        };
        if self.use_portal {
            let t0 = std::time::Instant::now();
            let picked = rfd::FileDialog::new()
                .set_title(match purpose {
                    filebrowser::Mode::SaveAs => "Save As",
                    filebrowser::Mode::ExportXml => "Save as Project XML",
                    filebrowser::Mode::ExportPng => "Copy Picture",
                    _ => "Export",
                })
                .add_filter(filter_name, &[ext])
                .set_file_name(&default_name)
                .save_file();
            match picked {
                Some(p) => {
                    self.finish_save_dialog(purpose, p);
                    return;
                }
                None if !cfg!(target_os = "linux") || t0.elapsed().as_millis() > 300 => return,
                None => self.use_portal = false,
            }
        }
        let mut fb = filebrowser::FileBrowser::new(purpose, self.path.clone());
        fb.file_name = default_name;
        self.file_browser = Some(fb);
    }

    pub fn finish_save_dialog(&mut self, purpose: filebrowser::Mode, path: PathBuf) {
        match purpose {
            filebrowser::Mode::ExportCsv => {
                let path = if path.extension().is_none() { path.with_extension("csv") } else { path };
                match std::fs::write(&path, crate::io::to_csv(&self.project)) {
                    Ok(()) => self.set_status(format!("Exported {}", path.display())),
                    Err(e) => self.error(format!("Couldn't export: {e}")),
                }
            }
            filebrowser::Mode::ExportJiraCsv => {
                let path = if path.extension().is_none() { path.with_extension("csv") } else { path };
                match std::fs::write(&path, crate::jira::export_csv(&self.project)) {
                    Ok(()) => self.set_status(format!("Exported {} tickets to {}", self.project.tickets.len(), path.display())),
                    Err(e) => self.error(format!("Couldn't export: {e}")),
                }
            }
            filebrowser::Mode::ExportXml => {
                let path = if path.extension().is_none() { path.with_extension("xml") } else { path };
                match std::fs::write(&path, crate::mspdi::to_xml(&self.project)) {
                    Ok(()) => self.set_status(format!("Saved {} as Project XML", path.display())),
                    Err(e) => self.error(format!("Couldn't save: {e}")),
                }
            }
            filebrowser::Mode::ExportPng => {
                let path = if path.extension().is_none() { path.with_extension("png") } else { path };
                if let Picture::Ready(bytes) = std::mem::take(&mut self.picture) {
                    match std::fs::write(&path, bytes) {
                        Ok(()) => self.set_status(format!("Saved the picture as {}", path.display())),
                        Err(e) => self.error(format!("Couldn't save the picture: {e}")),
                    }
                }
            }
            filebrowser::Mode::ImportJiraCsv => self.start_jira_import(path),
            filebrowser::Mode::Open => self.open_path(path),
            filebrowser::Mode::SaveAs => {
                let after = self.pending.take();
                if self.save_to(path) {
                    if let Some(a) = after {
                        self.deferred_action = Some(a);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ commands

    pub fn execute(&mut self, cmd: Command, ctx: &egui::Context) {
        use Command::*;
        // In the Resource Sheet, row commands act on resources.
        if self.view.main == MainView::ResourceSheet {
            match cmd {
                InsertTask => {
                    self.res_commit_edit();
                    let at = self.res_active_index().unwrap_or(self.project.resources.len());
                    if let Some(i) = self.edit_project("Insert Resource", |p| {
                        let n = p.resources.len() + 1;
                        let mut name = format!("Resource {n}");
                        let mut k = n;
                        while p.resource_by_name(&name).is_some() {
                            k += 1;
                            name = format!("Resource {k}");
                        }
                        Ok(p.insert_resource(at, &name, crate::model::ResourceType::Work))
                    }) {
                        self.res_select(Some(i));
                        self.sheet.col = 1;
                        self.res_begin_edit(None);
                    }
                    return;
                }
                DeleteTasks => {
                    self.sheet.edit = None;
                    let rows = self.res_selected_rows();
                    if !rows.is_empty() {
                        let first = rows[0];
                        self.edit_project(if rows.len() == 1 { "Delete Resource" } else { "Delete Resources" }, |p| {
                            p.delete_resources(&rows);
                            Ok(())
                        });
                        let n = self.project.resources.len();
                        self.res_select(if n == 0 { None } else { Some(first.min(n - 1)) });
                    }
                    return;
                }
                TaskInfo => return self.execute(ResourceInfo, ctx),
                EditCell => return self.res_begin_edit(None),
                Copy | Cut | Paste | Indent | Outdent | MoveUp | MoveDown | LinkTasks | UnlinkTasks | FillDown | ResetField => return,
                _ => {}
            }
        }
        // On the Board, row commands act on tickets.
        if self.view.main == MainView::Board {
            match cmd {
                InsertTask => return self.execute(NewTicket, ctx),
                DeleteTasks => return self.execute(DeleteTicket, ctx),
                TaskInfo | EditCell => return self.execute(EditTicket, ctx),
                Find => {
                    self.board.focus_filter = true;
                    return;
                }
                Copy | Cut | Paste | Indent | Outdent | MoveUp | MoveDown | LinkTasks | UnlinkTasks | FillDown | ResetField | InsertSummary | InsertMilestone | InsertRecurring => return,
                _ => {}
            }
        }
        // Most commands act on committed data: finish any in-progress edit.
        if !matches!(cmd, Undo | Redo) {
            self.commit_edit();
        }
        match cmd {
            New => self.guarded(PendingAction::New, ctx),
            NewFromSample => self.guarded(PendingAction::NewSample, ctx),
            NewFromAgileSample => self.guarded(PendingAction::NewAgileSample, ctx),
            Open => self.guarded(PendingAction::Open, ctx),
            Save => {
                if let Some(p) = self.path.clone() {
                    self.save_to(p);
                } else {
                    self.pick_save(filebrowser::Mode::SaveAs);
                }
            }
            SaveAs => self.pick_save(filebrowser::Mode::SaveAs),
            ExportCsv => self.pick_save(filebrowser::Mode::ExportCsv),
            ExportJiraCsv => self.pick_save(filebrowser::Mode::ExportJiraCsv),
            ExportXml => self.pick_save(filebrowser::Mode::ExportXml),
            ImportJiraCsv => self.pick_open_for(filebrowser::Mode::ImportJiraCsv),
            Options => self.dialogs.open_options(&self.project),
            Quit => self.guarded(PendingAction::Quit, ctx),

            Undo => {
                self.edit = None;
                match self.history.undo(&mut self.project) {
                    Some(l) => {
                        self.recalc();
                        self.dirty = true;
                        self.set_status(format!("Undo {l}"));
                    }
                    None => self.set_status("Nothing to undo"),
                }
            }
            Redo => {
                self.edit = None;
                match self.history.redo(&mut self.project) {
                    Some(l) => {
                        self.recalc();
                        self.dirty = true;
                        self.set_status(format!("Redo {l}"));
                    }
                    None => self.set_status("Nothing to redo"),
                }
            }
            Copy => self.copy_tasks(ctx),
            Cut => {
                self.copy_tasks(ctx);
                let rows = self.selected_rows_sorted();
                if !rows.is_empty() {
                    self.edit_project("Cut", |p| {
                        p.delete_tasks(&rows);
                        Ok(())
                    });
                    self.fix_selection_after_delete(rows[0]);
                }
            }
            Paste => self.paste_tasks(),
            FillDown => self.fill_down(),
            ResetField => {
                if let (Some(r), Some(c)) = (self.active_index(), self.columns.get(self.active_col)) {
                    let col = c.id;
                    self.edit_project("Reset field", |p| columns::reset_cell(p, r, col));
                }
            }
            EditCell => self.begin_edit(None),
            SelectAll => {
                self.selected = self.project.tasks.iter().map(|t| t.uid).collect();
            }
            SelectRow => {
                if let RowRef::Task(u) = self.active_row {
                    self.selected = vec![u];
                }
            }
            Find => self.dialogs.find_open = true,
            FindNext => self.find_next(),
            GoTo => self.dialogs.goto_open = true,

            InsertTask => self.insert_task("", false),
            InsertMilestone => self.insert_task("<New Milestone>", true),
            InsertSummary => self.insert_summary(),
            DeleteTasks => {
                let rows = self.selected_rows_sorted();
                if !rows.is_empty() {
                    let label = if rows.len() == 1 { "Delete Task" } else { "Delete Tasks" };
                    self.edit_project(label, |p| {
                        p.delete_tasks(&rows);
                        Ok(())
                    });
                    self.fix_selection_after_delete(rows[0]);
                }
            }
            Indent => {
                let rows = self.selected_rows_sorted();
                if !rows.is_empty() {
                    self.edit_project("Indent", |p| p.indent(&rows).map(|_| ()));
                }
            }
            Outdent => {
                let rows = self.selected_rows_sorted();
                if !rows.is_empty() {
                    self.edit_project("Outdent", |p| p.outdent(&rows).map(|_| ()));
                }
            }
            MoveUp | MoveDown => {
                if let Some(r) = self.active_index() {
                    let up = cmd == MoveUp;
                    self.edit_project(if up { "Move Up" } else { "Move Down" }, |p| {
                        if up { p.move_up(r) } else { p.move_down(r) }.map(|_| ())
                    });
                    if let Some(r) = self.active_index() {
                        self.ensure_visible = Some(r);
                    }
                }
            }
            LinkTasks => {
                let rows = self.selected_rows();
                if rows.len() < 2 {
                    self.error("Select two or more tasks to link.");
                } else {
                    self.edit_project("Link Tasks", |p| p.link_tasks(&rows).map(|_| ()));
                }
            }
            UnlinkTasks => {
                let rows = self.selected_rows();
                if rows.is_empty() {
                    return;
                }
                self.edit_project("Unlink Tasks", |p| {
                    p.unlink_tasks(&rows);
                    Ok(())
                });
            }
            TaskInfo => {
                if let Some(r) = self.active_index() {
                    // a recurring task's summary opens its pattern, as in Project
                    if self.project.tasks[r].recurrence.is_some() {
                        self.open_recurring_dialog(Some(r));
                    } else {
                        self.dialogs.open_task_info(&self.project, r, dialogs::InfoTab::General);
                    }
                }
            }
            InsertRecurring => self.open_recurring_dialog(None),
            TaskNotes => {
                if let Some(r) = self.active_index() {
                    self.dialogs.open_task_info(&self.project, r, dialogs::InfoTab::Notes);
                }
            }
            SetPercent(v) => {
                let rows = self.selected_rows();
                if !rows.is_empty() && rows.iter().all(|&r| self.project.progress_from_board(r)) {
                    self.error("This task's % complete comes from the Board (its tickets, or the epic that drives it).".to_string());
                    return;
                }
                self.edit_project(&format!("{v}% Complete"), |p| {
                    for r in rows {
                        if !p.is_summary(r) && !p.progress_from_board(r) {
                            p.set_percent_complete(r, v as f64)?;
                        }
                    }
                    Ok(())
                });
            }
            ToggleActive => {
                let rows = self.selected_rows();
                if let Some(&first) = rows.first() {
                    // like Project: if the first is active, inactivate them all
                    let on = !self.project.tasks[first].active;
                    self.edit_project(if on { "Activate Task" } else { "Inactivate Task" }, |p| {
                        for r in rows {
                            p.tasks[r].active = on;
                        }
                        Ok(())
                    });
                }
            }
            SetManual | SetAuto => {
                let rows = self.selected_rows();
                let mode = if cmd == SetManual { TaskMode::Manual } else { TaskMode::Auto };
                self.edit_project(
                    if mode == TaskMode::Manual { "Manually Schedule" } else { "Auto Schedule" },
                    |p| {
                        for r in rows {
                            p.set_mode(r, mode);
                        }
                        Ok(())
                    },
                );
            }
            ToggleNewTaskMode => {
                self.project.settings.new_tasks_auto = !self.project.settings.new_tasks_auto;
                self.dirty = true;
            }
            MarkMilestone => {
                let rows = self.selected_rows();
                self.edit_project("Mark as Milestone", |p| {
                    for r in rows {
                        if !p.is_summary(r) {
                            p.tasks[r].milestone = !p.tasks[r].milestone;
                        }
                    }
                    Ok(())
                });
            }
            AddToTimeline => {
                let rows = self.selected_rows();
                self.edit_project("Add to Timeline", |p| {
                    let on = rows.iter().any(|&r| !p.tasks[r].on_timeline);
                    for r in rows {
                        p.tasks[r].on_timeline = on;
                    }
                    Ok(())
                });
                self.view.timeline = true;
            }
            ScrollToTask => {
                if let Some(s) = self.active_index().and_then(|r| self.project.tasks[r].calc.start) {
                    self.scroll_gantt_to(s);
                }
            }

            HideSubtasks | ShowSubtasks => {
                let collapse = cmd == HideSubtasks;
                let rows = self.selected_rows();
                for r in rows {
                    // Hide on a subtask collapses its parent, like Project.
                    let target = if self.project.is_summary(r) {
                        Some(r)
                    } else if collapse {
                        self.project.parent_of(r)
                    } else {
                        None
                    };
                    if let Some(t) = target {
                        self.project.tasks[t].collapsed = collapse;
                        if collapse {
                            self.active_row = RowRef::Task(self.project.tasks[t].uid);
                            self.selected = vec![self.project.tasks[t].uid];
                        }
                    }
                }
            }
            ShowAllSubtasks => self.project.show_outline_level(None),
            OutlineLevel(l) => self.project.show_outline_level(l),

            ProjectInfo => self.dialogs.open_project_info(&self.project),
            WorkingTime => self.dialogs.open_working_time(&self.project),
            Calculate => {
                schedule(&mut self.project);
                self.needs_calc = false;
                self.set_status("Project calculated");
            }
            ToggleAutoCalc => {
                self.auto_calc = !self.auto_calc;
                if self.auto_calc {
                    self.recalc();
                }
                self.set_status(if self.auto_calc {
                    "Calculation: Automatic"
                } else {
                    "Calculation: Manual (press F9 to calculate)"
                });
            }

            ZoomIn => self.zoom_to(self.gantt.ppd * 1.5, 0.5),
            ZoomOut => self.zoom_to(self.gantt.ppd / 1.5, 0.5),
            SetTimescale(t) => self.zoom_to(t.ppd(), 0.0),
            ZoomEntireProject => {
                let (a, b) = self.project_span();
                self.fit_range(a, b);
            }
            ZoomSelectedTasks => {
                let rows = self.selected_rows();
                let s = rows.iter().filter_map(|&r| self.project.tasks[r].calc.start).min();
                let f = rows.iter().filter_map(|&r| self.project.tasks[r].calc.finish).max();
                if let (Some(s), Some(f)) = (s, f) {
                    self.fit_range(s, f);
                }
            }
            ScrollProjectStart => {
                let (a, _) = self.project_span();
                self.scroll_gantt_to(a);
            }
            ScrollProjectEnd => {
                let (_, b) = self.project_span();
                let w = self.gantt_view_w as f64 / self.gantt.ppd as f64;
                self.gantt.left_days = to_days(b) - w * 0.85;
            }
            ScrollLeft | ScrollRight => {
                let step = 60.0 / self.gantt.ppd as f64;
                self.gantt.left_days += if cmd == ScrollLeft { -step } else { step };
            }
            PageLeft | PageRight => {
                let step = self.gantt_view_w as f64 / self.gantt.ppd as f64;
                self.gantt.left_days += if cmd == PageLeft { -step } else { step };
            }
            ToggleTimeline => self.view.timeline = !self.view.timeline,
            SetFilter(f) => {
                self.filter = f;
                self.set_status(format!("Filter: {}", self.filter_label(f)));
            }
            SetGroup(g) => {
                self.group = g;
                self.view.collapsed_groups.clear();
                self.scroll_y = 0.0;
                self.set_status(if g == filters::GroupBy::None { "No Group".into() } else { format!("Grouped by {}", g.label()) });
            }
            ToggleRelatedSummaries => self.view.related_summaries = !self.view.related_summaries,
            ToggleGroupRow(k) => {
                let label = self.groups.borrow().get(k).map(|g| g.label.clone());
                if let Some(l) = label {
                    if !self.view.collapsed_groups.remove(&l) {
                        self.view.collapsed_groups.insert(l);
                    }
                }
            }
            DateRangeFilterDlg => self.dialogs.date_range = Some(view_dialogs::DateRangeDlg::new(self)),
            UsingResourceFilterDlg => self.dialogs.using_resource = Some(view_dialogs::UsingResourceDlg::new(self)),
            NewFilter => self.dialogs.filter_def = Some(view_dialogs::FilterDefDlg::new(None, &self.project)),
            EditFilter(k) => self.dialogs.filter_def = Some(view_dialogs::FilterDefDlg::new(Some(k), &self.project)),
            CustomGroupDlg => self.dialogs.custom_group = Some(view_dialogs::CustomGroupDlg::new(self.group)),

            ToggleCritical => {
                self.edit_project("Critical Tasks", |p| {
                    p.format.show_critical = !p.format.show_critical;
                    Ok(())
                });
            }
            ToggleSlack => {
                self.edit_project("Slack", |p| {
                    p.format.show_slack = !p.format.show_slack;
                    Ok(())
                });
            }
            CopyPicture => {
                let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                self.picture = Picture::Requested(slot.clone());
                let c = ctx.clone();
                ctx.request_screenshot(move |img| {
                    if let Ok(mut s) = slot.lock() {
                        *s = Some(img);
                    }
                    c.request_repaint();
                });
                self.set_status("Taking a picture of the view…");
            }
            ToggleSlippage => {
                self.edit_project("Slippage", |p| {
                    p.format.show_slippage = !p.format.show_slippage;
                    Ok(())
                });
            }
            ToggleLate => {
                self.edit_project("Late Tasks", |p| {
                    p.format.show_late = !p.format.show_late;
                    Ok(())
                });
            }
            ToggleBaselineBars => {
                self.edit_project("Baseline", |p| {
                    p.format.show_baseline = !p.format.show_baseline;
                    Ok(())
                });
            }
            SetGanttView(v) => {
                self.view.main = MainView::Gantt;
                if self.project.format.view == v {
                    return;
                }
                self.edit_project(v.label(), |p| {
                    p.format.view = v;
                    Ok(())
                });
            }
            ToggleMultipleCriticalPaths => {
                self.edit_project("Multiple Critical Paths", |p| {
                    p.settings.multiple_critical_paths = !p.settings.multiple_critical_paths;
                    Ok(())
                });
            }
            TaskPath(role) => {
                use bars::PathRole::*;
                let v = &mut self.view;
                match role {
                    Predecessor => v.path_preds = !v.path_preds,
                    DrivingPredecessor => v.path_driving = !v.path_driving,
                    Successor => v.path_succs = !v.path_succs,
                    DrivenSuccessor => v.path_driven = !v.path_driven,
                }
            }
            ClearTaskPath => {
                let v = &mut self.view;
                v.path_preds = false;
                v.path_driving = false;
                v.path_succs = false;
                v.path_driven = false;
            }
            SetHighlight(f) => {
                self.view.highlight = f;
                self.set_status(if f == Filter::AllTasks {
                    "Highlight cleared".to_string()
                } else {
                    format!("Highlight: {}", f.label())
                });
            }
            ToggleInspector => self.view.inspector = !self.view.inspector,
            SetBaseline => {
                let rows = self.selected_rows_sorted();
                let all = rows.len() <= 1 || rows.len() == self.project.tasks.len();
                self.dialogs.baseline = Some(format_dialogs::BaselineDlg { selected_only: !all, clear: false, n: 0 });
            }
            ClearBaseline => {
                self.dialogs.baseline = Some(format_dialogs::BaselineDlg { selected_only: false, clear: true, n: 0 });
            }
            BarStylesDlg => self.dialogs.open_bar_styles(&self.project),
            FormatBarDlg => {
                if let Some(r) = self.active_index() {
                    self.dialogs.format_bar = Some(format_dialogs::FormatBarDlg {
                        color: self.project.tasks[r].bar_color,
                    });
                }
            }
            TextStylesDlg => self.dialogs.text_styles = Some(format_dialogs::TextStylesDlg {
                styles: self.project.format.text,
                size: self.prefs.text_size,
            }),
            GridlinesDlg => self.dialogs.gridlines = Some(self.project.format.gridlines),
            LayoutDlg => self.dialogs.layout = Some(self.project.format.layout),
            TimescaleDlg => self.dialogs.timescale = Some(self.project.format.timescale),
            ApplyStylePreset(k) => {
                if let Some(preset) = crate::format::STYLE_PRESETS.get(k) {
                    self.edit_project(&format!("Gantt Chart Style: {}", preset.name), |p| {
                        p.format.styles.apply_preset(preset);
                        Ok(())
                    });
                }
            }
            SetBarColor(c) => {
                let rows = self.selected_rows();
                self.edit_project("Bar Color", |p| {
                    for r in rows {
                        p.tasks[r].bar_color = c;
                    }
                    Ok(())
                });
            }
            ToggleSummaryRows => self.view.summary_rows = !self.view.summary_rows,
            ToggleProjectSummary => self.view.project_summary = !self.view.project_summary,
            ToggleOutlineNumber => self.view.outline_number = !self.view.outline_number,
            ToggleBarText => {
                self.edit_project("Bar Text", |p| {
                    p.format.layout.show_bar_text = !p.format.layout.show_bar_text;
                    Ok(())
                });
            }

            TextBigger => self.set_text_size(self.prefs.text_size + 1.0),
            TextSmaller => self.set_text_size(self.prefs.text_size - 1.0),
            TextReset => {
                self.set_text_size(theme::DEFAULT_TEXT_SIZE);
                self.set_ui_zoom(1.0);
            }
            UiZoomIn | UiZoomOut => {
                let cur = self.prefs.ui_zoom;
                let next = if cmd == UiZoomIn {
                    prefs::ZOOM_STEPS.iter().copied().find(|z| *z > cur + 0.001)
                } else {
                    prefs::ZOOM_STEPS.iter().rev().copied().find(|z| *z < cur - 0.001)
                };
                if let Some(z) = next {
                    self.set_ui_zoom(z);
                }
            }
            SetUiZoom(z) => self.set_ui_zoom(z),
            MarkOnTrack => {
                let rows = self.selected_rows();
                self.edit_project("Mark on Track", |p| p.mark_on_track(&rows).map(|_| ()));
            }
            UpdateTasks => self.open_update_tasks(),
            UpdateProject => self.open_update_project(),
            ProjectStatistics => self.dialogs.statistics = true,
            SetTable(t) => {
                self.commit_edit();
                let k = self.prefs.text_size / theme::DEFAULT_TEXT_SIZE;
                self.columns = t
                    .columns()
                    .into_iter()
                    .map(|id| ColumnState { id, width: (id.default_width() * k).round() })
                    .collect();
                self.active_col = self.columns.iter().position(|c| c.id == columns::ColumnId::Name).unwrap_or(0);
                self.shown_col = usize::MAX;
                self.table = t;
                self.set_status(format!("Table: {}", t.label()));
            }
            ToggleProgressLines => {
                self.edit_project("Progress Lines", |p| {
                    p.format.gridlines.progress_lines = !p.format.gridlines.progress_lines;
                    Ok(())
                });
            }
            SetShownBaseline(n) => {
                self.edit_project(&format!("Show {}", baseline_name(n as usize)), |p| {
                    p.format.baseline_index = n.min(10);
                    p.format.show_baseline = true;
                    Ok(())
                });
            }
            NewSprint => {
                if let Some(row) = self.edit_project("New Sprint", |p| Ok(p.new_sprint())) {
                    let uid = self.project.tasks[row].uid;
                    self.board.sprint = Some(uid);
                    if self.view.main == MainView::Gantt {
                        self.select_single(RowRef::Task(uid));
                    }
                }
            }
            ToggleSprint => {
                let rows = self.selected_rows();
                if rows.is_empty() {
                    return;
                }
                let on = !rows.iter().all(|&r| self.project.tasks[r].is_sprint);
                self.edit_project(if on { "Mark as Sprint" } else { "Unmark Sprint" }, |p| {
                    for &r in &rows {
                        p.set_sprint(r, on)?;
                    }
                    Ok(())
                });
            }
            OpenOnBoard => {
                let uid = self.active_index().map(|r| self.project.tasks[r].uid).filter(|&u| self.project.task_by_uid(u).is_some_and(|t| t.is_sprint));
                match uid {
                    Some(u) => self.open_sprint_on_board(u),
                    None => self.view.main = MainView::Board,
                }
                self.tab = RibbonTab::Board;
            }
            NewTicket => {
                self.view.main = MainView::Board;
                self.board_new_ticket();
            }
            EditTicket => {
                if let Some(id) = self.board.selected {
                    self.open_ticket_dialog(Some(id));
                }
            }
            DeleteTicket => self.board_delete_selected(),
            NewEpic => {
                self.view.main = MainView::Board;
                self.board_new_epic();
            }
            TicketToBacklog => self.board_send_to_backlog(),
            SetEstimate(e) => {
                if self.project.board.estimate != e {
                    self.edit_project(&format!("Estimate in {}", e.label()), |p| {
                        p.board.estimate = e;
                        Ok(())
                    });
                }
            }
            ToggleBoardCharts => self.board.show_charts = !self.board.show_charts,
            SetThemeMode(m) => self.set_theme_mode(m),
            SetAccent(a) => self.set_accent(a),
            ToggleDarkMode => {
                let m = if self.is_dark(ctx) { theme::ThemeMode::Light } else { theme::ThemeMode::Dark };
                self.set_theme_mode(m);
            }
            ShowView(v) => {
                self.commit_edit();
                self.res_commit_edit();
                self.view.main = v;
                if v == MainView::ResourceSheet && self.sheet.active.is_none() {
                    if let Some(r) = self.project.resources.first() {
                        self.sheet.active = Some(r.uid);
                        self.sheet.selected = vec![r.uid];
                    }
                }
                self.set_status(v.label());
            }
            AssignResources => {
                if self.dialogs.assign.is_some() {
                    self.dialogs.assign = None;
                } else {
                    self.dialogs.assign = Some(resource_dialogs::AssignDlg::default());
                }
            }
            AddResource(t) => {
                let label = match t {
                    crate::model::ResourceType::Work => "Work Resource",
                    crate::model::ResourceType::Material => "Material Resource",
                    crate::model::ResourceType::Cost => "Cost Resource",
                };
                if let Some(i) = self.edit_project(&format!("Add {label}"), |p| {
                    let mut k = p.resources.len() + 1;
                    let mut name = format!("{label} {k}");
                    while p.resource_by_name(&name).is_some() {
                        k += 1;
                        name = format!("{label} {k}");
                    }
                    Ok(p.add_resource(&name, t))
                }) {
                    self.view.main = MainView::ResourceSheet;
                    self.res_select(Some(i));
                    self.sheet.col = 1;
                    self.res_begin_edit(None);
                }
            }
            ResourceInfo => {
                let row = match self.view.main {
                    MainView::ResourceSheet | MainView::ResourceUsage => self.res_active_index(),
                    _ => self.res_active_index(),
                };
                match row {
                    Some(r) => self.dialogs.open_resource_info(&self.project, r),
                    None => self.set_status("Select a resource first"),
                }
            }
            LevelAll | LevelSelection => {
                let rows = if cmd == LevelSelection { Some(self.selected_rows_sorted()) } else { None };
                if let Some(res) = self.edit_project("Level Resources", |p| Ok(crate::leveling::level(p, rows.as_deref()))) {
                    let mut msg = format!("Leveling delayed {} task(s).", res.delayed.len());
                    if res.unresolved.is_empty() {
                        self.set_status(msg);
                    } else {
                        msg.push_str("\n\nCould not resolve:\n• ");
                        msg.push_str(&res.unresolved.join("\n• "));
                        self.dialogs.message = Some(("Resource Leveling".into(), msg));
                    }
                }
            }
            ClearLeveling => {
                self.edit_project("Clear Leveling", |p| {
                    crate::leveling::clear_leveling(p, None);
                    Ok(())
                });
            }
            LevelingOptions => {
                self.dialogs.leveling = Some(resource_dialogs::LevelingDlg {
                    order: self.project.settings.leveling_order,
                    within_slack: self.project.settings.level_within_slack,
                });
            }
            NextOverallocation => {
                match crate::leveling::next_overallocation(&self.project, self.active_index()) {
                    Some(i) => {
                        for a in self.project.ancestors_of(i) {
                            self.project.tasks[a].collapsed = false;
                        }
                        if self.view.main != MainView::Gantt && self.view.main != MainView::TaskUsage {
                            self.view.main = MainView::Gantt;
                        }
                        self.select_index(i);
                        if let Some(s) = self.project.tasks[i].calc.start {
                            self.scroll_gantt_to(s);
                        }
                        self.set_status(format!("Overallocation: task {} {}", i + 1, self.project.tasks[i].name));
                    }
                    None => self.set_status("No overallocations"),
                }
            }
            Shortcuts => self.dialogs.shortcuts_open = true,
            About => self.dialogs.about_open = true,
        }
    }

    /// Task Path roles for the current selection (Format ▸ Task Path).
    pub fn path_roles(&self) -> std::collections::HashMap<usize, bars::PathRole> {
        let mut out = std::collections::HashMap::new();
        if !self.view.any_path() {
            return out;
        }
        let rows = self.selected_rows();
        if rows.is_empty() {
            return out;
        }
        let tp = crate::analysis::task_path(&self.project, &rows);
        let v = &self.view;
        if v.path_preds {
            out.extend(tp.predecessors.iter().map(|&i| (i, bars::PathRole::Predecessor)));
        }
        if v.path_driving {
            out.extend(tp.driving_predecessors.iter().map(|&i| (i, bars::PathRole::DrivingPredecessor)));
        }
        if v.path_succs {
            out.extend(tp.successors.iter().map(|&i| (i, bars::PathRole::Successor)));
        }
        if v.path_driven {
            out.extend(tp.driven_successors.iter().map(|&i| (i, bars::PathRole::DrivenSuccessor)));
        }
        out
    }

    /// Change the table/chart text size. Column widths and the table width
    /// scale with it so text that fit before still fits.
    pub fn set_text_size(&mut self, size: f32) {
        let new = size.clamp(theme::MIN_TEXT_SIZE, theme::MAX_TEXT_SIZE).round();
        let old = self.prefs.text_size;
        if (new - old).abs() < 0.01 {
            return;
        }
        let k = new / old;
        for c in self.columns.iter_mut() {
            c.width = (c.width * k).round();
        }
        self.table_width = (self.table_width * k).round();
        self.scroll_y *= k;
        self.table_scroll_x *= k;
        self.prefs.text_size = new;
        theme::set_text_size(new);
        self.save_prefs();
        self.set_status(format!("Text size {new} pt"));
    }

    pub fn set_ui_zoom(&mut self, z: f32) {
        self.prefs.ui_zoom = z.clamp(0.5, 3.0);
        self.save_prefs();
        self.set_status(format!("Interface zoom {}%", (self.prefs.ui_zoom * 100.0).round()));
    }

    pub fn save_prefs(&self) {
        if let Some(p) = &self.prefs_path {
            let _ = self.prefs.save(p);
        }
    }

    pub fn insert_task(&mut self, name: &str, milestone: bool) {
        let at = self.active_index().unwrap_or(self.project.tasks.len());
        let name = name.to_string();
        let label = if milestone { "Insert Milestone" } else { "Insert Task" };
        if let Some(idx) = self.edit_project(label, |p| {
            let i = p.insert_task(at, &name);
            if milestone {
                p.tasks[i].duration = crate::duration::Duration::zero();
            }
            Ok(i)
        }) {
            self.select_index(idx);
            if name.is_empty() {
                if let Some(c) = self.columns.iter().position(|c| c.id == ColumnId::Name) {
                    self.active_col = c;
                }
                self.begin_edit(Some(String::new()));
            }
        }
    }

    pub fn insert_summary(&mut self) {
        let rows = self.selected_rows_sorted();
        let at = rows.first().copied().or(self.active_index()).unwrap_or(self.project.tasks.len());
        if let Some(idx) = self.edit_project("Insert Summary Task", |p| {
            let i = p.insert_task(at, "<New Summary Task>");
            let shifted: Vec<usize> = rows.iter().map(|r| r + 1).collect();
            if !shifted.is_empty() {
                p.indent(&shifted)?;
            }
            Ok(i)
        }) {
            self.select_index(idx);
            self.begin_edit(None);
        }
    }

    fn fix_selection_after_delete(&mut self, first_row: usize) {
        let n = self.project.tasks.len();
        if n == 0 {
            self.select_single(RowRef::Blank);
        } else {
            self.select_index(first_row.min(n));
        }
    }

    fn copy_tasks(&mut self, ctx: &egui::Context) {
        let rows = self.selected_rows_sorted();
        if rows.is_empty() {
            return;
        }
        let mut idxs = vec![];
        for r in rows {
            if idxs.last().is_some_and(|&l: &usize| r < l) {
                continue;
            }
            let end = self.project.subtree_end(r);
            for i in r..end {
                if !idxs.contains(&i) {
                    idxs.push(i);
                }
            }
        }
        self.clipboard = idxs.iter().map(|&i| self.project.tasks[i].clone()).collect();
        // Also put a tab-separated version on the system clipboard.
        let mut tsv = String::new();
        for &i in &idxs {
            let cols = [ColumnId::Name, ColumnId::Duration, ColumnId::Start, ColumnId::Finish, ColumnId::Predecessors];
            let line: Vec<String> = cols.iter().map(|c| columns::cell_text(&self.project, i, *c)).collect();
            tsv.push_str(&line.join("\t"));
            tsv.push('\n');
        }
        ctx.copy_text(tsv);
        self.set_status(format!("Copied {} task(s)", self.clipboard.len()));
    }

    fn paste_tasks(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let at = self.active_index().unwrap_or(self.project.tasks.len());
        let clip = self.clipboard.clone();
        if let Some(first) = self.edit_project("Paste", move |p| {
            let base_level = if at < p.tasks.len() {
                p.tasks[at].outline_level
            } else {
                p.tasks.last().map(|t| t.outline_level).unwrap_or(1)
            };
            let min_level = clip.iter().map(|t| t.outline_level).min().unwrap_or(1);
            let mut map = std::collections::HashMap::new();
            let mut new_tasks = vec![];
            for t in &clip {
                let mut nt = t.clone();
                let nu = p.alloc_uid();
                map.insert(t.uid, nu);
                nt.uid = nu;
                nt.outline_level = t.outline_level - min_level + base_level;
                new_tasks.push(nt);
            }
            for nt in new_tasks.iter_mut() {
                nt.predecessors = nt
                    .predecessors
                    .iter()
                    .filter_map(|l| map.get(&l.pred).map(|&u| crate::model::Link { pred: u, ..*l }))
                    .collect();
            }
            for (k, nt) in new_tasks.into_iter().enumerate() {
                p.tasks.insert(at + k, nt);
            }
            p.normalize_outline();
            Ok(at)
        }) {
            self.select_index(first);
        }
    }

    fn fill_down(&mut self) {
        let rows = self.selected_rows_sorted();
        let Some(col) = self.columns.get(self.active_col).map(|c| c.id) else {
            return;
        };
        if rows.len() < 2 || col.edit_kind() == columns::EditKind::ReadOnly {
            return;
        }
        let src = columns::edit_text(&self.project, rows[0], col);
        self.edit_project("Fill Down", |p| {
            for &r in &rows[1..] {
                columns::set_cell(p, r, col, &src)?;
            }
            Ok(())
        });
    }

    pub fn find_next(&mut self) {
        let needle = self.dialogs.find_text.to_lowercase();
        if needle.is_empty() {
            self.dialogs.find_open = true;
            return;
        }
        let n = self.project.tasks.len();
        let start = self.active_index().map(|i| i + 1).unwrap_or(0);
        for k in 0..n {
            let i = (start + k) % n;
            if self.project.tasks[i].name.to_lowercase().contains(&needle) {
                // reveal collapsed ancestors
                for a in self.project.ancestors_of(i) {
                    self.project.tasks[a].collapsed = false;
                }
                self.select_index(i);
                self.set_status(format!("Found \"{}\" at task {}", self.dialogs.find_text, i + 1));
                return;
            }
        }
        self.set_status(format!("\"{}\" not found", self.dialogs.find_text));
    }

    // ------------------------------------------------------------ editing

    pub fn begin_edit(&mut self, initial: Option<String>) {
        let Some(col) = self.columns.get(self.active_col) else {
            return;
        };
        let kind = col.id.edit_kind();
        if kind == columns::EditKind::ReadOnly {
            self.set_status(format!("{} is a calculated field", col.id.title()));
            return;
        }
        let row = self.active_row;
        if row == RowRef::Blank && col.id != ColumnId::Name && initial.is_none() {
            // Project lets you start a task from any column; we start with the name.
        }
        let existing = match row {
            RowRef::Task(u) => match self.row_of_uid(u) {
                Some(i) => columns::edit_text(&self.project, i, col.id),
                None => return,
            },
            RowRef::Blank => String::new(),
        };
        let (text, select_all) = match initial {
            Some(t) => (t, false),
            None => (existing, true),
        };
        self.edit = Some(CellEdit {
            row,
            col: self.active_col,
            text,
            focus_requested: false,
            select_all,
            picker_open: false,
        });
    }

    /// Commit the in-progress cell edit (if any). Returns false on error.
    pub fn commit_edit(&mut self) -> bool {
        let Some(e) = self.edit.take() else {
            return true;
        };
        let Some(col) = self.columns.get(e.col).map(|c| c.id) else {
            return true;
        };
        match e.row {
            RowRef::Task(u) => {
                let Some(i) = self.row_of_uid(u) else { return true };
                if columns::edit_text(&self.project, i, col) == e.text {
                    return true; // unchanged
                }
                let text = e.text.clone();
                self.edit_project(&format!("Edit {}", col.title()), |p| columns::set_cell(p, i, col, &text))
                    .is_some()
            }
            RowRef::Blank => {
                if e.text.trim().is_empty() {
                    return true;
                }
                let text = e.text.clone();
                let made = self.edit_project("New Task", |p| {
                    let i = p.append_task("");
                    if col == ColumnId::Name {
                        p.tasks[i].name = text.clone();
                    } else {
                        columns::set_cell(p, i, col, &text)?;
                    }
                    Ok(i)
                });
                if let Some(i) = made {
                    let u = self.project.tasks[i].uid;
                    self.active_row = RowRef::Task(u);
                    self.selected = vec![u];
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn cancel_edit(&mut self) {
        self.edit = None;
    }

    pub fn move_active_to_last_task(&mut self, extend: bool) {
        let n = self.display_rows().iter().filter(|r| matches!(r, DisplayRow::Task(_))).count() as i32;
        self.move_active(-(i32::MAX / 2), 0, extend);
        self.move_active(n - 1, 0, extend);
    }

    /// Move the active cell by rows (in display order) and columns.
    pub fn move_active(&mut self, drow: i32, dcol: i32, extend: bool) {
        let rows = self.display_rows();
        let task_rows: Vec<usize> = rows
            .iter()
            .filter_map(|r| match r {
                DisplayRow::Task(i) => Some(*i),
                _ => None,
            })
            .collect();
        let cur_pos = match self.active_row {
            RowRef::Task(u) => self
                .row_of_uid(u)
                .and_then(|i| task_rows.iter().position(|&r| r == i))
                .unwrap_or(0) as i32,
            RowRef::Blank => task_rows.len() as i32,
        };
        let new_pos = (cur_pos + drow).clamp(0, task_rows.len() as i32);
        let new_row = if (new_pos as usize) < task_rows.len() {
            RowRef::Task(self.project.tasks[task_rows[new_pos as usize]].uid)
        } else {
            RowRef::Blank
        };
        let ncols = self.columns.len() as i32;
        self.active_col = (self.active_col as i32 + dcol).clamp(0, ncols - 1) as usize;
        if extend {
            if let (RowRef::Task(u), Some(anchor)) = (new_row, self.anchor) {
                // by position on screen (grouped views reorder tasks)
                let pos = |uid: u32| self.row_of_uid(uid).and_then(|i| task_rows.iter().position(|&r| r == i));
                if let (Some(a), Some(b)) = (pos(anchor), pos(u)) {
                    let (lo, hi) = (a.min(b), a.max(b));
                    self.selected = task_rows[lo..=hi].iter().map(|&r| self.project.tasks[r].uid).collect();
                }
            }
            self.active_row = new_row;
        } else {
            self.select_single(new_row);
        }
        match new_row {
            RowRef::Task(u) => self.ensure_visible = self.row_of_uid(u),
            RowRef::Blank => self.ensure_visible = Some(self.project.tasks.len()),
        }
    }
}

// ---------------------------------------------------------------- frame

impl ProjectFcgApp {
    /// Draw one frame. Separated from `eframe::App` so tests can drive it
    /// through a bare `egui::Context`.
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.inactive_rows = self.project.inactive_mask();
        theme::set_text_size(self.prefs.text_size);
        if (ctx.zoom_factor() - self.prefs.ui_zoom).abs() > 0.001 {
            ctx.set_zoom_factor(self.prefs.ui_zoom);
        }
        if !self.themed {
            theme::apply(&ctx);
            self.themed = true;
        }
        self.update_palette(&ctx);
        if let Some(a) = self.deferred_action.take() {
            self.run_pending(a, &ctx);
        }
        // Window close: ask to save.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(PendingAction::Quit);
            self.dialogs.save_prompt = true;
        }
        // Files dropped onto the window open like File ▸ Open.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.guarded(PendingAction::OpenPath(p), &ctx);
        }

        self.handle_keyboard(&ctx);
        self.update_title(&ctx);
        // Copy Picture: the screenshot asked for last frame
        if let Picture::Requested(slot) = &self.picture {
            let shot = slot.lock().ok().and_then(|mut s| s.take());
            if let Some(img) = shot {
                let region = img.region(&self.last_view_rect, Some(ctx.pixels_per_point()));
                match encode_png(&region) {
                    Ok(bytes) => {
                        self.picture = Picture::Ready(bytes);
                        self.pick_save(filebrowser::Mode::ExportPng);
                    }
                    Err(e) => {
                        self.picture = Picture::None;
                        self.error(format!("Couldn't make the picture: {e}"));
                    }
                }
            }
        }

        egui::Panel::top("ribbon")
            .resizable(false)
            .exact_size(28.0 + GROUP_ROW_H)
            .frame(egui::Frame::new().fill(theme::pal().ribbon))
            .show(ui, |ui| ribbon::show(self, ui));
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::pal().status_bar)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| self.status_bar(ui));
        if self.view.timeline {
            egui::Panel::top("timeline")
                .exact_size(64.0)
                .frame(egui::Frame::new().fill(theme::pal().bg))
                .show(ui, |ui| timeline::show(self, ui));
        }
        if self.view.inspector {
            egui::Panel::left("inspector")
                .resizable(true)
                .default_size(290.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::pal().panel)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show(ui, |ui| inspector::show(self, ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.main_view(ui));

        dialogs::show_all(self, &ctx);
        format_dialogs::show_all(self, &ctx);
        resource_dialogs::show_all(self, &ctx);
        board::show_dialog(self, &ctx);
        tracking_dialogs::show_all(self, &ctx);
        interchange::show_all(self, &ctx);
        sprint_dialogs::show_all(self, &ctx);
        view_dialogs::show_all(self, &ctx);
        release_dialogs::show_all(self, &ctx);
        self.check_filters();
        if let Some(mut fb) = self.file_browser.take() {
            match fb.show(&ctx) {
                filebrowser::Outcome::Open => self.file_browser = Some(fb),
                filebrowser::Outcome::Cancel => {}
                filebrowser::Outcome::Chosen(path) => self.finish_save_dialog(fb.mode, path),
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let col = if self.status_error { theme::pal().warn } else { theme::pal().text };
            ui.label(egui::RichText::new(&self.status).color(col).size(12.0));
            ui.separator();
            let mode = if self.project.settings.new_tasks_auto {
                "New Tasks : Auto Scheduled"
            } else {
                "New Tasks : Manually Scheduled"
            };
            ui.menu_button(egui::RichText::new(mode).size(12.0), |ui| {
                if ui
                    .selectable_label(self.project.settings.new_tasks_auto, "Auto Scheduled - Task dates are calculated by ProjectFCG")
                    .clicked()
                {
                    self.project.settings.new_tasks_auto = true;
                    ui.close();
                }
                if ui
                    .selectable_label(!self.project.settings.new_tasks_auto, "Manually Scheduled - Task dates are not automatically updated")
                    .clicked()
                {
                    self.project.settings.new_tasks_auto = false;
                    ui.close();
                }
            });
            if self.project.calc.resources.iter().any(|r| r.overallocated) {
                ui.separator();
                if ui
                    .link(egui::RichText::new("⚠ Overallocated resources").color(theme::pal().warn).size(12.0))
                    .on_hover_text("Go to the next overallocated task (Resource ▸ Next Overallocation)")
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.execute(Command::NextOverallocation, &ctx);
                }
            }
            if self.needs_calc {
                ui.separator();
                if ui.link(egui::RichText::new("Calculate").color(theme::pal().warn).size(12.0)).clicked() {
                    let ctx = ui.ctx().clone();
                    self.execute(Command::Calculate, &ctx);
                }
            }
            if self.filter != Filter::AllTasks {
                ui.separator();
                ui.label(egui::RichText::new(format!("Filter Applied: {}", self.filter_label(self.filter))).size(12.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ctx2 = ui.ctx().clone();
                // light / dark quick toggle
                let dark = theme::pal().dark;
                if ui
                    .small_button(if dark { "☀" } else { "🌙" })
                    .on_hover_text(if dark { "Switch to the light theme (Ctrl+Shift+F11)" } else { "Switch to the dark theme (Ctrl+Shift+F11)" })
                    .clicked()
                {
                    self.execute(Command::ToggleDarkMode, &ctx2);
                }
                ui.separator();
                // Text size quick buttons
                if ui.small_button("A+").on_hover_text("Larger text (Ctrl+Shift+>)").clicked() {
                    self.execute(Command::TextBigger, &ctx2);
                }
                ui.label(egui::RichText::new(format!("{} pt", self.prefs.text_size)).size(11.0).color(theme::pal().text_dim));
                if ui.small_button("A−").on_hover_text("Smaller text (Ctrl+Shift+<)").clicked() {
                    self.execute(Command::TextSmaller, &ctx2);
                }
                ui.separator();
                // Zoom slider, like Project's status bar.
                let mut z = (self.gantt.ppd.ln() - 0.05f32.ln()) / (2400f32.ln() - 0.05f32.ln());
                let r = ui.add(egui::Slider::new(&mut z, 0.0..=1.0).show_value(false));
                if r.changed() {
                    let ppd = (0.05f32.ln() + z * (2400f32.ln() - 0.05f32.ln())).exp();
                    self.zoom_to(ppd, 0.5);
                }
                ui.label(egui::RichText::new(Timescale::for_ppd(self.gantt.ppd).label()).size(12.0).color(theme::pal().text_dim));
                if let Some(f) = self.project.calc.finish {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("Finish: {}", fmt_date(f)))
                            .size(12.0)
                            .color(theme::pal().text_dim),
                    );
                }
            });
        });
    }

    fn main_view(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        self.last_view_rect = full;
        match self.view.main {
            MainView::ResourceSheet => return resource_sheet::show(self, ui, full),
            MainView::ResourceUsage => return usage::show(self, ui, full, usage::UsageKind::Resource),
            MainView::TaskUsage => return usage::show(self, ui, full, usage::UsageKind::Task),
            MainView::Board => return board::show(self, ui, full),
            MainView::Gantt => {}
        }
        let min_table = 120.0;
        let max_table = (full.width() - 150.0).max(min_table);
        self.table_width = self.table_width.clamp(min_table, max_table);
        let table_rect = egui::Rect::from_min_size(full.min, egui::vec2(self.table_width, full.height()));
        let split_rect = egui::Rect::from_min_size(
            egui::pos2(table_rect.right(), full.top()),
            egui::vec2(theme::SPLITTER_W, full.height()),
        );
        let gantt_rect = egui::Rect::from_min_max(egui::pos2(split_rect.right(), full.top()), full.max);

        // Shared vertical metrics
        let rows = self.display_rows();
        let rows_area_h = full.height() - theme::header_h() - theme::SCROLLBAR;
        self.rows_view_h = rows_area_h;
        self.gantt_view_w = gantt_rect.width() - theme::SCROLLBAR;
        let content_h = (rows.len() as f32 + 3.0) * theme::row_h();
        let max_scroll = (content_h - rows_area_h).max(0.0);

        // Keep the active row on screen after keyboard navigation.
        if let Some(target) = self.ensure_visible.take() {
            let pos = if target >= self.project.tasks.len() {
                Some(rows.len())
            } else {
                rows.iter().position(|r| *r == DisplayRow::Task(target))
            };
            if let Some(pos) = pos {
                let y = pos as f32 * theme::row_h();
                if y < self.scroll_y {
                    self.scroll_y = y;
                } else if y + theme::row_h() > self.scroll_y + rows_area_h {
                    self.scroll_y = y + theme::row_h() - rows_area_h;
                }
            }
        }
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll.max(0.0));

        self.last_table_rect = table_rect;
        self.last_gantt_rect = gantt_rect;
        table::show(self, ui, table_rect, &rows);

        // Splitter
        let sresp = ui.interact(split_rect, ui.scope_id().with("splitter"), egui::Sense::drag());
        if sresp.hovered() || sresp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if sresp.dragged() {
            self.table_width = (self.table_width + sresp.drag_delta().x).clamp(min_table, max_table);
        }
        let painter = ui.painter();
        painter.rect_filled(split_rect, 0.0, theme::pal().splitter);
        painter.line_segment(
            [split_rect.center_top(), split_rect.center_bottom()],
            egui::Stroke::new(1.0, theme::pal().header_border),
        );

        gantt::show(self, ui, gantt_rect, &rows);

        // Vertical scrollbar (right edge of gantt)
        let vbar = egui::Rect::from_min_max(
            egui::pos2(gantt_rect.right() - theme::SCROLLBAR, gantt_rect.top() + theme::header_h()),
            egui::pos2(gantt_rect.right(), gantt_rect.bottom() - theme::SCROLLBAR),
        );
        if let Some(v) = scrollbar(ui, vbar, true, self.scroll_y, rows_area_h, content_h.max(rows_area_h), "vscroll") {
            self.scroll_y = v.clamp(0.0, max_scroll);
        }
    }

    /// Keyboard for the Resource Sheet and usage views: shortcuts work as
    /// in the Gantt view; navigation and typing go to the view.
    fn other_view_keys(&mut self, ctx: &egui::Context, events: &[egui::Event]) {
        for ev in events {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(cmd) = commands::shortcut(*key, *modifiers) {
                        self.execute(cmd, ctx);
                        continue;
                    }
                    if modifiers.alt {
                        continue;
                    }
                    match self.view.main {
                        MainView::ResourceSheet => {
                            self.res_key(*key, *modifiers);
                        }
                        MainView::ResourceUsage | MainView::TaskUsage => {
                            let kind = if self.view.main == MainView::ResourceUsage {
                                usage::UsageKind::Resource
                            } else {
                                usage::UsageKind::Task
                            };
                            match key {
                                egui::Key::ArrowDown => usage::move_selection(self, kind, 1),
                                egui::Key::ArrowUp => usage::move_selection(self, kind, -1),
                                egui::Key::PageDown => usage::move_selection(self, kind, 20),
                                egui::Key::PageUp => usage::move_selection(self, kind, -20),
                                _ => {}
                            }
                        }
                        MainView::Board => board::key(self, *key, *modifiers),
                        MainView::Gantt => {}
                    }
                }
                egui::Event::Text(t) => {
                    let m = ctx.input(|i| i.modifiers);
                    if let Some(cmd) = commands::text_shortcut(t, m) {
                        self.execute(cmd, ctx);
                        continue;
                    }
                    if m.ctrl || m.command || m.alt {
                        continue;
                    }
                    if self.view.main == MainView::ResourceSheet && !t.chars().all(|c| c.is_control()) {
                        if let Some(e) = self.sheet.edit.as_mut() {
                            e.text.push_str(t);
                        } else if self.sheet.columns.get(self.sheet.col).is_some_and(|c| c.0.editable()) {
                            self.res_begin_edit(Some(t.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if self.dialogs.any_modal_open() || self.file_browser.is_some() {
            return;
        }
        // While a cell editor has focus, it owns the keyboard except for
        // the keys the table handles (Enter/Tab/Esc) — see table.rs.
        if self.edit.is_some() || self.sheet.edit.is_some() {
            return;
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        if self.view.main != MainView::Gantt {
            self.other_view_keys(ctx, &events);
            return;
        }
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(cmd) = commands::shortcut(key, modifiers) {
                        self.execute(cmd, ctx);
                        continue;
                    }
                    let ctrl = modifiers.ctrl || modifiers.command;
                    let shift = modifiers.shift;
                    if modifiers.alt {
                        continue;
                    }
                    match key {
                        // Ctrl+Down: last task row (not the blank row), as in Project
                        egui::Key::ArrowDown if ctrl => self.move_active_to_last_task(shift),
                        egui::Key::ArrowUp if ctrl => self.move_active(-(i32::MAX / 2), 0, shift),
                        egui::Key::ArrowDown => self.move_active(1, 0, shift),
                        egui::Key::ArrowUp => self.move_active(-1, 0, shift),
                        egui::Key::ArrowLeft if ctrl => self.move_active(0, -1000, false),
                        egui::Key::ArrowRight if ctrl => self.move_active(0, 1000, false),
                        egui::Key::ArrowLeft => self.move_active(0, -1, false),
                        egui::Key::ArrowRight => self.move_active(0, 1, false),
                        egui::Key::Home if ctrl => {
                            self.active_col = 0;
                            self.move_active(-(i32::MAX / 2), 0, shift);
                        }
                        egui::Key::End if ctrl => {
                            self.active_col = self.columns.len().saturating_sub(1);
                            self.move_active_to_last_task(shift);
                        }
                        egui::Key::Home => self.active_col = 0,
                        egui::Key::End => self.active_col = self.columns.len().saturating_sub(1),
                        egui::Key::PageDown => {
                            let n = (self.rows_view_h / theme::row_h()).floor().max(1.0) as i32;
                            self.move_active(n, 0, shift);
                        }
                        egui::Key::PageUp => {
                            let n = (self.rows_view_h / theme::row_h()).floor().max(1.0) as i32;
                            self.move_active(-n, 0, shift);
                        }
                        egui::Key::Enter => {
                            if shift {
                                self.move_active(-1, 0, false)
                            } else {
                                self.move_active(1, 0, false)
                            }
                        }
                        egui::Key::Tab => self.move_active(0, if shift { -1 } else { 1 }, false),
                        egui::Key::Escape => {
                            if let RowRef::Task(u) = self.active_row {
                                self.selected = vec![u];
                            }
                        }
                        egui::Key::Backspace => self.begin_edit(Some(String::new())),
                        _ => {}
                    }
                }
                egui::Event::Text(t) => {
                    let m = ctx.input(|i| i.modifiers);
                    if let Some(cmd) = commands::text_shortcut(&t, m) {
                        self.execute(cmd, ctx);
                        continue;
                    }
                    if m.ctrl || m.command || m.alt {
                        continue;
                    }
                    // Typing into a cell replaces its contents (like Project/Excel).
                    if self.edit.is_none() && !t.chars().all(|c| c.is_control()) {
                        let ok = self
                            .columns
                            .get(self.active_col)
                            .is_some_and(|c| c.id.edit_kind() != columns::EditKind::ReadOnly);
                        if ok {
                            self.begin_edit(Some(t));
                        }
                    } else if let Some(e) = self.edit.as_mut() {
                        // further characters typed in the same frame
                        e.text.push_str(&t);
                    }
                }
                egui::Event::Copy => self.execute(Command::Copy, ctx),
                egui::Event::Cut => self.execute(Command::Cut, ctx),
                egui::Event::Paste(_) => self.execute(Command::Paste, ctx),
                _ => {}
            }
        }
    }
}

/// A simple scrollbar; returns a new offset if the user moved it.
pub fn scrollbar(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    vertical: bool,
    offset: f32,
    view: f32,
    content: f32,
    id: &str,
) -> Option<f32> {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::pal().scroll_track);
    let len = if vertical { rect.height() } else { rect.width() };
    if content <= 0.0 || len <= 4.0 {
        return None;
    }
    let frac_view = (view / content).clamp(0.05, 1.0);
    let thumb_len = (len * frac_view).max(20.0).min(len);
    let travel = (len - thumb_len).max(0.0);
    let max_off = (content - view).max(0.0);
    let t = if max_off > 0.0 { offset / max_off } else { 0.0 };
    let thumb = if vertical {
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, rect.top() + travel * t),
            egui::vec2(rect.width() - 4.0, thumb_len),
        )
    } else {
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + travel * t, rect.top() + 2.0),
            egui::vec2(thumb_len, rect.height() - 4.0),
        )
    };
    let resp = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click_and_drag());
    let col = if resp.dragged() || resp.hovered() {
        theme::pal().scroll_thumb_hover
    } else {
        theme::pal().scroll_thumb
    };
    ui.painter().rect_filled(thumb, 4.0, col);
    if (resp.dragged() || resp.clicked()) && travel > 0.0 {
        if let Some(p) = resp.interact_pointer_pos() {
            let pos = if vertical { p.y - rect.top() } else { p.x - rect.left() };
            let nt = ((pos - thumb_len / 2.0) / travel).clamp(0.0, 1.0);
            return Some(nt * max_off);
        }
    }
    None
}

impl ProjectFcgApp {
    /// Is the interface dark this frame?
    pub fn is_dark(&self, ctx: &egui::Context) -> bool {
        match self.prefs.theme_mode {
            theme::ThemeMode::Light => false,
            theme::ThemeMode::Dark => true,
            theme::ThemeMode::System => system_theme::prefers_dark()
                .or_else(|| ctx.system_theme().map(|t| t == egui::Theme::Dark))
                .unwrap_or(false),
        }
    }

    fn update_palette(&mut self, ctx: &egui::Context) {
        system_theme::set_wanted(self.prefs.theme_mode == theme::ThemeMode::System);
        let dark = self.is_dark(ctx);
        let key = (self.prefs.accent, dark);
        let pal = theme::Palette::new(key.0, key.1);
        theme::set_palette(pal);
        if self.applied_theme != Some(key) {
            theme::apply_palette(ctx, &pal);
            self.applied_theme = Some(key);
        }
    }

    pub fn set_theme_mode(&mut self, m: theme::ThemeMode) {
        self.prefs.theme_mode = m;
        self.save_prefs();
        self.set_status(format!("Theme: {}", m.label()));
    }

    pub fn set_accent(&mut self, a: theme::Accent) {
        self.prefs.accent = a;
        self.save_prefs();
        self.set_status(format!("Theme colour: {}", a.label()));
    }
}

impl eframe::App for ProjectFcgApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

/// Entry point used by `main.rs`.
/// `sample`: None = blank project, Some(false) = the classic sample,
/// Some(true) = the agile (sprints + board) sample.
pub fn run(open: Option<PathBuf>, sample: Option<bool>) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_app_id("io.github.projectfcg")
            .with_inner_size([1360.0, 820.0])
            .with_min_inner_size([720.0, 420.0])
            .with_icon(std::sync::Arc::new(app_icon())),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            system_theme::start_watcher(cc.egui_ctx.clone());
            let mut app = match sample {
                Some(false) => ProjectFcgApp::new(sample::software_release()),
                Some(true) => {
                    let mut a = ProjectFcgApp::new(sample::agile_release());
                    a.view.main = MainView::Board;
                    a.tab = RibbonTab::Board;
                    a
                }
                None => ProjectFcgApp::default(),
            };
            if let Some(path) = prefs::default_path() {
                let loaded = prefs::load_with_migration(&path);
                // apply the saved text size through the normal path so
                // column widths scale to match
                let size = loaded.text_size;
                app.prefs.ui_zoom = loaded.ui_zoom;
                app.prefs.theme_mode = loaded.theme_mode;
                app.prefs.accent = loaded.accent;
                app.set_text_size(size);
                app.prefs_path = Some(path);
            }
            if let Some(p) = open {
                app.open_path(p);
            }
            Ok(Box::new(app))
        }),
    )
}

/// 64×64 window icon (raw RGBA rendered from packaging/projectfcg.svg).
pub fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!("../../packaging/icon64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}

/// "Baseline" for 0, "Baseline 1" … "Baseline 10".
pub fn baseline_name(n: usize) -> String {
    if n == 0 {
        "Baseline".into()
    } else {
        format!("Baseline {n}")
    }
}
