//! Commands (ribbon buttons, menu items, shortcuts) and the keyboard map.

use egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    AllTasks,
    Critical,
    Milestones,
    Summary,
    Incomplete,
    Completed,
    Late,
    Slipping,
}

impl Filter {
    pub const ALL: [Filter; 8] = [
        Filter::AllTasks,
        Filter::Critical,
        Filter::Milestones,
        Filter::Summary,
        Filter::Incomplete,
        Filter::Completed,
        Filter::Late,
        Filter::Slipping,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Filter::AllTasks => "All Tasks",
            Filter::Critical => "Critical",
            Filter::Milestones => "Milestones",
            Filter::Summary => "Summary Tasks",
            Filter::Incomplete => "Incomplete Tasks",
            Filter::Completed => "Completed Tasks",
            Filter::Late => "Late Tasks",
            Filter::Slipping => "Slipping Tasks",
        }
    }
}

/// Timescale presets from Project's View ▸ Zoom ▸ Timescale list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timescale {
    Hours,
    QuarterDays,
    Days,
    Weeks,
    ThirdsOfMonths,
    Months,
    Quarters,
    HalfYears,
    Years,
}

impl Timescale {
    pub const ALL: [Timescale; 9] = [
        Timescale::Hours,
        Timescale::QuarterDays,
        Timescale::Days,
        Timescale::Weeks,
        Timescale::ThirdsOfMonths,
        Timescale::Months,
        Timescale::Quarters,
        Timescale::HalfYears,
        Timescale::Years,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Timescale::Hours => "Hours",
            Timescale::QuarterDays => "Quarter Days",
            Timescale::Days => "Days",
            Timescale::Weeks => "Weeks",
            Timescale::ThirdsOfMonths => "Thirds of Months",
            Timescale::Months => "Months",
            Timescale::Quarters => "Quarters",
            Timescale::HalfYears => "Half Years",
            Timescale::Years => "Years",
        }
    }
    /// Pixels per day for the preset.
    pub fn ppd(&self) -> f32 {
        match self {
            Timescale::Hours => 720.0,
            Timescale::QuarterDays => 200.0,
            Timescale::Days => 28.0,
            Timescale::Weeks => 9.0,
            Timescale::ThirdsOfMonths => 4.0,
            Timescale::Months => 1.6,
            Timescale::Quarters => 0.6,
            Timescale::HalfYears => 0.3,
            Timescale::Years => 0.12,
        }
    }
    /// The preset whose tiers are drawn at a given zoom.
    pub fn for_ppd(ppd: f32) -> Timescale {
        if ppd >= 400.0 {
            Timescale::Hours
        } else if ppd >= 110.0 {
            Timescale::QuarterDays
        } else if ppd >= 15.0 {
            Timescale::Days
        } else if ppd >= 5.5 {
            Timescale::Weeks
        } else if ppd >= 2.6 {
            Timescale::ThirdsOfMonths
        } else if ppd >= 0.9 {
            Timescale::Months
        } else if ppd >= 0.4 {
            Timescale::Quarters
        } else if ppd >= 0.18 {
            Timescale::HalfYears
        } else {
            Timescale::Years
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    // File
    New,
    NewFromSample,
    NewFromAgileSample,
    Open,
    Save,
    SaveAs,
    ExportCsv,
    ImportJiraCsv,
    ExportJiraCsv,
    ExportXml,
    Options,
    Quit,
    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    FillDown,
    ResetField,
    EditCell,
    SelectAll,
    SelectRow,
    Find,
    FindNext,
    GoTo,
    // Task
    InsertTask,
    InsertSummary,
    InsertMilestone,
    InsertRecurring,
    ToggleActive,
    DeleteTasks,
    Indent,
    Outdent,
    MoveUp,
    MoveDown,
    LinkTasks,
    UnlinkTasks,
    TaskInfo,
    TaskNotes,
    SetPercent(u8),
    SetManual,
    SetAuto,
    ToggleNewTaskMode,
    MarkMilestone,
    AddToTimeline,
    ScrollToTask,
    // Outline
    HideSubtasks,
    ShowSubtasks,
    ShowAllSubtasks,
    OutlineLevel(Option<u32>),
    // Project
    ProjectInfo,
    WorkingTime,
    Calculate,
    ToggleAutoCalc,
    // View
    ZoomIn,
    ZoomOut,
    SetTimescale(Timescale),
    ZoomEntireProject,
    ZoomSelectedTasks,
    ScrollProjectStart,
    ScrollProjectEnd,
    ScrollLeft,
    ScrollRight,
    PageLeft,
    PageRight,
    ToggleTimeline,
    SetFilter(Filter),
    // Format
    ToggleCritical,
    ToggleSlack,
    ToggleSummaryRows,
    ToggleProjectSummary,
    ToggleOutlineNumber,
    ToggleBarText,
    ToggleLate,
    ToggleBaselineBars,
    SetGanttView(crate::format::GanttViewKind),
    ToggleMultipleCriticalPaths,
    TaskPath(super::bars::PathRole),
    ClearTaskPath,
    SetHighlight(Filter),
    ToggleInspector,
    SetBaseline,
    ClearBaseline,
    BarStylesDlg,
    FormatBarDlg,
    TextStylesDlg,
    GridlinesDlg,
    LayoutDlg,
    TimescaleDlg,
    ApplyStylePreset(usize),
    SetBarColor(Option<crate::format::Rgb>),
    // Display
    TextBigger,
    TextSmaller,
    TextReset,
    UiZoomIn,
    UiZoomOut,
    SetUiZoom(f32),
    SetThemeMode(super::theme::ThemeMode),
    SetAccent(super::theme::Accent),
    ToggleDarkMode,
    // Resources
    ShowView(super::MainView),
    AssignResources,
    AddResource(crate::model::ResourceType),
    ResourceInfo,
    LevelAll,
    LevelSelection,
    ClearLeveling,
    LevelingOptions,
    NextOverallocation,
    // Tracking
    MarkOnTrack,
    UpdateTasks,
    UpdateProject,
    ProjectStatistics,
    SetTable(super::columns::Table),
    ToggleProgressLines,
    SetShownBaseline(u8),
    // Board
    NewSprint,
    ToggleSprint,
    OpenOnBoard,
    NewTicket,
    EditTicket,
    DeleteTicket,
    TicketToBacklog,
    NewEpic,
    SetEstimate(crate::board::Estimate),
    ToggleBoardCharts,
    // Help
    Shortcuts,
    About,
}

/// Map a key press to a command, following Microsoft Project's published
/// keyboard shortcuts. Navigation keys (arrows, Enter, Tab…) are handled by
/// the table itself and return `None` here.
pub fn shortcut(key: Key, m: Modifiers) -> Option<Command> {
    use Command::*;
    let ctrl = m.ctrl || m.command;
    let alt = m.alt;
    let shift = m.shift;
    Some(match (ctrl, alt, shift, key) {
        // File
        (true, false, false, Key::N) => New,
        (true, false, false, Key::O) => Open,
        (true, false, false, Key::F12) => Open,
        (true, false, false, Key::S) => Save,
        (false, false, false, Key::F12) => SaveAs,
        (true, false, false, Key::Q) => Quit,
        // Edit
        (true, false, false, Key::Z) => Undo,
        (true, false, false, Key::Y) => Redo,
        (true, false, true, Key::Z) => Redo,
        (true, false, false, Key::X) => Cut,
        (true, false, false, Key::C) => Copy,
        (true, false, false, Key::V) => Paste,
        (true, false, false, Key::D) => FillDown,
        (true, false, false, Key::Delete) => ResetField,
        (false, false, false, Key::F2) => EditCell,
        (true, false, false, Key::A) => SelectAll,
        (true, false, true, Key::Space) => SelectAll,
        (false, false, true, Key::Space) => SelectRow,
        (true, false, false, Key::F) => Find,
        (false, false, true, Key::F5) => Find,
        (false, false, true, Key::F4) => FindNext,
        (false, false, false, Key::F5) => GoTo,
        // Task
        (false, false, false, Key::Insert) => InsertTask,
        (false, false, false, Key::Delete) => DeleteTasks,
        (true, false, false, Key::Minus) => DeleteTasks,
        (false, true, true, Key::ArrowRight) => Indent,
        (false, true, true, Key::ArrowLeft) => Outdent,
        (false, true, true, Key::ArrowUp) => MoveUp,
        (false, true, true, Key::ArrowDown) => MoveDown,
        (true, false, false, Key::F2) => LinkTasks,
        (true, false, true, Key::F2) => UnlinkTasks,
        (false, false, true, Key::F2) => TaskInfo,
        (true, false, true, Key::M) => SetManual,
        (true, true, true, Key::A) => SetAuto,
        (true, false, true, Key::F5) => ScrollToTask,
        // Outline
        (false, true, true, Key::Minus) => HideSubtasks,
        (false, true, true, Key::Equals) | (false, true, true, Key::Plus) => ShowSubtasks,
        (false, true, true, Key::Num8) => ShowAllSubtasks, // Alt+Shift+* on US layouts
        // Project
        (false, false, false, Key::F9) => Calculate,
        (true, false, false, Key::F9) => ToggleAutoCalc,
        // View / timescale
        (true, false, false, Key::Slash) => ZoomIn,
        (true, false, true, Key::Num8) => ZoomOut, // Ctrl+* (Shift+8)
        (true, false, false, Key::Equals) | (true, false, false, Key::Plus) => ZoomIn,
        (false, true, false, Key::Home) => ScrollProjectStart,
        (false, true, false, Key::End) => ScrollProjectEnd,
        (false, true, false, Key::ArrowLeft) => ScrollLeft,
        (false, true, false, Key::ArrowRight) => ScrollRight,
        (false, true, false, Key::PageUp) => PageLeft,
        (false, true, false, Key::PageDown) => PageRight,
        (true, false, false, Key::F3) => SetFilter(Filter::AllTasks),
        (false, false, false, Key::F1) => Shortcuts,
        (false, true, false, Key::F10) => AssignResources,
        // Text size: Office's Ctrl+Shift+> / Ctrl+Shift+<; Ctrl+0 resets.
        (true, false, true, Key::Period) => TextBigger,
        (true, false, true, Key::Comma) => TextSmaller,
        (true, false, false, Key::Num0) => TextReset,
        // Whole-interface zoom
        (true, true, false, Key::Equals) | (true, true, false, Key::Plus) => UiZoomIn,
        (true, true, false, Key::Minus) => UiZoomOut,
        (true, false, true, Key::F11) => ToggleDarkMode,
        _ => return None,
    })
}

/// Text ("*") events for numpad multiply, which egui does not expose as a
/// key: Ctrl+Numpad* zooms out, Alt+Shift+Numpad* shows all subtasks.
pub fn text_shortcut(text: &str, m: Modifiers) -> Option<Command> {
    if (m.ctrl || m.command) && !m.alt {
        match text {
            ">" => return Some(Command::TextBigger),
            "<" => return Some(Command::TextSmaller),
            _ => {}
        }
    }
    if text == "*" {
        if (m.ctrl || m.command) && !m.alt {
            return Some(Command::ZoomOut);
        }
        if m.alt && m.shift {
            return Some(Command::ShowAllSubtasks);
        }
    }
    None
}

/// Reference list shown by F1 (key, action).
pub const SHORTCUT_HELP: &[(&str, &str)] = &[
    ("Ctrl+N", "New project"),
    ("Ctrl+O / Ctrl+F12", "Open"),
    ("Ctrl+S", "Save"),
    ("F12", "Save As"),
    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
    ("Ctrl+X / C / V", "Cut / Copy / Paste tasks"),
    ("Insert", "Insert new task above selection"),
    ("Delete / Ctrl+-", "Delete selected tasks"),
    ("Ctrl+Delete", "Reset selected field"),
    ("F2", "Edit cell"),
    ("Enter / Shift+Enter", "Accept and move down / up"),
    ("Tab / Shift+Tab", "Accept and move right / left"),
    ("Esc", "Cancel entry"),
    ("Ctrl+D", "Fill down"),
    ("Shift+F2", "Task Information"),
    ("Ctrl+F2", "Link selected tasks (Finish-to-Start)"),
    ("Ctrl+Shift+F2", "Unlink selected tasks"),
    ("Alt+Shift+→ / ←", "Indent / Outdent"),
    ("Alt+Shift+↑ / ↓", "Move task up / down"),
    ("Alt+Shift+- / =", "Hide / Show subtasks"),
    ("Alt+Shift+Numpad *", "Show all subtasks"),
    ("Ctrl+Shift+M", "Manually schedule"),
    ("Ctrl+Alt+Shift+A", "Auto schedule"),
    ("Ctrl+Shift+F5", "Scroll to task"),
    ("F5", "Go To"),
    ("Ctrl+F / Shift+F5", "Find"),
    ("Shift+F4", "Find next"),
    ("F9", "Calculate project"),
    ("Ctrl+F9", "Toggle automatic calculation"),
    ("Ctrl+/ / Ctrl+*", "Zoom timescale in / out"),
    ("Ctrl+mouse wheel", "Zoom timescale"),
    ("Alt+Home / Alt+End", "Timescale to project start / finish"),
    ("Alt+← / →", "Scroll timescale"),
    ("Alt+PgUp / PgDn", "Scroll timescale one page"),
    ("Ctrl+↑ / ↓", "First / last row"),
    ("Home / End", "First / last field in row"),
    ("Ctrl+Home / End", "First / last cell"),
    ("Shift+Space", "Select row"),
    ("Ctrl+A / Ctrl+Shift+Space", "Select all"),
    ("Ctrl+F3", "Show all tasks (clear filter)"),
    ("Alt+F10", "Assign Resources"),
    ("Ctrl+Shift+> / Ctrl+Shift+<", "Larger / smaller table and chart text"),
    ("Ctrl+Alt+= / Ctrl+Alt+-", "Zoom the whole interface in / out"),
    ("Ctrl+Shift+F11", "Switch between light and dark theme"),
    ("Board: arrows", "Select a ticket card"),
    ("Board: Ctrl+← / Ctrl+→", "Move the card to the previous / next column"),
    ("Board: Ctrl+↑ / Ctrl+↓", "Move the card up / down in its column"),
    ("Board: Insert / Enter / Delete", "New ticket / edit ticket / delete ticket"),
    ("Board: Ctrl+F / Esc", "Filter the tickets / clear the filter"),
    ("Ctrl+0", "Reset text size and interface zoom"),
    ("F1", "This list"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            alt,
            ctrl,
            shift,
            mac_cmd: false,
            command: ctrl,
        }
    }

    #[test]
    fn project_shortcuts() {
        assert_eq!(shortcut(Key::ArrowRight, m(false, true, true)), Some(Command::Indent));
        assert_eq!(shortcut(Key::ArrowLeft, m(false, true, true)), Some(Command::Outdent));
        assert_eq!(shortcut(Key::F2, m(true, false, false)), Some(Command::LinkTasks));
        assert_eq!(shortcut(Key::F2, m(true, false, true)), Some(Command::UnlinkTasks));
        assert_eq!(shortcut(Key::F2, m(false, false, true)), Some(Command::TaskInfo));
        assert_eq!(shortcut(Key::F2, m(false, false, false)), Some(Command::EditCell));
        assert_eq!(shortcut(Key::Insert, m(false, false, false)), Some(Command::InsertTask));
        assert_eq!(shortcut(Key::F5, m(true, false, true)), Some(Command::ScrollToTask));
        assert_eq!(shortcut(Key::Home, m(false, true, false)), Some(Command::ScrollProjectStart));
        assert_eq!(shortcut(Key::Slash, m(true, false, false)), Some(Command::ZoomIn));
        assert_eq!(text_shortcut("*", m(true, false, false)), Some(Command::ZoomOut));
        assert_eq!(shortcut(Key::M, m(true, false, true)), Some(Command::SetManual));
        assert_eq!(shortcut(Key::A, m(true, true, true)), Some(Command::SetAuto));
        assert_eq!(shortcut(Key::Minus, m(false, true, true)), Some(Command::HideSubtasks));
        assert_eq!(shortcut(Key::F12, m(false, false, false)), Some(Command::SaveAs));
        // plain arrows are table navigation, not commands
        assert_eq!(shortcut(Key::ArrowDown, m(false, false, false)), None);
        assert_eq!(shortcut(Key::Period, m(true, false, true)), Some(Command::TextBigger));
        assert_eq!(shortcut(Key::Comma, m(true, false, true)), Some(Command::TextSmaller));
        assert_eq!(text_shortcut(">", m(true, false, true)), Some(Command::TextBigger));
        // Ctrl+Minus stays Project's "delete row", never a zoom
        assert_eq!(shortcut(Key::Minus, m(true, false, false)), Some(Command::DeleteTasks));
    }

    #[test]
    fn timescale_roundtrip() {
        for t in Timescale::ALL {
            assert_eq!(Timescale::for_ppd(t.ppd()), t, "{t:?}");
        }
    }
}
