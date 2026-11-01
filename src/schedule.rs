//! Critical-path scheduler.
//!
//! Model (matching Microsoft Project's semantics):
//! * Only *leaf* tasks are scheduled; summary tasks roll up the earliest start
//!   and latest finish of their descendants.
//! * A link whose predecessor or successor is a summary task applies to every
//!   leaf beneath it. That is how Project lets you link phases, and it means a
//!   leaf's start is driven by its own links *and* its ancestors' links.
//! * Dependencies are expanded to a leaf-level DAG which is topologically
//!   sorted; forward pass gives early dates, backward pass late dates.
//! * Constraints: ASAP/ALAP/SNET/SNLT/FNET/FNLT/MSO/MFO. With "honor
//!   constraint dates" on (Project's default), inflexible constraints win
//!   over links and the conflict is reported; slack goes negative.
//! * Manually scheduled tasks keep their user-entered dates but still drive
//!   their successors.
//! * Lags are measured in working time on the project calendar (or wall-clock
//!   time for elapsed lags); percent lags are a percentage of the
//!   predecessor's duration.

use crate::calendar::Calendar;
use crate::duration::Lag;
use crate::model::{ConstraintType, Link, LinkType, Project, StartDriver, TaskMode, TaskStatus};
use chrono::{Duration as ChronoDur, NaiveDateTime};
use std::collections::VecDeque;

/// Outline structure derived from outline levels.
pub struct Structure {
    pub n: usize,
    pub is_summary: Vec<bool>,
    pub subtree_end: Vec<usize>,
    pub parent: Vec<Option<usize>>,
    /// Leaf descendants (a leaf's list is itself). A summary's list leaves
    /// out inactive subtasks (unless they all are), so they don't roll up.
    pub leaves: Vec<Vec<usize>>,
    /// Inactive, itself or through a summary.
    pub inactive: Vec<bool>,
}

impl Structure {
    pub fn build(p: &Project) -> Self {
        let n = p.tasks.len();
        let mut is_summary = vec![false; n];
        let mut subtree_end = vec![0; n];
        let mut parent = vec![None; n];
        let mut stack: Vec<usize> = vec![];
        for i in 0..n {
            let lvl = p.tasks[i].outline_level;
            while let Some(&top) = stack.last() {
                if p.tasks[top].outline_level >= lvl {
                    stack.pop();
                } else {
                    break;
                }
            }
            parent[i] = stack.last().copied();
            if let Some(par) = parent[i] {
                is_summary[par] = true;
            }
            stack.push(i);
        }
        for i in 0..n {
            subtree_end[i] = p.subtree_end(i);
        }
        let inactive = p.inactive_mask();
        let leaves = (0..n)
            .map(|i| {
                if is_summary[i] {
                    let all: Vec<usize> = (i + 1..subtree_end[i]).filter(|&j| !is_summary[j]).collect();
                    let active: Vec<usize> = all.iter().copied().filter(|&j| !inactive[j]).collect();
                    if active.is_empty() {
                        all
                    } else {
                        active
                    }
                } else {
                    vec![i]
                }
            })
            .collect();
        Structure {
            n,
            is_summary,
            subtree_end,
            parent,
            leaves,
            inactive,
        }
    }

    /// Links from this row have no effect: it's inactive, or a summary
    /// whose subtasks all are.
    pub fn drives_nothing(&self, i: usize) -> bool {
        self.inactive[i] || (self.is_summary[i] && self.leaves[i].iter().all(|&l| self.inactive[l]))
    }

    pub fn is_ancestor(&self, a: usize, d: usize) -> bool {
        a < d && d < self.subtree_end[a]
    }

    pub fn ancestors(&self, i: usize) -> Vec<usize> {
        let mut v = vec![];
        let mut c = self.parent[i];
        while let Some(p) = c {
            v.push(p);
            c = self.parent[p];
        }
        v
    }
}

/// A validated dependency between task indices.
#[derive(Clone, Copy, Debug)]
pub struct EdgeLink {
    pub pred: usize,
    pub succ: usize,
    pub link: Link,
    /// Set for a link made from a board dependency: the ticket waited on
    /// (`pred` is the sprint it's planned in).
    pub board: Option<u32>,
}

/// Validate one link; returns an error message if Project would refuse it.
pub fn validate_link(st: &Structure, pred: usize, succ: usize, kind: LinkType) -> Result<(), String> {
    if pred == succ {
        return Err("A task cannot be linked to itself.".into());
    }
    if st.is_ancestor(pred, succ) || st.is_ancestor(succ, pred) {
        return Err("You cannot link a summary task to one of its subtasks.".into());
    }
    if st.is_summary[succ] && matches!(kind, LinkType::FF | LinkType::SF) {
        return Err(
            "Summary tasks cannot have a finish-to-finish or start-to-finish predecessor.".into(),
        );
    }
    Ok(())
}

pub struct Graph {
    pub links: Vec<EdgeLink>,
    /// Per leaf: links that constrain it (own + ancestors' predecessors).
    pub incoming: Vec<Vec<usize>>,
    /// Per leaf: links it drives (own + ancestors' successors).
    pub outgoing: Vec<Vec<usize>>,
    pub order: Vec<usize>,
    /// Leaves that participate in a cycle (empty if acyclic).
    pub cyclic: Vec<usize>,
    pub errors: Vec<String>,
    /// Per leaf: earliest starts from finished board items it waits on
    /// (done time, lag, the duration a % lag is of, ticket).
    pub floors: Vec<Vec<(NaiveDateTime, Lag, i64, u32)>>,
    /// Per row: unfinished board items it waits on, and problems with them.
    pub board_waiting: Vec<Vec<u32>>,
    pub board_notes: Vec<Vec<String>>,
}

