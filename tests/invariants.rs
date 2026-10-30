//! Seeded randomized tests: properties that must hold on any project, not
//! just hand-built examples.
mod common;
use common::{random_project, Rng, Shape};
use projectfcg::history::History;
use chrono::NaiveDate;
use projectfcg::duration::Lag;
use projectfcg::model::LinkType;
use projectfcg::schedule::schedule;

const SEEDS: u64 = 40;

fn shape(seed: u64) -> Shape {
    Shape { tasks: 30 + (seed as usize * 37) % 170, resources: 2 + (seed as usize % 6), ..Default::default() }
}

/// Every link is satisfied by the computed dates (unless a constraint or a
/// manual task legitimately overrides it, which is flagged as a conflict
/// or is the manual task itself).
#[test]
fn links_are_respected() {
    for seed in 0..SEEDS {
        let p = random_project(seed, &shape(seed));
        let idx: std::collections::HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        for s in p.tasks.iter() {
            if s.calc.is_summary || s.mode == projectfcg::model::TaskMode::Manual || s.calc.conflict.is_some() {
                continue;
            }
            for l in &s.predecessors {
                let pi = idx[&l.pred];
                let pr = &p.tasks[pi];
                if pr.calc.is_summary || !l.lag.is_zero() {
                    continue; // summary links and lags are covered by the hand-built tests
                }
                let (ps, pf) = (pr.calc.start.unwrap(), pr.calc.finish.unwrap());
                let (ss, sf) = (s.calc.start.unwrap(), s.calc.finish.unwrap());
                // "x is not before y" in working time: equal instants across
                // a weekend or holiday (Fri 5 PM vs Tue 8 AM) count as equal
                let not_before = |x: chrono::NaiveDateTime, y: chrono::NaiveDateTime| x >= y || p.calendar.work_between(x, y) == 0;
                let ok = match l.kind {
                    LinkType::FS => not_before(ss, pf),
                    LinkType::SS => not_before(ss, ps),
                    LinkType::FF => not_before(sf, pf),
                    LinkType::SF => not_before(sf, ps),
                };
                assert!(ok, "seed {seed}: {:?} link {} -> {} violated ({ps}..{pf} vs {ss}..{sf})", l.kind, pr.name, s.name);
            }
        }
    }
}

/// Slack is consistent with the dates, and critical tasks are exactly the
/// ones at or under the threshold.
#[test]
fn slack_and_critical_agree() {
    for seed in 0..SEEDS {
        let p = random_project(seed, &shape(seed));
        for t in p.tasks.iter().filter(|t| !t.calc.is_summary) {
            let c = &t.calc;
            assert!(c.start <= c.finish, "seed {seed}: {} starts after it finishes", t.name);
            assert_eq!(c.critical, c.total_slack <= p.settings.critical_slack_threshold, "seed {seed}: {}", t.name);
            assert!(c.free_slack <= c.total_slack.max(0) || c.total_slack < 0, "seed {seed}: {} free slack {} > total {}", t.name, c.free_slack, c.total_slack);
        }
    }
}

/// Summaries span their children exactly, and roll up work and cost.
#[test]
fn summaries_roll_up() {
    for seed in 0..SEEDS {
        let p = random_project(seed, &shape(seed));
        for (i, t) in p.tasks.iter().enumerate().filter(|(_, t)| t.calc.is_summary) {
            let kids: Vec<usize> = ((i + 1)..p.subtree_end(i)).filter(|&k| !p.tasks[k].calc.is_summary).collect();
            if kids.is_empty() {
                continue;
            }
            let s = kids.iter().filter_map(|&k| p.tasks[k].calc.start).min();
            let f = kids.iter().filter_map(|&k| p.tasks[k].calc.finish).max();
            assert_eq!(t.calc.start, s, "seed {seed}: summary {} start", t.name);
            assert_eq!(t.calc.finish, f, "seed {seed}: summary {} finish", t.name);
            let work: i64 = kids.iter().map(|&k| p.tasks[k].calc.work).sum();
            assert_eq!(t.calc.work, work, "seed {seed}: summary {} work", t.name);
            let cost: f64 = kids.iter().map(|&k| p.tasks[k].calc.cost).sum::<f64>() + t.fixed_cost;
            assert!((t.calc.cost - cost).abs() < 0.01, "seed {seed}: summary {} cost {} vs {}", t.name, t.calc.cost, cost);
        }
        let total: f64 = p.tasks.iter().filter(|t| p.parent_of(p.index_of_uid(t.uid).unwrap()).is_none()).map(|t| t.calc.cost).sum();
        assert!((p.calc.total_cost - total).abs() < 0.01, "seed {seed}: project cost");
    }
}

