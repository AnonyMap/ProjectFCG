//! Scheduling behaviour tests. Expected dates are hand-derived from
//! Microsoft Project's rules using the Standard calendar (Mon–Fri,
//! 8–12 / 13–17) and a project starting Monday 2026-01-05 8:00 AM.

use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::calendar::CalendarException;
use projectfcg::duration::{DurUnit, Lag};
use projectfcg::model::{ConstraintType, LinkType, Project, TaskMode};
use projectfcg::ops::Field;
use projectfcg::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

fn proj() -> Project {
    Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
}

/// Add a task with duration text; returns its row.
fn add(p: &mut Project, name: &str, dur: &str) -> usize {
    let r = p.append_task(name);
    p.set_field_text(r, Field::Duration, dur).unwrap();
    r
}

fn link(p: &mut Project, a: usize, b: usize, kind: LinkType, lag: &str) {
    let lag = Lag::parse(lag, &p.settings.durations).unwrap();
    p.add_link(a, b, kind, lag).unwrap();
}

fn start(p: &Project, r: usize) -> NaiveDateTime {
    p.tasks[r].calc.start.unwrap()
}
fn finish(p: &Project, r: usize) -> NaiveDateTime {
    p.tasks[r].calc.finish.unwrap()
}

#[test]
fn single_task_five_days() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    schedule(&mut p);
    assert_eq!(start(&p, a), dt(1, 5, 8));
    assert_eq!(finish(&p, a), dt(1, 9, 17));
    assert_eq!(p.calc.finish, Some(dt(1, 9, 17)));
}

#[test]
fn new_tasks_default_to_one_day_estimated() {
    let mut p = proj();
    let r = p.append_task("New");
    assert!(p.tasks[r].duration.estimated);
    assert_eq!(p.tasks[r].duration.format(&p.settings.durations), "1 day?");
    // typing a duration clears the estimate flag
    p.set_field_text(r, Field::Duration, "2d").unwrap();
    assert!(!p.tasks[r].duration.estimated);
}

#[test]
fn finish_to_start_crosses_weekend() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "3d");
    link(&mut p, a, b, LinkType::FS, "");
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 12, 8));
    assert_eq!(finish(&p, b), dt(1, 14, 17));
}

#[test]
fn fs_with_lag_and_lead() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "1d");
    link(&mut p, a, b, LinkType::FS, "+2d");
    link(&mut p, a, c, LinkType::FS, "-1d");
    schedule(&mut p);
    // Fri finish + 2 working days lag (Mon, Tue) → starts Wed
    assert_eq!(start(&p, b), dt(1, 14, 8));
    // 1-day lead overlaps A's last day
    assert_eq!(start(&p, c), dt(1, 9, 8));
}

#[test]
fn start_to_start_with_lag() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    link(&mut p, a, b, LinkType::SS, "+2d");
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 7, 8));
    assert_eq!(finish(&p, b), dt(1, 8, 17));
}

#[test]
fn finish_to_finish() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    link(&mut p, a, b, LinkType::FF, "");
    schedule(&mut p);
    assert_eq!(finish(&p, b), dt(1, 9, 17));
    assert_eq!(start(&p, b), dt(1, 8, 8));
}

#[test]
fn start_to_finish() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    p.set_constraint(a, ConstraintType::SNET, Some(dt(1, 12, 8))).unwrap();
    link(&mut p, a, b, LinkType::SF, "");
    schedule(&mut p);
    // B must finish when A starts (Mon 1/12 8 AM) → previous working end Fri 5 PM
    assert_eq!(finish(&p, b), dt(1, 9, 17));
    assert_eq!(start(&p, b), dt(1, 8, 8));
}

#[test]
fn percent_lag_uses_predecessor_duration() {
    let mut p = proj();
    let a = add(&mut p, "A", "4d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "+50%");
    schedule(&mut p);
    // A ends Thu; 2 days lag (Fri, Mon) → B starts Tue
    assert_eq!(start(&p, b), dt(1, 13, 8));
}

#[test]
fn elapsed_lag_counts_weekends() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "+2ed");
    schedule(&mut p);
    // Fri 5 PM + 48h = Sun 5 PM → next working time Mon 8 AM
    assert_eq!(start(&p, b), dt(1, 12, 8));
}

