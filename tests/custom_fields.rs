//! Custom fields: typed values, aliases, filters and groups, files and
//! Project XML (extended attributes with Project's field IDs).
use chrono::NaiveDate;
use projectfcg::app::columns::{cell_text, set_cell, ColumnId};
use projectfcg::app::commands::{Command, Filter};
use projectfcg::app::filters::GroupBy;
use projectfcg::app::{DisplayRow, ProjectFcgApp};
use projectfcg::custom_fields::{CustomField, CustomValue, FieldKind};
use projectfcg::model::Project;
use projectfcg::views::{Condition, CustomFilter, Test};

fn f(kind: FieldKind, n: u8) -> ColumnId {
    ColumnId::Custom(CustomField::new(kind, n))
}

fn plan() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    for (name, team, risk, flag, cost) in [("API", "Backend", "8", "yes", "$1,200"), ("UI", "Frontend", "3", "no", ""), ("Docs", "", "5.5", "y", "250")] {
        let r = p.append_task(name);
        set_cell(&mut p, r, f(FieldKind::Text, 1), team).unwrap();
        set_cell(&mut p, r, f(FieldKind::Number, 1), risk).unwrap();
        set_cell(&mut p, r, f(FieldKind::Flag, 1), flag).unwrap();
        set_cell(&mut p, r, f(FieldKind::Cost, 2), cost).unwrap();
    }
    set_cell(&mut p, 0, f(FieldKind::Date, 1), "2/14/2026").unwrap();
    p.set_custom_alias(CustomField::new(FieldKind::Text, 1), "Team").unwrap();
    p.set_custom_alias(CustomField::new(FieldKind::Number, 1), "Risk").unwrap();
    projectfcg::schedule::schedule(&mut p);
    p
}

#[test]
fn values_are_typed_and_shown_like_project() {
    let mut p = plan();
    assert_eq!(cell_text(&p, 0, f(FieldKind::Text, 1)), "Backend");
    assert_eq!(cell_text(&p, 2, f(FieldKind::Number, 1)), "5.5");
    assert_eq!(cell_text(&p, 1, f(FieldKind::Flag, 1)), "No");
    assert_eq!(cell_text(&p, 0, f(FieldKind::Cost, 2)), "$1,200.00");
    assert_eq!(cell_text(&p, 1, f(FieldKind::Cost, 2)), "$0.00", "unset costs show 0");
    assert_eq!(cell_text(&p, 1, f(FieldKind::Date, 1)), "NA");
    assert!(cell_text(&p, 0, f(FieldKind::Date, 1)).contains("2/14/26"));
    // bad values are refused with the field's name
    assert!(set_cell(&mut p, 0, f(FieldKind::Number, 1), "high").unwrap_err().contains("Risk"));
    assert!(set_cell(&mut p, 0, f(FieldKind::Number, 1), "NaN").is_err());
    assert!(set_cell(&mut p, 0, f(FieldKind::Flag, 1), "maybe").is_err());
    assert!(set_cell(&mut p, 0, f(FieldKind::Date, 1), "soon").is_err());
    // clearing removes the value
    set_cell(&mut p, 0, f(FieldKind::Text, 1), "").unwrap();
    assert!(!p.tasks[0].custom.contains_key("Text1"));
    projectfcg::app::columns::reset_cell(&mut p, 0, f(FieldKind::Cost, 2)).unwrap();
    assert!(!p.tasks[0].custom.contains_key("Cost2"));
}

#[test]
fn aliases_name_columns_and_must_be_unique() {
    let mut p = plan();
    assert_eq!(f(FieldKind::Text, 1).title_in(&p), "Team");
    assert_eq!(f(FieldKind::Text, 2).title_in(&p), "Text2");
    assert!(p.set_custom_alias(CustomField::new(FieldKind::Text, 2), "team").is_err());
    p.set_custom_alias(CustomField::new(FieldKind::Text, 1), "").unwrap();
    assert_eq!(f(FieldKind::Text, 1).title_in(&p), "Text1");
    assert_eq!(p.custom_fields_in_use().len(), 5, "Text1, Number1, Flag1, Date1, Cost2");
}

