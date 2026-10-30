//! Resource leveling (Resource ▸ Level All / Level Selection / Clear
//! Leveling).
//!
//! A serial leveling heuristic, as Project uses: tasks are taken in the
//! leveling order and each is placed at the earliest time at or after its
//! current start where every resource it uses has spare capacity for its
//! assignments. The only start times worth trying are the current start and
//! the ends of other bookings on the same resources. The placement is
//! stored as a Leveling Delay (working time) so the schedule stays
//! explainable and Clear Leveling restores it. Passes repeat until no
//! leveled resource is overallocated (or a small pass limit is reached).
//!
//! Never moved: summary, manually scheduled, Must Start/Finish On, and
//! priority-1000 ("do not level") tasks.

use crate::model::{ConstraintType, LevelingOrder, Project, ResourceType, TaskMode};
use crate::schedule::schedule;
use chrono::NaiveDateTime;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelResult {
    /// Rows that received a leveling delay.
    pub delayed: Vec<usize>,
    /// Problems leveling couldn't resolve (user-facing).
    pub unresolved: Vec<String>,
}

pub const DO_NOT_LEVEL: u32 = 1000;

fn levelable(p: &Project, i: usize, inactive: &[bool]) -> bool {
    let t = &p.tasks[i];
    !inactive[i]
        && !t.calc.is_summary
        && t.mode == TaskMode::Auto
        && !matches!(t.constraint, ConstraintType::MSO | ConstraintType::MFO)
        && t.priority < DO_NOT_LEVEL
        && t.actual_start.is_none()
        && !p.work_assignments(i).is_empty()
}

/// Remove leveling delays (all tasks, or the given rows).
pub fn clear_leveling(p: &mut Project, rows: Option<&[usize]>) {
    match rows {
        None => p.tasks.iter_mut().for_each(|t| t.leveling_delay = 0),
        Some(rows) => {
            for &r in rows {
                if r < p.tasks.len() {
                    p.tasks[r].leveling_delay = 0;
                }
            }
        }
    }
    schedule(p);
}

/// One booking of a resource: [start, finish) at `units`.
type Booking = (NaiveDateTime, NaiveDateTime, f64);
/// Bookings per resource uid, each tagged with the task row that made it.
type Book = HashMap<u32, Vec<(usize, Booking)>>;

/// Peak total units of `bookings` overlapping [a, b).
fn peak_load(bookings: &[(usize, Booking)], a: NaiveDateTime, b: NaiveDateTime) -> f64 {
    let mut ev: Vec<(NaiveDateTime, f64)> = vec![];
    for &(_, (s, f, u)) in bookings {
        if s < b && a < f {
            ev.push((s.max(a), u));
            ev.push((f.min(b), -u));
        }
    }
    ev.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.partial_cmp(&y.1).unwrap()));
    let mut cur = 0.0f64;
    let mut peak = 0.0f64;
    for (_, d) in ev {
        cur += d;
        peak = peak.max(cur);
    }
    peak
}

/// What may move when tasks are delayed: link successors (transitively),
/// summaries rolling up a moved child, the children of a summary that is
/// itself moved by a link, and every ALAP task (they hang off the project
/// finish). Used to skip reschedules that cannot change anything.
struct Influence {
    succ: Vec<Vec<usize>>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    alap: Vec<usize>,
}

impl Influence {
    fn build(p: &Project, idx: &HashMap<u32, usize>, g: &crate::schedule::Graph) -> Influence {
        let n = p.tasks.len();
        let mut succ = vec![vec![]; n];
        for (s, t) in p.tasks.iter().enumerate() {
            for l in &t.predecessors {
                if let Some(&pi) = idx.get(&l.pred) {
                    succ[pi].push(s);
                }
            }
        }
        // waiting on a board item = following its sprint: exactly the board
        // links the scheduler kept (loops it refused aren't edges)
        for e in g.links.iter().filter(|e| e.board.is_some()) {
            succ[e.pred].push(e.succ);
        }
        let parent: Vec<Option<usize>> = (0..n).map(|i| p.parent_of(i)).collect();
        let mut children = vec![vec![]; n];
        for (i, par) in parent.iter().enumerate() {
            if let Some(pp) = par {
                children[*pp].push(i);
            }
        }
        let alap = (0..n).filter(|&i| p.tasks[i].constraint == ConstraintType::ALAP).collect();
        Influence { succ, parent, children, alap }
    }