pub fn build_graph(p: &Project, st: &Structure) -> Graph {
    let n = st.n;
    let mut errors = vec![];
    let mut links = vec![];
    let uid_to_idx: std::collections::HashMap<u32, usize> =
        p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    for (succ, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            let Some(&pred) = uid_to_idx.get(&l.pred) else {
                continue;
            };
            // an inactive task doesn't move anything (nor does a summary
            // whose subtasks all are)
            if st.drives_nothing(pred) {
                continue;
            }
            match validate_link(st, pred, succ, l.kind) {
                Ok(()) => links.push(EdgeLink {
                    pred,
                    succ,
                    link: *l,
                    board: None,
                }),
                Err(e) => errors.push(format!("Task {}: {}", succ + 1, e)),
            }
        }
    }
    // ---- board dependencies: an unfinished story makes the task follow
    // its sprint (a finish-to-start link from the sprint task); a finished
    // one is an earliest start at the time it was done ----
    let mut floors: Vec<Vec<(NaiveDateTime, Lag, i64, u32)>> = vec![vec![]; n];
    let mut board_waiting: Vec<Vec<u32>> = vec![vec![]; n];
    let mut board_notes: Vec<Vec<String>> = vec![vec![]; n];
    // what each task waits on: its own board dependencies, and for a
    // release's milestone, every story in the release
    struct Job {
        succ: usize,
        label: String,
        target: crate::board::DepTarget,
        lag: Lag,
        /// The item named on the links (None: the first unfinished one per sprint).
        ticket: Option<u32>,
    }
    let mut jobs: Vec<Job> = vec![];
    for (succ, t) in p.tasks.iter().enumerate() {
        for d in &t.board_deps {
            if let Some(target) = p.resolve_board_dep(d.ticket) {
                jobs.push(Job { succ, label: p.ticket_key(d.ticket), target, lag: d.lag, ticket: Some(d.ticket) });
            }
        }
    }
    for r in &p.board.releases {
        if let Some(&succ) = r.task.and_then(|u| uid_to_idx.get(&u)) {
            jobs.push(Job { succ, label: format!("release {}", r.name), target: p.resolve_release(r.id), lag: Lag::default(), ticket: None });
        }
    }
    if !jobs.is_empty() {
        // leaf adjacency of the links so far, to refuse loops
        let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
        let add_adj = |adj: &mut Vec<Vec<usize>>, e: &EdgeLink| {
            for &s in &st.leaves[e.pred] {
                for &t in &st.leaves[e.succ] {
                    adj[s].push(t);
                }
            }
        };
        for e in &links {
            add_adj(&mut adj, e);
        }
        let reaches = |adj: &Vec<Vec<usize>>, from: &[usize], to: &[usize]| -> bool {
            let mut seen = vec![false; n];
            let mut stack: Vec<usize> = from.to_vec();
            while let Some(x) = stack.pop() {
                if to.contains(&x) {
                    return true;
                }
                if !std::mem::replace(&mut seen[x], true) {
                    stack.extend(adj[x].iter().copied());
                }
            }
            false
        };
        for job in &jobs {
            let succ = job.succ;
            let key = &job.label;
            let target = &job.target;
            board_waiting[succ].extend(target.waiting.iter().copied());
            for &u in &target.unplanned {
                board_notes[succ].push(format!("Waits on {}, which isn't planned into a sprint yet, so it can't be scheduled against.", p.ticket_key(u)));
            }
            for &(done, in_sprint, item) in &target.done {
                // a % lag is a share of the story's sprint
                let sprint_row = in_sprint.and_then(|u| uid_to_idx.get(&u).copied());
                if let Some(sr) = sprint_row {
                    // the sprint itself (or its summary) waiting on its own story
                    if sr == succ || st.is_ancestor(sr, succ) || st.is_ancestor(succ, sr) {
                        board_notes[succ].push(format!("Waits on {key}, which is in this task's own sprint; ignored."));
                        continue;
                    }
                }
                let pdur = sprint_row.map(|r| p.tasks[r].duration.minutes).unwrap_or(0);
                for &l in &st.leaves[succ] {
                    floors[l].push((done, job.lag, pdur, job.ticket.unwrap_or(item)));
                }
            }
            for &sp in &target.sprints {
                let Some(&pred) = uid_to_idx.get(&sp) else { continue };
                if st.drives_nothing(pred) {
                    board_notes[succ].push(format!("Waits on {key} in {}, which is inactive; ignored.", p.tasks[pred].name));
                    continue;
                }
                let item = job.ticket.or_else(|| target.by_sprint.iter().find(|x| x.0 == sp).map(|x| x.1));
                let e = EdgeLink { pred, succ, link: Link { pred: sp, kind: LinkType::FS, lag: job.lag }, board: item };
                let sprint_name = &p.tasks[pred].name;
                if validate_link(st, pred, succ, LinkType::FS).is_err() {
                    board_notes[succ].push(format!("Waits on {key} in {sprint_name}, which contains this task or is this task; ignored."));
                    continue;
                }
                if reaches(&adj, &st.leaves[succ], &st.leaves[pred]) {
                    board_notes[succ].push(format!("Waits on {key} in {sprint_name}, but {sprint_name} already waits on this task. That loop is ignored: move the story to another sprint or remove one of the links."));
                    continue;
                }
                add_adj(&mut adj, &e);
                links.push(e);
            }
        }
        for w in board_waiting.iter_mut() {
            w.sort();
            w.dedup();
        }
    }
    let mut incoming = vec![vec![]; n];
    let mut outgoing = vec![vec![]; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut indeg = vec![0usize; n];
    for (li, e) in links.iter().enumerate() {
        for &t in &st.leaves[e.succ] {
            incoming[t].push(li);
        }
        for &s in &st.leaves[e.pred] {
            outgoing[s].push(li);
            for &t in &st.leaves[e.succ] {
                adj[s].push(t);
                indeg[t] += 1;
            }
        }
    }
    // Kahn's algorithm, stable by row index.
    let mut q: VecDeque<usize> = (0..n).filter(|&i| !st.is_summary[i] && indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    let mut placed = vec![false; n];
    while let Some(u) = q.pop_front() {
        order.push(u);
        placed[u] = true;
        for &v in &adj[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                q.push_back(v);
            }
        }
    }
    let cyclic: Vec<usize> = (0..n).filter(|&i| !st.is_summary[i] && !placed[i]).collect();
    if !cyclic.is_empty() {
        let ids: Vec<String> = cyclic.iter().map(|i| (i + 1).to_string()).collect();
        errors.push(format!(
            "Circular dependency between tasks {}. These links are ignored until the loop is removed.",
            ids.join(", ")
        ));
        order.extend(cyclic.iter().copied());
    }
    Graph {
        links,
        incoming,
        outgoing,
        order,
        cyclic,
        errors,
        floors,
        board_waiting,
        board_notes,
    }
}

