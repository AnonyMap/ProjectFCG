//! Search (Alt+Q), as in Outlook and Word ("Search for what you want to
//! do"): type what you want to do and pick the action, without knowing
//! which tab it's on or what it's called.
//!
//! * Every action in the ribbon (and the views, tables, filters, groups,
//!   sorts and timescales in its lists) is in `catalog()`, with its ribbon
//!   location, its shortcut and tags: other words people use for it
//!   ("remove" for Delete, "dependency" for Link, "holiday" for Change
//!   Working Time).
//! * Matching is forgiving: word prefixes, plurals and small typos count,
//!   and every word typed has to match something.
//! * Before anything is typed it shows the recently used actions (kept in
//!   the preferences) and a few suggestions. Below the actions: search the
//!   plan for the words instead (Ctrl+E, item_search.rs), and Find.
//! * Unavailable actions are shown greyed, with the reason.

use super::commands::{Command, Filter, Timescale};
use super::ProjectFcgApp;
use egui::{pos2, text::LayoutJob, vec2, Align2, Color32, FontId, Id, Key, Modifiers, Rect, Sense, TextFormat};

/// The search box's id (Alt+Q focuses it).
pub fn box_id() -> Id {
    Id::unique("tell_me_box")
}

#[derive(Clone, Copy)]
pub enum Run {
    Cmd(Command),
    Do(fn(&mut ProjectFcgApp, &egui::Context)),
}