    /// Lazy mode: mark every leaf downstream of `from` stale. Stops at
    /// leaves already stale (their downstream is stale too, since a leaf
    /// is only refreshed after everything upstream of it). `seen`/`gen`
    /// keep the walk linear without clearing arrays.
    fn mark_lazy(&self, from: usize, stale: &mut [bool], seen: &mut [(u32, u32)], gen: u32, out: &mut Vec<usize>) {
        out.clear();
        // (row, true = dates move / false = summary only rolls up), as in `spread`
        let mut queue: Vec<(usize, bool)> = self.succ[from].iter().map(|&s| (s, true)).collect();
        if let Some(pp) = self.parent[from] {
            queue.push((pp, false));
        }
        while let Some((i, is_moved)) = queue.pop() {
            // seen[i] = (generation reached as moved, generation reached as rolled)
            if is_moved {
                if seen[i].0 == gen {
                    continue;
                }
                seen[i].0 = gen;
            } else {
                if seen[i].1 == gen || seen[i].0 == gen {
                    continue;
                }
                seen[i].1 = gen;
            }
            let leaf = self.children[i].is_empty();
            if is_moved {
                if leaf {
                    if stale[i] {
                        continue; // already stale, and so is all of its downstream
                    }
                    stale[i] = true;
                }
                out.push(i);
                for &c in &self.children[i] {
                    queue.push((c, true));
                }
            }
            for &s in &self.succ[i] {
                queue.push((s, true));
            }
            if let Some(pp) = self.parent[i] {
                queue.push((pp, false));
            }
        }
    }

    /// Mark everything a delay of `from` can move; returns the newly marked
    /// rows. `from` itself is not marked: it was just placed exactly, and a
    /// later delay that pushes it again must still be noticed.
    fn spread(&self, from: usize, moved: &mut [bool], rolled: &mut [bool], out: &mut Vec<usize>) {
        // (row, true = dates moved / false = only rolled up from a child)
        let mut queue: Vec<(usize, bool)> = self.succ[from].iter().map(|&s| (s, true)).collect();
        for &c in &self.children[from] {
            queue.push((c, true));
        }
        if let Some(pp) = self.parent[from] {
            queue.push((pp, false));
        }
        for &a in &self.alap {
            queue.push((a, true));
        }
        while let Some((i, is_moved)) = queue.pop() {
            if is_moved {
                if moved[i] {
                    continue;
                }
                moved[i] = true;
                out.push(i);
                for &c in &self.children[i] {
                    queue.push((c, true));
                }
            } else {
                if rolled[i] || moved[i] {
                    continue;
                }
                rolled[i] = true;
                out.push(i);
            }
            for &s in &self.succ[i] {
                queue.push((s, true));
            }
            if let Some(pp) = self.parent[i] {
                queue.push((pp, false));
            }
        }
    }
}

/// Candidates in leveling order, but never before anything that can move
/// them (a serial schedule-generation scheme): among the tasks whose
/// predecessors — including SF/SS-lead ones that start later than they do —
/// are all placed, take the best by the leveling order. Then delaying a task
/// can never push a task that was already placed into a conflict.
fn precedence_order(p: &Project, infl: &Influence, is_cand: &[bool]) -> Vec<usize> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let n = p.tasks.len();
    // nodes: 2i = task i's dates move, 2i+1 = summary i rolls up a child
    let mut out: Vec<Vec<usize>> = vec![vec![]; 2 * n];
    for i in 0..n {
        for &s in &infl.succ[i] {
            out[2 * i].push(2 * s);
            out[2 * i + 1].push(2 * s);
        }
        for &c in &infl.children[i] {
            out[2 * i].push(2 * c);
        }
        if let Some(pp) = infl.parent[i] {
            out[2 * i].push(2 * pp + 1);
            out[2 * i + 1].push(2 * pp + 1);
        }
    }
    let mut indeg = vec![0usize; 2 * n];
    for v in &out {
        for &w in v {
            indeg[w] += 1;
        }
    }
    // (group, start, slack, -priority, row): lower pops first; non-candidates
    // (group i64::MIN) are released as soon as they're ready
    type Key = (i64, Option<NaiveDateTime>, i64, i64, usize);
    let key = |node: usize| -> Key {
        let i = node / 2;
        if node % 2 == 1 || !is_cand[i] {
            return (i64::MIN, None, 0, 0, node);
        }
        let t = &p.tasks[i];
        let prio = -(t.priority as i64);
        match p.settings.leveling_order {
            LevelingOrder::IdOnly => (0, None, 0, 0, i),
            LevelingOrder::Standard => (0, t.calc.start, t.calc.total_slack, prio, i),
            LevelingOrder::PriorityStandard => (prio, t.calc.start, t.calc.total_slack, prio, i),
        }
    };
    let mut heap: BinaryHeap<Reverse<(Key, usize)>> = (0..2 * n).filter(|&v| indeg[v] == 0).map(|v| Reverse((key(v), v))).collect();
    let mut order = Vec::with_capacity(n);
    let mut seen = 0;
    while let Some(Reverse((_, v))) = heap.pop() {
        seen += 1;
        if v % 2 == 0 && is_cand[v / 2] {
            order.push(v / 2);
        }
        for &w in &out[v] {
            indeg[w] -= 1;
            if indeg[w] == 0 {
                heap.push(Reverse((key(w), w)));
            }
        }
    }
    if seen < 2 * n {
        // a cycle (the scheduler reports circular links separately): fall
        // back to plain priority order for whatever is left
        let mut rest: Vec<usize> = (0..n).filter(|&i| is_cand[i] && !order.contains(&i)).collect();
        rest.sort_by_key(|&i| key(2 * i));
        order.extend(rest);
    }
    order
}