/// Returns Err with a message if the project's links contain a cycle or an
/// invalid link. Used by editing operations to reject a change before it is
/// committed (Project refuses circular links outright).
pub fn check_links(p: &Project) -> Result<(), String> {
    let st = Structure::build(p);
    let g = build_graph(p, &st);
    match g.errors.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct Ctx<'a> {
    cal: &'a Calendar,
}

impl Ctx<'_> {
    /// Apply a lag forward in time.
    fn shift(&self, dt: NaiveDateTime, lag_mins: i64, elapsed: bool) -> NaiveDateTime {
        if lag_mins == 0 {
            dt
        } else if elapsed {
            dt + ChronoDur::minutes(lag_mins)
        } else {
            self.cal.add_work(dt, lag_mins)
        }
    }
    fn unshift(&self, dt: NaiveDateTime, lag_mins: i64, elapsed: bool) -> NaiveDateTime {
        self.shift(dt, -lag_mins, elapsed)
    }
}

/// How each task's duration maps onto calendars. A task with work
/// resources occupies each assignment's span (work / units) on that
/// resource's calendar; its start is the earliest assignment start and its
/// finish the latest assignment finish. Other tasks use the project
/// calendar. Elapsed durations ignore calendars.
struct SpanSet {
    cals: Vec<Calendar>,
    parts: Vec<Vec<(usize, i64)>>,
    elapsed: Vec<bool>,
    mins: Vec<i64>,
}

impl SpanSet {
    fn build(p: &Project, st: &Structure, cal: &Calendar) -> SpanSet {
        let n = st.n;
        let mut cals = vec![cal.clone()];
        let mut res_ci: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut work_res: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for r in &p.resources {
            if r.rtype == crate::model::ResourceType::Work {
                work_res.insert(r.uid);
            }
            if r.has_own_calendar() {
                cals.push(r.effective_calendar(cal));
                res_ci.insert(r.uid, cals.len() - 1);
            } else {
                res_ci.insert(r.uid, 0);
            }
        }
        let idx: std::collections::HashMap<u32, usize> =
            p.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
        let mut parts: Vec<Vec<(usize, i64)>> = vec![vec![]; n];
        for (ai, a) in p.assignments.iter().enumerate() {
            let Some(&ti) = idx.get(&a.task) else { continue };
            if st.is_summary[ti] || !work_res.contains(&a.resource) || a.units <= 1e-9 {
                continue;
            }
            parts[ti].push((res_ci.get(&a.resource).copied().unwrap_or(0), p.assignment_span(ai)));
        }
        let mut elapsed = vec![false; n];
        let mut mins = vec![0; n];
        for i in 0..n {
            let d = &p.tasks[i].duration;
            elapsed[i] = d.elapsed;
            mins[i] = d.minutes;
            if parts[i].is_empty() || d.elapsed {
                parts[i] = vec![(0, d.minutes)];
            }
        }
        SpanSet { cals, parts, elapsed, mins }
    }
    fn finish(&self, t: usize, s: NaiveDateTime) -> NaiveDateTime {
        if self.elapsed[t] {
            return s + ChronoDur::minutes(self.mins[t]);
        }
        self.parts[t]
            .iter()
            .map(|&(c, m)| if m == 0 { s } else { self.cals[c].add_work(s, m) })
            .max()
            .unwrap_or(s)
    }
    fn start(&self, t: usize, f: NaiveDateTime) -> NaiveDateTime {
        if self.elapsed[t] {
            return f - ChronoDur::minutes(self.mins[t]);
        }
        self.parts[t]
            .iter()
            .map(|&(c, m)| if m == 0 { f } else { self.cals[c].sub_work(f, m) })
            .min()
            .unwrap_or(f)
    }
    fn snap(&self, t: usize, s: NaiveDateTime, zero: bool) -> NaiveDateTime {
        self.parts[t].iter().map(|&(c, _)| self.cals[c].snap_start(s, zero)).min().unwrap_or(s)
    }
    fn snap_finish(&self, t: usize, f: NaiveDateTime, zero: bool) -> NaiveDateTime {
        self.parts[t].iter().map(|&(c, _)| self.cals[c].snap_finish(f, zero)).max().unwrap_or(f)
    }
}