#[test]
fn milestone_sits_at_predecessor_finish() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let m = add(&mut p, "Done", "0d");
    link(&mut p, a, m, LinkType::FS, "");
    schedule(&mut p);
    assert!(p.tasks[m].is_milestone());
    assert_eq!(start(&p, m), dt(1, 9, 17));
    assert_eq!(finish(&p, m), dt(1, 9, 17));
}

#[test]
fn summary_rolls_up_children() {
    let mut p = proj();
    let s = add(&mut p, "Phase", "1d");
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.indent(&[a, b]).unwrap();
    link(&mut p, a, b, LinkType::FS, "");
    p.tasks[a].percent_complete = 100.0;
    schedule(&mut p);
    assert!(p.tasks[s].calc.is_summary);
    assert_eq!(start(&p, s), dt(1, 5, 8));
    assert_eq!(finish(&p, s), dt(1, 9, 17));
    assert_eq!(p.tasks[s].calc.summary_duration, 5 * 480);
    // duration-weighted rollup: 2 of 5 days complete
    assert!((p.tasks[s].calc.percent_complete - 40.0).abs() < 1e-9);
    assert_eq!(p.tasks[b].calc.outline_number, "1.2");
}

#[test]
fn summary_duration_cannot_be_typed() {
    let mut p = proj();
    let s = add(&mut p, "Phase", "1d");
    let a = add(&mut p, "A", "2d");
    p.indent(&[a]).unwrap();
    assert!(p.set_field_text(s, Field::Duration, "4d").is_err());
}

#[test]
fn link_between_summaries_drives_all_children() {
    let mut p = proj();
    let s1 = add(&mut p, "Design", "1d");
    let a = add(&mut p, "A", "2d");
    let s2 = add(&mut p, "Build", "1d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "3d");
    p.indent(&[a]).unwrap();
    p.indent(&[b, c]).unwrap();
    link(&mut p, s1, s2, LinkType::FS, "");
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 7, 8));
    assert_eq!(start(&p, c), dt(1, 7, 8));
    assert_eq!(finish(&p, s2), dt(1, 9, 17));
}

#[test]
fn summary_as_predecessor_waits_for_latest_child() {
    let mut p = proj();
    let _s = add(&mut p, "Phase", "1d");
    let a = add(&mut p, "A", "2d");
    let b = add(&mut p, "B", "3d");
    p.indent(&[a, b]).unwrap();
    let d = add(&mut p, "D", "1d");
    p.outdent(&[d]).unwrap();
    link(&mut p, 0, d, LinkType::FS, "");
    schedule(&mut p);
    assert_eq!(start(&p, d), dt(1, 8, 8));
}

#[test]
fn critical_path_and_slack() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    let c = add(&mut p, "C", "2d");
    link(&mut p, a, c, LinkType::FS, "");
    link(&mut p, b, c, LinkType::FS, "");
    schedule(&mut p);
    assert!(p.tasks[a].calc.critical);
    assert!(p.tasks[c].calc.critical);
    assert!(!p.tasks[b].calc.critical);
    assert_eq!(p.tasks[b].calc.total_slack, 3 * 480);
    assert_eq!(p.tasks[b].calc.free_slack, 3 * 480);
    assert_eq!(p.tasks[a].calc.total_slack, 0);
    assert_eq!(p.tasks[b].calc.late_start, Some(dt(1, 8, 8)));
}

#[test]
fn free_slack_less_than_total_slack() {
    // B(1d) → C(1d) → E ; A(5d) → E. B has 0 free slack but 3d total.
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "1d");
    let e = add(&mut p, "E", "1d");
    link(&mut p, a, e, LinkType::FS, "");
    link(&mut p, b, c, LinkType::FS, "");
    link(&mut p, c, e, LinkType::FS, "");
    schedule(&mut p);
    assert_eq!(p.tasks[b].calc.total_slack, 3 * 480);
    assert_eq!(p.tasks[b].calc.free_slack, 0);
    assert_eq!(p.tasks[c].calc.free_slack, 3 * 480);
}