#[derive(Clone)]
pub struct Action {
    /// Stable key (recently used actions are remembered by it).
    pub id: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
    /// Where it lives: "Task ▸ Insert ▸ Task".
    pub path: &'static str,
    pub keys: &'static str,
    /// Other words for it.
    pub tags: &'static str,
    pub run: Run,
    /// Available? And if not, why.
    pub when: Option<(fn(&ProjectFcgApp) -> bool, &'static str)>,
    /// An on/off setting: is it on?
    pub on: Option<fn(&ProjectFcgApp) -> bool>,
}

impl Action {
    pub fn available(&self, app: &ProjectFcgApp) -> bool {
        self.when.is_none_or(|(f, _)| f(app))
    }
    fn when(mut self, f: fn(&ProjectFcgApp) -> bool, why: &'static str) -> Self {
        self.when = Some((f, why));
        self
    }
    fn on(mut self, f: fn(&ProjectFcgApp) -> bool) -> Self {
        self.on = Some(f);
        self
    }
}

fn a(id: &'static str, icon: &'static str, label: &'static str, path: &'static str, keys: &'static str, tags: &'static str, run: Run) -> Action {
    Action { id, icon, label, path, keys, tags, run, when: None, on: None }
}

use Run::{Cmd, Do};

fn task_selected(app: &ProjectFcgApp) -> bool {
    app.active_index().is_some()
}
fn two_tasks(app: &ProjectFcgApp) -> bool {
    app.selected_rows_sorted().len() >= 2
}
const SELECT_TASK: &str = "Select a task first";

/// Every action Search can find.
pub fn catalog() -> Vec<Action> {
    use super::MainView as V;
    use crate::format::GanttViewKind as G;
    use crate::model::ResourceType as R;
    let mut v = vec![
        // ---- File
        a("new", "🗋", "New Project", "File ▸ Project ▸ New", "Ctrl+N", "blank empty start create fresh plan schedule file", Cmd(Command::New)),
        a("open", "📂", "Open", "File ▸ Project ▸ Open", "Ctrl+O", "load file browse existing import mpp project xml pfcg", Cmd(Command::Open)),
        a("save", "💾", "Save", "File ▸ Project ▸ Save", "Ctrl+S", "store write keep file disk", Cmd(Command::Save)),
        a("save_as", "💾", "Save As", "File ▸ Project ▸ Save As", "F12", "copy rename new name duplicate file location", Cmd(Command::SaveAs)),
        a("sample", "⭐", "Open the Sample Project", "File ▸ Templates ▸ Sample Project", "", "example demo template software release tutorial", Cmd(Command::NewFromSample)),
        a("agile_sample", "📋", "Open the Agile Sample", "File ▸ Templates ▸ Agile Sample", "", "example demo template scrum sprints kanban tickets backlog", Cmd(Command::NewFromAgileSample)),
        a("print", "🖶", "Print", "File ▸ Print ▸ Print…", "Ctrl+P", "pdf paper page setup printer output report", Cmd(Command::PrintDlg)),
        a("save_pdf", "📄", "Save as PDF", "File ▸ Print ▸ Print… ▸ Save PDF", "", "pdf export print document report share", Cmd(Command::SavePdf)),
        a("export_xml", "📤", "Save as Microsoft Project XML", "File ▸ Share ▸ Save as XML", "", "export ms project projectlibre mpxj xml share msp", Cmd(Command::ExportXml)),
        a("export_csv", "📊", "Export Tasks as CSV", "File ▸ Share ▸ Export CSV", "", "excel spreadsheet sheets table export comma", Cmd(Command::ExportCsv)),
        a("import_jira", "📥", "Import Jira CSV", "File ▸ Share ▸ Import Jira CSV", "", "jira atlassian issues tickets import load csv", Cmd(Command::ImportJiraCsv)),
        a("export_jira", "📤", "Export Jira CSV", "File ▸ Share ▸ Export Jira CSV", "", "jira atlassian issues tickets export csv", Cmd(Command::ExportJiraCsv)),
        a("options", "⚙", "Options", "File ▸ Settings ▸ Options", "", "settings preferences hours per day default start time currency autorecover configuration", Cmd(Command::Options)),
        // ---- Clipboard and editing
        a("undo", "⟲", "Undo", "Quick Access ▸ Undo", "Ctrl+Z", "revert back mistake reverse", Cmd(Command::Undo)).when(|a| a.history.can_undo(), "Nothing to undo"),
        a("redo", "⟳", "Redo", "Quick Access ▸ Redo", "Ctrl+Y", "again repeat forward", Cmd(Command::Redo)).when(|a| a.history.can_redo(), "Nothing to redo"),
        a("cut", "✂", "Cut Tasks", "Task ▸ Clipboard ▸ Cut", "Ctrl+X", "move clipboard", Cmd(Command::Cut)).when(task_selected, SELECT_TASK),
        a("copy", "🗐", "Copy Tasks", "Task ▸ Clipboard ▸ Copy", "Ctrl+C", "duplicate clipboard", Cmd(Command::Copy)).when(task_selected, SELECT_TASK),
        a("paste", "📋", "Paste Tasks", "Task ▸ Clipboard ▸ Paste", "Ctrl+V", "clipboard insert", Cmd(Command::Paste)),
        a("copy_picture", "📷", "Copy Picture", "Task ▸ Clipboard ▸ Copy Picture", "", "screenshot image png snapshot export picture capture", Cmd(Command::CopyPicture)),
        a("fill_down", "⬇", "Fill Down", "Task ▸ Editing", "Ctrl+D", "copy value repeat cells", Cmd(Command::FillDown)).when(task_selected, SELECT_TASK),
        a("reset_field", "✖", "Reset Field", "Task ▸ Editing", "Ctrl+Delete", "clear default value blank", Cmd(Command::ResetField)).when(task_selected, SELECT_TASK),
        a("select_all", "▦", "Select All Tasks", "Task ▸ Editing", "Ctrl+A", "everything", Cmd(Command::SelectAll)),
        a("find", "🔍", "Find", "Task ▸ Editing ▸ Find", "Ctrl+F", "search look locate", Cmd(Command::Find)),
        a("replace", "🔁", "Replace", "Task ▸ Editing ▸ Replace", "Ctrl+H", "find substitute change rename bulk", Cmd(Command::ReplaceDlg)),
        a("search_items", "🔍", "Search Tasks, Tickets, People and Risks", "Title bar ▸ Search", "Ctrl+E", "find look up locate item ticket key task id person resource risk", Do(|a, c| a.focus_item_search(c, None))),
        a("go_to", "👉", "Go To", "Task ▸ Editing ▸ Go To", "F5", "jump id date navigate", Cmd(Command::GoTo)),
        a("scroll_to_task", "🎯", "Scroll to Task", "Task ▸ Editing ▸ Scroll to Task", "Ctrl+Shift+F5", "show bar chart navigate locate", Cmd(Command::ScrollToTask)).when(task_selected, SELECT_TASK),
        // ---- Tasks
        a("insert_task", "➕", "Insert Task", "Task ▸ Insert ▸ Task", "Insert", "add new create row activity", Cmd(Command::InsertTask)),
        a("insert_summary", "📑", "Insert Summary Task", "Task ▸ Insert ▸ Summary", "", "add group phase parent wbs heading", Cmd(Command::InsertSummary)),
        a("insert_milestone", "🔶", "Insert Milestone", "Task ▸ Insert ▸ Milestone", "", "add new event deliverable checkpoint gate zero duration diamond", Cmd(Command::InsertMilestone)),
        a("insert_recurring", "🔁", "Insert Recurring Task", "Task ▸ Insert ▸ Recurring…", "", "add repeating repeat weekly monthly meeting standup routine", Cmd(Command::InsertRecurring)),
        a("delete_task", "🗑", "Delete Tasks", "Task ▸ Editing", "Delete", "remove erase drop", Cmd(Command::DeleteTasks)).when(task_selected, SELECT_TASK),
        a("inactivate", "⊘", "Inactivate Task", "Task ▸ Tasks ▸ Inactivate", "", "disable what if cancel exclude deactivate activate", Cmd(Command::ToggleActive)).when(task_selected, SELECT_TASK),
        a("indent", "➡", "Indent Task", "Task ▸ Schedule ▸ Indent", "Alt+Shift+→", "subtask demote child nest outline level", Cmd(Command::Indent)).when(task_selected, SELECT_TASK),
        a("outdent", "⬅", "Outdent Task", "Task ▸ Schedule ▸ Outdent", "Alt+Shift+←", "promote parent unnest outline level", Cmd(Command::Outdent)).when(task_selected, SELECT_TASK),
        a("move_up", "⬆", "Move Task Up", "Task ▸ Tasks ▸ Move Up", "Alt+Shift+↑", "reorder order", Cmd(Command::MoveUp)).when(task_selected, SELECT_TASK),
        a("move_down", "⬇", "Move Task Down", "Task ▸ Tasks ▸ Move Down", "Alt+Shift+↓", "reorder order", Cmd(Command::MoveDown)).when(task_selected, SELECT_TASK),
        a("link", "🔗", "Link Tasks", "Task ▸ Schedule ▸ Link", "Ctrl+F2", "dependency dependencies predecessor successor connect chain finish start fs relationship", Cmd(Command::LinkTasks)).when(two_tasks, "Select two or more tasks"),
        a("unlink", "✖", "Unlink Tasks", "Task ▸ Schedule ▸ Unlink", "Ctrl+Shift+F2", "remove dependency dependencies predecessor disconnect break link", Cmd(Command::UnlinkTasks)).when(task_selected, SELECT_TASK),
        a("manual", "📌", "Manually Schedule", "Task ▸ Tasks ▸ Manually Schedule", "Ctrl+Shift+M", "pin fixed dates manual mode", Cmd(Command::SetManual)).when(task_selected, SELECT_TASK),
        a("auto", "🔄", "Auto Schedule", "Task ▸ Tasks ▸ Auto Schedule", "Ctrl+Alt+Shift+A", "automatic calculate engine auto mode", Cmd(Command::SetAuto)).when(task_selected, SELECT_TASK),
        a("new_task_mode", "⚙", "New Tasks: Auto or Manually Scheduled", "Status bar ▸ New Tasks", "", "default mode new task scheduling", Cmd(Command::ToggleNewTaskMode)).on(|a| a.project.settings.new_tasks_auto),
        a("task_info", "ℹ", "Task Information", "Task ▸ Properties ▸ Information", "Shift+F2", "details properties edit dialog constraint deadline calendar priority notes", Cmd(Command::TaskInfo)).when(task_selected, SELECT_TASK),
        a("notes", "📝", "Task Notes", "Task ▸ Properties ▸ Notes", "", "comment memo remarks description", Cmd(Command::TaskNotes)).when(task_selected, SELECT_TASK),
        a("add_to_timeline", "📅", "Add to Timeline", "Task ▸ Properties ▸ Add to Timeline", "", "timeline show highlight bar roadmap", Cmd(Command::AddToTimeline)).when(task_selected, SELECT_TASK),
        a("mark_milestone", "🔶", "Mark as Milestone", "Task ▸ Properties ▸ Mark Milestone", "", "milestone flag diamond convert", Cmd(Command::MarkMilestone)).when(task_selected, SELECT_TASK),
        a("move_tasks", "📆", "Move Tasks by Days", "Task ▸ Tasks ▸ Move", "", "shift delay postpone push later earlier slip reschedule", Cmd(Command::MoveTasksDlg)).when(task_selected, SELECT_TASK),
        a("move_1w", "⏩", "Move Tasks 1 Week Later", "Task ▸ Tasks ▸ Move", "", "shift delay postpone push week", Cmd(Command::MoveTasks(5))).when(task_selected, SELECT_TASK),
        a("hide_subtasks", "⊟", "Hide Subtasks", "View ▸ Data ▸ Outline", "Alt+Shift+-", "collapse fold close outline", Cmd(Command::HideSubtasks)).when(task_selected, SELECT_TASK),
        a("show_subtasks", "⊞", "Show Subtasks", "View ▸ Data ▸ Outline", "Alt+Shift+=", "expand unfold open outline", Cmd(Command::ShowSubtasks)).when(task_selected, SELECT_TASK),
        a("show_all_subtasks", "⊞", "Show All Subtasks", "View ▸ Data ▸ Outline ▸ All Subtasks", "", "expand everything outline", Cmd(Command::ShowAllSubtasks)),
        a("inspect", "🔎", "Task Inspector", "Task ▸ Inspect", "", "why start date drivers explain problem issues warnings fix", Cmd(Command::ToggleInspector)).on(|a| a.view.inspector),
        // ---- Task path
        a("path_driving", "⛓", "Highlight Driving Predecessors", "Format ▸ Bar Styles ▸ Task Path", "", "task path drivers critical chain what drives why", Cmd(Command::TaskPath(super::bars::PathRole::DrivingPredecessor))).when(task_selected, SELECT_TASK),
        a("path_pred", "⛓", "Highlight Predecessors", "Format ▸ Bar Styles ▸ Task Path", "", "task path dependencies before", Cmd(Command::TaskPath(super::bars::PathRole::Predecessor))).when(task_selected, SELECT_TASK),
        a("path_succ", "⛓", "Highlight Successors", "Format ▸ Bar Styles ▸ Task Path", "", "task path dependencies after impact", Cmd(Command::TaskPath(super::bars::PathRole::Successor))).when(task_selected, SELECT_TASK),
        a("path_driven", "⛓", "Highlight Driven Successors", "Format ▸ Bar Styles ▸ Task Path", "", "task path impact affected downstream", Cmd(Command::TaskPath(super::bars::PathRole::DrivenSuccessor))).when(task_selected, SELECT_TASK),
        a("path_clear", "⛓", "Clear Task Path", "Format ▸ Bar Styles ▸ Task Path", "", "remove highlighting task path off", Cmd(Command::ClearTaskPath)),
        // ---- Tracking
        a("pct_0", "◔", "0% Complete", "Task ▸ Tracking", "", "progress percent not started reset", Cmd(Command::SetPercent(0))).when(task_selected, SELECT_TASK),
        a("pct_25", "◔", "25% Complete", "Task ▸ Tracking", "", "progress percent quarter", Cmd(Command::SetPercent(25))).when(task_selected, SELECT_TASK),
        a("pct_50", "◑", "50% Complete", "Task ▸ Tracking", "", "progress percent half", Cmd(Command::SetPercent(50))).when(task_selected, SELECT_TASK),
        a("pct_75", "◕", "75% Complete", "Task ▸ Tracking", "", "progress percent", Cmd(Command::SetPercent(75))).when(task_selected, SELECT_TASK),
        a("pct_100", "●", "100% Complete", "Task ▸ Tracking", "", "progress percent done finished complete mark done", Cmd(Command::SetPercent(100))).when(task_selected, SELECT_TASK),
        a("mark_on_track", "✔", "Mark on Track", "Task ▸ Tracking ▸ Mark on Track", "", "progress status date on schedule", Cmd(Command::MarkOnTrack)).when(task_selected, SELECT_TASK),
        a("update_tasks", "📝", "Update Tasks", "Task ▸ Tracking ▸ Update Tasks", "", "progress actual start finish remaining duration percent complete status", Cmd(Command::UpdateTasks)).when(task_selected, SELECT_TASK),
        a("update_project", "🔄", "Update Project", "Project ▸ Status ▸ Update Project", "", "progress status date complete through reschedule uncompleted work", Cmd(Command::UpdateProject)),
        a("move_incomplete", "⏩", "Move Incomplete Parts to Status Date", "Project ▸ Status ▸ Update Project", "", "reschedule remaining unfinished work status date", Do(|a, _| a.move_incomplete_to_status_date())).when(task_selected, SELECT_TASK),
        a("statistics", "📊", "Project Statistics", "Project ▸ Status ▸ Statistics", "", "summary totals cost work variance earned value spi cpi report", Cmd(Command::ProjectStatistics)),
        a("set_baseline", "📌", "Set Baseline", "Project ▸ Baseline ▸ Set Baseline", "", "snapshot save plan original compare freeze", Cmd(Command::SetBaseline)),
        a("clear_baseline", "🗑", "Clear Baseline", "Project ▸ Baseline ▸ Clear Baseline", "", "remove delete baseline reset", Cmd(Command::ClearBaseline)),
        // ---- Project
        a("project_info", "ℹ", "Project Information", "Project ▸ Properties ▸ Project Information", "", "start date finish date schedule from status date calendar title properties", Cmd(Command::ProjectInfo)),
        a("working_time", "📅", "Change Working Time", "Project ▸ Properties ▸ Change Working Time", "", "calendar holiday holidays vacation day off nonworking exceptions hours shift weekend work week", Cmd(Command::WorkingTime)),
        a("custom_fields", "🏷", "Custom Fields", "Project ▸ Properties ▸ Custom Fields", "", "text number flag date cost user defined columns rename field", Cmd(Command::CustomFieldsDlg)),
        a("calculate", "🔃", "Calculate Project", "Project ▸ Schedule ▸ Calculate Project", "F9", "recalculate reschedule refresh update compute", Cmd(Command::Calculate)),
        a("auto_calc", "🔃", "Automatic Calculation", "Project ▸ Schedule", "Ctrl+F9", "recalculate manual calculation automatic on off", Cmd(Command::ToggleAutoCalc)).on(|a| a.auto_calc),
        a("multiple_cp", "🔥", "Multiple Critical Paths", "Project ▸ Critical Path", "", "critical path independent chains", Cmd(Command::ToggleMultipleCriticalPaths)).on(|a| a.project.settings.multiple_critical_paths),
        a("show_cp", "🔥", "Show the Critical Path", "Project ▸ Critical Path ▸ Show Critical Path", "", "critical path cpm longest chain red detail gantt", Do(|a, c| {
            a.execute(Command::SetGanttView(G::Detail), c);
            a.execute(Command::SetFilter(Filter::Critical), c);
        })),
        // ---- Resources
        a("add_work_res", "👷", "Add a Work Resource", "Resource ▸ Add Resources ▸ Work Resource", "", "person people team member staff worker employee equipment human new resource", Cmd(Command::AddResource(R::Work))),
        a("add_material_res", "📦", "Add a Material Resource", "Resource ▸ Add Resources ▸ Material Resource", "", "consumable supplies material new resource", Cmd(Command::AddResource(R::Material))),
        a("add_cost_res", "💲", "Add a Cost Resource", "Resource ▸ Add Resources ▸ Cost Resource", "", "expense travel fee money budget new resource", Cmd(Command::AddResource(R::Cost))),
        a("assign", "👥", "Assign Resources", "Resource ▸ Assignments ▸ Assign Resources", "Alt+F10", "assign people staff allocate who team member", Cmd(Command::AssignResources)).when(task_selected, SELECT_TASK),
        a("resource_info", "ℹ", "Resource Information", "Resource ▸ Properties ▸ Information", "", "rate cost calendar max units resource details", Cmd(Command::ResourceInfo)),
        a("level_all", "⚖", "Level All Resources", "Resource ▸ Level ▸ Level All", "", "leveling overallocation overallocated overbooked conflicts resolve balance workload", Cmd(Command::LevelAll)),
        a("level_selection", "⚖", "Level Selected Tasks", "Resource ▸ Level ▸ Level Selection", "", "leveling overallocation overallocated resolve", Cmd(Command::LevelSelection)).when(task_selected, SELECT_TASK),
        a("clear_leveling", "✖", "Clear Leveling", "Resource ▸ Level ▸ Clear Leveling", "", "remove leveling delays undo leveling reset", Cmd(Command::ClearLeveling)),
        a("leveling_options", "⚙", "Leveling Options", "Resource ▸ Level ▸ Leveling Options", "", "leveling order priority slack settings", Cmd(Command::LevelingOptions)),
        a("next_overallocation", "➡", "Next Overallocation", "Resource ▸ Level ▸ Next Overallocation", "", "overallocated overbooked conflict find red", Cmd(Command::NextOverallocation)),
        // ---- Board
        a("new_sprint", "📆", "New Sprint", "Board ▸ Sprints ▸ New Sprint", "", "iteration agile scrum add sprint", Cmd(Command::NewSprint)),
        a("start_sprint", "▶", "Start Sprint", "Board ▸ Sprints ▸ Start Sprint", "", "begin iteration commit goal agile scrum", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.open_sprint_dialog(super::sprint_dialogs::SprintAction::Start);
        })),
        a("complete_sprint", "✔", "Complete Sprint", "Board ▸ Sprints ▸ Complete Sprint", "", "finish end close iteration velocity agile scrum", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.open_sprint_dialog(super::sprint_dialogs::SprintAction::Complete);
        })),
        a("sprint_goal", "✏", "Sprint Goal", "Board ▸ Sprints ▸ Sprint Goal…", "", "objective iteration agile", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.open_sprint_dialog(super::sprint_dialogs::SprintAction::Goal);
        })),
        a("board_columns", "▥", "Board Columns", "Board ▸ Sprints ▸ Columns…", "", "kanban workflow status wip limits lanes", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.open_columns_dialog();
        })),
        a("toggle_sprint", "📆", "Mark Task as Sprint", "Task ▸ context menu ▸ Mark as Sprint", "", "sprint iteration agile convert unmark", Cmd(Command::ToggleSprint)).when(task_selected, SELECT_TASK),
        a("open_on_board", "📋", "Open Sprint on the Board", "Task ▸ context menu", "", "board kanban sprint tickets", Cmd(Command::OpenOnBoard)).when(task_selected, SELECT_TASK),
        a("new_ticket", "✚", "New Ticket", "Board ▸ Tickets ▸ New Ticket", "", "story bug issue card user story add jira", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.execute(Command::NewTicket, c);
        })),
        a("new_epic", "🏷", "New Epic", "Board ▸ Tickets ▸ New Epic", "", "feature theme group stories initiative", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.execute(Command::NewEpic, c);
        })),
        a("edit_ticket", "✏", "Edit Ticket", "Board ▸ Tickets ▸ Edit", "Enter", "story bug issue card change", Cmd(Command::EditTicket)).when(|a| a.view.main == V::Board && a.board.selected.is_some(), "Select a ticket on the Board first"),
        a("ticket_backlog", "📥", "Send Ticket to Backlog", "Board ▸ Tickets ▸ To Backlog", "", "move story backlog unplanned", Cmd(Command::TicketToBacklog)).when(|a| a.view.main == V::Board && a.board.selected.is_some(), "Select a ticket on the Board first"),
        a("delete_ticket", "✖", "Delete Ticket", "Board ▸ Tickets ▸ Delete", "Delete", "remove story bug issue card", Cmd(Command::DeleteTicket)).when(|a| a.view.main == V::Board && a.board.selected.is_some(), "Select a ticket on the Board first"),
        a("new_release", "⚑", "New Release", "Board ▸ Tickets ▸ New Release", "", "fix version version ship launch milestone release", Do(|a, c| {
            a.execute(Command::ShowView(V::Board), c);
            a.board.side = super::board::SidePanel::Releases;
        })),
        a("estimate_points", "#", "Estimate in Story Points", "Board ▸ Estimate", "", "story points estimation velocity", Cmd(Command::SetEstimate(crate::board::Estimate::StoryPoints))),
        a("estimate_count", "#", "Estimate by Ticket Count", "Board ▸ Estimate", "", "issue count estimation velocity", Cmd(Command::SetEstimate(crate::board::Estimate::TicketCount))),
        a("board_charts", "📈", "Board Charts", "Board ▸ Charts", "", "burndown burnup velocity cumulative flow chart graph", Cmd(Command::ToggleBoardCharts)).on(|a| a.board.show_charts),
        // ---- Risks
        a("new_risk", "⚠", "Add a Risk", "View ▸ Risk Register ▸ New", "", "risk register threat issue opportunity raid log add", Do(|a, c| {
            a.execute(Command::ShowView(V::Risks), c);
            a.new_risk(crate::risks::RiskKind::Threat, vec![]);
        })),
        a("new_opportunity", "⚠", "Add an Opportunity", "View ▸ Risk Register ▸ New", "", "risk register opportunity upside", Do(|a, c| {
            a.execute(Command::ShowView(V::Risks), c);
            a.new_risk(crate::risks::RiskKind::Opportunity, vec![]);
        })),
        a("new_issue", "⚠", "Add an Issue", "View ▸ Risk Register ▸ New", "", "risk register issue problem raid log realized", Do(|a, c| {
            a.execute(Command::ShowView(V::Risks), c);
            a.new_risk(crate::risks::RiskKind::Issue, vec![]);
        })),
        a("risk_for_task", "⚠", "Add Risk for Task", "Task ▸ context menu ▸ Add Risk for Task", "", "risk register threat tie link task", Do(|a, _| a.add_risk_for_active_task())).when(task_selected, SELECT_TASK),
        a("export_risks", "📤", "Export the Risk Register as CSV", "View ▸ Risk Register ▸ Export CSV", "", "risk register export excel spreadsheet report", Do(|a, _| a.pick_save(super::filebrowser::Mode::ExportRisksCsv))),
        // ---- Views and zoom
        a("zoom_in", "🔍", "Zoom In", "View ▸ Zoom ▸ Zoom In", "Ctrl+/", "timescale closer magnify", Cmd(Command::ZoomIn)),
        a("zoom_out", "🔎", "Zoom Out", "View ▸ Zoom ▸ Zoom Out", "Ctrl+*", "timescale further shrink", Cmd(Command::ZoomOut)),
        a("entire_project", "🔭", "Zoom to the Entire Project", "View ▸ Zoom ▸ Entire Project", "", "fit whole overview timescale", Cmd(Command::ZoomEntireProject)),
        a("zoom_selected", "🔬", "Zoom to Selected Tasks", "View ▸ Zoom ▸ Selected Tasks", "", "fit selection timescale", Cmd(Command::ZoomSelectedTasks)).when(task_selected, SELECT_TASK),
        a("timescale_dlg", "📏", "Timescale", "View ▸ Zoom ▸ Timescale…", "", "tiers units weeks months header nonworking time", Cmd(Command::TimescaleDlg)),
        a("project_start", "⏮", "Scroll to the Project Start", "View ▸ Zoom", "Alt+Home", "beginning first date", Cmd(Command::ScrollProjectStart)),
        a("project_end", "⏭", "Scroll to the Project Finish", "View ▸ Zoom", "Alt+End", "end last date", Cmd(Command::ScrollProjectEnd)),
        a("timeline", "▭", "Timeline", "View ▸ Split View ▸ Timeline", "", "roadmap overview summary bar show hide", Cmd(Command::ToggleTimeline)).on(|a| a.view.timeline),
        a("text_bigger", "A+", "Larger Text", "View ▸ Text Size", "Ctrl+Shift+>", "font size increase bigger readability", Cmd(Command::TextBigger)),
        a("text_smaller", "A−", "Smaller Text", "View ▸ Text Size", "Ctrl+Shift+<", "font size decrease", Cmd(Command::TextSmaller)),
        a("ui_zoom_in", "⊕", "Zoom the Whole Interface In", "View ▸ Text Size", "Ctrl+Alt+=", "scale ui bigger magnify accessibility", Cmd(Command::UiZoomIn)),
        a("ui_zoom_out", "⊖", "Zoom the Whole Interface Out", "View ▸ Text Size", "Ctrl+Alt+-", "scale ui smaller", Cmd(Command::UiZoomOut)),
        a("text_reset", "↺", "Reset Text Size and Zoom", "View ▸ Text Size ▸ Reset", "Ctrl+0", "default normal size 100%", Cmd(Command::TextReset)),
        a("sort_dlg", "⇅", "Sort…", "View ▸ Data ▸ Sort", "", "order arrange ascending descending", Cmd(Command::SortDlg)),
        a("filter_date_range", "🔽", "Filter by Date Range", "View ▸ Data ▸ Filter ▸ Date Range…", "", "between dates period window this week month", Cmd(Command::DateRangeFilterDlg)),
        a("filter_resource", "🔽", "Filter by Resource", "View ▸ Data ▸ Filter ▸ Using Resource…", "", "person assigned whose tasks who", Cmd(Command::UsingResourceFilterDlg)),
        a("new_filter", "🔽", "New Filter", "View ▸ Data ▸ Filter ▸ New Filter…", "", "custom filter criteria condition", Cmd(Command::NewFilter)),
        a("custom_group", "⊞", "Custom Group", "View ▸ Data ▸ Group By ▸ Custom…", "", "group by field custom", Cmd(Command::CustomGroupDlg)),
        a("related_summaries", "⊞", "Show Related Summary Rows", "View ▸ Data ▸ Filter", "", "filter summary parents context", Cmd(Command::ToggleRelatedSummaries)),
        // ---- Format
        a("text_styles", "🔤", "Text Styles", "Format ▸ Format ▸ Text Styles", "", "font color bold categories rows", Cmd(Command::TextStylesDlg)),
        a("gridlines", "▦", "Gridlines", "Format ▸ Format ▸ Gridlines", "", "lines rows columns date lines status date line", Cmd(Command::GridlinesDlg)),
        a("layout", "⛓", "Layout", "Format ▸ Format ▸ Layout", "", "link style arrows date format bar height", Cmd(Command::LayoutDlg)),
        a("bar_styles", "🎨", "Bar Styles", "Format ▸ Bar Styles ▸ Bar Styles", "", "bars colors shapes appearance gantt look", Cmd(Command::BarStylesDlg)),
        a("format_bar", "🖊", "Format Bar", "Format ▸ Bar Styles ▸ Format Bar", "", "color colour bar task highlight paint", Cmd(Command::FormatBarDlg)).when(task_selected, SELECT_TASK),
        a("reset_columns", "↺", "Reset Columns", "Format ▸ Columns ▸ Reset Columns", "", "default columns table restore", Do(|a, _| a.reset_columns())),
        a("show_critical", "🔥", "Critical Tasks", "Format ▸ Bar Styles ▸ Critical Tasks", "", "critical path red highlight show hide", Cmd(Command::ToggleCritical)).on(|a| a.project.format.show_critical),
        a("show_slack", "▬", "Slack", "Format ▸ Bar Styles ▸ Slack", "", "float total slack bars buffer show hide", Cmd(Command::ToggleSlack)).on(|a| a.project.format.show_slack),
        a("show_late", "⏰", "Late Tasks", "Format ▸ Bar Styles ▸ Late Tasks", "", "behind overdue delayed highlight show hide", Cmd(Command::ToggleLate)).on(|a| a.project.format.show_late),
        a("show_baseline", "▬", "Baseline Bars", "Format ▸ Bar Styles ▸ Baseline", "", "baseline compare bars show hide", Cmd(Command::ToggleBaselineBars)).on(|a| a.project.format.show_baseline),
        a("show_slippage", "↔", "Slippage", "Format ▸ Bar Styles ▸ Slippage", "", "baseline variance delay moved show hide", Cmd(Command::ToggleSlippage)).on(|a| a.project.format.show_slippage),
        a("show_bar_text", "T", "Bar Text", "Format ▸ Bar Styles ▸ Bar Text", "", "labels names resources text next to bars show hide", Cmd(Command::ToggleBarText)).on(|a| a.project.format.layout.show_bar_text),
        a("progress_lines", "〰", "Progress Lines", "Format ▸ Tracking ▸ Progress Lines", "", "status line ahead behind tracking show hide", Cmd(Command::ToggleProgressLines)).on(|a| a.project.format.gridlines.progress_lines),
        a("outline_number", "1.2", "Outline Numbers", "Format ▸ Show/Hide ▸ Outline Number", "", "wbs numbering hierarchy show hide", Cmd(Command::ToggleOutlineNumber)).on(|a| a.view.outline_number),
        a("project_summary", "Σ", "Project Summary Task", "Format ▸ Show/Hide ▸ Project Summary Task", "", "total row task 0 show hide", Cmd(Command::ToggleProjectSummary)).on(|a| a.view.project_summary),
        a("summary_rows", "Σ", "Summary Tasks", "Format ▸ Show/Hide ▸ Summary Tasks", "", "phases parents rows show hide", Cmd(Command::ToggleSummaryRows)).on(|a| a.view.summary_rows),
        a("dark_mode", "🌙", "Switch Between Light and Dark", "File ▸ Theme", "Ctrl+Shift+F11", "dark mode night theme light colours colors appearance", Cmd(Command::ToggleDarkMode)),
        // ---- Help
        a("shortcuts", "⌨", "Keyboard Shortcuts", "Help ▸ Keyboard Shortcuts", "F1", "keys hotkeys help list", Cmd(Command::Shortcuts)),
        a("about", "ℹ", "About ProjectFCG", "Help ▸ About", "", "version license credits", Cmd(Command::About)),
    ];
    if !cfg!(target_arch = "wasm32") {
        v.push(a("exit", "🚪", "Exit", "File ▸ Settings ▸ Exit", "Ctrl+Q", "quit close leave", Cmd(Command::Quit)));
    }
    // views
    for (id, icon, label, tags, run) in [
        ("view_gantt", "📊", "Gantt Chart", "gantt bars timeline schedule default view", Cmd(Command::SetGanttView(G::Gantt))),
        ("view_tracking", "📉", "Tracking Gantt", "gantt actual baseline progress view", Cmd(Command::SetGanttView(G::Tracking))),
        ("view_detail", "📐", "Detail Gantt", "gantt slack critical path float view", Cmd(Command::SetGanttView(G::Detail))),
        ("view_leveling", "📶", "Leveling Gantt", "gantt leveling delay view", Cmd(Command::SetGanttView(G::Leveling))),
        ("view_network", "🕸", "Network Diagram", "pert boxes dependencies flow chart view", Cmd(Command::ShowView(V::NetworkDiagram))),
        ("view_calendar", "📅", "Calendar View", "month calendar view schedule", Cmd(Command::ShowView(V::Calendar))),
        ("view_risks", "⚠", "Risk Register", "risks risk matrix heat map issues opportunities raid log dod pmi view", Cmd(Command::ShowView(V::Risks))),
        ("view_board", "📋", "Board", "kanban scrum sprint backlog tickets agile view", Cmd(Command::ShowView(V::Board))),
        ("view_task_usage", "📊", "Task Usage", "assignments work hours per day view", Cmd(Command::ShowView(V::TaskUsage))),
        ("view_res_sheet", "👷", "Resource Sheet", "resources people team list rates view", Cmd(Command::ShowView(V::ResourceSheet))),
        ("view_res_usage", "📊", "Resource Usage", "workload allocation hours capacity overallocation view", Cmd(Command::ShowView(V::ResourceUsage))),
    ] {
        v.push(a(id, icon, label, "View ▸ Views", "", tags, run));
    }
    // tables
    for (t, id, tags) in [
        (super::columns::Table::Entry, "table_entry", "default columns"),
        (super::columns::Table::Schedule, "table_schedule", "late start early finish slack"),
        (super::columns::Table::Tracking, "table_tracking", "actual progress remaining"),
        (super::columns::Table::Cost, "table_cost", "money budget fixed cost baseline variance"),
        (super::columns::Table::Work, "table_work", "hours effort baseline variance"),
        (super::columns::Table::EarnedValue, "table_ev", "evm ev pv ac sv cv spi cpi"),
        (super::columns::Table::Variance, "table_variance", "baseline start finish slip"),
    ] {
        let label: &'static str = Box::leak(format!("{} Table", t.label()).into_boxed_str());
        v.push(a(id, "▤", label, "View ▸ Data ▸ Tables", "", Box::leak(format!("table columns {tags}").into_boxed_str()), Cmd(Command::SetTable(t))));
    }
    // filters and highlights
    for f in Filter::ALL {
        let id: &'static str = Box::leak(format!("filter_{:?}", f).to_lowercase().into_boxed_str());
        let label: &'static str = if f == Filter::AllTasks { "Show All Tasks (No Filter)" } else { Box::leak(format!("Filter: {}", f.label()).into_boxed_str()) };
        let keys = if f == Filter::AllTasks { "Ctrl+F3" } else { "" };
        v.push(a(id, "🔽", label, "View ▸ Data ▸ Filter", keys, "filter show only hide others clear reset", Cmd(Command::SetFilter(f))));
        if f != Filter::AllTasks {
            let id: &'static str = Box::leak(format!("highlight_{:?}", f).to_lowercase().into_boxed_str());
            let label: &'static str = Box::leak(format!("Highlight: {}", f.label()).into_boxed_str());
            v.push(a(id, "🖊", label, "View ▸ Data ▸ Highlight", "", "highlight mark color yellow", Cmd(Command::SetHighlight(f))));
        }
    }
    // groups
    use super::filters::GroupBy;
    for (g, id, label) in [
        (GroupBy::None, "group_none", "No Group"),
        (GroupBy::CompleteIncomplete, "group_complete", "Group: Complete and Incomplete Tasks"),
        (GroupBy::ConstraintType, "group_constraint", "Group: Constraint Type"),
        (GroupBy::Critical, "group_critical", "Group: Critical"),
        (GroupBy::Duration, "group_duration", "Group: Duration"),
        (GroupBy::Milestones, "group_milestones", "Group: Milestones"),
        (GroupBy::Priority, "group_priority", "Group: Priority"),
        (GroupBy::Resource, "group_resource", "Group: Resource"),
        (GroupBy::Status, "group_status", "Group: Status"),
        (GroupBy::Active, "group_active", "Group: Active and Inactive"),
    ] {
        v.push(a(id, "⊞", label, "View ▸ Data ▸ Group By", "", "group by categorize bucket organize ungroup", Cmd(Command::SetGroup(g))));
    }
    // sorts
    for s in super::sort::SortPreset::ALL {
        let id: &'static str = Box::leak(format!("sort_{:?}", s).to_lowercase().into_boxed_str());
        let label: &'static str = Box::leak(format!("Sort {}", s.label()).into_boxed_str());
        v.push(a(id, "⇅", label, "View ▸ Data ▸ Sort", "", "sort order arrange", Cmd(Command::SortBy(s))));
    }
    // timescales
    for t in Timescale::ALL {
        let id: &'static str = Box::leak(format!("timescale_{:?}", t).to_lowercase().into_boxed_str());
        let label: &'static str = Box::leak(format!("Timescale: {}", t.label()).into_boxed_str());
        v.push(a(id, "📏", label, "View ▸ Zoom ▸ Timescale", "", "zoom timescale units show by", Cmd(Command::SetTimescale(t))));
    }
    // outline levels
    for (lvl, id, label) in [(Some(1), "outline_1", "Show Outline Level 1"), (Some(2), "outline_2", "Show Outline Level 2"), (Some(3), "outline_3", "Show Outline Level 3"), (None, "outline_all", "Show All Outline Levels")] {
        v.push(a(id, "⊟", label, "View ▸ Data ▸ Outline", "", "outline level collapse expand phases summary only", Cmd(Command::OutlineLevel(lvl))));
    }
    // theme
    use super::theme::{Accent, ThemeMode};
    for (m, id, label) in [(ThemeMode::Light, "theme_light", "Light Theme"), (ThemeMode::Dark, "theme_dark", "Dark Theme"), (ThemeMode::System, "theme_system", "Theme: Follow the System")] {
        v.push(a(id, "🎨", label, "File ▸ Theme", "", "theme appearance colours colors mode dark light", Cmd(Command::SetThemeMode(m))));
    }
    for acc in Accent::ALL {
        let id: &'static str = Box::leak(format!("accent_{:?}", acc).to_lowercase().into_boxed_str());
        let label: &'static str = Box::leak(format!("Accent Colour: {:?}", acc).into_boxed_str());
        v.push(a(id, "🎨", label, "File ▸ Theme", "", "accent color colour theme ribbon", Cmd(Command::SetAccent(acc))));
    }
    v
}