fn lag_minutes(lag: &Lag, pred_duration: i64) -> (i64, bool) {
    match lag {
        Lag::Time {
            minutes, elapsed, ..
        } => (*minutes, *elapsed),
        Lag::Percent(pct) => (((pred_duration as f64) * pct / 100.0).round() as i64, false),
    }
}

/// Everything the scheduler derives from the project's *structure* (outline,
/// links, assignment spans, calendars). Leveling only changes leveling
/// delays, so it prepares once per pass and reruns the passes cheaply.
pub(crate) struct Prepared {
    st: Structure,
    pub(crate) g: Graph,
    sp: SpanSet,
    cal: Calendar,
    has_alap: bool,
    cyclic: std::collections::HashSet<usize>,
}

pub(crate) fn prepare(p: &mut Project) -> Prepared {
    p.normalize_outline();
    let st = Structure::build(p);
    let g = build_graph(p, &st);
    let cal = p.calendar.clone();
    let sp = SpanSet::build(p, &st, &cal);
    let has_alap = p.tasks.iter().any(|t| t.constraint == ConstraintType::ALAP && t.mode == TaskMode::Auto);
    let cyclic = g.cyclic.iter().copied().collect();
    Prepared { st, g, sp, cal, has_alap, cyclic }
}

/// Run the scheduler, filling every task's `calc` and the project's `calc`.
pub fn schedule(p: &mut Project) {
    // sprint progress comes from the Kanban board
    p.apply_sprint_progress();
    let prep = prepare(p);
    run(p, &prep, true);
}

/// Leveling's cheap reschedule: dates only (forward pass, ALAP placement
/// if needed, assignment dates). Slack, status, costs and overallocation
/// are left as they were until the next full `schedule`.
pub(crate) fn reschedule_dates(p: &mut Project, prep: &Prepared) {
    run(p, prep, false);
}

