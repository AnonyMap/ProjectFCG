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

fn add_filter(app: &mut ProjectFcgApp, f: CustomFilter) -> u32 {
    app.edit_project("New Filter", |p| Ok(p.views.add(f))).unwrap()
}

fn cf(name: &str, conditions: Vec<Condition>, related_summaries: bool) -> CustomFilter {
    CustomFilter { id: 0, name: name.into(), conditions, related_summaries }
}

#[test]
fn custom_filter_compares_durations_as_durations() {
    let (mut a, ctx) = app();
    // "10" < "2" as text; as durations 2 days < 10 days
    let k = add_filter(&mut a, cf("Long", vec![cond(ColumnId::Duration, Test::GreaterOrEqual, "2d")], false));
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    // Build (summary, 6 days) passes too
    assert_eq!(names(&a), ["Design", "Build", "Code"]);
    assert_eq!(a.filter_label(a.filter), "Long");
}

#[test]
fn and_or_conditions_and_text_tests() {
    let (mut a, ctx) = app();
    let f = cf(
        "Alice or milestones",
        vec![
            cond(ColumnId::ResourceNames, Test::Contains, "alice"),
            cond(ColumnId::Duration, Test::Less, "2d"),
            Condition { or: true, ..cond(ColumnId::Milestone, Test::Equals, "Yes") },
        ],
        false,
    );
    let k = add_filter(&mut a, f);
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    // (Alice And < 2 days) Or milestone
    assert_eq!(names(&a), ["Test", "Launch"]);
}

#[test]
fn dates_compare_by_day_and_within_takes_two_values() {
    let (mut a, ctx) = app();
    // Code runs Jan 7 – Jan 13
    let f = cf("Starts in week 2", vec![Condition { value2: "1/9/26".into(), ..cond(ColumnId::Start, Test::Within, "1/7/26") }], false);
    let k = add_filter(&mut a, f);
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    assert_eq!(names(&a), ["Build", "Code"]);
    let f = cf("Start on Jan 7", vec![cond(ColumnId::Start, Test::Equals, "1/7/2026")], false);
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
    let k = add_filter(&mut a, cf("Mine", vec![cond(ColumnId::Name, Test::Contains, "e")], true));
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
fn deleting_and_undoing_keeps_the_active_filter() {
    let (mut a, ctx) = app();
    let only = |n: &str| cf(n, vec![cond(ColumnId::Name, Test::Equals, n)], false);
    let k0 = add_filter(&mut a, only("Design"));
    let k1 = add_filter(&mut a, only("Code"));
    a.execute(Command::SetFilter(Filter::Custom(k1)), &ctx);
    a.delete_custom_filter(k0);
    assert_eq!(a.filter_label(a.filter), "Code");
    assert_eq!(names(&a), ["Code"]);
    // undo brings "Design" back; the view still shows "Code"
    a.execute(Command::Undo, &ctx);
    a.check_filters();
    assert_eq!(a.filter_label(a.filter), "Code");
    assert_eq!(names(&a), ["Code"]);
    // deleting the active one stops it applying
    a.delete_custom_filter(k1);
    assert_eq!(a.filter, Filter::AllTasks);
}

// ---- the 0.8 review's findings ----

#[test]
fn review_shift_arrow_extends_by_screen_order_when_grouped() {
    let (mut a, ctx) = app();
    // by duration: Launch (0), Test (1), Design (2), Code (5)
    a.execute(Command::SetGroup(GroupBy::Duration), &ctx);
    let test = a.project.tasks.iter().position(|t| t.name == "Test").unwrap();
    a.select_index(test);
    a.move_active(1, 0, true);
    let sel: Vec<String> = a.selected.iter().map(|&u| a.project.task_by_uid(u).unwrap().name.clone()).collect();
    assert_eq!(sel, ["Test", "Design"]);
}

#[test]
fn review_opening_another_file_drops_this_files_highlight() {
    let (mut a, ctx) = app();
    let k = add_filter(&mut a, cf("Mine", vec![], false));
    a.execute(Command::SetHighlight(Filter::Custom(k)), &ctx);
    a.view.collapsed_groups.insert("x".into());
    let mut other = Project::new(d(3, 2));
    other.views.add(cf("Theirs", vec![], false));
    a.replace_project(other, None);
    assert_eq!(a.view.highlight, Filter::AllTasks);
    assert!(a.view.collapsed_groups.is_empty());
}

#[test]
fn review_numbers_compare_as_shown_and_parse_strictly() {
    let (mut a, ctx) = app();
    a.edit_project("cost", |p| {
        p.tasks[0].fixed_cost = 0.1 + 0.2;
        Ok(())
    });
    let k = add_filter(&mut a, cf("30 cents", vec![cond(ColumnId::FixedCost, Test::Equals, "$0.30")], false));
    a.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
    assert_eq!(names(&a), ["Design"]);
    assert!(validate(&a.project, &cond(ColumnId::Cost, Test::Less, "(500)")).is_err());
    assert!(validate(&a.project, &cond(ColumnId::Cost, Test::Less, "1,500.50")).is_ok());
    assert!(validate(&a.project, &cond(ColumnId::PercentComplete, Test::Less, "50%")).is_ok());
}

#[test]
fn review_grouped_summary_filter_lists_summaries_without_double_counting() {
    let (mut a, ctx) = app();
    a.execute(Command::SetFilter(Filter::Summary), &ctx);
    a.execute(Command::SetGroup(GroupBy::Critical), &ctx);
    let n = names(&a);
    assert!(n.contains(&"Build".to_string()), "{n:?}");
    // all tasks: summaries' work isn't added on top of their subtasks'
    a.execute(Command::SetFilter(Filter::AllTasks), &ctx);
    a.execute(Command::SetGroup(GroupBy::Milestones), &ctx);
    a.display_rows();
    let total: i64 = a.groups.borrow().iter().map(|g| g.work).sum();
    assert_eq!(total, a.project.calc.total_work);
}

#[test]
fn review_old_files_get_filter_ids() {
    let (mut a, _) = app();
    a.project.views.filters = vec![cf("A", vec![], false), cf("B", vec![], false)];
    a.project.views.next_filter_id = 0;
    let text = projectfcg::io::to_json(&a.project).unwrap();
    let back = projectfcg::io::from_json(&text).unwrap();
    let ids: Vec<u32> = back.views.filters.iter().map(|f| f.id).collect();
    assert!(ids[0] != 0 && ids[1] != 0 && ids[0] != ids[1], "{ids:?}");
    assert!(back.views.next_filter_id > ids[0].max(ids[1]));
}