/// Everyday actions: a small head start over rarer ones that happen to
/// share the words ("add task" is Insert Task before Add Risk for Task).
const COMMON: [&str; 24] = [
    "insert_task", "delete_task", "link", "unlink", "task_info", "set_baseline", "level_all", "print", "save", "open", "find", "indent", "outdent",
    "insert_milestone", "insert_summary", "assign", "update_tasks", "view_gantt", "working_time", "project_info", "undo", "copy", "paste", "new_ticket",
];

/// What the empty search offers after the recently used actions.
const SUGGESTED: [&str; 7] = ["insert_task", "link", "set_baseline", "level_all", "update_tasks", "view_risks", "print"];

/// Words that carry no meaning in a request ("how do I add a task").
const STOP: [&str; 33] = [
    "a", "an", "the", "to", "my", "of", "for", "how", "do", "does", "did", "i", "can", "could", "want", "on", "in", "is", "are", "be", "it", "this", "that", "and", "with", "me", "please", "make", "what", "where", "will", "would", "should",
];

/// Verbs that say little about which action is meant ("add a dependency"
/// is Link): they count when they match and are no obstacle when not.
const GENERIC: [&str; 14] = ["add", "create", "new", "set", "show", "turn", "change", "put", "get", "use", "edit", "see", "give", "start"];

