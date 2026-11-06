//! Seeded stability fuzzing: random edit sequences, random app commands and
//! UI input, and mutated files must never panic or hang.
//!
//! Every case is reproducible from its seed. A failing case is shrunk to a
//! minimal list of steps and printed with the panic location.
//!
//!   cargo test --release --test fuzz_stability            (bounded, ~30 s)
//!   FUZZ_SCALE=20 cargo test --release --test fuzz_stability -- --nocapture
//!   FUZZ_SEED=1234 cargo test --release --test fuzz_stability ops   (one seed)
//!
//! Each test runs all its cases, then fails listing every distinct panic
//! (by location) with a shrunk step list. Environment switches:
//!   FUZZ_SCALE=n      multiply every iteration count (exploration runs)
//!   FUZZ_SEED=n       run just that seed
//!   FUZZ_HANG_SECS=n  per-step hang timeout (default 20; use ~300 in debug)
//!   FUZZ_BT=1         capture a backtrace with each panic (debug builds)
//!   FUZZ_TRACE=1      print every step with its time
//!   FUZZ_TAME=1       keep generated dates in 1900..2200 in the ops campaign
//!   FUZZ_WILD_DATES=1 allow extreme dates in the app campaign (tame by default)
//!   FUZZ_WHY=1        tally why mutated files were rejected
//! Cases that abort the process (stack overflow) or hang are kept out of
//! the generators and covered by tests/fuzz_repros.rs.

mod common;
use common::Rng;

use chrono::{Duration as CDur, NaiveDate, NaiveDateTime};
use projectfcg::calendar::{Calendar, CalendarException, Shift};
use projectfcg::model::{ConstraintType, LinkType, Project, ResourceType, TaskType};
use projectfcg::ops::Field;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ infra

/// Seconds one step may run before it counts as a hang (FUZZ_HANG_SECS
/// overrides it; debug builds are ~10x slower).
const HANG_SECS: u64 = 20;
fn hang_secs() -> u64 {
    std::env::var("FUZZ_HANG_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(HANG_SECS)
}
/// A single step slower than this is reported as "slow" (hang suspicion).
const SLOW_STEP_SECS: f64 = 2.0;

fn scale() -> u64 {
    std::env::var("FUZZ_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1).max(1)
}
fn only_seed() -> Option<u64> {
    std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse().ok())
}

thread_local! {
    static CAPTURE: RefCell<bool> = const { RefCell::new(false) };
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

static HOOK: Once = Once::new();
fn install_hook() {
    HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let capturing = CAPTURE.with(|c| *c.borrow());
            if !capturing {
                return default(info);
            }
            let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "<non-string panic>".to_string()
            };
            let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            let bt = if std::env::var_os("FUZZ_BT").is_some() {
                format!("\n{}", std::backtrace::Backtrace::force_capture())
            } else {
                String::new()
            };
            LAST.with(|l| *l.borrow_mut() = Some(format!("{msg} @ {loc}{bt}")));
        }));
    });
}

/// Run `f`, turning a panic into `Err("message @ file:line")`.
fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    install_hook();
    CAPTURE.with(|c| *c.borrow_mut() = true);
    LAST.with(|l| *l.borrow_mut() = None);
    let r = catch_unwind(AssertUnwindSafe(f));
    CAPTURE.with(|c| *c.borrow_mut() = false);
    r.map_err(|_| LAST.with(|l| l.borrow_mut().take()).unwrap_or_else(|| "<panic>".into()))
}

/// The part of a panic report that identifies the bug: its location, and
/// the message with numbers stripped (so the same bug at different values
/// counts once).
fn signature(err: &str) -> String {
    let first = err.lines().next().unwrap_or("");
    let (msg, loc) = first.rsplit_once(" @ ").unwrap_or((first, ""));
    let msg: String = msg.chars().filter(|c| !c.is_ascii_digit()).take(60).collect();
    format!("{loc} | {msg}")
}

/// A per-thread watchdog: if the current case runs longer than HANG_SECS,
/// print what it was and abort the whole process (a hung case can't be
/// interrupted from outside).
struct Watchdog {
    state: std::sync::Arc<Mutex<(Instant, String, bool)>>,
    stop: std::sync::Arc<AtomicBool>,
}
impl Watchdog {
    fn new(name: &'static str) -> Self {
        let state = std::sync::Arc::new(Mutex::new((Instant::now(), String::new(), false)));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let (s2, st2) = (state.clone(), stop.clone());
        std::thread::spawn(move || {
            while !st2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                let g = s2.lock().unwrap();
                if g.2 && g.0.elapsed() > Duration::from_secs(hang_secs()) {
                    eprintln!("\n=== HANG in {name}: step ran > {}s ===\n{}\n", hang_secs(), g.1);
                    std::process::exit(99);
                }
            }
        });
        Watchdog { state, stop }
    }
    fn start(&self, what: String) {
        *self.state.lock().unwrap() = (Instant::now(), what, true);
    }
    /// A new step: the timeout counts from here.
    fn note(&self, what: String) {
        let mut g = self.state.lock().unwrap();
        g.0 = Instant::now();
        g.1 = what;
    }
    fn done(&self) {
        self.state.lock().unwrap().2 = false;
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Distinct failures found by one campaign.
#[derive(Default)]
struct Findings {
    by_sig: BTreeMap<String, (usize, String)>,
    slow: Vec<String>,
    broken: BTreeMap<String, (usize, String)>,
}
impl Findings {
    fn add(&mut self, sig: String, repro: impl FnOnce() -> String) -> bool {
        if let Some(e) = self.by_sig.get_mut(&sig) {
            e.0 += 1;
            false
        } else {
            self.by_sig.insert(sig, (1, repro()));
            true
        }
    }
    fn broken(&mut self, what: &str, detail: String) {
        self.broken.entry(what.to_string()).or_insert((0, detail)).0 += 1;
    }
    fn finish(self, campaign: &str) {
        let mut s = String::new();
        for (sig, (n, repro)) in &self.by_sig {
            s += &format!("\n--- [{campaign}] PANIC x{n}: {sig}\n{repro}\n");
        }
        // slow steps are reported but don't fail the test (machine load
        // varies); real hangs fail through the watchdog
        for w in &self.slow {
            eprintln!("--- [{campaign}] SLOW: {w}");
        }
        if !self.broken.is_empty() {
            eprintln!("\n[{campaign}] suspicious state (not failing the test):");
            for (k, (n, d)) in &self.broken {
                eprintln!("  x{n} {k}: {d}");
            }
        }
        if !s.is_empty() {
            panic!("{campaign}: {} distinct panic(s), {} slow step(s):{s}", self.by_sig.len(), self.slow.len());
        }
    }
}

/// Greedy delta-debugging: drop chunks, then single steps, while `fails`
/// still reproduces the same signature.
fn shrink<T: Clone>(steps: &[T], sig: &str, fails: &dyn Fn(&[T]) -> Option<String>) -> Vec<T> {
    let mut cur = steps.to_vec();
    let same = |v: &[T]| fails(v).is_some_and(|e| signature(&e) == sig);
    let mut chunk = (cur.len() / 2).max(1);
    let mut budget = 400usize;
    while chunk >= 1 && budget > 0 {
        let mut i = 0;
        let mut progressed = false;
        while i < cur.len() && budget > 0 {
            let mut cand = cur.clone();
            let end = (i + chunk).min(cand.len());
            cand.drain(i..end);
            budget -= 1;
            if same(&cand) {
                cur = cand;
                progressed = true;
            } else {
                i += chunk;
            }
        }
        if !progressed {
            if chunk == 1 {
                break;
            }
            chunk /= 2;
        }
    }
    cur
}

fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, dd).unwrap()
}
fn dt(y: i32, m: u32, dd: u32, h: u32) -> NaiveDateTime {
    d(y, m, dd).and_hms_opt(h, 0, 0).unwrap()
}