#[test]
fn typing_start_sets_snet_constraint() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    p.set_field_text(a, Field::Start, "1/7/26").unwrap();
    assert_eq!(p.tasks[a].constraint, ConstraintType::SNET);
    schedule(&mut p);
    assert_eq!(start(&p, a), dt(1, 7, 8));
    assert_eq!(finish(&p, a), dt(1, 8, 17));
}

#[test]
fn typing_finish_sets_fnet_constraint() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    p.set_field_text(a, Field::Finish, "1/9/26").unwrap();
    assert_eq!(p.tasks[a].constraint, ConstraintType::FNET);
    schedule(&mut p);
    assert_eq!(finish(&p, a), dt(1, 9, 17));
    assert_eq!(start(&p, a), dt(1, 8, 8));
}

#[test]
fn must_start_on_beats_links_and_reports_conflict() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "");
    p.set_constraint(b, ConstraintType::MSO, Some(dt(1, 7, 8))).unwrap();
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 7, 8));
    assert!(p.tasks[b].calc.conflict.is_some());
    assert!(p.tasks[a].calc.total_slack < 0, "predecessor gets negative slack");
}

#[test]
fn constraint_conflict_with_honor_off_lets_links_win() {
    let mut p = proj();
    p.settings.honor_constraints = false;
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "");
    p.set_constraint(b, ConstraintType::MSO, Some(dt(1, 7, 8))).unwrap();
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 12, 8));
}

#[test]
fn finish_no_later_than_produces_negative_slack() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "2d");
    link(&mut p, a, b, LinkType::FS, "");
    p.set_constraint(b, ConstraintType::FNLT, Some(dt(1, 12, 17))).unwrap();
    schedule(&mut p);
    assert_eq!(finish(&p, b), dt(1, 12, 17));
    assert!(p.tasks[b].calc.conflict.is_some());
    assert_eq!(p.tasks[a].calc.total_slack, -480);
}

#[test]
fn start_no_later_than_without_conflict() {
    let mut p = proj();
    let a = add(&mut p, "A", "2d");
    p.set_constraint(a, ConstraintType::SNLT, Some(dt(1, 7, 8))).unwrap();
    schedule(&mut p);
    assert_eq!(start(&p, a), dt(1, 5, 8));
    assert!(p.tasks[a].calc.conflict.is_none());
    assert_eq!(p.tasks[a].calc.late_start, Some(dt(1, 5, 8)));
}

#[test]
fn as_late_as_possible() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "1d");
    let d = add(&mut p, "D", "1d");
    link(&mut p, a, d, LinkType::FS, "");
    link(&mut p, b, d, LinkType::FS, "");
    p.set_constraint(b, ConstraintType::ALAP, None).unwrap();
    p.set_constraint(c, ConstraintType::ALAP, None).unwrap();
    schedule(&mut p);
    // B lands the day before its successor D
    assert_eq!(start(&p, b), dt(1, 9, 8));
    // C has no successors → ends at project finish (D's finish Mon 1/12)
    assert_eq!(finish(&p, c), dt(1, 12, 17));
}

#[test]
fn deadline_flags_and_drives_slack() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    p.tasks[a].deadline = Some(dt(1, 8, 17));
    schedule(&mut p);
    assert!(p.tasks[a].calc.deadline_missed);
    assert_eq!(p.tasks[a].calc.total_slack, -480);
    assert!(p.tasks[a].calc.critical);
}

#[test]
fn holiday_pushes_finish() {
    let mut p = proj();
    let h = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
    p.calendar.exceptions.push(CalendarException {
        name: "Holiday".into(),
        start: h,
        end: h,
        shifts: vec![],
    });
    let a = add(&mut p, "A", "5d");
    schedule(&mut p);
    assert_eq!(finish(&p, a), dt(1, 12, 17));
}