fn norm(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '%' { c.to_lowercase().next().unwrap_or(c) } else { ' ' }).collect()
}

/// A rough stem: plurals and verb endings, so "dependencies" meets
/// "dependency" and "linking" meets "link".
fn stem(w: &str) -> String {
    let w = w.to_string();
    for (end, with) in [("ies", "y"), ("ing", ""), ("ed", ""), ("es", ""), ("s", "")] {
        if w.len() > end.len() + 2 && w.ends_with(end) {
            return format!("{}{}", &w[..w.len() - end.len()], with);
        }
    }
    w
}

fn words(s: &str) -> Vec<String> {
    norm(s).split_whitespace().map(stem).collect()
}

/// Edit distance ≤ `k` (with transpositions), for typos.
fn near(a: &str, b: &str, k: usize) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > k {
        return false;
    }
    let n = b.len();
    let mut prev2 = vec![0usize; n + 1];
    let mut prev: Vec<usize> = (0..=n).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; n + 1];
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        if cur.iter().min().copied().unwrap_or(0) > k {
            return false;
        }
        prev2 = std::mem::replace(&mut prev, cur);
    }
    prev[n] <= k
}

fn typo_allowance(t: &str) -> usize {
    match t.chars().count() {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// How well one typed word matches a list of words, and whether only as
/// a typo.
fn word_score(t: &str, ws: &[String], exact: u32, prefix: u32, fuzzy: u32) -> (u32, bool) {
    let k = typo_allowance(t);
    let mut best = (0, false);
    for w in ws {
        let s = if w == t {
            exact
        } else if w.starts_with(t) && (t.len() >= 2 || w.len() <= 3) {
            prefix
        } else if k > 0 && w.len() >= 3 && (near(t, w, k) || (w.len() > t.len() && w.is_char_boundary(t.len()) && near(t, &w[..t.len()], k.min(1)))) {
            fuzzy + 1000 // marked as a typo match
        } else {
            0
        };
        let s = if s >= 1000 { (s - 1000, true) } else { (s, false) };
        if s.0 > best.0 {
            best = s;
        }
    }
    best
}

/// The query's meaningful words.
pub fn tokens(q: &str) -> Vec<String> {
    let all: Vec<String> = norm(q).split_whitespace().map(|s| s.to_string()).collect();
    let kept: Vec<String> = all.iter().filter(|w| !STOP.contains(&w.as_str())).map(|w| stem(w)).collect();
    if kept.is_empty() {
        all.iter().map(|w| stem(w)).collect()
    } else {
        kept
    }
}

/// The action's score for the query, or None when a word matches nothing.
pub fn score(act: &Action, q: &str) -> Option<u32> {
    score_typo(act, q).map(|(s, _)| s)
}

/// The score, and whether a word only matched as a typo.
fn score_typo(act: &Action, q: &str) -> Option<(u32, bool)> {
    let toks = tokens(q);
    if toks.is_empty() {
        return None;
    }
    let label = words(act.label);
    let tags = words(act.tags);
    let path = words(act.path);
    let keys = words(act.keys);
    let mut total = 0;
    let mut matched = 0;
    let mut typo = false;
    for t in &toks {
        let (s, ty) = [word_score(t, &label, 12, 9, 6), word_score(t, &tags, 9, 7, 4), word_score(t, &path, 4, 3, 0), word_score(t, &keys, 5, 0, 0)].into_iter().max_by_key(|x| x.0).unwrap_or((0, false));
        typo |= ty;
        if s == 0 {
            if GENERIC.contains(&t.as_str()) && toks.len() > 1 {
                continue;
            }
            return None;
        }
        matched += 1;
        total += s;
    }
    if matched == 0 {
        return None;
    }
    let nq = norm(q).split_whitespace().collect::<Vec<_>>().join(" ");
    let nl = norm(act.label).split_whitespace().collect::<Vec<_>>().join(" ");
    if nl.starts_with(&nq) {
        total += 14;
    } else if nl.contains(&nq) {
        total += 7;
    }
    Some((total, typo))
}

/// The matching actions, best first (catalog indices).
pub fn search(cat: &[Action], q: &str, recent: &[String]) -> Vec<usize> {
    let mut hits: Vec<(u32, usize)> = cat
        .iter()
        .enumerate()
        .filter_map(|(i, act)| {
            score_typo(act, q).map(|(s, typo)| {
                let boost = recent.iter().position(|r| r == act.id).map_or(0, |p| 12u32.saturating_sub(2 * p as u32));
                // (a guess at a typo isn't confident enough to favour)
                let common = if COMMON.contains(&act.id) && !typo { 5 } else { 0 };
                (s + boost + common, i)
            })
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(cat[a.1].label.len().cmp(&cat[b.1].label.len())).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, i)| i).collect()
}

// ------------------------------------------------------------------- UI

#[derive(Default)]
pub struct TellMe {
    pub open: bool,
    pub query: String,
    pub sel: usize,
    cat: Vec<Action>,
    /// Where the list was drawn last frame (a click there isn't "elsewhere").
    list: Option<Rect>,
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Action(usize),
    /// Search the plan for the words (Ctrl+E).
    Items,
    Find,
    Header(&'static str),
}

const MAX_ACTIONS: usize = 8;

impl ProjectFcgApp {
    /// Format ▸ Columns ▸ Reset Columns.
    pub fn reset_columns(&mut self) {
        let k = self.prefs.text_size / super::theme::DEFAULT_TEXT_SIZE;
        self.columns = super::columns::ColumnId::default_set().into_iter().map(|(id, width)| super::ColumnState { id, width: (width * k).round() }).collect();
        self.active_col = 2;
    }

    /// Alt+Q: put the cursor in the search box.
    pub fn focus_search(&mut self, ctx: &egui::Context) {
        self.tell_me.open = true;
        self.tell_me.sel = 0;
        ctx.memory_mut(|m| m.request_focus(box_id()));
    }

    fn tell_me_rows(&mut self) -> Vec<Row> {
        if self.tell_me.cat.is_empty() {
            self.tell_me.cat = catalog();
        }
        let q = self.tell_me.query.trim().to_string();
        let cat = &self.tell_me.cat;
        let mut rows = vec![];
        if q.is_empty() {
            let recent: Vec<usize> = self.prefs.recent_actions.iter().filter_map(|id| cat.iter().position(|a| a.id == id)).take(5).collect();
            if !recent.is_empty() {
                rows.push(Row::Header("Recently used"));
                rows.extend(recent.iter().map(|&i| Row::Action(i)));
            }
            rows.push(Row::Header("Suggested"));
            rows.extend(SUGGESTED.iter().filter_map(|id| cat.iter().position(|a| a.id == *id)).filter(|i| !recent.contains(i)).take(6).map(Row::Action));
            return rows;
        }
        let hits = search(cat, &q, &self.prefs.recent_actions);
        if !hits.is_empty() {
            rows.push(Row::Header("Actions"));
            rows.extend(hits.into_iter().take(MAX_ACTIONS).map(Row::Action));
        }
        rows.push(Row::Header("More"));
        rows.push(Row::Items);
        rows.push(Row::Find);
        rows
    }

    fn tell_me_run(&mut self, row: Row, ctx: &egui::Context) {
        let close = |app: &mut ProjectFcgApp| {
            app.tell_me.open = false;
            app.tell_me.query.clear();
            app.tell_me.sel = 0;
            ctx.memory_mut(|m| m.surrender_focus(box_id()));
        };
        match row {
            Row::Action(i) => {
                let act = self.tell_me.cat[i].clone();
                if let Some((ok, why)) = act.when {
                    if !ok(self) {
                        self.set_status(format!("{}: {why}", act.label));
                        return;
                    }
                }
                close(self);
                let p = &mut self.prefs.recent_actions;
                p.retain(|x| x != act.id);
                p.insert(0, act.id.to_string());
                p.truncate(8);
                self.save_prefs();
                match act.run {
                    Run::Cmd(c) => self.execute(c, ctx),
                    Run::Do(f) => f(self, ctx),
                }
            }
            Row::Items => {
                let q = self.tell_me.query.trim().to_string();
                close(self);
                self.focus_item_search(ctx, Some(q));
            }
            Row::Find => {
                let q = self.tell_me.query.trim().to_string();
                close(self);
                self.dialogs.find_text = q;
                self.dialogs.find.replace_mode = false;
                self.find_next();
            }
            Row::Header(_) => {}
        }
    }
}

/// The search box in the tab strip (`r`), and its list of results.
pub fn search_box(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let pal = super::theme::pal();
    let id = box_id();
    let focused = ui.memory(|m| m.has_focus(id));
    // the list's keys, before the text box sees them
    let (mut down, mut up, mut enter, mut esc) = (false, false, false, false);
    if focused {
        ui.input_mut(|i| {
            down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
            up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
            enter = i.consume_key(Modifiers::NONE, Key::Enter);
            esc = i.consume_key(Modifiers::NONE, Key::Escape);
        });
    }
    let before = app.tell_me.query.clone();
    // the hint is painted in the theme's weak text colour: on the dark
    // title bar it must be a light one while the box is unfocused
    let hint = if focused { pal.text_dim } else { pal.on_band.gamma_multiply(0.8) };
    let saved_weak = ui.visuals().weak_text_color;
    ui.visuals_mut().weak_text_color = Some(hint);
    let resp = ui.put(
        r,
        egui::TextEdit::singleline(&mut app.tell_me.query)
            .id(id)
            .hint_text(egui::RichText::new("💡  Tell me what you want to do").color(pal.on_band.gamma_multiply(0.75)))
            .text_color(if focused { pal.text } else { pal.on_band })
            .background_color(if focused { pal.bg } else { pal.band_hover })
            .margin(egui::Margin::symmetric(8, 3)),
    );
    ui.visuals_mut().weak_text_color = saved_weak;
    resp.clone().on_hover_text("Tell me: search for a command (as in Outlook and Word): type what you want to do");
    if resp.gained_focus() || (focused && !app.tell_me.open) {
        app.tell_me.open = true;
    }
    if app.tell_me.query != before {
        app.tell_me.sel = 0;
    }
    // Escape (egui may already have taken the focus away for it), or the
    // box losing focus to a click anywhere but the list: close
    let over_list = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())).is_some_and(|p| app.tell_me.list.is_some_and(|l| l.contains(p)));
    if esc || (app.tell_me.open && ui.input(|i| i.key_pressed(Key::Escape))) || (resp.lost_focus() && !over_list) {
        app.tell_me.open = false;
        app.tell_me.query.clear();
        app.tell_me.list = None;
        ctx.memory_mut(|m| m.surrender_focus(id));
        return;
    }
    if !app.tell_me.open {
        return;
    }
    let rows = app.tell_me_rows();
    let pickable: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| !matches!(r, Row::Header(_))).map(|(i, _)| i).collect();
    let n = pickable.len();
    if n > 0 {
        app.tell_me.sel = app.tell_me.sel.min(n - 1);
        if down {
            app.tell_me.sel = (app.tell_me.sel + 1) % n;
        }
        if up {
            app.tell_me.sel = (app.tell_me.sel + n - 1) % n;
        }
    }
    if enter && n > 0 {
        let row = rows[pickable[app.tell_me.sel]];
        app.tell_me_run(row, &ctx);
        return;
    }

    // ---- the list
    let width = 470.0f32.min(ctx.content_rect().width() - 16.0);
    let x = r.left().min(ctx.content_rect().right() - width - 8.0).max(8.0);
    let mut clicked: Option<Row> = None;
    let mut hovered_any = false;
    let area = egui::Area::new(Id::unique("tell_me_list")).order(egui::Order::Foreground).fixed_pos(pos2(x, r.bottom() + 3.0)).show(&ctx, |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
            ui.set_width(width);
            let toks = tokens(&app.tell_me.query);
            let mut k = 0usize;
            for row in &rows {
                match *row {
                    Row::Header(h) => {
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(h).size(11.0).strong().color(pal.text_dim));
                    }
                    _ => {
                        let sel = k == app.tell_me.sel;
                        let (rect, resp) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
                        if resp.hovered() {
                            hovered_any = true;
                            if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                                app.tell_me.sel = k;
                            }
                        }
                        let p = ui.painter();
                        if sel || resp.hovered() {
                            p.rect_filled(rect, 4.0, pal.accent.gamma_multiply(if sel { 0.22 } else { 0.12 }));
                        }
                        let (icon, title, sub, keys, enabled, on, why) = match *row {
                            Row::Action(i) => {
                                let act = &app.tell_me.cat[i];
                                let ok = act.available(app);
                                (act.icon.to_string(), act.label.to_string(), act.path.to_string(), act.keys.to_string(), ok, act.on.map(|f| f(app)), act.when.filter(|_| !ok).map(|(_, w)| w))
                            }
                            Row::Items => {
                                let n = super::item_search::search(&app.project, app.tell_me.query.trim()).len();
                                ("🔍".into(), format!("Search the plan for \u{201c}{}\u{201d}", app.tell_me.query.trim()), format!("{n} task(s), ticket(s), people or risks match"), "Ctrl+E".into(), true, None, None)
                            }
                            Row::Find => ("🔍".into(), format!("Find \u{201c}{}\u{201d} in tasks", app.tell_me.query.trim()), "Task ▸ Editing ▸ Find (any field, next match)".into(), "Ctrl+F".into(), true, None, None),
                            Row::Header(_) => unreachable!(),
                        };
                        let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
                        p.text(pos2(rect.left() + 18.0, rect.center().y), Align2::CENTER_CENTER, &icon, FontId::proportional(15.0), dim(pal.text));
                        // title, with the typed words picked out
                        let mut job = LayoutJob::default();
                        let base = TextFormat { font_id: FontId::proportional(13.0), color: dim(pal.text), ..Default::default() };
                        let strong = TextFormat { font_id: FontId::proportional(13.0), color: dim(pal.accent), ..Default::default() };
                        for (piece, hit) in highlight(&title, &toks) {
                            job.append(&piece, 0.0, if hit { strong.clone() } else { base.clone() });
                        }
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        p.galley(pos2(rect.left() + 36.0, rect.top() + 3.0), g, pal.text);
                        p.text(pos2(rect.left() + 36.0, rect.bottom() - 4.0), Align2::LEFT_BOTTOM, why.unwrap_or(&sub), FontId::proportional(10.5), if why.is_some() { Color32::from_rgb(200, 90, 60) } else { pal.text_dim });
                        let mut right = rect.right() - 8.0;
                        if let Some(on) = on {
                            p.text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, if on { "✔ On" } else { "Off" }, FontId::proportional(11.0), if on { pal.accent } else { pal.text_dim });
                            right -= 44.0;
                        }
                        if !keys.is_empty() {
                            p.text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, &keys, FontId::monospace(10.5), pal.text_dim);
                        }
                        if resp.clicked() {
                            clicked = Some(*row);
                        }
                        k += 1;
                    }
                }
            }
            if pickable.len() <= 1 && !app.tell_me.query.trim().is_empty() {
                ui.label(egui::RichText::new("No actions match. Try other words: \u{201c}add\u{201d}, \u{201c}remove\u{201d}, \u{201c}holiday\u{201d}, \u{201c}dependency\u{201d}…").size(11.0).color(pal.text_dim));
            }
        });
    });
    if let Some(row) = clicked {
        app.tell_me_run(row, &ctx);
        return;
    }
    app.tell_me.list = Some(area.response.rect);
    // open without the cursor in the box (a row was pressed): a click
    // anywhere else closes it
    if !ui.memory(|m| m.has_focus(id)) && !hovered_any && ui.input(|i| i.pointer.any_pressed()) {
        app.tell_me.open = false;
        app.tell_me.list = None;
    }
}

/// `text` cut into pieces, marking the words that start with a typed word.
fn highlight(text: &str, toks: &[String]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = vec![];
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<(String, bool)>| {
        if word.is_empty() {
            return;
        }
        let lw = word.to_lowercase();
        let hit = toks.iter().any(|t| !t.is_empty() && (lw.starts_with(t.as_str()) || stem(&lw) == *t));
        out.push((std::mem::take(word), hit));
    };
    for c in text.chars() {
        if c.is_alphanumeric() {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push((c.to_string(), false));
        }
    }
    flush(&mut word, &mut out);
    out
}