fn run(p: &mut Project, prep: &Prepared, full: bool) {
    let st = &prep.st;
    let g = &prep.g;
    let n = st.n;
    let cal = &prep.cal;
    let ctx = Ctx { cal };
    let sp = &prep.sp;
    let threshold = p.settings.critical_slack_threshold;

    let cyclic = &prep.cyclic;

    let span = |i: usize, s: &Vec<Option<NaiveDateTime>>, f: &Vec<Option<NaiveDateTime>>| {
        let mut lo: Option<NaiveDateTime> = None;
        let mut hi: Option<NaiveDateTime> = None;
        for &l in &st.leaves[i] {
            if let Some(v) = s[l] {
                lo = Some(lo.map_or(v, |x: NaiveDateTime| x.min(v)));
            }
            if let Some(v) = f[l] {
                hi = Some(hi.map_or(v, |x: NaiveDateTime| x.max(v)));
            }
        }
        match (lo, hi) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    };
    let dur_of = |i: usize, s: &Vec<Option<NaiveDateTime>>, f: &Vec<Option<NaiveDateTime>>| -> i64 {
        if st.is_summary[i] {
            span(i, s, f).map_or(0, |(a, b)| cal.work_between(a, b))
        } else {
            p.tasks[i].duration.minutes
        }
    };

    let Forward { mut es, mut ef, mut conflict, mut driver, unleveled, mut driving, mut driving_leaves } = prep.forward_all(p);

    // inactive tasks don't set the project finish (unless nothing is active)
    let any_active = (0..n).any(|i| !st.is_summary[i] && !st.inactive[i]);
    let project_finish = (0..n)
        .filter(|&i| !st.is_summary[i] && (!any_active || !st.inactive[i]))
        .filter_map(|i| ef[i])
        .max()
        .unwrap_or(p.start);

    // ---------------- backward pass (+ ALAP placement) ----------------
    let mut ls: Vec<Option<NaiveDateTime>> = vec![None; n];
    let mut lf: Vec<Option<NaiveDateTime>> = vec![None; n];
    let backward: &[usize] = if full || prep.has_alap { &g.order } else { &[] };
    for &t in backward.iter().rev() {
        let task = &p.tasks[t];
        let mins = task.duration.minutes;
        let zero = mins == 0;
        let (Some(t_es), Some(t_ef)) = (es[t], ef[t]) else {
            continue;
        };
        // With "multiple critical paths", a chain that ends early is
        // measured against its own finish rather than the project's.
        let chain_end = if p.settings.multiple_critical_paths && g.outgoing[t].is_empty() {
            t_ef
        } else {
            project_finish
        };
        let mut late_f = chain_end;
        let mut alap_f = project_finish;
        if !cyclic.contains(&t) {
            for &li in &g.outgoing[t] {
                let e = &g.links[li];
                // SS/SF from a summary only bind the leaves that drive its start.
                if e.pred != t && matches!(e.link.kind, LinkType::SS | LinkType::SF) {
                    if let Some((ps, _)) = span(e.pred, &es, &ef) {
                        if t_es != ps {
                            continue;
                        }
                    }
                }
                let pdur = dur_of(e.pred, &es, &ef);
                let (lag, lag_el) = lag_minutes(&e.link.lag, pdur);
                let bound = |ss: NaiveDateTime, sf: NaiveDateTime| -> NaiveDateTime {
                    match e.link.kind {
                        LinkType::FS => ctx.unshift(ss, lag, lag_el),
                        LinkType::FF => ctx.unshift(sf, lag, lag_el),
                        LinkType::SS => sp.finish(t, ctx.unshift(ss, lag, lag_el)),
                        LinkType::SF => sp.finish(t, ctx.unshift(sf, lag, lag_el)),
                    }
                };
                if let Some((sls, slf)) = span(e.succ, &ls, &lf) {
                    late_f = late_f.min(bound(sls, slf));
                }
                if let Some((ses, sef)) = span(e.succ, &es, &ef) {
                    alap_f = alap_f.min(bound(ses, sef));
                }
            }
        }
        // own + ancestors' finish-limiting constraints and deadlines
        let mut limits: Vec<NaiveDateTime> = vec![];
        if let Some(cd) = task.constraint_date {
            match task.constraint {
                ConstraintType::SNLT => limits.push(sp.finish(t, cd)),
                ConstraintType::FNLT => limits.push(cd),
                ConstraintType::MSO | ConstraintType::MFO => limits.push(t_ef),
                _ => {}
            }
        }
        if let Some(d) = task.deadline {
            limits.push(d);
            alap_f = alap_f.min(d);
        }
        for a in st.ancestors(t) {
            let at = &p.tasks[a];
            if let (ConstraintType::FNLT, Some(cd)) = (at.constraint, at.constraint_date) {
                limits.push(cd);
            }
            if let Some(d) = at.deadline {
                limits.push(d);
            }
        }
        for l in limits {
            late_f = late_f.min(l);
        }
        let late_f = sp.snap_finish(t, late_f, zero);
        lf[t] = Some(late_f);
        ls[t] = Some(if zero {
            late_f
        } else {
            sp.start(t, late_f)
        });

        if task.mode == TaskMode::Auto && task.constraint == ConstraintType::ALAP && task.actual_start.is_none() && !cyclic.contains(&t) {
            let af = sp.snap_finish(t, alap_f, zero);
            let a_s = if zero { af } else { sp.start(t, af) };
            if a_s > t_es {
                es[t] = Some(a_s);
                ef[t] = Some(af);
                driver[t] = StartDriver::AsLateAsPossible;
                driving[t].clear();
                driving_leaves[t].clear();
            }
        }
    }

    if !full {
        // dates only: update in place, keep everything else
        for i in 0..n {
            let c = &mut p.tasks[i].calc;
            if st.is_summary[i] {
                if let Some((s, f)) = span(i, &es, &ef) {
                    c.start = Some(s);
                    c.finish = Some(f);
                    c.early_start = Some(s);
                    c.early_finish = Some(f);
                }
            } else {
                c.start = es[i];
                c.finish = ef[i];
                c.early_start = es[i];
                c.early_finish = ef[i];
                c.unleveled_start = unleveled[i];
                c.start_driver = driver[i];
            }
        }
        p.calc.finish = Some(project_finish);
        crate::resources::update_assignment_dates(p);
        return;
    }

    // ---------------- slack ----------------
    let mut total_slack = vec![0i64; n];
    let mut free_slack = vec![0i64; n];
    for &t in &g.order {
        let (Some(a), Some(b), Some(c), Some(d)) = (es[t], ef[t], ls[t], lf[t]) else {
            continue;
        };
        let ts = cal.work_between(a, c).min(cal.work_between(b, d));
        total_slack[t] = ts;
        let mut fs: Option<i64> = None;
        if !cyclic.contains(&t) {
            for &li in &g.outgoing[t] {
                let e = &g.links[li];
                let Some((ses, sef)) = span(e.succ, &es, &ef) else {
                    continue;
                };
                let pdur = dur_of(e.pred, &es, &ef);
                let (lag, lag_el) = lag_minutes(&e.link.lag, pdur);
                let gap = match e.link.kind {
                    LinkType::FS => cal.work_between(b, ctx.unshift(ses, lag, lag_el)),
                    LinkType::SS => cal.work_between(a, ctx.unshift(ses, lag, lag_el)),
                    LinkType::FF => cal.work_between(b, ctx.unshift(sef, lag, lag_el)),
                    LinkType::SF => cal.work_between(a, ctx.unshift(sef, lag, lag_el)),
                };
                fs = Some(fs.map_or(gap, |x| x.min(gap)));
            }
        }
        let fs = fs.unwrap_or(ts);
        free_slack[t] = fs.min(ts).max(0);
    }

    // ---------------- write results ----------------
    let status_date = p.status_date.unwrap_or_else(|| chrono::Local::now().naive_local());
    let mut leaf_status: Vec<(TaskStatus, bool)> = vec![(TaskStatus::FutureTask, false); n];
    for i in 0..n {
        if let (false, Some(s), Some(f)) = (st.is_summary[i], es[i], ef[i]) {
            leaf_status[i] = task_status(&cal, s, f, p.tasks[i].percent_complete, status_date);
        }
    }
    let mut outline_counters: Vec<u32> = vec![];
    for i in 0..n {
        let lvl = p.tasks[i].outline_level as usize;
        outline_counters.truncate(lvl);
        while outline_counters.len() < lvl {
            outline_counters.push(0);
        }
        outline_counters[lvl - 1] += 1;
        let outline_number = outline_counters
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(".");

        let mut c = crate::model::Computed {
            outline_number,
            is_summary: st.is_summary[i],
            ..Default::default()
        };
        if st.is_summary[i] {
            if let Some((s, f)) = span(i, &es, &ef) {
                c.start = Some(s);
                c.finish = Some(f);
                c.early_start = Some(s);
                c.early_finish = Some(f);
                c.summary_duration = cal.work_between(s, f);
            }
            if let Some((s, f)) = span(i, &ls, &lf) {
                c.late_start = Some(s);
                c.late_finish = Some(f);
            }
            let leaves = &st.leaves[i];
            c.total_slack = leaves.iter().map(|&l| total_slack[l]).min().unwrap_or(0);
            c.free_slack = leaves.iter().map(|&l| free_slack[l]).min().unwrap_or(0);
            let mut wsum = 0.0;
            let mut w = 0.0;
            for &l in leaves {
                let d = p.tasks[l].duration.minutes as f64;
                wsum += d * p.tasks[l].percent_complete;
                w += d;
            }
            c.percent_complete = if w > 0.0 {
                wsum / w
            } else if leaves.is_empty() {
                0.0
            } else {
                leaves.iter().map(|&l| p.tasks[l].percent_complete).sum::<f64>() / leaves.len() as f64
            };
            c.critical = leaves.iter().any(|&l| total_slack[l] <= threshold);
            if let (Some(a), Some(b), Some(x), Some(y)) = (c.start, c.finish, c.late_start, c.late_finish) {
                c.start_slack = cal.work_between(a, x);
                c.finish_slack = cal.work_between(b, y);
            }
            let sts: Vec<TaskStatus> = leaves.iter().map(|&l| leaf_status[l].0).collect();
            c.late = leaves.iter().any(|&l| leaf_status[l].1);
            c.status = if !sts.is_empty() && sts.iter().all(|s| *s == TaskStatus::Complete) {
                TaskStatus::Complete
            } else if c.late {
                TaskStatus::Late
            } else if sts.iter().all(|s| *s == TaskStatus::FutureTask) {
                TaskStatus::FutureTask
            } else {
                TaskStatus::OnSchedule
            };
        } else {
            c.start = es[i];
            c.finish = ef[i];
            c.early_start = es[i];
            c.early_finish = ef[i];
            c.late_start = ls[i];
            c.late_finish = lf[i];
            c.total_slack = total_slack[i];
            c.free_slack = free_slack[i];
            c.critical = total_slack[i] <= threshold;
            c.percent_complete = p.tasks[i].percent_complete;
            c.conflict = conflict[i].take();
            c.start_driver = driver[i];
            c.unleveled_start = unleveled[i];
            c.driving_links = driving[i]
                .iter()
                .map(|&li| (p.tasks[g.links[li].pred].uid, g.links[li].link.kind))
                .collect();
            c.driving_preds = std::mem::take(&mut driving_leaves[i]);
            c.board_driving = driving[i].iter().filter_map(|&li| g.links[li].board).collect();
            if let (Some(a), Some(b), Some(x), Some(y)) = (es[i], ef[i], ls[i], lf[i]) {
                c.start_slack = cal.work_between(a, x);
                c.finish_slack = cal.work_between(b, y);
            }
            (c.status, c.late) = leaf_status[i];
            if cyclic.contains(&i) {
                c.conflict = Some("Part of a circular dependency.".into());
            }
        }
        if let (Some(d), Some(f)) = (p.tasks[i].deadline, c.finish) {
            c.deadline_missed = f > d;
        }
        c.board_waiting = g.board_waiting[i].clone();
        c.board_notes = g.board_notes[i].clone();
        p.tasks[i].calc = c;
    }
    p.calc.finish = Some(project_finish);
    p.calc.errors = g.errors.clone();
    crate::resources::compute_resources(p);
}