#[test]
fn elapsed_duration_ignores_calendar() {
    let mut p = proj();
    let a = add(&mut p, "Cure concrete", "3ed");
    schedule(&mut p);
    assert_eq!(start(&p, a), dt(1, 5, 8));
    assert_eq!(finish(&p, a), dt(1, 8, 8));
}

#[test]
fn manual_task_stays_put_but_drives_successors() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let m = add(&mut p, "Manual", "2d");
    let s = add(&mut p, "S", "1d");
    p.set_mode(m, TaskMode::Manual);
    p.tasks[m].manual_start = Some(dt(1, 6, 8));
    link(&mut p, a, m, LinkType::FS, "");
    link(&mut p, m, s, LinkType::FS, "");
    schedule(&mut p);
    assert_eq!(start(&p, m), dt(1, 6, 8), "manual task not moved by link");
    assert_eq!(start(&p, s), dt(1, 8, 8), "successor follows manual task");
}

#[test]
fn switching_to_manual_freezes_current_dates() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "");
    schedule(&mut p);
    p.set_mode(b, TaskMode::Manual);
    p.set_field_text(a, Field::Duration, "1d").unwrap();
    schedule(&mut p);
    assert_eq!(start(&p, b), dt(1, 12, 8));
}

#[test]
fn circular_links_are_rejected() {
    let mut p = proj();
    let a = add(&mut p, "A", "1d");
    let b = add(&mut p, "B", "1d");
    let c = add(&mut p, "C", "1d");
    link(&mut p, a, b, LinkType::FS, "");
    link(&mut p, b, c, LinkType::FS, "");
    let err = p.add_link(c, a, LinkType::FS, Lag::default()).unwrap_err();
    assert!(err.contains("Circular"), "{err}");
    assert!(p.tasks[a].predecessors.is_empty());
    let err = p.set_field_text(a, Field::Predecessors, "3").unwrap_err();
    assert!(err.contains("Circular"));
    assert!(p.tasks[a].predecessors.is_empty());
}

#[test]
fn summary_to_own_subtask_link_rejected() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    p.indent(&[a]).unwrap();
    assert!(p.add_link(s, a, LinkType::FS, Lag::default()).is_err());
    assert!(p.add_link(a, s, LinkType::FS, Lag::default()).is_err());
}

#[test]
fn finish_finish_to_summary_rejected() {
    let mut p = proj();
    let x = add(&mut p, "X", "1d");
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    p.indent(&[a]).unwrap();
    assert!(p.add_link(x, s, LinkType::FF, Lag::default()).is_err());
    assert!(p.add_link(x, s, LinkType::FS, Lag::default()).is_ok());
}

#[test]
fn indenting_under_predecessor_drops_invalid_link() {
    let mut p = proj();
    let a = add(&mut p, "A", "1d");
    let b = add(&mut p, "B", "1d");
    link(&mut p, a, b, LinkType::FS, "");
    p.indent(&[b]).unwrap();
    assert!(p.tasks[b].predecessors.is_empty());
}

#[test]
fn predecessor_text_roundtrip() {
    let mut p = proj();
    for n in ["A", "B", "C", "D"] {
        add(&mut p, n, "1d");
    }
    p.set_field_text(3, Field::Predecessors, "1, 2SS+1d ,3ff-50%").unwrap();
    assert_eq!(p.predecessors_text(3), "1,2SS+1 day,3FF-50%");
    assert_eq!(p.tasks[3].predecessors[1].kind, LinkType::SS);
    assert!(p.set_field_text(3, Field::Predecessors, "9").is_err());
    assert!(p.set_field_text(3, Field::Predecessors, "1XX").is_err());
    // IDs follow row position after inserting above
    p.insert_task(0, "New first");
    assert_eq!(p.predecessors_text(4), "2,3SS+1 day,4FF-50%");
}