/// Suspicious-but-not-panicking state after scheduling.
fn check_state(p: &Project, f: &mut Findings, ctx: &str) {
    for (i, t) in p.tasks.iter().enumerate() {
        let c = &t.calc;
        if let (Some(s), Some(e)) = (c.start, c.finish) {
            if e < s && !c.is_summary {
                f.broken("finish < start", format!("{ctx}: row {i} {s} .. {e}"));
            }
        }
        if c.work < 0 {
            f.broken("negative work", format!("{ctx}: row {i} work {}", c.work));
        }
        if !c.cost.is_finite() {
            f.broken("non-finite cost", format!("{ctx}: row {i} cost {}", c.cost));
        }
        if !t.percent_complete.is_finite() || t.percent_complete < 0.0 || t.percent_complete > 100.0 {
            f.broken("percent out of 0..100", format!("{ctx}: row {i} {}", t.percent_complete));
        }
        if t.duration.minutes < 0 {
            f.broken("negative duration", format!("{ctx}: row {i} {}", t.duration.minutes));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for t in &p.tasks {
        if !seen.insert(t.uid) {
            f.broken("duplicate task uid", format!("{ctx}: uid {}", t.uid));
            break;
        }
    }
    for a in &p.assignments {
        if !a.units.is_finite() || a.work < 0 {
            f.broken("bad assignment", format!("{ctx}: units {} work {}", a.units, a.work));
        }
    }
}

// --------------------------------------------------------- edge values

const DURS: &[&str] = &[
    "0", "-1d", "99999999d", "1e308h", "NaN", "0.0001m", "?", "1", "5d", "1w", "3ed", "0.5h", "1mo", "36500d", "52000w",
    "1200mo", "1 day?", "inf", "", "  ", "1,000,000m", "52704000m", "52703999m", "3.5ew", "36600ed", "1e11m", "0ed", "1m",
    "0.4m", "999999999999999999999d", "1.7976931348623157e308m", "5.", ".5d", "1,,,0d", "2 days?", "1 emo", "e", "5e",
];
const DATES: &[&str] = &[
    "1/1/1", "1/1/0001", "0001-01-01", "1900-01-01", "2100-12-31", "9999-12-31", "12/31/262143", "262143-12-31",
    "262142-12-31", "1/1/-262144", "-262144-01-01", "2026-01-05", "1/5/26 5:00 PM", "2/29/2100", "1/5/26 23:59", "1/5/26 0:00",
    "12/31/262142 11:59 PM", "1/1/1970", "Mon 1/5/26", "13/1/26", "1/5", "", "12/31/262143 11:59 PM", "1/1/-100",
    "1/1/1 0:00", "2026-02-30", "1/5/26 12:00 AM", "1/5/26 24:00",
];
const PCTS: &[&str] = &["-5", "101", "1e9", "0", "50", "100", "NaN", "99.9999", "1e-300", "-0", "100.0000001", "33.3%"];
const LAGS: &[&str] = &[
    "-999999d", "1e9%", "+100000%", "-100000%", "+2d", "-36500d", "+36500ed", "50%", "-50%", "+1e11m", "-52704000m",
    "+52704000m", "-1ed", "+0", "", "-", "+", "%", "-1e308%",
];
const RES: &[&str] = &[
    "R1[0%]", "R1[1e12%]", "R1[-5%]", "R2[1e11%]", "Mat[1e11 kg]", "Cost[$1e11]", "R1[NaN%]", "A, A, A", "", "[]", "[",
    "]", "R1[50%],R2", "R1", "R2[200%]", "Mat[5 kg]", "Cost[$100]", "R1[0.0001%]", "R1[99999999999%]", ",,,", "R1[$-5]",
    "Mat[-3 kg]", "R1[1e-12%]",
];
const WORK: &[&str] = &["0", "1e11h", "99999999h", "8h", "1h", "52704000m", "0.0001m", "-8h", "1w", "36600ed"];
const NAMES: &[&str] = &["", " ", "T", "\u{0}", "名前", "a\nb", "\"quoted\"", "<x>&amp;", "\u{202e}rtl"];

thread_local! {
    /// Keep generated dates within 1900..2200 (so one known date-range bug
    /// doesn't end every case before it reaches other code).
    static TAME: RefCell<bool> = const { RefCell::new(false) };
}

fn date_val(r: &mut Rng) -> NaiveDateTime {
    if TAME.with(|t| *t.borrow()) {
        return dt(1900 + r.below(300) as i32, 1 + r.below(12) as u32, 1 + r.below(28) as u32, r.below(24) as u32);
    }
    match r.below(12) {
        0 => NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap(),
        1 => NaiveDate::MAX.and_hms_opt(23, 59, 0).unwrap(),
        2 => dt(1, 1, 1, 8),
        3 => dt(1900, 1, 1, 8),
        4 => dt(9999, 12, 31, 17),
        5 => dt(2100, 6, 1, 8),
        6 => NaiveDate::MAX.and_hms_opt(0, 0, 0).unwrap() - CDur::days(r.below(400) as i64),
        7 => NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap() + CDur::days(r.below(400) as i64),
        _ => dt(2026, 1, 5, 8) + CDur::minutes(r.below(400 * 1440) as i64) - CDur::days(100),
    }
}

fn big_string(r: &mut Rng) -> String {
    match r.below(4) {
        0 => "x".repeat(100_000),
        1 => "名".repeat(10_000),
        2 => (0..2000).map(|i| char::from_u32(32 + (i * 7919) % 0x2fff).unwrap_or('?')).collect(),
        _ => r.pick(NAMES).to_string(),
    }
}

fn weird_calendar(r: &mut Rng) -> Calendar {
    let mut c = match r.below(4) {
        0 => Calendar::standard(),
        1 => Calendar::twenty_four_hours(),
        2 => Calendar::night_shift(),
        _ => Calendar { name: "empty".into(), week: Default::default(), exceptions: vec![], disjoint: false },
    };
    match r.below(8) {
        0 => c.week = Default::default(),
        1 => {
            c.week = Default::default();
            c.week[r.below(7) as usize] = vec![Shift::new(480, 481)];
        }
        2 => {
            // nonworking for a century, one working minute somewhere
            // (1..9999 makes work_between_fast expand millions of exception
            // days per call: seconds per schedule — reported)
            c.exceptions.push(CalendarException { name: "long".into(), start: d(2000, 1, 1), end: d(2100, 12, 31), shifts: vec![] });
            if r.chance(50) {
                c.exceptions.push(CalendarException { name: "one".into(), start: d(2026, 3, 2), end: d(2026, 3, 2), shifts: vec![Shift::new(600, 601)] });
            }
        }
        3 => {
            c.exceptions.push(CalendarException {
                // (1..9999 makes Calendar::intersect + add_work quadratic: a hang, reported)
                name: "work a century".into(),
                start: d(2000, 1, 1),
                end: d(2100, 12, 31),
                shifts: vec![Shift::new(0, 1440)],
            });
        }
        4 => {
            for k in 0..r.below(200) {
                let s = d(2026, 1, 1) + CDur::days((k * 3) as i64);
                c.exceptions.push(CalendarException { name: format!("h{k}"), start: s, end: s + CDur::days(r.below(3) as i64), shifts: vec![] });
            }
        }
        5 => {
            c.exceptions.push(CalendarException { name: "max".into(), start: NaiveDate::MAX - CDur::days(10), end: NaiveDate::MAX, shifts: vec![Shift::new(0, 1440)] });
            c.exceptions.push(CalendarException { name: "min".into(), start: NaiveDate::MIN, end: NaiveDate::MIN + CDur::days(10), shifts: vec![] });
        }
        6 => {
            // inverted exception range
            c.exceptions.push(CalendarException { name: "inv".into(), start: d(2026, 5, 1), end: d(2026, 1, 1), shifts: vec![] });
        }
        _ => {}
    }
    c
}

// ------------------------------------------------- campaign 1: model ops

#[derive(Clone, Debug)]
enum Op {
    Append(String),
    Insert(usize),
    Delete(Vec<usize>),
    Set(usize, Field, String),
    Indent(Vec<usize>),
    Outdent(Vec<usize>),
    MoveUp(usize),
    MoveDown(usize),
    Link(usize, usize, LinkType, String),
    LinkTasks(Vec<usize>),
    Unlink(Vec<usize>),
    Constraint(usize, ConstraintType, Option<NaiveDateTime>),
    Deadline(usize, Option<NaiveDateTime>),
    ActualStart(usize, Option<NaiveDateTime>),
    ActualFinish(usize, Option<NaiveDateTime>),
    ActualDur(usize, i64),
    RemainingDur(usize, i64),
    Percent(usize, f64),
    Mode(usize, bool),
    TaskType(usize, u8),
    AddCal(String, u8),
    TaskCal(usize, Option<usize>, bool),
    ProjectCal(u64),
    ResCal(usize, u64),
    BaseCal(usize, u64),
    DeleteCal(usize),
    ProjectStart(NaiveDateTime),
    StatusDate(Option<NaiveDateTime>),
    Recurring(usize, u8, u32, u32, NaiveDate, String, bool),
    Baseline(usize, bool, Option<Vec<usize>>),
    UpdateProject(NaiveDateTime, bool, Option<NaiveDateTime>),
    Reschedule(NaiveDateTime),
    MarkOnTrack(Vec<usize>),
    Level(bool, bool),
    ClearLevel,
    AddRes(String, u8),
    DeleteRes(Vec<usize>),
    ResMaxUnits(usize, f64),
    ResRate(usize, f64),
    AssignUnits(usize, f64),
    Custom(usize, u8, u8, String),
    Ticket(String),
    Sprint(usize, bool),
    NewSprint,
    MoveTicket(usize, usize, usize),
    StartSprint(usize, NaiveDateTime),
    CompleteSprint(usize, NaiveDateTime),
    Release(String, Option<NaiveDate>),
    ReleaseMilestone(usize),
    Collapse(usize),
    OutlineLevel(Option<u32>),
    Settings(u8, i64),
    Fixed(usize, f64),
    Priority(usize, u32),
    Inactive(usize),
    Milestone(usize),
    RoundTrip,
}

fn rows(r: &mut Rng, n: usize) -> Vec<usize> {
    let k = 1 + r.below(4) as usize;
    let mut v: Vec<usize> = (0..k).map(|_| r.below(n.max(1) as u64) as usize).collect();
    if r.chance(10) {
        v = (0..n).collect();
    }
    v.sort();
    v.dedup();
    v
}

fn gen_op(r: &mut Rng, n: usize) -> Op {
    let row = r.below(n.max(1) as u64) as usize;
    let row2 = r.below(n.max(1) as u64) as usize;
    let fields = [Field::Name, Field::Duration, Field::Start, Field::Finish, Field::Predecessors, Field::PercentComplete, Field::Mode, Field::ResourceNames, Field::Work];
    let maybe_date = |r: &mut Rng| if r.chance(15) { None } else { Some(date_val(r)) };
    match r.below(64) {
        0..=5 => Op::Append(if r.chance(5) { big_string(r) } else { format!("T{}", r.below(100)) }),
        6 => Op::Insert(r.below(n as u64 + 2) as usize),
        7 => Op::Delete(rows(r, n)),
        8..=17 => {
            let f = *r.pick(&fields);
            let text = match f {
                Field::Name => big_string(r),
                Field::Duration => r.pick(DURS).to_string(),
                Field::Start | Field::Finish => r.pick(DATES).to_string(),
                Field::Predecessors => match r.below(4) {
                    0 => format!("{}{}{}", r.below(n as u64 + 2), r.pick(&["", "SS", "FF", "SF", "fs", "XX"]), r.pick(LAGS)),
                    1 => format!("{},{}", r.below(n as u64 + 1), r.below(n as u64 + 1)),
                    2 => r.pick(&["0", "99999999999999999999", "1;;2", "#1", "APP-1", "1SS-", "1FS+50%", "1+", "1 2"]).to_string(),
                    _ => (1..=n.min(30)).map(|i| i.to_string()).collect::<Vec<_>>().join(","),
                },
                Field::PercentComplete => r.pick(PCTS).to_string(),
                Field::Mode => r.pick(&["Auto", "Manual", "m", "x"]).to_string(),
                Field::ResourceNames => r.pick(RES).to_string(),
                Field::Work => r.pick(WORK).to_string(),
            };
            Op::Set(row, f, text)
        }
        18 => Op::Indent(rows(r, n)),
        19 => Op::Outdent(rows(r, n)),
        20 => Op::MoveUp(row),
        21 => Op::MoveDown(row),
        22..=24 => Op::Link(row, row2, *r.pick(&[LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF]), r.pick(LAGS).to_string()),
        25 => Op::LinkTasks(rows(r, n)),
        26 => Op::Unlink(rows(r, n)),
        27 => Op::Constraint(row, *r.pick(&ConstraintType::ALL), maybe_date(r)),
        28 => Op::Deadline(row, maybe_date(r)),
        29 => Op::ActualStart(row, maybe_date(r)),
        30 => Op::ActualFinish(row, maybe_date(r)),
        31 => Op::ActualDur(row, *r.pick(&[0, -1, 1, 480, i64::MAX, i64::MIN, 52_704_000, 1 << 40])),
        32 => Op::RemainingDur(row, *r.pick(&[0, -1, 1, 480, i64::MAX, i64::MIN, 52_704_000, 1 << 40])),
        33 => Op::Percent(row, *r.pick(&[-5.0, 101.0, 1e9, f64::NAN, f64::INFINITY, 50.0, 100.0, 0.0])),
        34 => Op::Mode(row, r.chance(50)),
        35 => Op::TaskType(row, r.below(3) as u8),
        36 => Op::AddCal(format!("C{}", r.below(5)), r.below(4) as u8),
        37 => Op::TaskCal(row, if r.chance(20) { None } else { Some(r.below(4) as usize) }, r.chance(50)),
        38 => Op::ProjectCal(r.next()),
        39 => Op::ResCal(r.below(4) as usize, r.next()),
        40 => Op::BaseCal(r.below(4) as usize, r.next()),
        41 => Op::DeleteCal(r.below(4) as usize),
        42 => Op::ProjectStart(date_val(r)),
        43 => Op::StatusDate(maybe_date(r)),
        44 => Op::Recurring(
            row,
            r.below(4) as u8,
            *r.pick(&[0, 1, 2, 7, 100, 1000, 100_000, u32::MAX / 12 + 1, u32::MAX]),
            *r.pick(&[0, 1, 5, 999, 1000, u32::MAX]),
            date_val(r).date(),
            r.pick(DURS).to_string(),
            r.chance(50),
        ),
        45 => Op::Baseline(r.below(11) as usize, r.chance(70), if r.chance(50) { None } else { Some(rows(r, n)) }),
        46 => Op::UpdateProject(date_val(r), r.chance(50), if r.chance(50) { None } else { Some(date_val(r)) }),
        47 => Op::Reschedule(date_val(r)),
        48 => Op::MarkOnTrack(rows(r, n)),
        49 => Op::Level(r.chance(50), r.chance(30)),
        50 => Op::ClearLevel,
        51 => Op::AddRes(format!("R{}", r.below(4)), r.below(3) as u8),
        52 => Op::DeleteRes(vec![r.below(4) as usize]),
        53 => {
            if r.chance(50) {
                Op::ResMaxUnits(r.below(4) as usize, *r.pick(&[0.0, -1.0, 1e12, f64::NAN, 0.5, 1e-12]))
            } else {
                Op::ResRate(r.below(4) as usize, *r.pick(&[0.0, -1.0, 1e300, f64::NAN, 50.0]))
            }
        }
        54 => Op::AssignUnits(r.below(8) as usize, *r.pick(&[0.0, -1.0, 1e12, f64::NAN, 0.5, 1e-12, f64::INFINITY])),
        55 => Op::Custom(row, r.below(5) as u8, 1 + r.below(30) as u8, r.pick(&["", "1e308", "-1", "NaN", "yes", "1/1/262143", "$1e11", "abc", "9999-12-31"]).to_string()),
        56 => Op::Ticket(format!("S{}", r.below(50))),
        57 => Op::Sprint(row, r.chance(70)),
        58 => {
            if r.chance(50) {
                Op::NewSprint
            } else {
                Op::MoveTicket(r.below(10) as usize, r.below(4) as usize, r.below(6) as usize)
            }
        }
        59 => {
            if r.chance(50) {
                Op::StartSprint(r.below(3) as usize, date_val(r))
            } else {
                Op::CompleteSprint(r.below(3) as usize, date_val(r))
            }
        }
        60 => {
            if r.chance(50) {
                Op::Release(format!("v{}", r.below(3)), if r.chance(50) { Some(date_val(r).date()) } else { None })
            } else {
                Op::ReleaseMilestone(r.below(3) as usize)
            }
        }
        61 => match r.below(3) {
            0 => Op::Collapse(row),
            1 => Op::OutlineLevel(if r.chance(30) { None } else { Some(r.below(12) as u32) }),
            _ => Op::Settings(r.below(6) as u8, *r.pick(&[0, 1, -1, 60, 480, 1440, 10080, 1 << 31, i64::MAX])),
        },
        62 => match r.below(5) {
            0 => Op::Fixed(row, *r.pick(&[1e308, -1.0, f64::NAN, 100.0])),
            1 => Op::Priority(row, *r.pick(&[0, 1000, u32::MAX])),
            2 => Op::Inactive(row),
            3 => Op::Milestone(row),
            _ => Op::RoundTrip,
        },
        _ => Op::RoundTrip,
    }
}

fn rows_ok(p: &Project, v: &[usize]) -> Vec<usize> {
    v.iter().copied().filter(|&i| i < p.tasks.len()).collect()
}

/// Apply one op through the public API, the way the UI does (refused edits
/// are rolled back by the caller, so only the panic-freedom matters here).
fn apply(p: &mut Project, op: &Op) {
    use projectfcg::recurring::{RecurEnd, RecurPattern, Recurrence};
    let n = p.tasks.len();
    let row_ok = |r: usize| r < n;
    let s = p.settings.durations;
    let _ = match op {
        Op::Append(name) => {
            p.append_task(name);
            Ok(())
        }
        Op::Insert(at) => {
            p.insert_task(*at, "ins");
            Ok(())
        }
        Op::Delete(v) => {
            p.delete_tasks(&rows_ok(p, v));
            Ok(())
        }
        Op::Set(row, f, text) if row_ok(*row) => p.set_field_text(*row, *f, text),
        Op::Indent(v) => p.indent(&rows_ok(p, v)).map(|_| ()),
        Op::Outdent(v) => p.outdent(&rows_ok(p, v)).map(|_| ()),
        Op::MoveUp(r) if row_ok(*r) => p.move_up(*r).map(|_| ()),
        Op::MoveDown(r) if row_ok(*r) => p.move_down(*r).map(|_| ()),
        Op::Link(a, b, k, lag) if row_ok(*a) && row_ok(*b) => match projectfcg::duration::Lag::parse(lag, &s) {
            Ok(l) => p.add_link(*a, *b, *k, l),
            Err(e) => Err(e),
        },
        Op::LinkTasks(v) => p.link_tasks(&rows_ok(p, v)).map(|_| ()),
        Op::Unlink(v) => {
            p.unlink_tasks(&rows_ok(p, v));
            Ok(())
        }
        Op::Constraint(r, k, dt) if row_ok(*r) => p.set_constraint(*r, *k, *dt),
        // fields the app only sets through a parsed or picked date, which
        // keeps them within 1900–2200 (wilder dates are refused there)
        Op::Deadline(r, dt) if row_ok(*r) => {
            p.tasks[*r].deadline = dt.map(projectfcg::calendar::clamp_datetime);
            Ok(())
        }
        Op::ActualStart(r, dt) if row_ok(*r) => p.set_actual_start(*r, *dt),
        Op::ActualFinish(r, dt) if row_ok(*r) => p.set_actual_finish(*r, *dt),
        Op::ActualDur(r, m) if row_ok(*r) => p.set_actual_duration(*r, *m),
        Op::RemainingDur(r, m) if row_ok(*r) => p.set_remaining_duration(*r, *m),
        Op::Percent(r, v) if row_ok(*r) => p.set_percent_complete(*r, *v),
        Op::Mode(r, manual) if row_ok(*r) => {
            p.set_mode(*r, if *manual { projectfcg::model::TaskMode::Manual } else { projectfcg::model::TaskMode::Auto });
            Ok(())
        }
        Op::TaskType(r, k) if row_ok(*r) => {
            p.tasks[*r].task_type = [TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork][*k as usize % 3];
            Ok(())
        }
        Op::AddCal(name, t) => p.add_base_calendar(name, projectfcg::calendars::Template::ALL[*t as usize % 4]).map(|_| ()),
        Op::TaskCal(r, k, ign) if row_ok(*r) => {
            let uid = k.and_then(|k| p.calendars.get(k).map(|c| c.uid));
            p.set_task_calendar(*r, uid, *ign)
        }
        Op::ProjectCal(seed) => {
            p.calendar = weird_calendar(&mut Rng::new(*seed));
            if std::env::var_os("FUZZ_TRACE").is_some() {
                let c = &p.calendar;
                eprintln!("  calendar {} week {:?} {} exceptions, first {:?} last {:?}", c.name, c.week, c.exceptions.len(), c.exceptions.first(), c.exceptions.last());
            }
            Ok(())
        }
        Op::ResCal(k, seed) => {
            if let Some(res) = p.resources.get_mut(*k) {
                let c = weird_calendar(&mut Rng::new(*seed));
                res.exceptions = c.exceptions;
            }
            Ok(())
        }
        Op::BaseCal(k, seed) => match p.calendars.get(*k).map(|c| (c.uid, c.calendar.name.clone())) {
            Some((uid, name)) => {
                let mut c = weird_calendar(&mut Rng::new(*seed));
                c.name = name;
                p.update_base_calendar(uid, c)
            }
            None => Ok(()),
        },
        Op::DeleteCal(k) => match p.calendars.get(*k).map(|c| c.uid) {
            Some(uid) => p.delete_base_calendar(uid),
            None => Ok(()),
        },
        Op::ProjectStart(t) => {
            p.start = projectfcg::calendar::clamp_datetime(*t);
            Ok(())
        }
        Op::StatusDate(t) => {
            p.status_date = t.map(projectfcg::calendar::clamp_datetime);
            Ok(())
        }
        Op::Recurring(at, pat, every, count, start, dur, by) => {
            let duration = p.parse_duration(dur).unwrap_or_else(|_| projectfcg::duration::Duration::days(1.0, &s));
            let rec = Recurrence {
                pattern: RecurPattern::ALL[*pat as usize % 4],
                every: *every,
                weekdays: [true, false, true, false, true, false, false],
                working_days_only: *count % 2 == 0,
                start: *start,
                end: if *by { RecurEnd::By(start.checked_add_days(chrono::Days::new(*count as u64 % 100_000)).unwrap_or(*start)) } else { RecurEnd::After(*count) },
                duration,
                occurrences: vec![],
            };
            p.insert_recurring(*at, "Rec", rec).map(|_| ())
        }
        Op::Baseline(k, set, rws) => {
            let rws = rws.as_ref().map(|v| rows_ok(p, v));
            if *set {
                p.set_baseline_n(*k, rws.as_deref());
            } else {
                p.clear_baseline_n(*k, rws.as_deref());
            }
            Ok(())
        }
        Op::UpdateProject(t, prop, after) => {
            let mode = if *prop { projectfcg::tracking::UpdateMode::Proportional } else { projectfcg::tracking::UpdateMode::ZeroOrHundred };
            p.update_project(*t, mode, None, *after);
            Ok(())
        }
        Op::Reschedule(t) => {
            p.reschedule_uncompleted(*t, None);
            Ok(())
        }
        Op::MarkOnTrack(v) => p.mark_on_track(&rows_ok(p, v)).map(|_| ()),
        Op::Level(exh, within) => {
            p.settings.level_within_slack = *within;
            if *exh {
                projectfcg::leveling::level_exhaustive(p, None);
            } else {
                projectfcg::leveling::level(p, None);
            }
            Ok(())
        }
        Op::ClearLevel => {
            projectfcg::leveling::clear_leveling(p, None);
            Ok(())
        }
        Op::AddRes(name, t) => {
            p.add_resource(name, [ResourceType::Work, ResourceType::Material, ResourceType::Cost][*t as usize % 3]);
            Ok(())
        }
        Op::DeleteRes(v) => {
            let v: Vec<usize> = v.iter().copied().filter(|&i| i < p.resources.len()).collect();
            p.delete_resources(&v);
            Ok(())
        }
        Op::ResMaxUnits(k, u) => {
            if let Some(r) = p.resources.get_mut(*k) {
                r.max_units = *u;
            }
            Ok(())
        }
        Op::ResRate(k, u) => {
            if let Some(r) = p.resources.get_mut(*k) {
                r.std_rate = *u;
            }
            Ok(())
        }
        Op::AssignUnits(k, u) if *k < p.assignments.len() => p.set_assignment_units(*k, *u),
        Op::Custom(r, kind, num, text) if row_ok(*r) => {
            let f = projectfcg::custom_fields::CustomField::new(projectfcg::custom_fields::FieldKind::ALL[*kind as usize % 5], *num);
            p.set_custom(*r, f, text)
        }
        Op::Ticket(t) => p.add_ticket(t, None).map(|_| ()),
        Op::Sprint(r, on) if row_ok(*r) => p.set_sprint(*r, *on),
        Op::NewSprint => {
            p.new_sprint();
            Ok(())
        }
        Op::MoveTicket(k, sp, col) => {
            let sprints: Vec<u32> = p.tasks.iter().filter(|t| t.is_sprint).map(|t| t.uid).collect();
            match p.tickets.get(*k).map(|t| t.id) {
                Some(id) => p.move_ticket(id, sprints.get(*sp).copied(), *col, None),
                None => Ok(()),
            }
        }
        Op::StartSprint(k, now) => match p.tasks.iter().filter(|t| t.is_sprint).nth(*k).map(|t| t.uid) {
            Some(u) => p.start_sprint(u, "goal", *now),
            None => Ok(()),
        },
        Op::CompleteSprint(k, now) => match p.tasks.iter().filter(|t| t.is_sprint).nth(*k).map(|t| t.uid) {
            Some(u) => p.complete_sprint(u, None, *now).map(|_| ()),
            None => Ok(()),
        },
        Op::Release(name, date) => p.add_release(name, *date).map(|_| ()),
        Op::ReleaseMilestone(k) => match p.board.releases.get(*k).map(|r| r.id) {
            Some(id) => p.add_release_milestone(id).map(|_| ()),
            None => Ok(()),
        },
        Op::Collapse(r) if row_ok(*r) => {
            p.toggle_collapse(*r);
            Ok(())
        }
        Op::OutlineLevel(l) => {
            p.show_outline_level(*l);
            Ok(())
        }
        Op::Settings(k, v) => {
            // values a hand-edited or foreign file can carry
            let st = &mut p.settings;
            match k {
                0 => st.durations.minutes_per_day = *v,
                1 => st.durations.minutes_per_week = *v,
                2 => st.durations.days_per_month = *v,
                3 => st.default_start_minute = (*v).clamp(0, u32::MAX as i64) as u32,
                4 => st.default_finish_minute = (*v).clamp(0, u32::MAX as i64) as u32,
                _ => st.critical_slack_threshold = *v,
            }
            Ok(())
        }
        Op::Fixed(r, v) if row_ok(*r) => {
            p.tasks[*r].fixed_cost = *v;
            Ok(())
        }
        Op::Priority(r, v) if row_ok(*r) => {
            p.tasks[*r].priority = *v;
            Ok(())
        }
        Op::Inactive(r) if row_ok(*r) => {
            p.tasks[*r].active = !p.tasks[*r].active;
            Ok(())
        }
        Op::Milestone(r) if row_ok(*r) => {
            p.tasks[*r].milestone = !p.tasks[*r].milestone;
            Ok(())
        }
        Op::RoundTrip => (|| -> Result<(), String> {
            let j = projectfcg::io::to_json(p)?;
            *p = projectfcg::io::from_json(&j)?;
            let x = projectfcg::mspdi::to_xml(p);
            let _ = projectfcg::mspdi::from_xml(&x)?;
            let _ = projectfcg::jira::export_csv(p);
            let _ = projectfcg::io::to_csv(p);
            Ok(())
        })(),
        _ => Ok(()),
    };
}

/// Ops that write raw values into fields (settings, calendars, dates)
/// stand for what a file can carry rather than what the UI lets you type;
/// they're tagged so findings can say which path reaches them.
fn is_file_level(op: &Op) -> bool {
    matches!(op, Op::Settings(..) | Op::ProjectCal(_) | Op::ResCal(..) | Op::BaseCal(..) | Op::ResMaxUnits(..) | Op::ResRate(..) | Op::Fixed(..) | Op::Priority(..) | Op::ProjectStart(_) | Op::StatusDate(_) | Op::Deadline(..))
}

/// Run a step list on a fresh project, scheduling after each step. Returns
/// the panic (if any).
fn run_ops(ops: &[Op], wd: Option<&Watchdog>, slow: &mut Vec<String>, seed: u64) -> Option<String> {
    run_ops_p(ops, wd, slow, seed).err()
}

fn run_ops_p(ops: &[Op], wd: Option<&Watchdog>, slow: &mut Vec<String>, seed: u64) -> Result<Project, String> {
    let mut p = Project::new(d(2026, 1, 5));
    for (i, op) in ops.iter().enumerate() {
        if let Some(w) = wd {
            w.note(format!("seed {seed} step {i}: {op:?}"));
        }
        let t0 = Instant::now();
        let before = p.clone();
        let r = guarded(|| {
            apply(&mut p, op);
            projectfcg::schedule::schedule(&mut p);
        });
        let el = t0.elapsed().as_secs_f64();
        if std::env::var_os("FUZZ_TRACE").is_some() {
            eprintln!("seed {seed} step {i} {el:.3}s tasks={} {op:?}", p.tasks.len());
        }
        if el > SLOW_STEP_SECS {
            slow.push(format!("seed {seed} step {i} took {el:.1}s: {op:?}"));
        }
        if let Err(e) = r {
            return Err(format!("{e}\n  at step {i}: {op:?}"));
        }
        // a refused op is rolled back by the app; mimic that sometimes
        if p.tasks.len() > 400 {
            p = before;
        }
    }
    Ok(p)
}

fn fmt_steps(ops: &[Op]) -> String {
    ops.iter().map(|o| format!("    {}{o:?}\n", if is_file_level(o) { "[file-level] " } else { "" })).collect()
}

#[test]
fn ops_never_panic() {
    let wd = Watchdog::new("ops_never_panic");
    let mut f = Findings::default();
    TAME.with(|t| *t.borrow_mut() = std::env::var_os("FUZZ_TAME").is_some());
    let cases = 300 * scale();
    let seeds: Vec<u64> = match only_seed() {
        Some(s) => vec![s],
        None => (0..cases).collect(),
    };
    for seed in seeds {
        let mut r = Rng::new(seed ^ 0xA11CE);
        let len = 20 + r.below(60) as usize;
        let mut ops = vec![];
        // grow a little project first so row ops have targets
        for k in 0..3 + r.below(8) {
            ops.push(Op::Append(format!("T{k}")));
        }
        let mut n = ops.len();
        for _ in 0..len {
            let op = gen_op(&mut r, n);
            match &op {
                Op::Append(_) | Op::Insert(_) => n += 1,
                Op::Recurring(..) => n += 5,
                Op::Delete(v) => n = n.saturating_sub(v.len()),
                _ => {}
            }
            ops.push(op);
        }
        wd.start(format!("seed {seed}"));
        let mut slow = vec![];
        let res = run_ops_p(&ops, Some(&wd), &mut slow, seed);
        f.slow.extend(slow);
        match res {
            Err(e) => {
                let sig = signature(&e);
                if !f.by_sig.contains_key(&sig) {
                    wd.note(format!("shrinking seed {seed}"));
                    let fails = |v: &[Op]| run_ops(v, None, &mut vec![], seed);
                    let min = shrink(&ops, &sig, &fails);
                    let err = fails(&min).unwrap_or(e.clone());
                    f.add(sig, || format!("seed {seed}: {err}\n  minimal steps ({}):\n{}", min.len(), fmt_steps(&min)));
                } else {
                    f.add(sig, String::new);
                }
            }
            Ok(p) => check_state(&p, &mut f, &format!("ops seed {seed}")),
        }
        wd.done();
    }
    f.finish("ops");
}

// ---------------------------------------------- campaign 2: app commands

use egui::{pos2, vec2, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use projectfcg::app::commands::{Command, Filter, Timescale};
use projectfcg::app::{MainView, ProjectFcgApp, RowRef};

/// Keep the app's file dialogs and Print away from the real home folder
/// and desktop: a scratch HOME, and no PATH so "open with system" fails.
fn sandbox_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let home = std::env::temp_dir().join("projectfcg-fuzz-home");
        let _ = std::fs::create_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", home.join("no-bin"));
    });
}