/// Bookings of every task in `rows`, from the scheduled assignment dates.
fn bookings_from_calc(p: &Project, rows: impl Iterator<Item = usize>, idx: &HashMap<u32, usize>) -> Book {
    let inactive = p.inactive_mask();
    let want: HashSet<usize> = rows.filter(|&r| !inactive[r]).collect();
    let mut book: Book = HashMap::new();
    for (ai, a) in p.assignments.iter().enumerate() {
        let Some(&ti) = idx.get(&a.task) else { continue };
        if !want.contains(&ti) || a.units <= 1e-9 {
            continue;
        }
        if let Some(c) = p.calc.assignments.get(ai) {
            if let (Some(s), Some(f)) = (c.start, c.finish) {
                if f > s {
                    book.entry(a.resource).or_default().push((ti, (s, f, a.units)));
                }
            }
        }
    }
    book
}

/// Replace row `r`'s bookings with its current assignment dates (after the
/// lazy leveler brought its dates up to date).
fn rebook(p: &Project, r: usize, ridx: &HashMap<u32, usize>, book: &mut Book) {
    let uid = p.tasks[r].uid;
    for (ai, a) in p.assignments.iter().enumerate() {
        if a.task != uid || a.units <= 1e-9 {
            continue;
        }
        if let Some(v) = book.get_mut(&a.resource) {
            v.retain(|x| x.0 != r);
        }
        let Some(&ri) = ridx.get(&a.resource) else { continue };
        if let Some((s, f)) = crate::resources::assignment_dates(p, &p.calc.resource_calendars, ai, r, ri) {
            if f > s {
                book.entry(a.resource).or_default().push((r, (s, f, a.units)));
            }
        }
    }
}

/// Bring leaf `t`'s forward-pass dates up to date, first refreshing any
/// stale leaves it depends on (iteratively: chains can be thousands long).
fn refresh(prep: &crate::schedule::Prepared, p: &mut Project, fw: &mut crate::schedule::Forward, stale: &mut [bool], t: usize) {
    if !stale[t] {
        return;
    }
    let mut stack: Vec<(usize, bool)> = vec![(t, false)];
    while let Some((x, ready)) = stack.pop() {
        if !stale[x] {
            continue;
        }
        if ready {
            prep.forward_node(p, fw, x);
            stale[x] = false;
            let c = &mut p.tasks[x].calc;
            c.start = fw.es[x];
            c.finish = fw.ef[x];
            c.early_start = fw.es[x];
            c.early_finish = fw.ef[x];
            c.unleveled_start = fw.unleveled[x];
            continue;
        }
        stack.push((x, true));
        for d in prep.deps(x) {
            if stale[d] {
                stack.push((d, false));
            }
        }
    }
}

/// Level the project (rows = None) or only the given rows.
pub fn level(p: &mut Project, rows: Option<&[usize]>) -> LevelResult {
    level_impl(p, rows, false)
}

/// Reference version that reschedules after every delay instead of only
/// when something it reads may have moved. Same results, much slower; the
/// tests check the two agree.
#[doc(hidden)]
pub fn level_exhaustive(p: &mut Project, rows: Option<&[usize]>) -> LevelResult {
    level_impl(p, rows, true)
}

