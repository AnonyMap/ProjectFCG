//! Filters (built-in, Date Range, Using Resource, custom) and Group By.

use chrono::NaiveDate;
use projectfcg::app::columns::{set_cell, ColumnId};
use projectfcg::app::commands::{Command, Filter};
use projectfcg::app::filters::{validate, GroupBy};
use projectfcg::app::{DisplayRow, ProjectFcgApp};
use projectfcg::model::Project;
use projectfcg::views::{Condition, CustomFilter, Test};

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

/// 1 Design (2d, Alice) → 2 Build (summary) [3 Code (5d, Bob), 4 Test (1d, Alice)] → 5 Launch (milestone)
fn app() -> (ProjectFcgApp, egui::Context) {
    let mut p = Project::new(d(1, 5));
    for (name, lvl, dur) in [("Design", 1, 2), ("Build", 1, 0), ("Code", 2, 5), ("Test", 2, 1), ("Launch", 1, 0)] {
        let r = p.append_task(name);
        p.tasks[r].outline_level = lvl;
        p.tasks[r].duration.minutes = dur * 480;
        p.tasks[r].duration.estimated = false;
    }
    projectfcg::schedule::refresh_structure(&mut p);
    set_cell(&mut p, 1, ColumnId::Predecessors, "1").unwrap();
    set_cell(&mut p, 4, ColumnId::Predecessors, "2").unwrap();
    set_cell(&mut p, 3, ColumnId::Predecessors, "3").unwrap();
    set_cell(&mut p, 0, ColumnId::ResourceNames, "Alice").unwrap();
    set_cell(&mut p, 2, ColumnId::ResourceNames, "Bob").unwrap();
    set_cell(&mut p, 3, ColumnId::ResourceNames, "Alice").unwrap();
    (ProjectFcgApp::new(p), egui::Context::default())
}

fn names(app: &ProjectFcgApp) -> Vec<String> {
    let rows = app.display_rows();
    let groups = app.groups.borrow();
    rows.iter()
        .map(|r| match r {
            DisplayRow::Task(i) => app.project.tasks[*i].name.clone(),
            DisplayRow::Group(g) => format!("[{}]", groups[*g].label),
            DisplayRow::ProjectSummary => "[project]".into(),
        })
        .collect()
}

fn cond(field: ColumnId, test: Test, v: &str) -> Condition {
    Condition { field: field.key(), test, value: v.into(), ..Default::default() }
}

fn add_filter(app: &mut ProjectFcgApp, f: CustomFilter) -> usize {
    app.edit_project("New Filter", |p| {
        p.views.filters.push(f);
        Ok(p.views.filters.len() - 1)
    })
    .unwrap()
}

#[test]
fn custom_filter_compares_durations_as_durations() {
    let (mut a, ctx) = app();
    // "10" < "2" as text; as durations 2 days < 10 days
    let k = add_filter(&mut a, CustomFilter { name: "Long".into(), conditions: vec![cond(ColumnId::Duration, Test::GreaterOrEqual, "2d")], related_summaries: false });
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    // Build (summary, 6 days) passes too
    assert_eq!(names(&a), ["Design", "Build", "Code"]);
    assert_eq!(a.filter_label(a.filter), "Long");
}

#[test]
fn and_or_conditions_and_text_tests() {
    let (mut a, ctx) = app();
    let f = CustomFilter {
        name: "Alice or milestones".into(),
        conditions: vec![
            cond(ColumnId::ResourceNames, Test::Contains, "alice"),
            cond(ColumnId::Duration, Test::Less, "2d"),
            Condition { or: true, ..cond(ColumnId::Milestone, Test::Equals, "Yes") },
        ],
        related_summaries: false,
    };
    let k = add_filter(&mut a, f);
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    // (Alice And < 2 days) Or milestone
    assert_eq!(names(&a), ["Test", "Launch"]);
}

#[test]
fn dates_compare_by_day_and_within_takes_two_values() {
    let (mut a, ctx) = app();
    // Code runs Jan 7 – Jan 13
    let f = CustomFilter {
        name: "Starts in week 2".into(),
        conditions: vec![Condition { value2: "1/9/26".into(), ..cond(ColumnId::Start, Test::Within, "1/7/26") }],
        related_summaries: false,
    };
    let k = add_filter(&mut a, f);
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    assert_eq!(names(&a), ["Build", "Code"]);
    let f = CustomFilter { name: "Start on Jan 7".into(), conditions: vec![cond(ColumnId::Start, Test::Equals, "1/7/2026")], related_summaries: false };
    let k = add_filter(&mut a, f);
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    assert_eq!(names(&a), ["Build", "Code"]);
}