/// Work = units × span for every work assignment, and the task's duration
/// is its longest assignment.
#[test]
fn work_units_duration() {
    for seed in 0..SEEDS {
        let p = random_project(seed, &shape(seed));
        for (i, t) in p.tasks.iter().enumerate() {
            let w = p.work_assignments(i);
            if w.is_empty() || t.calc.is_summary {
                continue;
            }
            let longest = w.iter().map(|&ai| p.assignment_span(ai)).max().unwrap();
            assert!((t.duration.minutes - longest).abs() <= 1, "seed {seed}: {} duration {} vs longest assignment {}", t.name, t.duration.minutes, longest);
            for &ai in &w {
                let a = &p.assignments[ai];
                let span = p.assignment_span(ai) as f64;
                assert!((a.work as f64 - a.units * span).abs() <= 1.0 + a.units, "seed {seed}: {} work {} != {}×{}", t.name, a.work, a.units, span);
            }
        }
    }
}

/// Saving and loading reproduces the project exactly, including every
/// computed date.
#[test]
fn file_round_trip_is_exact() {
    for seed in 0..SEEDS {
        let p = random_project(seed, &shape(seed));
        let json = projectfcg::io::to_json(&p).unwrap();
        let q = projectfcg::io::from_json(&json).unwrap();
        assert_eq!(projectfcg::io::to_json(&q).unwrap(), json, "seed {seed}");
        for (a, b) in p.tasks.iter().zip(&q.tasks) {
            assert_eq!(a.calc.start, b.calc.start, "seed {seed} {}", a.name);
            assert_eq!(a.calc.finish, b.calc.finish);
            assert_eq!(a.calc.total_slack, b.calc.total_slack);
        }
        assert_eq!(p.calc.total_cost, q.calc.total_cost);
    }
}

/// Scheduling is a pure function of the inputs: running it again changes
/// nothing.
#[test]
fn scheduling_is_idempotent() {
    for seed in 0..SEEDS {
        let mut p = random_project(seed, &shape(seed));
        let before: Vec<_> = p.tasks.iter().map(|t| (t.calc.start, t.calc.finish, t.calc.total_slack)).collect();
        schedule(&mut p);
        let after: Vec<_> = p.tasks.iter().map(|t| (t.calc.start, t.calc.finish, t.calc.total_slack)).collect();
        assert_eq!(before, after, "seed {seed}");
    }
}

/// A random sequence of edits, then undoing all of them, gets back exactly
/// the original project; redoing them all gets back the edited one.
#[test]
fn undo_redo_random_edits() {
    use projectfcg::ops::Field;
    for seed in 0..SEEDS / 2 {
        let mut p = random_project(seed, &shape(seed));
        let original = projectfcg::io::to_json(&p).unwrap();
        let mut h = History::default();
        let mut r = Rng::new(seed + 1000);
        for step in 0..25 {
            let before = p.clone();
            let n = p.tasks.len();
            let row = r.below(n as u64) as usize;
            let res = match r.below(11) {
                7 => p.add_ticket(&format!("Ticket {step}"), None).map(|_| ()),
                8 => {
                    p.board.sprint_days = 1 + r.below(15) as u32;
                    p.title = format!("Title {step}");
                    Ok(())
                }
                9 => p.set_sprint(row, r.chance(50)),
                10 => p.move_up(row).map(|_| ()),
                0 => p.set_field_text(row, Field::Duration, &format!("{}d", 1 + r.below(9))),
                1 => p.set_field_text(row, Field::Name, &format!("Edited {step}")),
                2 => p.indent(&[row]).map(|_| ()),
                3 => p.outdent(&[row]).map(|_| ()),
                4 => p.add_link(r.below(n as u64) as usize, row, LinkType::FS, Default::default()),
                5 => {
                    p.delete_tasks(&[row]);
                    Ok(())
                }
                _ => p.set_field_text(row, Field::ResourceNames, if r.chance(50) { "R0" } else { "" }),
            };
            if res.is_ok() {
                schedule(&mut p);
                h.checkpoint(&format!("step {step}"), &before, &p);
            } else {
                p = before; // refused edits must leave nothing behind
            }
        }
        let edited = projectfcg::io::to_json(&p).unwrap();
        while h.undo(&mut p).is_some() {}
        assert_eq!(projectfcg::io::to_json(&p).unwrap(), original, "seed {seed}: undo all");
        while h.redo(&mut p).is_some() {}
        assert_eq!(projectfcg::io::to_json(&p).unwrap(), edited, "seed {seed}: redo all");
    }
}