struct Ui {
    ctx: egui::Context,
    app: ProjectFcgApp,
    t: f64,
}
impl Ui {
    fn new(p: Project) -> Self {
        sandbox_env();
        let mut app = ProjectFcgApp::new(p);
        app.use_portal = false;
        let mut u = Ui { ctx: egui::Context::default(), app, t: 0.0 };
        u.frame(vec![]);
        u
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360.0, 820.0))),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.frame(ui));
        out.drop_without_applying_deltas();
    }
}

fn filters(r: &mut Rng, app: &ProjectFcgApp) -> Filter {
    match r.below(12) {
        0 => Filter::DateRange(date_val(r).date(), date_val(r).date()),
        1 => Filter::UsingResource(app.project.resources.first().map(|x| x.uid).unwrap_or(r.below(5) as u32)),
        2 => Filter::Custom(app.project.views.filters.first().map(|x| x.id).unwrap_or(r.below(3) as u32)),
        _ => *r.pick(&Filter::ALL),
    }
}

fn random_command(r: &mut Rng, app: &ProjectFcgApp) -> Command {
    use projectfcg::app::bars::PathRole;
    use projectfcg::app::columns::Table;
    use projectfcg::app::filters::GroupBy;
    use projectfcg::app::theme::{Accent, ThemeMode};
    use projectfcg::format::GanttViewKind;
    use Command::*;
    let simple = [
        New, NewFromSample, NewFromAgileSample, Open, Save, SaveAs, ExportCsv, ImportJiraCsv, ExportJiraCsv, ExportXml, PrintDlg,
        CustomFieldsDlg, SavePdf, Options, Quit, Undo, Redo, Cut, Copy, Paste, FillDown, ResetField, EditCell, SelectAll, SelectRow,
        Find, FindNext, GoTo, InsertTask, InsertSummary, InsertMilestone, InsertRecurring, ToggleActive, DeleteTasks, Indent, Outdent,
        MoveUp, MoveDown, LinkTasks, UnlinkTasks, TaskInfo, TaskNotes, SetManual, SetAuto, ToggleNewTaskMode, MarkMilestone,
        AddToTimeline, ScrollToTask, HideSubtasks, ShowSubtasks, ShowAllSubtasks, ProjectInfo, WorkingTime, Calculate, ToggleAutoCalc,
        ZoomIn, ZoomOut, ZoomEntireProject, ZoomSelectedTasks, ScrollProjectStart, ScrollProjectEnd, ScrollLeft, ScrollRight, PageLeft,
        PageRight, ToggleTimeline, ToggleCritical, ToggleSlack, ToggleSummaryRows, ToggleProjectSummary, ToggleOutlineNumber,
        ToggleBarText, ToggleLate, ToggleSlippage, DeleteKey, CopyPicture, ToggleBaselineBars, ToggleMultipleCriticalPaths, ClearTaskPath,
        ToggleRelatedSummaries, DateRangeFilterDlg, UsingResourceFilterDlg, NewFilter, CustomGroupDlg, ToggleInspector, SetBaseline,
        ClearBaseline, BarStylesDlg, FormatBarDlg, TextStylesDlg, GridlinesDlg, LayoutDlg, TimescaleDlg, TextBigger, TextSmaller,
        TextReset, UiZoomIn, UiZoomOut, ToggleDarkMode, AssignResources, ResourceInfo, LevelAll, LevelSelection, ClearLeveling,
        LevelingOptions, NextOverallocation, MarkOnTrack, UpdateTasks, UpdateProject, ProjectStatistics, ToggleProgressLines, NewSprint,
        ToggleSprint, OpenOnBoard, NewTicket, EditTicket, DeleteTicket, TicketToBacklog, NewEpic, ToggleBoardCharts, Shortcuts, About,
    ];
    match r.below(20) {
        0 => SetPercent(*r.pick(&[0, 25, 50, 75, 100, 101, 255])),
        1 => OutlineLevel(if r.chance(20) { None } else { Some(r.below(12) as u32) }),
        2 => SetTimescale(*r.pick(&Timescale::ALL)),
        3 => SetFilter(filters(r, app)),
        4 => TaskPath(*r.pick(&[PathRole::Predecessor, PathRole::DrivingPredecessor, PathRole::Successor, PathRole::DrivenSuccessor])),
        5 => SetHighlight(filters(r, app)),
        6 => SetGroup(if r.chance(70) { *r.pick(&GroupBy::BUILT_IN) } else { GroupBy::Field(*r.pick(projectfcg::app::columns::ColumnId::ALL)) }),
        7 => ToggleGroupRow(r.below(6) as usize),
        8 => EditFilter(app.project.views.filters.first().map(|x| x.id).unwrap_or(r.below(4) as u32)),
        9 => ApplyStylePreset(r.below(projectfcg::format::STYLE_PRESETS.len() as u64) as usize),
        10 => SetBarColor(if r.chance(30) { None } else { Some(projectfcg::format::Rgb(r.below(256) as u8, 0, 255)) }),
        11 => SetUiZoom(*r.pick(&[0.0, -1.0, 0.8, 1.0, 2.5, 100.0, f32::NAN, f32::INFINITY, 1e-6])),
        12 => match r.below(4) {
            0 => SetThemeMode(*r.pick(&ThemeMode::ALL)),
            1 => SetAccent(*r.pick(&Accent::ALL)),
            2 => SetGanttView(*r.pick(&[GanttViewKind::Gantt, GanttViewKind::Tracking, GanttViewKind::Detail, GanttViewKind::Leveling])),
            _ => SetEstimate(*r.pick(&projectfcg::board::Estimate::ALL)),
        },
        13 => ShowView(*r.pick(&[MainView::Gantt, MainView::TaskUsage, MainView::ResourceSheet, MainView::ResourceUsage, MainView::Board, MainView::NetworkDiagram, MainView::Calendar, MainView::Risks])),
        14 => AddResource(*r.pick(&[ResourceType::Work, ResourceType::Material, ResourceType::Cost])),
        15 => SetTable(*r.pick(&[Table::Entry, Table::Schedule, Table::Tracking, Table::Cost, Table::Work, Table::EarnedValue, Table::Variance])),
        16 => SetShownBaseline(r.below(13) as u8),
        _ => *r.pick(&simple),
    }
}