#[test]
fn related_summary_rows() {
    let (mut a, ctx) = app();
    a.execute(Command::SetFilter(Filter::UsingResource(a.project.resources.iter().find(|r| r.name == "Bob").unwrap().uid)), &ctx);
    assert_eq!(names(&a), ["Code"]);
    a.execute(Command::ToggleRelatedSummaries, &ctx);
    assert_eq!(names(&a), ["Build", "Code"]);
    assert!(a.filter_label(a.filter).contains("Bob"));
}

#[test]
fn date_range_shows_tasks_under_way() {
    let (mut a, ctx) = app();
    // Design Jan 5–6, Code Jan 7–13, Test Jan 14, Launch Jan 14
    a.execute(Command::SetFilter(Filter::DateRange(d(1, 6), d(1, 7))), &ctx);
    assert_eq!(names(&a), ["Design", "Build", "Code"]);
}

#[test]
fn group_by_resource_rolls_up_and_collapses() {
    let (mut a, ctx) = app();
    a.execute(Command::SetGroup(GroupBy::Resource), &ctx);
    // summaries aren't listed; groups sort "No Resource" first, then by name
    assert_eq!(names(&a), ["[No Resource]", "Launch", "[Resource: Alice]", "Design", "Test", "[Resource: Bob]", "Code"]);
    let rows = a.display_rows();
    let groups = a.groups.borrow().clone();
    let alice = &groups[1];
    assert_eq!(alice.count, 2);
    assert_eq!(alice.start.unwrap().date(), d(1, 5));
    assert_eq!(alice.finish.unwrap().date(), d(1, 14));
    assert_eq!(alice.work, 3 * 480);
    // collapse Alice's group
    let pos = rows.iter().position(|r| *r == DisplayRow::Group(1)).unwrap();
    assert_eq!(pos, 2);
    a.execute(Command::ToggleGroupRow(1), &ctx);
    assert_eq!(names(&a), ["[No Resource]", "Launch", "[Resource: Alice]", "[Resource: Bob]", "Code"]);
    // grouping combines with a filter
    a.execute(Command::SetFilter(Filter::Milestones), &ctx);
    assert_eq!(names(&a), ["[No Resource]", "Launch"]);
}

#[test]
fn group_by_field_and_built_ins() {
    let (mut a, ctx) = app();
    a.execute(Command::SetGroup(GroupBy::Duration), &ctx);
    assert_eq!(names(&a), ["[Duration: 0 days]", "Launch", "[Duration: 1 day]", "Test", "[Duration: 2 days]", "Design", "[Duration: 5 days]", "Code"]);
    a.execute(Command::SetGroup(GroupBy::Field(ColumnId::Start)), &ctx);
    assert_eq!(names(&a), ["[Start: Week of Mon 1/5/26]", "Design", "Code", "[Start: Week of Mon 1/12/26]", "Test", "Launch"]);
    a.execute(Command::SetGroup(GroupBy::Critical), &ctx);
    assert_eq!(names(&a)[0], "[Critical: Yes]");
}

#[test]
fn filters_are_saved_undone_and_validated() {
    let (mut a, ctx) = app();
    let k = add_filter(&mut a, CustomFilter { name: "Mine".into(), conditions: vec![cond(ColumnId::Name, Test::Contains, "e")], related_summaries: true });
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    let text = projectfcg::io::to_json(&a.project).unwrap();
    let back = projectfcg::io::from_json(&text).unwrap();
    assert_eq!(back.views, a.project.views);
    // undo removes it, and the view stops applying it
    a.execute(Command::Undo, &ctx);
    assert!(a.project.views.filters.is_empty());
    a.check_filters();
    assert_eq!(a.filter, Filter::AllTasks);
    // bad values are refused with the field's kind
    let e = validate(&a.project, &cond(ColumnId::Start, Test::Greater, "soon")).unwrap_err();
    assert!(e.contains("date"), "{e}");
    assert!(validate(&a.project, &cond(ColumnId::Duration, Test::Greater, "3w")).is_ok());
    assert!(validate(&a.project, &cond(ColumnId::Cost, Test::Less, "$1,200")).is_ok());
    assert!(validate(&a.project, &cond(ColumnId::Critical, Test::Equals, "maybe")).is_err());
}

#[test]
fn deleting_a_filter_shifts_the_active_one() {
    let (mut a, ctx) = app();
    let k0 = add_filter(&mut a, CustomFilter { name: "A".into(), conditions: vec![], related_summaries: false });
    let k1 = add_filter(&mut a, CustomFilter { name: "B".into(), conditions: vec![], related_summaries: false });
    a.execute(Command::SetFilter(Filter::Custom(k1)), &ctx);
    a.delete_custom_filter(k0);
    assert_eq!(a.filter, Filter::Custom(0));
    assert_eq!(a.filter_label(a.filter), "B");
}