/// Per-leaf results of the forward pass.
pub(crate) struct Forward {
    pub es: Vec<Option<NaiveDateTime>>,
    pub ef: Vec<Option<NaiveDateTime>>,
    pub conflict: Vec<Option<String>>,
    pub driver: Vec<StartDriver>,
    pub unleveled: Vec<Option<NaiveDateTime>>,
    pub driving: Vec<Vec<usize>>,
    pub driving_leaves: Vec<Vec<u32>>,
}

fn span_of(st: &Structure, i: usize, s: &[Option<NaiveDateTime>], f: &[Option<NaiveDateTime>]) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let mut lo: Option<NaiveDateTime> = None;
    let mut hi: Option<NaiveDateTime> = None;
    for &l in &st.leaves[i] {
        if let Some(v) = s[l] {
            lo = Some(lo.map_or(v, |x: NaiveDateTime| x.min(v)));
        }
        if let Some(v) = f[l] {
            hi = Some(hi.map_or(v, |x: NaiveDateTime| x.max(v)));
        }
    }
    match (lo, hi) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

fn dur_of(p: &Project, st: &Structure, cal: &Calendar, i: usize, s: &[Option<NaiveDateTime>], f: &[Option<NaiveDateTime>]) -> i64 {
    if st.is_summary[i] {
        span_of(st, i, s, f).map_or(0, |(a, b)| cal.work_between(a, b))
    } else {
        p.tasks[i].duration.minutes
    }
}

impl Prepared {
    pub(crate) fn forward_empty(&self) -> Forward {
        let n = self.st.n;
        Forward {
            es: vec![None; n],
            ef: vec![None; n],
            conflict: vec![None; n],
            driver: vec![StartDriver::ProjectStart; n],
            unleveled: vec![None; n],
            driving: vec![vec![]; n],
            driving_leaves: vec![vec![]; n],
        }
    }

    /// The forward pass for every leaf, in dependency order.
    pub(crate) fn forward_all(&self, p: &Project) -> Forward {
        let mut f = self.forward_empty();
        for &t in &self.g.order {
            self.forward_node(p, &mut f, t);
        }
        f
    }

