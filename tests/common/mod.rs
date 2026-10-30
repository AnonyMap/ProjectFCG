//! Seeded random projects for invariant and performance tests.
#![allow(dead_code)]

use chrono::NaiveDate;
use projectfcg::duration::Lag;
use projectfcg::model::{ConstraintType, LinkType, Project, ResourceType, TaskType};

/// xorshift64*: deterministic, dependency-free.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in 0..n.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    pub fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u64) as usize]
    }
}

pub struct Shape {
    pub tasks: usize,
    pub resources: usize,
    /// % of leaf tasks with assignments.
    pub assigned_pct: u64,
    pub constraints: bool,
    pub manual: bool,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { tasks: 60, resources: 6, assigned_pct: 60, constraints: true, manual: true }
    }
}

/// A plausible random project: phases of 3–12 tasks (some nested one level
/// deeper), links mostly to recent tasks, a few lags/leads, constraints,
/// milestones, manual tasks, holidays, resources with their own vacations
/// and part-time max units, and assignments of mixed units/task types.
pub fn random_project(seed: u64, shape: &Shape) -> Project {
    let mut r = Rng::new(seed);
    let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
    let mut p = Project::new(d(2026, 1, 5));
    p.calendar.exceptions.push(projectfcg::calendar::CalendarException {
        name: "Holiday".into(),
        start: d(2026, 2, 16),
        end: d(2026, 2, 16),
        shifts: vec![],
    });
    // outline
    let mut i = 0;
    while i < shape.tasks {
        let row = p.append_task(&format!("Phase {i}"));
        p.tasks[row].outline_level = 1;
        i += 1;
        let n = 3 + r.below(10) as usize;
        for k in 0..n {
            if i >= shape.tasks {
                break;
            }
            let row = p.append_task(&format!("T{i}"));
            p.tasks[row].outline_level = if k > 0 && r.chance(15) { 3 } else { 2 };
            let days = if r.chance(8) { 0 } else { 1 + r.below(12) as i64 };
            p.tasks[row].duration.minutes = days * 480 + if r.chance(10) { 240 } else { 0 };
            p.tasks[row].duration.estimated = false;
            i += 1;
        }
    }
    p.normalize_outline();
    // structure is known now; refresh it so summaries are marked
    projectfcg::schedule::schedule(&mut p);
    let n = p.tasks.len();
    let kinds = [LinkType::FS, LinkType::FS, LinkType::FS, LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF];
    for s in 1..n {
        let links = r.below(3);
        for _ in 0..links {
            let back = 1 + r.below(8.min(s as u64)) as usize;
            let pr = s - back;
            let kind = *r.pick(&kinds);
            let lag = match r.below(10) {
                0 => Lag::parse("2d", &p.settings.durations).unwrap(),
                1 => Lag::parse("-1d", &p.settings.durations).unwrap(),
                2 => Lag::parse("50%", &p.settings.durations).unwrap_or_default(),
                _ => Lag::default(),
            };
            let _ = p.add_link(pr, s, kind, lag); // invalid ones are refused
        }
    }
    if shape.constraints {
        for row in 0..n {
            if p.tasks[row].calc.is_summary {
                continue;
            }
            match r.below(40) {
                0 => {
                    let dt = d(2026, 1, 5 + r.below(40) as u32 % 25).and_hms_opt(8, 0, 0).unwrap();
                    let _ = p.set_constraint(row, ConstraintType::SNET, Some(dt));
                }
                1 => {
                    let _ = p.set_constraint(row, ConstraintType::ALAP, None);
                }
                2 => {
                    let dt = d(2026, 6, 1).and_hms_opt(17, 0, 0).unwrap();
                    let _ = p.set_constraint(row, ConstraintType::FNLT, Some(dt));
                }
                _ => {}
            }
        }
    }
    if shape.manual {
        for row in 0..n {
            if !p.tasks[row].calc.is_summary && r.chance(3) {
                p.tasks[row].mode = projectfcg::model::TaskMode::Manual;
            }
        }
    }
    // resources
    let mut uids = vec![];
    for k in 0..shape.resources {
        let ri = p.add_resource(&format!("R{k}"), ResourceType::Work);
        let res = &mut p.resources[ri];
        res.max_units = *[1.0, 1.0, 1.0, 0.5, 2.0].get(r.below(5) as usize).unwrap();
        res.std_rate = 40.0 + r.below(60) as f64;
        if r.chance(30) {
            let day = 5 + r.below(20) as u32;
            res.exceptions.push(projectfcg::calendar::CalendarException {
                name: "Vacation".into(),
                start: d(2026, 2, day),
                end: d(2026, 2, day + r.below(4) as u32),
                shifts: vec![],
            });
        }
        uids.push(res.uid);
    }
    if !uids.is_empty() {
        let types = [TaskType::FixedUnits, TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork];
        for row in 0..n {
            if p.tasks[row].calc.is_summary || p.tasks[row].duration.minutes == 0 || !r.chance(shape.assigned_pct) {
                continue;
            }
            p.tasks[row].task_type = *r.pick(&types);
            let k = 1 + r.below(2);
            for _ in 0..k {
                let uid = *r.pick(&uids);
                let units = *r.pick(&[0.25, 0.5, 1.0, 1.0, 1.0]);
                let _ = p.assign(row, uid, units);
            }
        }
    }
    projectfcg::schedule::schedule(&mut p);
    p
}