#[test]
fn ctrl_f2_links_in_selection_order_and_unlink() {
    let mut p = proj();
    for n in ["A", "B", "C"] {
        add(&mut p, n, "1d");
    }
    assert_eq!(p.link_tasks(&[0, 1, 2]).unwrap(), 2);
    schedule(&mut p);
    assert_eq!(start(&p, 2), dt(1, 7, 8));
    // non-contiguous selection order (C then A) links C → A
    let mut q = proj();
    for n in ["A", "B", "C"] {
        add(&mut q, n, "1d");
    }
    q.link_tasks(&[2, 0]).unwrap();
    assert_eq!(q.predecessors_text(0), "3");
    // unlink with one task selected removes all its links
    p.unlink_tasks(&[1]);
    assert!(p.tasks[1].predecessors.is_empty());
    assert!(p.tasks[2].predecessors.is_empty());
}

#[test]
fn insert_takes_level_of_row_below() {
    let mut p = proj();
    add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    add(&mut p, "B", "1d");
    p.indent(&[1, 2]).unwrap();
    let n = p.insert_task(a + 1, "between");
    assert_eq!(p.tasks[n].outline_level, 2);
    let e = p.append_task("end");
    assert_eq!(p.tasks[e].outline_level, 2);
}

#[test]
fn delete_summary_deletes_subtree_and_links() {
    let mut p = proj();
    let s = add(&mut p, "S", "1d");
    let a = add(&mut p, "A", "1d");
    let x = add(&mut p, "X", "1d");
    p.indent(&[a]).unwrap();
    link(&mut p, a, x, LinkType::FS, "");
    p.delete_tasks(&[s]);
    assert_eq!(p.tasks.len(), 1);
    assert_eq!(p.tasks[0].name, "X");
    assert!(p.tasks[0].predecessors.is_empty());
}

#[test]
fn outdent_adopts_following_siblings() {
    // A(1) B(2) C(2) D(2): outdent C → D becomes C's child, like Project.
    let mut p = proj();
    for n in ["A", "B", "C", "D"] {
        add(&mut p, n, "1d");
    }
    p.indent(&[1, 2, 3]).unwrap();
    p.outdent(&[2]).unwrap();
    let lv: Vec<u32> = p.tasks.iter().map(|t| t.outline_level).collect();
    assert_eq!(lv, vec![1, 2, 1, 2]);
    assert!(p.is_summary(2));
}

#[test]
fn cannot_indent_first_task() {
    let mut p = proj();
    add(&mut p, "A", "1d");
    assert!(p.indent(&[0]).is_err());
}

#[test]
fn move_up_down_moves_subtree() {
    let mut p = proj();
    for n in ["A", "S", "S1", "S2", "B"] {
        add(&mut p, n, "1d");
    }
    p.indent(&[2, 3]).unwrap();
    let r = p.move_up(1).unwrap();
    let names: Vec<&str> = p.tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["S", "S1", "S2", "A", "B"]);
    assert_eq!(r, 0);
    let r = p.move_down(0).unwrap();
    let names: Vec<&str> = p.tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["A", "S", "S1", "S2", "B"]);
    assert_eq!(r, 1);
    assert!(p.move_up(2).is_err(), "can't move child above its summary");
}

#[test]
fn collapse_hides_children() {
    let mut p = proj();
    for n in ["S", "A", "B", "C"] {
        add(&mut p, n, "1d");
    }
    p.indent(&[1, 2]).unwrap();
    p.toggle_collapse(0);
    assert_eq!(p.visible_rows(), vec![0, 3]);
    p.show_outline_level(None);
    assert_eq!(p.visible_rows(), vec![0, 1, 2, 3]);
    p.show_outline_level(Some(1));
    assert_eq!(p.visible_rows(), vec![0, 3]);
}

#[test]
fn undo_redo() {
    use projectfcg::history::History;
    let mut p = proj();
    let mut h = History::default();
    let before = p.clone();
    add(&mut p, "A", "1d");
    h.checkpoint("Add A", &before, &p);
    let before = p.clone();
    p.tasks[0].name = "Renamed".into();
    h.checkpoint("Rename", &before, &p);
    assert_eq!(h.undo(&mut p).as_deref(), Some("Rename"));
    assert_eq!(p.tasks[0].name, "A");
    h.undo(&mut p);
    assert!(p.tasks.is_empty());
    h.redo(&mut p);
    h.redo(&mut p);
    assert_eq!(p.tasks[0].name, "Renamed");
    assert!(!h.can_redo());
}