    /// Leaves whose dates leaf `t`'s forward computation reads.
    pub(crate) fn deps(&self, t: usize) -> impl Iterator<Item = usize> + '_ {
        self.g.incoming[t].iter().flat_map(move |&li| self.st.leaves[self.g.links[li].pred].iter().copied())
    }

    /// The lazy leveler needs a forward-only, acyclic schedule.
    pub(crate) fn forward_only_ok(&self) -> bool {
        !self.has_alap && self.cyclic.is_empty()
    }

    /// Early dates of one leaf from its predecessors' current results.
    pub(crate) fn forward_node(&self, p: &Project, f: &mut Forward, t: usize) {
        let st = &self.st;
        let g = &self.g;
        let sp = &self.sp;
        let cal = &self.cal;
        let ctx = Ctx { cal };
        let honor = p.settings.honor_constraints;
        f.conflict[t] = None;
        f.driving[t].clear();
        f.driving_leaves[t].clear();
        f.driver[t] = StartDriver::ProjectStart;
        f.unleveled[t] = None;
    let task = &p.tasks[t];
    let mins = task.duration.minutes;
    let zero = mins == 0;
    // Actuals are facts: a started task starts when it started, whatever
    // its links and constraints say (Project behaves the same way).
    if let Some(a) = task.actual_start {
        f.es[t] = Some(a);
        f.ef[t] = Some(match task.actual_finish {
            Some(af) if af >= a => af,
            _ if zero => a,
            _ => sp.finish(t, a),
        });
        f.unleveled[t] = Some(a);
        f.driver[t] = StartDriver::Actual;
        return;
    }
    if task.mode == TaskMode::Manual {
        let s = task.manual_start.unwrap_or(p.start);
        f.es[t] = Some(s);
        f.ef[t] = Some(sp.finish(t, s));
        f.driver[t] = StartDriver::ManualDates;
        return;
    }
    let mut start = p.start;
    let mut cands: Vec<(usize, NaiveDateTime)> = vec![];
    if !self.cyclic.contains(&t) {
        for &li in &g.incoming[t] {
            let e = &g.links[li];
            let Some((ps, pf)) = span_of(st, e.pred, &f.es, &f.ef) else {
                continue;
            };
            let pdur = dur_of(p, st, cal, e.pred, &f.es, &f.ef);
            let (lag, lag_el) = lag_minutes(&e.link.lag, pdur);
            let cand = match e.link.kind {
                LinkType::FS => ctx.shift(pf, lag, lag_el),
                LinkType::SS => ctx.shift(ps, lag, lag_el),
                LinkType::FF => sp.start(t, ctx.shift(pf, lag, lag_el)),
                LinkType::SF => sp.start(t, ctx.shift(ps, lag, lag_el)),
            };
            // A positive-duration successor can't start at a shift end.
            let cand = sp.snap(t, cand, zero);
            cands.push((li, cand));
            if cand > start {
                start = cand;
            }
        }
    }
    // finished board items it waits on
    let mut floor_best: Option<NaiveDateTime> = None;
    for &(done, lag, pdur, _) in &g.floors[t] {
        let (lm, el) = lag_minutes(&lag, pdur);
        let cand = sp.snap(t, ctx.shift(done, lm, el), zero);
        floor_best = Some(floor_best.map_or(cand, |b| b.max(cand)));
        if cand > start {
            start = cand;
        }
    }
    let after_links = start;
    // Constraints placed on summary ancestors bound their children.
    for a in st.ancestors(t) {
        let at = &p.tasks[a];
        if let Some(cd) = at.constraint_date {
            if matches!(at.constraint, ConstraintType::SNET | ConstraintType::MSO) && cd > start {
                start = cd;
            }
        }
    }
    let parent_raised = sp.snap(t, start, zero) > sp.snap(t, after_links, zero);
    let link_start = start;
    if let Some(cd) = task.constraint_date {
        match task.constraint {
            ConstraintType::ASAP | ConstraintType::ALAP => {}
            ConstraintType::SNET => start = start.max(cd),
            ConstraintType::FNET => start = start.max(sp.start(t, cd)),
            ConstraintType::MSO => {
                if link_start > cd {
                    f.conflict[t] = Some("Predecessor links push this task past its Must Start On date.".into());
                }
                if honor || link_start <= cd {
                    start = cd;
                }
            }
            ConstraintType::MFO => {
                let s = sp.start(t, cd);
                if link_start > s {
                    f.conflict[t] = Some("Predecessor links push this task past its Must Finish On date.".into());
                }
                if honor || link_start <= s {
                    start = s;
                }
            }
            // No Later Than caps apply after the leveling delay below
            ConstraintType::SNLT | ConstraintType::FNLT => {}
        }
    }
    let constraint_moved = sp.snap(t, start, zero) != sp.snap(t, link_start, zero);
    // Resource leveling delays the task from its early start (after
    // links and Start/Finish No Earlier Than), as Project defines the
    // Leveling Delay field; No Later Than caps still win below.
    let mut leveled = false;
    let before_delay = start;
    f.unleveled[t] = Some(sp.snap(t, start, zero));
    if task.leveling_delay > 0 && !self.cyclic.contains(&t) && !matches!(task.constraint, ConstraintType::MSO | ConstraintType::MFO) {
        let before = sp.snap(t, start, zero);
        start = cal.add_work(before, task.leveling_delay);
        leveled = sp.snap(t, start, zero) > before;
    }
    let mut capped = false;
    if let Some(cd) = task.constraint_date {
        let why = if leveled && before_delay <= cd { "The leveling delay pushes" } else { "Predecessor links push" };
        match task.constraint {
            ConstraintType::SNLT if start > cd => {
                f.conflict[t] = Some(format!("{why} this task past its Start No Later Than date."));
                if honor {
                    start = cd;
                    capped = true;
                }
            }
            ConstraintType::FNLT if sp.finish(t, start) > cd => {
                f.conflict[t] = Some(format!("{why} this task past its Finish No Later Than date."));
                if honor {
                    start = sp.start(t, cd);
                    capped = true;
                }
            }
            _ => {}
        }
    }
    let start = sp.snap(t, start, zero);
    f.es[t] = Some(start);
    f.ef[t] = Some(if zero {
        start
    } else {
        sp.finish(t, start)
    });
    // Record what drives this start (Task Inspector / Task Path).
    if self.cyclic.contains(&t) {
        f.driver[t] = StartDriver::Circular;
    } else if capped {
        f.driver[t] = StartDriver::Constraint;
    } else if leveled {
        f.driver[t] = StartDriver::Leveling;
    } else if constraint_moved {
        f.driver[t] = StartDriver::Constraint;
    } else if parent_raised {
        f.driver[t] = StartDriver::ParentConstraint;
    } else {
        let d: Vec<usize> = cands.iter().filter(|(_, c)| *c == start).map(|(li, _)| *li).collect();
        if d.is_empty() && floor_best == Some(start) && start > p.start {
            f.driver[t] = StartDriver::BoardItem;
        }
        if !d.is_empty() {
            f.driver[t] = StartDriver::Predecessors;
            for &li in &d {
                let e = &g.links[li];
                let span_p = span_of(st, e.pred, &f.es, &f.ef);
                for &l in &st.leaves[e.pred] {
                    let drives = match (e.link.kind, span_p) {
                        _ if !st.is_summary[e.pred] => true,
                        (LinkType::FS | LinkType::FF, Some((_, pf))) => f.ef[l] == Some(pf),
                        (LinkType::SS | LinkType::SF, Some((ps, _))) => f.es[l] == Some(ps),
                        _ => false,
                    };
                    if drives && !f.driving_leaves[t].contains(&p.tasks[l].uid) {
                        f.driving_leaves[t].push(p.tasks[l].uid);
                    }
                }
            }
            f.driving[t] = d;
        }
    }
    }
}