#[derive(Clone, Debug)]
enum AppStep {
    Cmd(Command),
    Select(Vec<usize>, usize, bool),
    Edit(Op),
    Frame(Vec<UiEv>),
    UndoStorm(u8),
}

#[derive(Clone, Debug)]
enum UiEv {
    Click(f32, f32, u8),
    Drag(f32, f32, f32, f32),
    Key(Key, u8),
    Text(String),
    Wheel(f32, f32, u8),
}

const KEYS: &[Key] = &[
    Key::Enter, Key::Escape, Key::Tab, Key::Delete, Key::Backspace, Key::Insert, Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft,
    Key::ArrowRight, Key::PageUp, Key::PageDown, Key::Home, Key::End, Key::F2, Key::F9, Key::F5, Key::F1, Key::Space, Key::A, Key::C,
    Key::V, Key::X, Key::Z, Key::Y, Key::D, Key::F, Key::I, Key::K, Key::Plus, Key::Minus, Key::Num0, Key::F10, Key::F11, Key::F12,
];

fn mods(m: u8) -> Modifiers {
    Modifiers { alt: m & 1 != 0, ctrl: m & 2 != 0, command: m & 2 != 0, shift: m & 4 != 0, ..Default::default() }
}

fn gen_ui_events(r: &mut Rng) -> Vec<UiEv> {
    (0..1 + r.below(6))
        .map(|_| {
            let (x, y) = (r.below(1360) as f32, r.below(820) as f32);
            match r.below(10) {
                0..=3 => UiEv::Click(x, y, (r.below(8) as u8) | if r.chance(10) { 8 } else { 0 } | if r.chance(10) { 16 } else { 0 }),
                4 => UiEv::Drag(x, y, r.below(1360) as f32, r.below(820) as f32),
                5..=7 => UiEv::Key(*r.pick(KEYS), if r.chance(60) { 0 } else { r.below(8) as u8 }),
                8 => UiEv::Text(match r.below(3) {
                    0 => r.pick(DURS).to_string(),
                    1 => r.pick(DATES).to_string(),
                    _ => r.pick(&["x", "1", "-", "名", "\u{0}", "999999999999"]).to_string(),
                }),
                _ => UiEv::Wheel(x, y, r.below(4) as u8),
            }
        })
        .collect()
}