#[test]
fn json_roundtrip_preserves_schedule() {
    let mut p = proj();
    let a = add(&mut p, "A", "5d");
    let b = add(&mut p, "B, with \"quotes\"", "2d");
    link(&mut p, a, b, LinkType::SS, "+1d");
    p.tasks[b].notes = "note".into();
    schedule(&mut p);
    let json = projectfcg::io::to_json(&p).unwrap();
    let q = projectfcg::io::from_json(&json).unwrap();
    assert_eq!(q.tasks.len(), 2);
    assert_eq!(q.tasks[1].calc.start, p.tasks[1].calc.start);
    assert_eq!(q.tasks[1].notes, "note");
    let csv = projectfcg::io::to_csv(&q);
    assert!(csv.contains("\"B, with \"\"quotes\"\"\""));
    assert!(projectfcg::io::from_json("{\"format\":\"other\",\"version\":1}").is_err());
}

#[test]
fn file_save_and_load() {
    let dir = std::env::temp_dir().join(format!("projectfcg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("x.projectfcg");
    let mut p = proj();
    add(&mut p, "A", "3d");
    schedule(&mut p);
    projectfcg::io::save(&p, &path).unwrap();
    let q = projectfcg::io::load(&path).unwrap();
    assert_eq!(q.tasks[0].calc.finish, Some(dt(1, 7, 17)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn large_chain_schedules_quickly_and_respects_links() {
    let mut p = proj();
    for i in 0..2000 {
        let r = p.append_task(&format!("T{i}"));
        p.tasks[r].duration.minutes = 480 * ((i % 5) as i64 + 1);
        p.tasks[r].duration.estimated = false;
        if i % 50 == 0 && i > 0 {
            p.tasks[r].outline_level = 1;
        }
    }
    for i in 1..2000 {
        let kind = LinkType::ALL[i % 4];
        if p.add_link(i - 1, i, kind, Lag::default()).is_err() {
            // links involving summaries may be invalid; skip them
        }
    }
    let t0 = std::time::Instant::now();
    schedule(&mut p);
    let el = t0.elapsed();
    assert!(el.as_millis() < 2000, "schedule took {el:?}");
    // invariant check for every FS link between leaves
    for (i, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            let pi = p.index_of_uid(l.pred).unwrap();
            if l.kind == LinkType::FS {
                assert!(p.tasks[i].calc.start.unwrap() >= p.tasks[pi].calc.finish.unwrap());
            }
        }
    }
    assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
}

#[test]
fn default_unit_option_applies_to_bare_numbers() {
    let mut p = proj();
    p.settings.default_duration_unit = DurUnit::Hour;
    let a = add(&mut p, "A", "4");
    assert_eq!(p.tasks[a].duration.minutes, 240);
}

#[test]
fn opens_files_saved_under_older_names() {
    let p = projectfcg::app::sample::software_release();
    let json = projectfcg::io::to_json(&p).unwrap();
    assert!(json.contains("\"projectfcg-project\""));
    for old in ["ganttry-project", "gantry-project"] {
        let legacy = json.replacen("\"projectfcg-project\"", &format!("\"{old}\""), 1);
        let q = projectfcg::io::from_json(&legacy).unwrap_or_else(|e| panic!("{old}: {e}"));
        assert_eq!(q.tasks.len(), p.tasks.len());
        assert_eq!(q.calc.finish, p.calc.finish);
    }
    for name in ["Plan.GANTRY", "plan.ganttry", "plan.pfcg", "Plan.PFCG"] {
        assert!(projectfcg::io::is_project_file(name), "{name}");
    }
    assert!(!projectfcg::io::is_project_file("plan.json"));
    assert!(!projectfcg::io::is_project_file("site.fcg"), "FastCGI scripts aren't projects");
    assert!(projectfcg::io::from_json(&json.replacen("\"projectfcg-project\"", "\"other\"", 1)).is_err());
}
