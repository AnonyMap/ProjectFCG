//! Schedule analysis used by the Task Path highlighter and Task Inspector,
//! plus baselines.
//!
//! Paths are walked at leaf level (like the scheduler), so a link to or from
//! a summary task connects every subtask beneath it.

use crate::model::{Baseline, Project};
use crate::schedule::{build_graph, Structure};
use std::collections::{BTreeSet, HashMap, VecDeque};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskPath {
    pub predecessors: BTreeSet<usize>,
    pub driving_predecessors: BTreeSet<usize>,
    pub successors: BTreeSet<usize>,
    pub driven_successors: BTreeSet<usize>,
}

fn bfs(starts: &[usize], next: impl Fn(usize) -> Vec<usize>) -> BTreeSet<usize> {
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut q: VecDeque<usize> = starts.iter().copied().collect();
    while let Some(u) = q.pop_front() {
        for v in next(u) {
            if seen.insert(v) {
                q.push_back(v);
            }
        }
    }
    for s in starts {
        seen.remove(s);
    }
    seen
}

/// Compute the task path of the given rows (Format ▸ Task Path). The
/// project must already be scheduled.
pub fn task_path(p: &Project, rows: &[usize]) -> TaskPath {
    let st = Structure::build(p);
    let g = build_graph(p, &st);
    let n = st.n;
    let starts: Vec<usize> = rows
        .iter()
        .filter(|&&r| r < n)
        .flat_map(|&r| st.leaves[r].clone())
        .collect();
    if starts.is_empty() {
        return TaskPath::default();
    }
    let pred_leaves = |t: usize| -> Vec<usize> {
        g.incoming[t].iter().flat_map(|&li| st.leaves[g.links[li].pred].clone()).collect()
    };
    let succ_leaves = |t: usize| -> Vec<usize> {
        g.outgoing[t].iter().flat_map(|&li| st.leaves[g.links[li].succ].clone()).collect()
    };
    let uid_idx: HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    let driving = |t: usize| -> Vec<usize> {
        p.tasks[t].calc.driving_preds.iter().filter_map(|u| uid_idx.get(u).copied()).collect()
    };
    let mut driven: HashMap<usize, Vec<usize>> = HashMap::new();
    for j in 0..n {
        for d in driving(j) {
            driven.entry(d).or_default().push(j);
        }
    }
    let mut tp = TaskPath {
        predecessors: bfs(&starts, pred_leaves),
        driving_predecessors: bfs(&starts, driving),
        successors: bfs(&starts, succ_leaves),
        driven_successors: bfs(&starts, |t| driven.get(&t).cloned().unwrap_or_default()),
    };
    // A selected summary's own subtasks are not part of its path.
    for s in &starts {
        tp.predecessors.remove(s);
        tp.successors.remove(s);
    }
    tp
}

/// Critical leaf tasks in schedule order.
pub fn critical_tasks(p: &Project) -> Vec<usize> {
    let mut v: Vec<usize> = (0..p.tasks.len())
        .filter(|&i| !p.tasks[i].calc.is_summary && p.tasks[i].calc.critical)
        .collect();
    v.sort_by_key(|&i| (p.tasks[i].calc.start, i));
    v
}

impl Project {
    /// Project ▸ Set Baseline (Baseline 0). See [`Project::set_baseline_n`].
    pub fn set_baseline(&mut self, rows: Option<&[usize]>) {
        self.set_baseline_n(0, rows);
    }

    /// Project ▸ Set Baseline ▸ Baseline `n` (0–10). `rows = None` baselines
    /// the entire project; otherwise the given tasks (and their subtasks),
    /// rolled up into their summary tasks (Project's "roll up baselines to
    /// all summary tasks"). Saves dates, duration, work and cost.
    pub fn set_baseline_n(&mut self, n: usize, rows: Option<&[usize]>) {
        if n > 10 {
            return;
        }
        let cal = self.calendar.clone();
        let take = |t: &crate::model::Task| -> Option<Baseline> {
            let (s, f) = (t.calc.start?, t.calc.finish?);
            let duration = if t.calc.is_summary { t.calc.summary_duration } else { t.duration.minutes };
            Some(Baseline { start: s, finish: f, duration, work: t.calc.work, cost: t.calc.cost })
        };
        match rows {
            None => {
                for t in self.tasks.iter_mut() {
                    *t.baseline_slot_mut(n) = take(t);
                }
            }
            Some(rows) => {
                for &r in rows {
                    if r < self.tasks.len() {
                        let end = self.subtree_end(r);
                        for i in r..end {
                            let b = take(&self.tasks[i]);
                            *self.tasks[i].baseline_slot_mut(n) = b;
                        }
                        for a in self.ancestors_of(r) {
                            // roll up: span of the subtasks' baselines, sums of work and cost
                            let end = self.subtree_end(a);
                            let kids: Vec<Baseline> = (a + 1..end)
                                .filter(|&j| !self.tasks[j].calc.is_summary)
                                .filter_map(|j| self.tasks[j].baseline_slot(n))
                                .collect();
                            if let (Some(s), Some(f)) = (kids.iter().map(|b| b.start).min(), kids.iter().map(|b| b.finish).max()) {
                                let work = kids.iter().map(|b| b.work).sum();
                                let cost = kids.iter().map(|b| b.cost).sum::<f64>() + self.tasks[a].fixed_cost;
                                *self.tasks[a].baseline_slot_mut(n) = Some(Baseline { start: s, finish: f, duration: cal.work_between(s, f), work, cost });
                            }
                        }
                    }
                }
            }
        }
        self.baseline_saved[n] = Some(chrono::Local::now().naive_local());
    }

    pub fn clear_baseline(&mut self, rows: Option<&[usize]>) {
        self.clear_baseline_n(0, rows);
    }

    pub fn clear_baseline_n(&mut self, n: usize, rows: Option<&[usize]>) {
        if n > 10 {
            return;
        }
        match rows {
            None => {
                self.tasks.iter_mut().for_each(|t| *t.baseline_slot_mut(n) = None);
                self.baseline_saved[n] = None;
            }
            Some(rows) => {
                for &r in rows {
                    if r < self.tasks.len() {
                        *self.tasks[r].baseline_slot_mut(n) = None;
                    }
                }
            }
        }
    }

    /// Does any task have Baseline `n`?
    pub fn has_baseline_n(&self, n: usize) -> bool {
        self.tasks.iter().any(|t| t.baseline_slot(n).is_some())
    }

    pub fn has_baseline(&self) -> bool {
        self.tasks.iter().any(|t| t.baseline.is_some())
    }
}