/// The fast leveler gives exactly the same delays as the reference that
/// reschedules after every single delay.
#[test]
fn fast_leveling_matches_reference() {
    for seed in 0..SEEDS {
        let mut p = random_project(seed, &shape(seed));
        p.settings.level_within_slack = seed % 5 == 0;
        let mut q = p.clone();
        let a = projectfcg::leveling::level(&mut p, None);
        let b = projectfcg::leveling::level_exhaustive(&mut q, None);
        let da: Vec<i64> = p.tasks.iter().map(|t| t.leveling_delay).collect();
        let db: Vec<i64> = q.tasks.iter().map(|t| t.leveling_delay).collect();
        assert_eq!(da, db, "seed {seed}: leveling delays differ");
        assert_eq!(a, b, "seed {seed}: results differ");
    }
}

/// Where nothing makes it impossible (no manual tasks, no constraints, no
/// assignment above its resource's maximum), leveling removes every
/// overallocation, delays only tasks that had a conflict, and Clear
/// Leveling restores the original schedule.
#[test]
fn leveling_resolves_everything_resolvable() {
    for seed in 0..SEEDS {
        let mut p = random_project(seed, &Shape { constraints: false, manual: false, ..shape(seed) });
        // cap units at each resource's maximum
        for ai in 0..p.assignments.len() {
            let max = p.resources[p.resource_index(p.assignments[ai].resource).unwrap()].max_units;
            if p.assignments[ai].units > max {
                let _ = p.set_assignment_units(ai, max);
            }
        }
        schedule(&mut p);
        let before: Vec<_> = p.tasks.iter().map(|t| (t.calc.start, t.calc.finish)).collect();
        let r = projectfcg::leveling::level(&mut p, None);
        let over: Vec<&str> = p.resources.iter().zip(&p.calc.resources).filter(|(_, c)| c.overallocated).map(|(r, _)| r.name.as_str()).collect();
        assert!(over.is_empty(), "seed {seed}: still overallocated {over:?}; reported {:?}", r.unresolved);
        assert!(r.unresolved.is_empty(), "seed {seed}: {:?}", r.unresolved);
        // nothing finishes earlier than before
        for (t, (s0, _)) in p.tasks.iter().zip(&before) {
            assert!(t.calc.start >= *s0 || t.calc.is_summary, "seed {seed}: {} moved earlier", t.name);
        }
        projectfcg::leveling::clear_leveling(&mut p, None);
        let cleared: Vec<_> = p.tasks.iter().map(|t| (t.calc.start, t.calc.finish)).collect();
        assert_eq!(cleared, before, "seed {seed}: clear leveling");
    }
}

/// Same equivalence on projects without ALAP tasks, so the lazy leveler
/// (incremental forward pass, no rescheduling) is the one being checked.
#[test]
fn lazy_leveling_matches_reference() {
    for seed in 100..100 + SEEDS {
        let mut p = random_project(seed, &Shape { constraints: false, ..shape(seed) });
        assert!(p.tasks.iter().all(|t| t.constraint != projectfcg::model::ConstraintType::ALAP));
        let mut q = p.clone();
        let a = projectfcg::leveling::level(&mut p, None);
        let b = projectfcg::leveling::level_exhaustive(&mut q, None);
        let da: Vec<i64> = p.tasks.iter().map(|t| t.leveling_delay).collect();
        let db: Vec<i64> = q.tasks.iter().map(|t| t.leveling_delay).collect();
        assert_eq!(da, db, "seed {seed}: leveling delays differ");
        assert_eq!(a, b, "seed {seed}: results differ");
        // and selection-only leveling
        let rows: Vec<usize> = (0..p.tasks.len()).filter(|i| i % 3 == 0).collect();
        let mut p2 = random_project(seed, &Shape { constraints: false, ..shape(seed) });
        let mut q2 = p2.clone();
        projectfcg::leveling::level(&mut p2, Some(&rows));
        projectfcg::leveling::level_exhaustive(&mut q2, Some(&rows));
        assert_eq!(
            p2.tasks.iter().map(|t| t.leveling_delay).collect::<Vec<_>>(),
            q2.tasks.iter().map(|t| t.leveling_delay).collect::<Vec<_>>(),
            "seed {seed}: selection leveling differs"
        );
    }
}