fn play(u: &mut Ui, evs: &[UiEv]) {
    let none = Modifiers::NONE;
    for e in evs {
        match e {
            UiEv::Click(x, y, m) => {
                let p = pos2(*x, *y);
                let md = mods(m & 7);
                let button = if m & 8 != 0 { PointerButton::Secondary } else { PointerButton::Primary };
                u.frame(vec![Event::ModifiersChanged(md), Event::PointerMoved(p)]);
                let reps = if m & 16 != 0 { 2 } else { 1 };
                for _ in 0..reps {
                    u.frame(vec![Event::PointerButton { pos: p, button, pressed: true, modifiers: md }]);
                    u.frame(vec![Event::PointerButton { pos: p, button, pressed: false, modifiers: md }]);
                }
                u.frame(vec![Event::ModifiersChanged(none)]);
            }
            UiEv::Drag(x0, y0, x1, y1) => {
                let (a, b) = (pos2(*x0, *y0), pos2(*x1, *y1));
                u.frame(vec![Event::PointerMoved(a)]);
                u.frame(vec![Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: none }]);
                for k in 1..=4 {
                    u.frame(vec![Event::PointerMoved(a + (b - a) * (k as f32 / 4.0))]);
                }
                u.frame(vec![Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: none }]);
            }
            UiEv::Key(k, m) => {
                let md = mods(*m);
                u.frame(vec![Event::ModifiersChanged(md), Event::Key { key: *k, physical_key: None, pressed: true, repeat: false, modifiers: md }]);
                u.frame(vec![Event::Key { key: *k, physical_key: None, pressed: false, repeat: false, modifiers: md }, Event::ModifiersChanged(none)]);
            }
            UiEv::Text(s) => u.frame(vec![Event::Text(s.clone())]),
            UiEv::Wheel(x, y, m) => {
                let md = mods(*m);
                u.frame(vec![
                    Event::PointerMoved(pos2(*x, *y)),
                    Event::ModifiersChanged(md),
                    Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, if m & 1 == 0 { 3.0 } else { -3.0 }), modifiers: md, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown },
                ]);
                u.frame(vec![Event::ModifiersChanged(none)]);
            }
        }
    }
}