fn level_impl(p: &mut Project, rows: Option<&[usize]>, always_fresh: bool) -> LevelResult {
    clear_leveling(p, rows);
    let selected: Option<HashSet<u32>> = rows.map(|r| r.iter().filter_map(|&i| p.tasks.get(i).map(|t| t.uid)).collect());
    let mut result = LevelResult::default();
    let mut delayed: HashSet<u32> = HashSet::new();

    for _pass in 0..6 {
        schedule(p);
        // structure, links and spans don't change while leveling, only delays
        let prep = crate::schedule::prepare(p);
        let n = p.tasks.len();
        // rows don't move during leveling, only dates
        let idx: HashMap<u32, usize> = p.tasks.iter().enumerate().map(|(i, x)| (x.uid, i)).collect();
        let infl = Influence::build(p, &idx, &prep.g);
        let inactive = p.inactive_mask();
        let cands: Vec<usize> = (0..n)
            .filter(|&i| levelable(p, i, &inactive))
            .filter(|&i| selected.as_ref().is_none_or(|s| s.contains(&p.tasks[i].uid)))
            .collect();
        let mut is_cand = vec![false; n];
        for &c in &cands {
            is_cand[c] = true;
        }
        let order = precedence_order(p, &infl, &is_cand);
        // tasks whose bookings count: everything except candidates not yet placed
        let mut processed = vec![false; n];
        let booked_rows = |processed: &[bool], is_cand: &[bool]| -> Vec<usize> { (0..n).filter(|&i| !is_cand[i] || processed[i]).collect() };
        let mut book = bookings_from_calc(p, booked_rows(&processed, &is_cand).into_iter(), &idx);
        // Lazy mode: candidates come in dependency order, so a delay can only
        // affect tasks not placed yet; instead of rescheduling, recompute the
        // forward pass just for the stale tasks the next candidate reads.
        let lazy = !always_fresh && !p.settings.level_within_slack && prep.forward_only_ok();
        let ridx: HashMap<u32, usize> = p.resources.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
        let mut fw = if lazy { Some(prep.forward_all(p)) } else { None };
        let mut lstale = vec![false; n];
        let mut seen = vec![(0u32, 0u32); n];
        let mut gen = 0u32;
        let mut rebook_rows: Vec<usize> = vec![];
        let has_asg: Vec<bool> = {
            let mut v = vec![false; n];
            for a in &p.assignments {
                if let Some(&i) = idx.get(&a.task) {
                    v[i] = true;
                }
            }
            v
        };
        let mut moved = vec![false; n];
        let mut rolled = vec![false; n];
        let mut stale = false; // a booked task may have moved since `book` was built
        let mut dirty = false; // some delay hasn't been scheduled yet
        let mut changed = false;
        let mut touched: Vec<usize> = vec![];

        for t in order {
            if let Some(fw) = fw.as_mut() {
                refresh(&prep, p, fw, &mut lstale, t);
                for r in std::mem::take(&mut rebook_rows) {
                    refresh(&prep, p, fw, &mut lstale, r);
                    if !inactive[r] {
                        rebook(p, r, &ridx, &mut book);
                    }
                }
            }
            let needs_fresh = !lazy && dirty && (always_fresh || p.settings.level_within_slack || moved[t] || rolled[t] || stale);
            if needs_fresh {
                if always_fresh || p.settings.level_within_slack {
                    schedule(p); // slack is needed (and the reference recomputes everything)
                } else {
                    crate::schedule::reschedule_dates(p, &prep);
                }
                book = bookings_from_calc(p, booked_rows(&processed, &is_cand).into_iter(), &idx);
                moved.iter_mut().for_each(|m| *m = false);
                rolled.iter_mut().for_each(|m| *m = false);
                stale = false;
                dirty = false;
            }
            let my: Vec<(usize, u32, f64, i64)> = p
                .work_assignments(t)
                .into_iter()
                .filter(|&ai| p.assignments[ai].units > 1e-9)
                .filter_map(|ai| {
                    let a = &p.assignments[ai];
                    let ri = p.resource_index(a.resource)?;
                    (p.resources[ri].rtype == ResourceType::Work).then(|| (ri, a.resource, a.units, p.assignment_span(ai)))
                })
                .collect();
            processed[t] = true;
            let Some(s0) = p.tasks[t].calc.start else { continue };
            // impossible: assignment needs more than the resource has
            if let Some(&(ri, _, u, _)) = my.iter().find(|&&(ri, _, u, _)| u > p.resources[ri].max_units + 1e-6) {
                result.unresolved.push(format!(
                    "{} is assigned at {} to \"{}\" but has a maximum of {}.",
                    p.resources[ri].name,
                    crate::resources::fmt_units(u),
                    p.tasks[t].name,
                    crate::resources::fmt_units(p.resources[ri].max_units)
                ));
                place(p, &my, s0, &mut book, t);
                continue;
            }
            let fits = |c: NaiveDateTime| -> bool {
                my.iter().all(|&(ri, ru, u, span)| {
                    let cal = &p.calc.resource_calendars[ri];
                    let a = cal.next_working_start(c);
                    let b = cal.add_work(a, span);
                    let load = book.get(&ru).map(|bk| peak_load(bk, a, b)).unwrap_or(0.0);
                    load + u <= p.resources[ri].max_units + 1e-6
                })
            };
            let mut tries: Vec<NaiveDateTime> = vec![s0];
            for &(_, ru, _, _) in &my {
                if let Some(bk) = book.get(&ru) {
                    tries.extend(bk.iter().map(|b| (b.1).1).filter(|&f| f > s0));
                }
            }
            tries.sort();
            tries.dedup();
            let slot = tries.iter().copied().find(|&c| fits(c));
            let mut at = s0;
            if let Some(slot) = slot.filter(|&s| s > s0) {
                // the total delay is measured from the unleveled early start
                // (as the scheduler applies it), not added to the current
                // start: snapping to a resource's working time can swallow
                // an increment, and then the task wouldn't move at all
                let base = p.tasks[t].calc.unleveled_start.unwrap_or(s0);
                let total = p.calendar.work_between(base, slot);
                let add = total - p.tasks[t].leveling_delay;
                if add > 0 {
                    if p.settings.level_within_slack && add > p.tasks[t].calc.total_slack {
                        result.unresolved.push(format!("\"{}\" would need a delay beyond its available slack.", p.tasks[t].name));
                    } else {
                        p.tasks[t].leveling_delay = total;
                        delayed.insert(p.tasks[t].uid);
                        changed = true;
                        dirty = true;
                        at = slot;
                        if lazy {
                            // t and everything downstream must be recomputed
                            // before anyone reads them; booked ones re-booked
                            gen += 1;
                            lstale[t] = true;
                            infl.mark_lazy(t, &mut lstale, &mut seen, gen, &mut touched);
                            for &r in &touched {
                                if (!is_cand[r] || processed[r]) && has_asg[r] {
                                    rebook_rows.push(r);
                                }
                            }
                            if matches!(p.tasks[t].constraint, ConstraintType::SNLT | ConstraintType::FNLT) {
                                rebook_rows.push(t);
                            }
                        }
                        // what this delay can move
                        touched.clear();
                        infl.spread(t, &mut moved, &mut rolled, &mut touched);
                        if touched.iter().any(|&r| r != t && (!is_cand[r] || processed[r]) && !p.assignments_of_task(r).is_empty()) {
                            stale = true;
                        }
                        // the scheduler may not move these by the full delay
                        // (ALAP sits on its late dates; No Later Than caps it),
                        // so the booking recorded below would be a guess
                        if matches!(p.tasks[t].constraint, ConstraintType::ALAP | ConstraintType::SNLT | ConstraintType::FNLT) {
                            stale = true;
                        }
                    }
                }
            }
            place(p, &my, at, &mut book, t);
        }
        if !changed {
            break;
        }
    }
    schedule(p);
    result.delayed = p
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| delayed.contains(&t.uid) && t.leveling_delay > 0)
        .map(|(i, _)| i)
        .collect();
    result.unresolved.sort();
    result.unresolved.dedup();
    // anything still overallocated among the resources we were allowed to level
    for (ri, r) in p.resources.iter().enumerate() {
        if p.calc.resources.get(ri).is_some_and(|c| c.overallocated) {
            let msg = format!("{} is still overallocated.", r.name);
            if !result.unresolved.contains(&msg) {
                result.unresolved.push(msg);
            }
        }
    }
    result
}

/// Record a placed task's bookings (as the scheduler will place them).
fn place(p: &Project, my: &[(usize, u32, f64, i64)], at: NaiveDateTime, book: &mut Book, row: usize) {
    for &(ri, ru, u, span) in my {
        let cal = &p.calc.resource_calendars[ri];
        let a = cal.next_working_start(at);
        let b = cal.add_work(a, span);
        if b > a {
            book.entry(ru).or_default().push((row, (a, b, u)));
        }
    }
}

/// Next task after `from` (row order, wrapping) with an overallocated
/// resource (Resource ▸ Next Overallocation).
pub fn next_overallocation(p: &Project, from: Option<usize>) -> Option<usize> {
    let n = p.tasks.len();
    let start = from.map(|f| f + 1).unwrap_or(0);
    (0..n)
        .map(|k| (start + k) % n)
        .find(|&i| !p.tasks[i].calc.is_summary && p.tasks[i].calc.overallocated)
}