#[test]
fn filters_and_groups_use_custom_fields() {
    let mut app = ProjectFcgApp::new(plan());
    let ctx = egui::Context::default();
    let cf = CustomFilter { id: 0, name: "Risky".into(), conditions: vec![Condition { field: f(FieldKind::Number, 1).key(), test: Test::Greater, value: "5".into(), ..Default::default() }], related_summaries: false };
    let id = app.edit_project("New Filter", |p| Ok(p.views.add(cf))).unwrap();
    app.execute(Command::SetFilter(Filter::Custom(id)), &ctx);
    let names: Vec<String> = app.display_rows().iter().filter_map(|r| if let DisplayRow::Task(i) = r { Some(app.project.tasks[*i].name.clone()) } else { None }).collect();
    assert_eq!(names, ["API", "Docs"], "8 and 5.5 are over 5 (as numbers)");
    app.execute(Command::SetFilter(Filter::AllTasks), &ctx);
    app.execute(Command::SetGroup(GroupBy::Field(f(FieldKind::Text, 1))), &ctx);
    app.display_rows();
    let labels: Vec<String> = app.groups.borrow().iter().map(|g| g.label.clone()).collect();
    assert_eq!(labels, ["Team: No value", "Team: Backend", "Team: Frontend"]);
    assert_eq!(ColumnId::from_key("Flag1"), Some(f(FieldKind::Flag, 1)));
}

#[test]
fn files_undo_and_repair() {
    let p = plan();
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.tasks[0].custom, p.tasks[0].custom);
    assert_eq!(q.custom_aliases, p.custom_aliases);
    let mut app = ProjectFcgApp::new(p);
    app.edit_project("Edit Team", |p| set_cell(p, 1, f(FieldKind::Text, 1), "Platform"));
    app.execute(Command::Undo, &egui::Context::default());
    assert_eq!(cell_text(&app.project, 1, f(FieldKind::Text, 1)), "Frontend");
    // hand-edited file: wrong kind, unknown field
    let mut r = app.project.clone();
    r.tasks[0].custom.insert("Number1".into(), CustomValue::Text("x".into()));
    r.tasks[0].custom.insert("Text99".into(), CustomValue::Text("x".into()));
    r.custom_aliases.insert("Bogus".into(), "y".into());
    assert_eq!(r.repair_custom_fields(), 3);
}

#[test]
fn project_xml_carries_them_with_projects_field_ids() {
    let p = plan();
    let xml = projectfcg::mspdi::to_xml(&p);
    assert!(xml.contains("<FieldID>188743731</FieldID>"), "Text1");
    assert!(xml.contains("<Alias>Team</Alias>"));
    assert!(xml.contains("<Value>120000</Value>"), "Cost2 $1,200 in hundredths");
    let (q, warnings) = projectfcg::mspdi::from_xml(&xml).unwrap();
    assert!(warnings.iter().all(|w| !w.contains("Custom")), "{warnings:?}");
    for (a, b) in p.tasks.iter().zip(&q.tasks) {
        assert_eq!(a.custom, b.custom, "{}", a.name);
    }
    assert_eq!(q.custom_title(CustomField::new(FieldKind::Number, 1)), "Risk");
    // a field we don't hold is reported, not lost silently
    let extra = xml.replacen("<ExtendedAttributes>", "<ExtendedAttributes><ExtendedAttribute><FieldID>188744000</FieldID><FieldName>Text20</FieldName><Alias>Region</Alias></ExtendedAttribute>", 1);
    let (_, warnings) = projectfcg::mspdi::from_xml(&extra).unwrap();
    assert!(warnings.iter().any(|w| w.contains("Region")), "{warnings:?}");
}

#[test]
fn numbers_keep_every_digit_through_fill_down_and_project_xml() {
    use projectfcg::app::columns::edit_text;
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let a = p.append_task("A");
    let b = p.append_task("B");
    let col = f(FieldKind::Number, 1);
    set_cell(&mut p, a, col, "0.125").unwrap();
    let src = edit_text(&p, a, col);
    set_cell(&mut p, b, col, &src).unwrap();
    assert_eq!(p.tasks[b].custom.get("Number1"), Some(&CustomValue::Number(0.125)));
    set_cell(&mut p, a, col, "0.001").unwrap();
    set_cell(&mut p, b, f(FieldKind::Cost, 1), "0.125").unwrap();
    projectfcg::schedule::schedule(&mut p);
    let (q, _) = projectfcg::mspdi::from_xml(&projectfcg::mspdi::to_xml(&p)).unwrap();
    assert_eq!(q.tasks[0].custom.get("Number1"), Some(&CustomValue::Number(0.001)));
    assert_eq!(q.tasks[1].custom.get("Number1"), Some(&CustomValue::Number(0.125)));
}