/// Sprints, stories (some done), board dependencies with lags and inactive
/// tasks sprinkled over a random project.
fn decorate_board(p: &mut projectfcg::model::Project, seed: u64, deps: bool, inactive: bool) {
    use projectfcg::board::IssueType;
    let mut r = common::Rng::new(seed ^ 0xabcdef);
    let n = p.tasks.len();
    let leaves: Vec<usize> = (0..n).filter(|&i| !p.tasks[i].calc.is_summary).collect();
    let mut sprints = vec![];
    for &l in &leaves {
        if r.chance(12) && p.tasks[l].duration.minutes > 0 && p.set_sprint(l, true).is_ok() {
            sprints.push(p.tasks[l].uid);
        }
    }
    let epic = p.add_ticket_of("E", None, IssueType::Epic, None).unwrap();
    let mut tickets = vec![epic];
    for k in 0..15 {
        let sp = if sprints.is_empty() || r.chance(15) { None } else { Some(*r.pick(&sprints)) };
        let parent = if r.chance(40) { Some(epic) } else { None };
        let id = p.add_ticket_of(&format!("S{k}"), sp, IssueType::Story, parent).unwrap();
        if r.chance(30) {
            let done = p.board.done_column();
            p.move_ticket(id, sp, done, None).unwrap();
            let kk = p.ticket_index(id).unwrap();
            p.tickets[kk].done_at = Some(NaiveDate::from_ymd_opt(2026, 1, 5 + r.below(25) as u32).unwrap().and_hms_opt(10, 0, 0).unwrap());
        }
        tickets.push(id);
    }
    for row in 0..n {
        if deps && r.chance(20) {
            let t = *r.pick(&tickets);
            let lag = match r.below(4) {
                0 => Lag::parse("2d", &p.settings.durations).unwrap(),
                1 => Lag::parse("50%", &p.settings.durations).unwrap(),
                _ => Lag::default(),
            };
            let _ = p.add_board_dep(row, t, lag);
        }
        if inactive && r.chance(5) {
            p.tasks[row].active = false;
        }
    }
    schedule(p);
}

#[test]
fn fast_leveler_matches_reference_with_board_deps_and_inactive_tasks() {
    let mut bad = vec![];
    for seed in 0..40u64 {
        for (deps, inactive) in [(true, false), (false, true), (true, true)] {
            let shape = Shape { tasks: 40 + (seed % 5) as usize * 10, resources: 3, assigned_pct: 70, constraints: seed % 2 == 0, manual: seed % 3 == 0 };
            let mut p = random_project(seed, &shape);
            decorate_board(&mut p, seed, deps, inactive);
            // the scheduler is idempotent with these too
            let once: Vec<_> = p.tasks.iter().map(|t| (t.calc.start, t.calc.finish)).collect();
            schedule(&mut p);
            assert_eq!(once, p.tasks.iter().map(|t| (t.calc.start, t.calc.finish)).collect::<Vec<_>>(), "seed {seed}");
            // links are honoured: a task waiting on an open story starts after its sprint
            for (i, t) in p.tasks.iter().enumerate() {
                if t.calc.is_summary || t.mode == projectfcg::model::TaskMode::Manual || t.actual_start.is_some() || p.inactive_mask()[i] {
                    continue;
                }
                for d in &t.board_deps {
                    let target = p.resolve_board_dep(d.ticket).unwrap();
                    for sp in target.sprints {
                        let srow = p.index_of_uid(sp).unwrap();
                        let ignored = t.calc.board_notes.iter().any(|n| n.contains(&p.tasks[srow].name));
                        if !ignored && d.lag.is_zero() && t.constraint != projectfcg::model::ConstraintType::MSO && t.constraint != projectfcg::model::ConstraintType::MFO
                            && t.constraint != projectfcg::model::ConstraintType::SNLT && t.constraint != projectfcg::model::ConstraintType::FNLT
                        {
                            assert!(t.calc.start >= p.tasks[srow].calc.finish, "seed {seed}: task {} before sprint {}", i + 1, srow + 1);
                        }
                    }
                }
            }
            let mut q = p.clone();
            let a = projectfcg::leveling::level(&mut p, None);
            let b = projectfcg::leveling::level_exhaustive(&mut q, None);
            let da: Vec<i64> = p.tasks.iter().map(|t| t.leveling_delay).collect();
            let db: Vec<i64> = q.tasks.iter().map(|t| t.leveling_delay).collect();
            if da != db || a != b {
                bad.push((seed, deps, inactive));
            }
            // leveling never delays an inactive task
            let mask = p.inactive_mask();
            assert!(p.tasks.iter().enumerate().all(|(i, t)| !mask[i] || t.leveling_delay == 0), "seed {seed}");
        }
    }
    assert!(bad.is_empty(), "lazy != exhaustive for {bad:?}");
}