fn base_project(kind: u64) -> Project {
    match kind % 4 {
        0 => projectfcg::app::sample::software_release(),
        1 => projectfcg::app::sample::agile_release_at(dt(2026, 3, 2, 9)),
        2 => common::random_project(kind, &common::Shape { tasks: 25, resources: 3, ..Default::default() }),
        _ => Project::new(d(2026, 1, 5)),
    }
}

fn run_app(base: u64, steps: &[AppStep], wd: Option<&Watchdog>, seed: u64, slow: &mut Vec<String>) -> Option<String> {
    let mut u = match guarded(|| Ui::new(base_project(base))) {
        Ok(u) => u,
        Err(e) => return Some(format!("{e}\n  creating the app")),
    };
    for (i, s) in steps.iter().enumerate() {
        if let Some(w) = wd {
            w.note(format!("seed {seed} step {i}: {s:?}"));
        }
        let t0 = Instant::now();
        let res = guarded(|| {
            let ctx = u.ctx.clone();
            match s {
                AppStep::Cmd(c) => u.app.execute(*c, &ctx),
                AppStep::Select(rows, col, whole) => {
                    let uids: Vec<u32> = rows.iter().filter_map(|&r| u.app.project.tasks.get(r).map(|t| t.uid)).collect();
                    match uids.first() {
                        Some(&f) => u.app.select_single(RowRef::Task(f)),
                        None => u.app.select_single(RowRef::Blank),
                    }
                    u.app.selected = uids;
                    u.app.active_col = *col % (u.app.columns.len() + 1);
                    u.app.row_selected = *whole;
                }
                AppStep::Edit(op) => {
                    let op = op.clone();
                    u.app.edit_project("fuzz", move |p| {
                        apply(p, &op);
                        Ok(())
                    });
                }
                AppStep::Frame(evs) => play(&mut u, evs),
                AppStep::UndoStorm(k) => {
                    for j in 0..*k {
                        u.app.execute(if j % 3 == 2 { Command::Redo } else { Command::Undo }, &ctx);
                    }
                }
            }
            // a frame after every step: dialogs and views draw the new state
            u.frame(vec![]);
        });
        let el = t0.elapsed().as_secs_f64();
        if el > SLOW_STEP_SECS {
            slow.push(format!("app seed {seed} step {i} took {el:.1}s: {s:?}"));
        }
        if let Err(e) = res {
            return Some(format!("{e}\n  at step {i}: {s:?}"));
        }
    }
    None
}

fn gen_app_steps(r: &mut Rng, u_proj: &Project, len: usize) -> Vec<AppStep> {
    let dummy = ProjectFcgApp::new(u_proj.clone());
    let n = u_proj.tasks.len().max(3);
    (0..len)
        .map(|_| match r.below(20) {
            0..=8 => AppStep::Cmd(random_command(r, &dummy)),
            9..=11 => AppStep::Select(rows(r, n + 2), r.below(12) as usize, r.chance(30)),
            12..=14 => AppStep::Edit(gen_op(r, n)),
            15..=18 => AppStep::Frame(gen_ui_events(r)),
            _ => AppStep::UndoStorm(1 + r.below(30) as u8),
        })
        .collect()
}

#[test]
fn app_commands_never_panic() {
    let wd = Watchdog::new("app_commands_never_panic");
    let mut f = Findings::default();
    TAME.with(|t| *t.borrow_mut() = std::env::var_os("FUZZ_WILD_DATES").is_none());
    let cases = 60 * scale();
    let seeds: Vec<u64> = match only_seed() {
        Some(s) => vec![s],
        None => (0..cases).collect(),
    };
    for seed in seeds {
        let mut r = Rng::new(seed ^ 0xB0A7);
        let base = r.below(4);
        let len = 20 + r.below(40) as usize;
        let steps = gen_app_steps(&mut r, &base_project(base), len);
        wd.start(format!("app seed {seed}"));
        let mut slow = vec![];
        let res = run_app(base, &steps, Some(&wd), seed, &mut slow);
        f.slow.extend(slow);
        if let Some(e) = res {
            let sig = signature(&e);
            if !f.by_sig.contains_key(&sig) {
                wd.note(format!("shrinking app seed {seed}"));
                let fails = |v: &[AppStep]| run_app(base, v, None, seed, &mut vec![]);
                let min = shrink(&steps, &sig, &fails);
                let err = fails(&min).unwrap_or(e.clone());
                let list: String = min.iter().map(|s| format!("    {s:?}\n")).collect();
                f.add(sig, || format!("app seed {seed} (base project {base}): {err}\n  minimal steps ({}):\n{list}", min.len()));
            } else {
                f.add(sig, String::new);
            }
        }
        wd.done();
    }
    f.finish("app");
}

// ------------------------------------------------- campaign 3: files

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Json,
    Xml,
    Csv,
}

fn seed_files() -> Vec<(Kind, String)> {
    let mut out = vec![];
    let mut projects = vec![
        projectfcg::app::sample::software_release(),
        projectfcg::app::sample::agile_release_at(dt(2026, 3, 2, 9)),
        common::random_project(7, &common::Shape { tasks: 40, resources: 4, ..Default::default() }),
    ];
    // a project with the rarer features: recurring, calendars, baselines,
    // custom fields, actuals
    {
        let mut p = common::random_project(3, &common::Shape { tasks: 15, resources: 2, ..Default::default() });
        let mut r = Rng::new(99);
        for _ in 0..40 {
            let op = gen_op(&mut r, p.tasks.len());
            if matches!(op, Op::ProjectCal(_) | Op::Settings(..) | Op::ProjectStart(_)) {
                continue;
            }
            let before = p.clone();
            if guarded(|| {
                apply(&mut p, &op);
                projectfcg::schedule::schedule(&mut p);
            })
            .is_err()
            {
                p = before;
            }
        }
        let _ = p.add_base_calendar("Night", projectfcg::calendars::Template::NightShift);
        p.set_baseline_n(0, None);
        p.set_baseline_n(3, None);
        projectfcg::schedule::schedule(&mut p);
        projects.push(p);
    }
    for p in &projects {
        out.push((Kind::Json, projectfcg::io::to_json(p).unwrap()));
        out.push((Kind::Xml, projectfcg::mspdi::to_xml(p)));
        out.push((Kind::Csv, projectfcg::jira::export_csv(p)));
    }
    let fx = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
    for f in ["agile_release_mpxj.xml", "software_release_mpxj.xml"] {
        if let Ok(s) = std::fs::read_to_string(format!("{fx}{f}")) {
            out.push((Kind::Xml, s));
        }
    }
    out
}

const NUMS: &[&str] = &[
    "1e308", "-1", "4294967295", "4294967296", "18446744073709551615", "-9223372036854775808", "9223372036854775807", "0", "-0",
    "2147483648", "1e-308", "NaN", "Infinity", "", "1.5", "65535", "-2147483649", "100000000", "1440", "1441",
];
const DATE_STRS: &[&str] = &[
    "+262143-12-31T23:59:00", "-262144-01-01T00:00:00", "0001-01-01T00:00:00", "9999-12-31T23:59:59", "1900-01-01T00:00:00",
    "2026-02-30T08:00:00", "2026-01-05", "2026-01-05T25:00:00", "", "262142-12-31T08:00:00", "+262142-12-31T23:59:00",
    "2026-01-05T08:00:00.999999999", "-0001-01-01T00:00:00",
];