/// Project's Status field at `status_date`: Complete, Future Task (starts
/// after the status date), Late (less complete than the schedule says it
/// should be by then) or On Schedule. Returns (status, late).
pub fn task_status(
    cal: &Calendar,
    start: NaiveDateTime,
    finish: NaiveDateTime,
    pct: f64,
    status_date: NaiveDateTime,
) -> (TaskStatus, bool) {
    if pct >= 100.0 {
        return (TaskStatus::Complete, false);
    }
    if start >= status_date {
        return (TaskStatus::FutureTask, false);
    }
    let expected = if status_date >= finish {
        100.0
    } else {
        let total = cal.work_between(start, finish);
        if total <= 0 {
            100.0
        } else {
            cal.work_between(start, status_date) as f64 / total as f64 * 100.0
        }
    };
    if pct + 0.5 < expected {
        (TaskStatus::Late, true)
    } else {
        (TaskStatus::OnSchedule, false)
    }
}

/// Refresh outline-derived values (summary flags, WBS numbers, summary
/// rollups) without rescheduling any task. Used while automatic calculation
/// is switched off (Ctrl+F9): like Project, dates stay put until F9.
pub fn refresh_structure(p: &mut Project) {
    p.normalize_outline();
    p.apply_sprint_progress();
    let st = Structure::build(p);
    let cal = p.calendar.clone();
    let pstart = p.start;
    for i in 0..st.n {
        if st.is_summary[i] {
            continue;
        }
        let t = &mut p.tasks[i];
        t.calc.is_summary = false;
        if t.calc.start.is_none() || t.calc.finish.is_none() {
            let s = match t.mode {
                TaskMode::Manual => t.manual_start.unwrap_or(pstart),
                TaskMode::Auto => cal.next_working_start(pstart),
            };
            let f = if t.duration.elapsed {
                s + ChronoDur::minutes(t.duration.minutes)
            } else {
                cal.add_work(s, t.duration.minutes)
            };
            t.calc.start = Some(s);
            t.calc.finish = Some(f);
        }
        t.calc.percent_complete = t.percent_complete;
    }
    let mut counters: Vec<u32> = vec![];
    for i in 0..st.n {
        let lvl = p.tasks[i].outline_level as usize;
        counters.truncate(lvl);
        while counters.len() < lvl {
            counters.push(0);
        }
        counters[lvl - 1] += 1;
        p.tasks[i].calc.outline_number = counters.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(".");
        if st.is_summary[i] {
            let starts = st.leaves[i].iter().filter_map(|&l| p.tasks[l].calc.start).min();
            let fins = st.leaves[i].iter().filter_map(|&l| p.tasks[l].calc.finish).max();
            let c = &mut p.tasks[i].calc;
            c.is_summary = true;
            c.start = starts;
            c.finish = fins;
            if let (Some(a), Some(b)) = (starts, fins) {
                c.summary_duration = cal.work_between(a, b);
            }
        }
    }
    p.calc.finish = p.tasks.iter().filter_map(|t| t.calc.finish).max().or(Some(pstart));
    crate::resources::compute_resources(p);
}
