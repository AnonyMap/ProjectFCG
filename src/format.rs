//! Gantt chart formatting: bar styles, text styles, gridlines, layout and
//! timescale. Mirrors Project's Format ▸ Bar Styles / Text Styles /
//! Gridlines / Layout / Timescale dialogs. Saved with the project (Project
//! keeps view formatting in the .mpp too). Every struct uses
//! `#[serde(default)]` so files from older versions still open.

use chrono::{Datelike, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Darker shade for borders derived from a fill colour.
    pub fn darker(self, f: f32) -> Rgb {
        let d = |c: u8| ((c as f32) * (1.0 - f)).round().clamp(0.0, 255.0) as u8;
        Rgb(d(self.0), d(self.1), d(self.2))
    }
    pub fn lighter(self, f: f32) -> Rgb {
        let l = |c: u8| (c as f32 + (255.0 - c as f32) * f).round().clamp(0.0, 255.0) as u8;
        Rgb(l(self.0), l(self.1), l(self.2))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pattern {
    Solid,
    Hatched,
    Light,
    Outline,
}

impl Pattern {
    pub const ALL: [Pattern; 4] = [Pattern::Solid, Pattern::Hatched, Pattern::Light, Pattern::Outline];
    pub fn label(&self) -> &'static str {
        match self {
            Pattern::Solid => "Solid",
            Pattern::Hatched => "Hatched",
            Pattern::Light => "Light fill",
            Pattern::Outline => "Outline only",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EndShape {
    None,
    Bracket,
    TriangleDown,
    Diamond,
    Circle,
    Arrow,
}

impl EndShape {
    pub const ALL: [EndShape; 6] = [
        EndShape::None,
        EndShape::Bracket,
        EndShape::TriangleDown,
        EndShape::Diamond,
        EndShape::Circle,
        EndShape::Arrow,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            EndShape::None => "None",
            EndShape::Bracket => "Bracket",
            EndShape::TriangleDown => "Triangle",
            EndShape::Diamond => "Diamond",
            EndShape::Circle => "Circle",
            EndShape::Arrow => "Arrow",
        }
    }
}

/// Fields that can be drawn next to / inside bars (Bar Styles ▸ Text).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BarField {
    None,
    Name,
    Id,
    Duration,
    Start,
    Finish,
    PercentComplete,
    TotalSlack,
    FreeSlack,
    Predecessors,
    Wbs,
    Notes,
    BaselineStart,
    BaselineFinish,
    Status,
    ResourceNames,
    ResourceInitials,
    Work,
    Cost,
}

impl BarField {
    pub const ALL: [BarField; 19] = [
        BarField::None,
        BarField::Name,
        BarField::Id,
        BarField::Duration,
        BarField::Start,
        BarField::Finish,
        BarField::PercentComplete,
        BarField::TotalSlack,
        BarField::FreeSlack,
        BarField::Predecessors,
        BarField::Wbs,
        BarField::Notes,
        BarField::BaselineStart,
        BarField::BaselineFinish,
        BarField::Status,
        BarField::ResourceNames,
        BarField::ResourceInitials,
        BarField::Work,
        BarField::Cost,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            BarField::None => "(none)",
            BarField::Name => "Name",
            BarField::Id => "ID",
            BarField::Duration => "Duration",
            BarField::Start => "Start",
            BarField::Finish => "Finish",
            BarField::PercentComplete => "% Complete",
            BarField::TotalSlack => "Total Slack",
            BarField::FreeSlack => "Free Slack",
            BarField::Predecessors => "Predecessors",
            BarField::Wbs => "WBS",
            BarField::Notes => "Notes",
            BarField::BaselineStart => "Baseline Start",
            BarField::BaselineFinish => "Baseline Finish",
            BarField::Status => "Status",
            BarField::ResourceNames => "Resource Names",
            BarField::ResourceInitials => "Resource Initials",
            BarField::Work => "Work",
            BarField::Cost => "Cost",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarText {
    pub left: BarField,
    pub right: BarField,
    pub top: BarField,
    pub bottom: BarField,
    pub inside: BarField,
}

impl Default for BarText {
    fn default() -> Self {
        BarText {
            left: BarField::None,
            right: BarField::None,
            top: BarField::None,
            bottom: BarField::None,
            inside: BarField::None,
        }
    }
}

impl BarText {
    pub fn right(f: BarField) -> Self {
        BarText { right: f, ..Default::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarStyle {
    pub show: bool,
    pub fill: Rgb,
    pub border: Rgb,
    pub pattern: Pattern,
    /// Bar thickness as a fraction of the row height.
    pub height: f32,
    pub start_shape: EndShape,
    pub end_shape: EndShape,
    pub text: BarText,
}

impl Default for BarStyle {
    fn default() -> Self {
        BarStyle {
            show: true,
            fill: Rgb(138, 180, 232),
            border: Rgb(73, 123, 190),
            pattern: Pattern::Solid,
            height: 0.45,
            start_shape: EndShape::None,
            end_shape: EndShape::None,
            text: BarText::default(),
        }
    }
}

/// Bar style categories (rows of Project's Bar Styles dialog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BarCategory {
    Task,
    CriticalTask,
    ManualTask,
    LateTask,
    Milestone,
    Summary,
    ProjectSummary,
    Progress,
    Slack,
    Baseline,
    Deadline,
    LevelingDelay,
}

impl BarCategory {
    pub const ALL: [BarCategory; 12] = [
        BarCategory::Task,
        BarCategory::CriticalTask,
        BarCategory::ManualTask,
        BarCategory::LateTask,
        BarCategory::Milestone,
        BarCategory::Summary,
        BarCategory::ProjectSummary,
        BarCategory::Progress,
        BarCategory::Slack,
        BarCategory::Baseline,
        BarCategory::Deadline,
        BarCategory::LevelingDelay,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            BarCategory::Task => "Task",
            BarCategory::CriticalTask => "Critical Task",
            BarCategory::ManualTask => "Manual Task",
            BarCategory::LateTask => "Late Task",
            BarCategory::Milestone => "Milestone",
            BarCategory::Summary => "Summary",
            BarCategory::ProjectSummary => "Project Summary",
            BarCategory::Progress => "Progress",
            BarCategory::Slack => "Total Slack",
            BarCategory::Baseline => "Baseline",
            BarCategory::Deadline => "Deadline",
            BarCategory::LevelingDelay => "Leveling Delay",
        }
    }
    /// Shown in the dialog to explain when the style applies.
    pub fn applies_to(&self) -> &'static str {
        match self {
            BarCategory::Task => "Normal, not critical, not manually scheduled",
            BarCategory::CriticalTask => "Critical (when Critical Tasks is on)",
            BarCategory::ManualTask => "Manually scheduled",
            BarCategory::LateTask => "Behind schedule at the status date (when Late Tasks is on)",
            BarCategory::Milestone => "Milestone",
            BarCategory::Summary => "Summary",
            BarCategory::ProjectSummary => "Project summary task",
            BarCategory::Progress => "% complete, drawn inside task bars",
            BarCategory::Slack => "From finish to late finish (when Slack is on)",
            BarCategory::Baseline => "Baseline dates (when Baseline is on)",
            BarCategory::Deadline => "Deadline marker",
            BarCategory::LevelingDelay => "Delay added by leveling (Leveling Gantt)",
        }
    }
    pub fn default_style(&self) -> BarStyle {
        let base = BarStyle::default();
        match self {
            BarCategory::Task => BarStyle { text: BarText::right(BarField::ResourceNames), ..base },
            BarCategory::CriticalTask => BarStyle {
                fill: Rgb(234, 128, 128),
                border: Rgb(192, 48, 48),
                text: BarText::right(BarField::ResourceNames),
                ..base
            },
            BarCategory::ManualTask => BarStyle {
                fill: Rgb(150, 210, 214),
                border: Rgb(52, 140, 150),
                start_shape: EndShape::Bracket,
                end_shape: EndShape::Bracket,
                text: BarText::right(BarField::ResourceNames),
                ..base
            },
            BarCategory::LateTask => BarStyle {
                fill: Rgb(70, 70, 70),
                border: Rgb(20, 20, 20),
                text: BarText::right(BarField::ResourceNames),
                ..base
            },
            BarCategory::Milestone => BarStyle {
                fill: Rgb(30, 30, 30),
                border: Rgb(30, 30, 30),
                height: 0.6,
                start_shape: EndShape::Diamond,
                text: BarText::right(BarField::Finish),
                ..base
            },
            BarCategory::Summary => BarStyle {
                fill: Rgb(58, 58, 58),
                border: Rgb(58, 58, 58),
                height: 0.25,
                start_shape: EndShape::TriangleDown,
                end_shape: EndShape::TriangleDown,
                ..base
            },
            BarCategory::ProjectSummary => BarStyle {
                fill: Rgb(140, 140, 140),
                border: Rgb(140, 140, 140),
                height: 0.25,
                start_shape: EndShape::TriangleDown,
                end_shape: EndShape::TriangleDown,
                ..base
            },
            BarCategory::Progress => BarStyle {
                fill: Rgb(28, 52, 98),
                border: Rgb(28, 52, 98),
                height: 0.14,
                ..base
            },
            BarCategory::Slack => BarStyle {
                fill: Rgb(40, 150, 140),
                border: Rgb(40, 150, 140),
                height: 0.1,
                ..base
            },
            BarCategory::Baseline => BarStyle {
                fill: Rgb(196, 196, 196),
                border: Rgb(140, 140, 140),
                height: 0.22,
                ..base
            },
            BarCategory::LevelingDelay => BarStyle {
                fill: Rgb(160, 140, 60),
                border: Rgb(160, 140, 60),
                height: 0.12,
                start_shape: EndShape::Bracket,
                ..base
            },
            BarCategory::Deadline => BarStyle {
                fill: Rgb(30, 150, 60),
                border: Rgb(30, 150, 60),
                height: 0.4,
                start_shape: EndShape::Arrow,
                ..base
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BarStyles {
    pub map: BTreeMap<BarCategory, BarStyle>,
}

impl BarStyles {
    pub fn get(&self, c: BarCategory) -> BarStyle {
        self.map.get(&c).copied().unwrap_or_else(|| c.default_style())
    }
    pub fn set(&mut self, c: BarCategory, s: BarStyle) {
        if s == c.default_style() {
            self.map.remove(&c);
        } else {
            self.map.insert(c, s);
        }
    }
    pub fn reset(&mut self) {
        self.map.clear();
    }
}

/// Gantt Chart Style gallery entries (Format ▸ Gantt Chart Style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StylePreset {
    pub name: &'static str,
    pub task: Rgb,
    pub summary: Rgb,
    pub milestone: Rgb,
}

pub const STYLE_PRESETS: &[StylePreset] = &[
    StylePreset { name: "Blue (default)", task: Rgb(138, 180, 232), summary: Rgb(58, 58, 58), milestone: Rgb(30, 30, 30) },
    StylePreset { name: "Navy", task: Rgb(68, 114, 196), summary: Rgb(31, 56, 100), milestone: Rgb(31, 56, 100) },
    StylePreset { name: "Green", task: Rgb(146, 208, 120), summary: Rgb(56, 87, 35), milestone: Rgb(56, 87, 35) },
    StylePreset { name: "Teal", task: Rgb(98, 190, 186), summary: Rgb(22, 94, 98), milestone: Rgb(22, 94, 98) },
    StylePreset { name: "Orange", task: Rgb(244, 177, 131), summary: Rgb(132, 60, 12), milestone: Rgb(132, 60, 12) },
    StylePreset { name: "Gold", task: Rgb(255, 213, 102), summary: Rgb(127, 96, 0), milestone: Rgb(127, 96, 0) },
    StylePreset { name: "Purple", task: Rgb(180, 160, 220), summary: Rgb(84, 50, 130), milestone: Rgb(84, 50, 130) },
    StylePreset { name: "Rose", task: Rgb(240, 160, 190), summary: Rgb(140, 40, 80), milestone: Rgb(140, 40, 80) },
    StylePreset { name: "Slate", task: Rgb(170, 180, 195), summary: Rgb(60, 70, 85), milestone: Rgb(60, 70, 85) },
    StylePreset { name: "Ink (print)", task: Rgb(220, 220, 220), summary: Rgb(0, 0, 0), milestone: Rgb(0, 0, 0) },
];

impl BarStyles {
    pub fn apply_preset(&mut self, p: &StylePreset) {
        let mut t = self.get(BarCategory::Task);
        t.fill = p.task;
        t.border = p.task.darker(0.3);
        self.set(BarCategory::Task, t);
        for c in [BarCategory::Summary, BarCategory::ProjectSummary] {
            let mut s = self.get(c);
            s.fill = if c == BarCategory::Summary { p.summary } else { p.summary.lighter(0.4) };
            s.border = s.fill;
            self.set(c, s);
        }
        let mut m = self.get(BarCategory::Milestone);
        m.fill = p.milestone;
        m.border = p.milestone;
        self.set(BarCategory::Milestone, m);
        let mut pr = self.get(BarCategory::Progress);
        pr.fill = p.task.darker(0.65);
        pr.border = pr.fill;
        self.set(BarCategory::Progress, pr);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GanttViewKind {
    #[default]
    Gantt,
    Tracking,
    Detail,
    Leveling,
}

impl GanttViewKind {
    pub const ALL: [GanttViewKind; 4] = [GanttViewKind::Gantt, GanttViewKind::Tracking, GanttViewKind::Detail, GanttViewKind::Leveling];
    pub fn label(&self) -> &'static str {
        match self {
            GanttViewKind::Gantt => "Gantt Chart",
            GanttViewKind::Tracking => "Tracking Gantt",
            GanttViewKind::Detail => "Detail Gantt",
            GanttViewKind::Leveling => "Leveling Gantt",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub color: Option<Rgb>,
    pub bold: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { color: None, bold: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyles {
    pub normal: TextStyle,
    pub critical: TextStyle,
    pub summary: TextStyle,
    pub milestone: TextStyle,
    pub highlighted_fill: Rgb,
}

impl Default for TextStyles {
    fn default() -> Self {
        TextStyles {
            normal: TextStyle::default(),
            critical: TextStyle::default(),
            summary: TextStyle { color: None, bold: true },
            milestone: TextStyle::default(),
            highlighted_fill: Rgb(255, 242, 160),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gridlines {
    pub gantt_rows: bool,
    pub sheet_rows: bool,
    pub sheet_columns: bool,
    pub major_columns: bool,
    pub minor_columns: bool,
    pub current_date: bool,
    pub status_date: bool,
    pub project_start: bool,
    pub project_finish: bool,
    /// Progress line at the status date (or today).
    pub progress_lines: bool,
    pub line_color: Rgb,
    pub current_date_color: Rgb,
    pub status_date_color: Rgb,
}

impl Default for Gridlines {
    fn default() -> Self {
        Gridlines {
            gantt_rows: false,
            sheet_rows: true,
            sheet_columns: true,
            major_columns: false,
            minor_columns: false,
            current_date: true,
            status_date: true,
            project_start: true,
            project_finish: false,
            progress_lines: false,
            line_color: Rgb(218, 218, 218),
            current_date_color: Rgb(205, 105, 15),
            status_date_color: Rgb(200, 40, 40),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkStyle {
    Elbow,
    Straight,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateFormat {
    DowMdyy,
    Mdyy,
    Mdyyyy,
    MonDyy,
    DowMonD,
    Iso,
    DMonYyyy,
    DdMmYyyy,
}

impl DateFormat {
    pub const ALL: [DateFormat; 8] = [
        DateFormat::DowMdyy,
        DateFormat::Mdyy,
        DateFormat::Mdyyyy,
        DateFormat::MonDyy,
        DateFormat::DowMonD,
        DateFormat::Iso,
        DateFormat::DMonYyyy,
        DateFormat::DdMmYyyy,
    ];
    pub fn format(&self, dt: NaiveDateTime) -> String {
        let (y, m, d) = (dt.year(), dt.month(), dt.day());
        match self {
            DateFormat::DowMdyy => format!("{} {}/{}/{:02}", dt.format("%a"), m, d, y % 100),
            DateFormat::Mdyy => format!("{}/{}/{:02}", m, d, y % 100),
            DateFormat::Mdyyyy => format!("{}/{}/{}", m, d, y),
            DateFormat::MonDyy => format!("{} {} '{:02}", dt.format("%b"), d, y % 100),
            DateFormat::DowMonD => format!("{} {} {}, '{:02}", dt.format("%a"), dt.format("%b"), d, y % 100),
            DateFormat::Iso => format!("{:04}-{:02}-{:02}", y, m, d),
            DateFormat::DMonYyyy => format!("{} {} {}", d, dt.format("%b"), y),
            DateFormat::DdMmYyyy => format!("{:02}/{:02}/{}", d, m, y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutOptions {
    pub links: LinkStyle,
    pub date_format: DateFormat,
    /// Bar height multiplier applied to every bar style (Layout ▸ Bar height).
    pub bar_scale: f32,
    /// Draw text labels on bars.
    pub show_bar_text: bool,
    /// Short date (m/d) for dates shown as bar text, as Project does.
    pub short_bar_dates: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            links: LinkStyle::Elbow,
            date_format: DateFormat::DowMdyy,
            bar_scale: 1.0,
            show_bar_text: true,
            short_bar_dates: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TsUnit {
    Hours,
    Days,
    Weeks,
    ThirdsOfMonths,
    Months,
    Quarters,
    HalfYears,
    Years,
}

impl TsUnit {
    pub const ALL: [TsUnit; 8] = [
        TsUnit::Hours,
        TsUnit::Days,
        TsUnit::Weeks,
        TsUnit::ThirdsOfMonths,
        TsUnit::Months,
        TsUnit::Quarters,
        TsUnit::HalfYears,
        TsUnit::Years,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            TsUnit::Hours => "Hours",
            TsUnit::Days => "Days",
            TsUnit::Weeks => "Weeks",
            TsUnit::ThirdsOfMonths => "Thirds of Months",
            TsUnit::Months => "Months",
            TsUnit::Quarters => "Quarters",
            TsUnit::HalfYears => "Half Years",
            TsUnit::Years => "Years",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tier {
    pub unit: TsUnit,
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NonworkingDraw {
    Behind,
    InFront,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimescaleSpec {
    /// Automatic tiers chosen from the zoom level (Project's presets).
    pub auto: bool,
    /// Number of tiers shown: 1, 2 or 3.
    pub tiers: u8,
    pub top: Tier,
    pub middle: Tier,
    pub bottom: Tier,
    pub nonworking: NonworkingDraw,
    pub nonworking_color: Rgb,
}

impl Default for TimescaleSpec {
    fn default() -> Self {
        TimescaleSpec {
            auto: true,
            tiers: 2,
            top: Tier { unit: TsUnit::Months, count: 1 },
            middle: Tier { unit: TsUnit::Weeks, count: 1 },
            bottom: Tier { unit: TsUnit::Days, count: 1 },
            nonworking: NonworkingDraw::Behind,
            nonworking_color: Rgb(236, 236, 236),
        }
    }
}

/// Everything the Format tab controls for the Gantt view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GanttFormat {
    pub view: GanttViewKind,
    pub styles: BarStyles,
    pub show_critical: bool,
    pub show_slack: bool,
    pub show_late: bool,
    pub show_baseline: bool,
    /// Slippage: a line from each task's baseline start to its start.
    pub show_slippage: bool,
    /// Which baseline the bars show (0 = Baseline, 1–10).
    pub baseline_index: u8,
    pub gridlines: Gridlines,
    pub layout: LayoutOptions,
    pub timescale: TimescaleSpec,
    pub text: TextStyles,
}

impl GanttFormat {
    /// Effective switches for the current view: Tracking and Detail Gantt
    /// always show the critical path; Tracking adds baselines, Detail adds
    /// slack bars.
    pub fn critical_on(&self) -> bool {
        self.show_critical || matches!(self.view, GanttViewKind::Tracking | GanttViewKind::Detail | GanttViewKind::Leveling)
    }
    pub fn slack_on(&self) -> bool {
        self.show_slack || matches!(self.view, GanttViewKind::Detail | GanttViewKind::Leveling)
    }
    pub fn leveling_on(&self) -> bool {
        self.view == GanttViewKind::Leveling
    }
    pub fn baseline_on(&self) -> bool {
        self.show_baseline || self.view == GanttViewKind::Tracking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_and_old_files_load() {
        let f = GanttFormat::default();
        let s = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<GanttFormat>(&s).unwrap(), f);
        // an empty object (older file) loads with defaults
        let g: GanttFormat = serde_json::from_str("{}").unwrap();
        assert_eq!(g, f);
    }

    #[test]
    fn styles_store_only_overrides() {
        let mut s = BarStyles::default();
        let mut t = s.get(BarCategory::Task);
        t.fill = Rgb(1, 2, 3);
        s.set(BarCategory::Task, t);
        assert_eq!(s.map.len(), 1);
        s.set(BarCategory::Task, BarCategory::Task.default_style());
        assert!(s.map.is_empty());
        s.apply_preset(&STYLE_PRESETS[2]);
        assert_eq!(s.get(BarCategory::Task).fill, STYLE_PRESETS[2].task);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<BarStyles>(&json).unwrap(), s);
    }

    #[test]
    fn date_formats() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_hms_opt(8, 0, 0).unwrap();
        assert_eq!(DateFormat::DowMdyy.format(d), "Mon 1/5/26");
        assert_eq!(DateFormat::Iso.format(d), "2026-01-05");
        assert_eq!(DateFormat::DMonYyyy.format(d), "5 Jan 2026");
        assert_eq!(DateFormat::DdMmYyyy.format(d), "05/01/2026");
        assert_eq!(DateFormat::MonDyy.format(d), "Jan 5 '26");
    }

    #[test]
    fn view_kinds_imply_switches() {
        let mut f = GanttFormat::default();
        assert!(!f.critical_on() && !f.baseline_on() && !f.slack_on());
        f.view = GanttViewKind::Tracking;
        assert!(f.critical_on() && f.baseline_on());
        f.view = GanttViewKind::Detail;
        assert!(f.slack_on());
    }
}