/// JSON: a few mutations at random places in the parsed tree (mostly
/// keeping the type, so the file still deserializes and reaches the
/// repair and scheduling code), plus semantic damage: colliding uids,
/// links to missing tasks, broken shifts, outline jumps.
fn mutate_json(v: &mut serde_json::Value, r: &mut Rng, _depth: usize) {
    use serde_json::Value as V;
    for _ in 0..1 + r.below(4) {
        if r.chance(35) {
            semantic_json(v, r);
            continue;
        }
        let mut paths = vec![];
        collect_paths(v, String::new(), &mut paths);
        if paths.is_empty() {
            return;
        }
        let path = r.pick(&paths).clone();
        let Some(x) = v.pointer_mut(&path) else { continue };
        *x = match &*x {
            V::Number(n) if n.is_i64() || n.is_u64() => {
                let v: i64 = *r.pick(&[0, -1, 1, 4294967295, 4294967296, 9223372036854775807, -9223372036854775807, 2147483648, 1441, 1440, 9, 100000000, 52704000, -52704000]);
                // unsigned fields mostly stay in u32 range so the file still loads
                let v = if n.is_u64() && r.chance(80) { v.clamp(0, 4294967295) } else { v };
                serde_json::json!(v)
            }
            V::Number(_) => serde_json::json!(*r.pick(&[1e308, -1e308, -1.0, 0.0, 1e-308, 4294967295.0, 1e12, -5.0, 101.0])),
            V::String(s) if s.len() >= 10 && s.as_bytes().get(4) == Some(&b'-') => V::String(r.pick(DATE_STRS).to_string()),
            V::String(_) => V::String(match r.below(3) { 0 => String::new(), 1 => "x".repeat(100_000), _ => "\u{0}名\n".into() }),
            V::Bool(b) => V::Bool(!b),
            V::Array(a) if !a.is_empty() => {
                let mut a = a.clone();
                match r.below(4) {
                    0 => a.clear(),
                    1 => {
                        let x = a[r.below(a.len() as u64) as usize].clone();
                        for _ in 0..1 + r.below(3) {
                            a.push(x.clone());
                        }
                    }
                    2 => {
                        a.remove(r.below(a.len() as u64) as usize);
                    }
                    _ => a.reverse(),
                }
                V::Array(a)
            }
            V::Null => serde_json::json!(*r.pick(&[0i64, 4294967295, -1])),
            other => extreme_for(other, r),
        };
    }
}

fn collect_paths(v: &serde_json::Value, at: String, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                collect_paths(x, format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")), out);
            }
        }
        serde_json::Value::Array(a) => {
            out.push(at.clone());
            for (i, x) in a.iter().enumerate() {
                collect_paths(x, format!("{at}/{i}"), out);
            }
        }
        _ => out.push(at),
    }
}

fn semantic_json(v: &mut serde_json::Value, r: &mut Rng) {
    use serde_json::json;
    let pj = &mut v["project"];
    let nt = pj["tasks"].as_array().map_or(0, |a| a.len());
    let ti = r.below(nt.max(1) as u64) as usize;
    let tj = r.below(nt.max(1) as u64) as usize;
    match r.below(16) {
        0 if nt > 1 => pj["tasks"][ti]["uid"] = pj["tasks"][tj]["uid"].clone(),
        1 if nt > 0 => pj["tasks"][ti]["predecessors"] = json!([{ "pred": 4000000000u32, "kind": "FS", "lag": { "Time": { "minutes": 0, "unit": "Day", "elapsed": false } } }]),
        2 if nt > 0 => pj["tasks"][ti]["outline_level"] = json!(*r.pick(&[0u32, 9, 100, 4294967295])),
        3 if nt > 0 => pj["tasks"][ti]["predecessors"] = json!([{ "pred": pj["tasks"][ti]["uid"].clone(), "kind": "SS", "lag": { "Percent": 1e308 } }]),
        4 => pj["calendar"]["week"][r.below(7) as usize] = json!([{ "start": *r.pick(&[1000u32, 0, 1440, 5000, 4294967295]), "end": *r.pick(&[10u32, 0, 1441, 4294967295]) }]),
        5 => pj["calendar"]["exceptions"] = json!([{ "name": "", "start": "2026-05-01", "end": "2026-01-01", "shifts": [{ "start": 600, "end": 100 }] }]),
        6 => pj["settings"]["durations"]["minutes_per_day"] = json!(*r.pick(&[0i64, -1, 9223372036854775807, 1])),
        7 => pj["settings"]["durations"]["days_per_month"] = json!(*r.pick(&[0i64, -1, 9223372036854775807])),
        8 => pj["next_uid"] = json!(*r.pick(&[0u32, 1, 4294967295])),
        9 if nt > 0 => pj["tasks"][ti]["uid"] = json!(4294967295u32),
        10 => {
            if let Some(a) = pj["assignments"].as_array_mut() {
                if let Some(x) = a.first().cloned() {
                    let mut y = x.clone();
                    y["task"] = json!(*r.pick(&[4000000000u32, 0]));
                    a.push(y);
                    a.push(x);
                }
            }
        }
        11 if nt > 0 => pj["tasks"][ti]["calendar"] = pj["tasks"][tj]["uid"].clone(),
        12 if nt > 0 => pj["tasks"][ti]["leveling_delay"] = json!(*r.pick(&[-9223372036854775807i64, 9223372036854775807, -52704000])),
        13 if nt > 0 => pj["tasks"][ti]["duration"]["minutes"] = json!(*r.pick(&[-9223372036854775807i64, 9223372036854775807, -1, 52704001])),
        14 if nt > 0 => {
            let lag = *r.pick(&[-9223372036854775807i64, 9223372036854775807, 52704000 * 4]);
            pj["tasks"][ti]["predecessors"] = json!([{ "pred": pj["tasks"][tj]["uid"].clone(), "kind": "FS", "lag": { "Time": { "minutes": lag, "unit": "Day", "elapsed": r.chance(50) } } }]);
        }
        _ => {
            if let Some(t) = pj["tickets"].as_array_mut() {
                if let Some(x) = t.first().cloned() {
                    let mut y = x.clone();
                    y["parent"] = x["id"].clone();
                    y["column"] = json!(4294967295u32);
                    t.push(y);
                }
            }
        }
    }
}

fn extreme_for(v: &serde_json::Value, r: &mut Rng) -> serde_json::Value {
    use serde_json::Value as V;
    match (v, r.below(4)) {
        (_, 0) => V::Null,
        (_, 1) => V::String(String::new()),
        _ => serde_json::json!(1e308),
    }
}

/// XML/CSV: text-level mutations of leaf values and blocks.
fn mutate_xml(s: &str, r: &mut Rng) -> String {
    let mut out = s.to_string();
    for _ in 0..1 + r.below(8) {
        match r.below(6) {
            0..=2 => {
                // replace a random leaf value  <Tag>value</Tag>
                let leaves: Vec<(usize, usize)> = leaf_spans(&out);
                if leaves.is_empty() {
                    continue;
                }
                let (a, b) = *r.pick(&leaves);
                let old = &out[a..b];
                let new = if old.contains('T') && old.len() >= 10 && old.as_bytes()[4] == b'-' {
                    r.pick(DATE_STRS).to_string()
                } else if old.starts_with("PT") {
                    r.pick(&["PT99999999H0M0S", "PT-8H0M0S", "PT0H0M0S", "P", "PT", "PT1e308H", "PT9223372036854775807M", "P99999999DT0H"]).to_string()
                } else {
                    r.pick(NUMS).to_string()
                };
                out.replace_range(a..b, &new);
            }
            3 => {
                // duplicate or delete a block
                let tag = *r.pick(&["Task", "Resource", "Assignment", "Calendar", "PredecessorLink", "Exception", "WeekDay", "WorkingTime"]);
                let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
                let starts: Vec<usize> = out.match_indices(&open).map(|(i, _)| i).collect();
                if starts.is_empty() {
                    continue;
                }
                let a = *r.pick(&starts);
                if let Some(rel) = out[a..].find(&close) {
                    let b = a + rel + close.len();
                    let block = out[a..b].to_string();
                    if r.chance(50) {
                        out.insert_str(b, &block);
                    } else {
                        out.replace_range(a..b, "");
                    }
                }
            }
            4 => {
                let tag = *r.pick(&["OutlineLevel", "UID", "PredecessorUID", "BaseCalendarUID", "CalendarUID", "TaskUID", "ResourceUID", "Type", "ConstraintType", "DayType"]);
                let open = format!("<{tag}>");
                let starts: Vec<usize> = out.match_indices(&open).map(|(i, _)| i + open.len()).collect();
                if starts.is_empty() {
                    continue;
                }
                let a = *r.pick(&starts);
                if let Some(rel) = out[a..].find('<') {
                    let v = r.pick(&["1", "9", "0", "-1", "4294967295", "2", "1000000", "99"]).to_string();
                    out.replace_range(a..a + rel, &v);
                }
            }
            _ => {
                if r.chance(50) {
                    out = mutate_bytes(&out, r);
                }
            }
        }
    }
    out
}

fn leaf_spans(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut v = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'>' {
            let a = i + 1;
            let mut j = a;
            while j < b.len() && b[j] != b'<' {
                j += 1;
            }
            if j + 1 < b.len() && b[j + 1] == b'/' && j > a && j - a < 64 {
                v.push((a, j));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    v
}

fn mutate_csv(s: &str, r: &mut Rng) -> String {
    let rows = projectfcg::jira::parse_csv(s);
    if rows.is_empty() {
        return s.to_string();
    }
    let mut rows = rows;
    for _ in 0..1 + r.below(10) {
        let i = r.below(rows.len() as u64) as usize;
        if rows[i].is_empty() {
            continue;
        }
        let j = r.below(rows[i].len() as u64) as usize;
        rows[i][j] = match r.below(8) {
            0 => r.pick(NUMS).to_string(),
            1 => r.pick(&["31/Dec/62 9:00 AM", "01/Jan/00 12:00 AM", "1/1/262143", "9999-12-31 23:59", "31/Feb/26 9:00 AM", "0/0/0", "32/Xyz/26 99:99 PM"]).to_string(),
            2 => rows[0][j].clone(),
            3 => "\"".into(),
            4 => "x".repeat(70_000),
            5 => rows.get(1).and_then(|r1| r1.get(j)).cloned().unwrap_or_default(),
            6 => r.pick(&["Epic", "Sub-task", "Story", "Bug", "Blocks", "is blocked by", "APP-1", "APP-99999999999999999999", "-1"]).to_string(),
            _ => String::new(),
        };
    }
    if r.chance(20) && rows.len() > 1 {
        let x = rows[1].clone();
        for _ in 0..r.below(30) {
            rows.push(x.clone());
        }
    }
    if r.chance(10) {
        rows[0].push("Sprint".into());
        rows[0].push("Parent".into());
    }
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row.iter().map(|c| if c.contains([',', '"', '\n']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.clone() }).collect();
        out += &cells.join(",");
        out += "\n";
    }
    if r.chance(15) {
        out = mutate_bytes(&out, r);
    }
    out
}

fn mutate_bytes(s: &str, r: &mut Rng) -> String {
    let mut b = s.as_bytes().to_vec();
    match r.below(3) {
        0 => {
            let n = r.below(b.len() as u64 + 1) as usize;
            b.truncate(n);
        }
        1 => {
            for _ in 0..1 + r.below(8) {
                if b.is_empty() {
                    break;
                }
                let i = r.below(b.len() as u64) as usize;
                b[i] ^= 1 << r.below(8);
            }
        }
        _ => {
            if !b.is_empty() {
                let i = r.below(b.len() as u64) as usize;
                let j = (i + r.below(200) as usize).min(b.len());
                let chunk = b[i..j].to_vec();
                b.splice(i..i, chunk);
            }
        }
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// Load a file of a kind and push the result through everything a user
/// would do next: schedule, save, export, print, a few frames.
fn exercise_file(kind: Kind, text: &str, frames: bool, f: &mut Findings, ctx: &str) -> Option<String> {
    let mut why = String::new();
    let loaded = guarded(|| {
        let r = match kind {
            Kind::Json => projectfcg::io::from_json(text),
            Kind::Xml => projectfcg::mspdi::from_xml(text).map(|(p, _)| p),
            Kind::Csv => {
                let mut p = projectfcg::app::sample::agile_release_at(dt(2026, 3, 2, 9));
                let opts = projectfcg::jira::ImportOptions { create_sprints: true, create_people: true };
                projectfcg::jira::import_csv(&mut p, text, &opts).map(|_| p)
            }
        };
        match r {
            Ok(p) => Some(p),
            Err(e) => {
                why = e.chars().filter(|c| !c.is_ascii_digit()).take(70).collect();
                None
            }
        }
    });
    let p = match loaded {
        Err(e) => return Some(format!("{e}\n  while loading")),
        Ok(None) => {
            if std::env::var_os("FUZZ_WHY").is_some() {
                f.broken(&format!("(stat) {kind:?} rejected: {why}"), String::new());
            } else {
                f.broken(&format!("(stat) {kind:?} rejected"), String::new());
            }
            return None;
        }
        Ok(Some(p)) => {
            f.broken(&format!("(stat) {kind:?} loaded"), String::new());
            p
        }
    };
    let res = guarded(|| {
        let mut p = p;
        projectfcg::schedule::schedule(&mut p);
        let j = projectfcg::io::to_json(&p).unwrap_or_default();
        if let Ok(q) = projectfcg::io::from_json(&j) {
            let _ = projectfcg::mspdi::to_xml(&q);
        }
        let _ = projectfcg::mspdi::to_xml(&p);
        let _ = projectfcg::jira::export_csv(&p);
        let _ = projectfcg::io::to_csv(&p);
        let _ = projectfcg::leveling::level(&mut p.clone(), None);
        let app = ProjectFcgApp::new(p.clone());
        let _ = projectfcg::app::print::print_pdf(&app, &projectfcg::app::print::PrintOptions::default());
        p
    });
    let p = match res {
        Err(e) => return Some(format!("{e}\n  after loading (schedule/save/export/print)")),
        Ok(p) => p,
    };
    check_state(&p, f, ctx);
    if frames {
        let r = guarded(|| {
            let mut u = Ui::new(p);
            let ctx = u.ctx.clone();
            for v in [MainView::Gantt, MainView::TaskUsage, MainView::ResourceUsage, MainView::Board, MainView::ResourceSheet, MainView::NetworkDiagram, MainView::Calendar, MainView::Risks] {
                u.app.execute(Command::ShowView(v), &ctx);
                u.frame(vec![]);
            }
            u.app.execute(Command::ShowView(MainView::Gantt), &ctx);
            u.app.execute(Command::ZoomEntireProject, &ctx);
            u.frame(vec![]);
            u.app.execute(Command::SetTimescale(Timescale::Years), &ctx);
            u.frame(vec![]);
            u.app.execute(Command::LevelAll, &ctx);
            u.frame(vec![]);
            u.app.execute(Command::ProjectStatistics, &ctx);
            u.frame(vec![]);
        });
        if let Err(e) = r {
            return Some(format!("{e}\n  in app frames after loading"));
        }
    }
    None
}

#[test]
fn files_never_panic() {
    let wd = Watchdog::new("files_never_panic");
    let mut f = Findings::default();
    let seeds = guarded(seed_files).expect("seed files build");
    let cases = 600 * scale();
    let list: Vec<u64> = match only_seed() {
        Some(s) => vec![s],
        None => (0..cases).collect(),
    };
    for seed in list {
        let mut r = Rng::new(seed ^ 0xF11E);
        let (kind, base) = &seeds[r.below(seeds.len() as u64) as usize];
        let text = match (kind, r.below(10)) {
            (_, 0) => mutate_bytes(base, &mut r),
            (Kind::Json, _) => {
                let mut v: serde_json::Value = serde_json::from_str(base).unwrap();
                mutate_json(&mut v, &mut r, 0);
                serde_json::to_string(&v).unwrap()
            }
            (Kind::Xml, _) => mutate_xml(base, &mut r),
            (Kind::Csv, _) => mutate_csv(base, &mut r),
        };
        let frames = seed % 25 == 0;
        wd.start(format!("files seed {seed} ({kind:?})"));
        let t0 = Instant::now();
        let res = exercise_file(*kind, &text, frames, &mut f, &format!("files seed {seed} {kind:?}"));
        let el = t0.elapsed().as_secs_f64();
        if el > 5.0 {
            f.slow.push(format!("files seed {seed} ({kind:?}) took {el:.1}s"));
        }
        if let Some(e) = res {
            let sig = signature(&e);
            let path = std::env::temp_dir().join(format!("projectfcg-fuzz-{seed}.{}", match kind { Kind::Json => "pfcg", Kind::Xml => "xml", Kind::Csv => "csv" }));
            let new = f.add(sig.clone(), || format!("files seed {seed} ({kind:?}), input saved to {}: {e}", path.display()));
            if new {
                let _ = std::fs::write(&path, &text);
            }
        }
        wd.done();
    }
    // truncation at every N bytes of each seed file
    for (k, (kind, base)) in seeds.iter().enumerate() {
        let step = (base.len() / (40 * scale() as usize)).max(1);
        let mut at = 0;
        while at < base.len() {
            let mut cut = at;
            while !base.is_char_boundary(cut) {
                cut += 1;
            }
            wd.start(format!("truncation of seed file {k} at {cut}"));
            if let Some(e) = exercise_file(*kind, &base[..cut], false, &mut f, "truncation") {
                f.add(signature(&e), || format!("seed file {k} ({kind:?}) truncated at byte {cut}: {e}"));
            }
            wd.done();
            at += step;
        }
    }
    f.finish("files");
}

// --------------------------------------- campaign 4: calendar math, hangs

#[derive(Clone, Debug)]
enum CalOp {
    Add(NaiveDateTime, i64),
    Sub(NaiveDateTime, i64),
    Next(NaiveDateTime),
    Prev(NaiveDateTime),
    Between(NaiveDateTime, NaiveDateTime),
    Intersect(u64),
}

fn cal_minutes(r: &mut Rng) -> i64 {
    *r.pick(&[0, 1, -1, 480, 2400, 52_704_000, -52_704_000, 210_816_000, i64::MAX, i64::MIN + 1, 1 << 40, 99_999_999])
}

#[test]
fn calendar_math_never_panics_or_hangs() {
    let wd = Watchdog::new("calendar_math");
    let mut f = Findings::default();
    let cases = 250 * scale();
    let list: Vec<u64> = match only_seed() {
        Some(s) => vec![s],
        None => (0..cases).collect(),
    };
    for seed in list {
        let mut r = Rng::new(seed ^ 0xCA1);
        let cal = weird_calendar(&mut r);
        let ops: Vec<CalOp> = (0..12)
            .map(|_| match r.below(6) {
                0 => CalOp::Add(date_val(&mut r), cal_minutes(&mut r)),
                1 => CalOp::Sub(date_val(&mut r), cal_minutes(&mut r)),
                2 => CalOp::Next(date_val(&mut r)),
                3 => CalOp::Prev(date_val(&mut r)),
                4 => CalOp::Between(date_val(&mut r), date_val(&mut r)),
                _ => CalOp::Intersect(r.next()),
            })
            .collect();
        for op in ops {
            wd.start(format!("calendar seed {seed}: {op:?} on {cal:?}"));
            let t0 = Instant::now();
            let res = guarded(|| match op {
                CalOp::Add(t, m) => drop(cal.add_work(t, m)),
                CalOp::Sub(t, m) => drop(cal.sub_work(t, m)),
                CalOp::Next(t) => drop(cal.next_working_start(t)),
                CalOp::Prev(t) => drop(cal.prev_working_end(t)),
                CalOp::Between(a, b) => drop(cal.work_between(a, b)),
                CalOp::Intersect(s) => {
                    let other = weird_calendar(&mut Rng::new(s));
                    let x = cal.intersect(&other);
                    let t = dt(2026, 1, 5, 8);
                    let _ = x.add_work(t, 4800);
                    let _ = x.work_between(t, dt(2030, 1, 1, 8));
                }
            });
            let el = t0.elapsed().as_secs_f64();
            if el > SLOW_STEP_SECS {
                f.slow.push(format!("calendar seed {seed} {el:.1}s: {op:?} (calendar {} week {:?}, {} exceptions)", cal.name, cal.week, cal.exceptions.len()));
            }
            if let Err(e) = res {
                let sig = signature(&e);
                let cal_desc = format!("week {:?}, exceptions {:?}", cal.week, cal.exceptions.iter().map(|x| (x.start, x.end, x.shifts.clone())).collect::<Vec<_>>());
                f.add(sig, || format!("calendar seed {seed}: {e}\n  op {op:?}\n  calendar {} {}", cal.name, cal_desc.chars().take(400).collect::<String>()));
            }
            wd.done();
        }
    }
    f.finish("calendar");
}
