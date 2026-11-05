//! Stability: keep the window alive and the user's work safe when
//! something goes wrong.
//!
//! * An edit that panics is undone (`edit_project` restores the copy it
//!   took before the edit) and reported; the plan is as it was.
//! * A frame that panics is caught. The first time, transient state
//!   (open dialogs, an edit in a cell, a bar being dragged) is dropped. If
//!   the next frame panics too, the last plan that drew cleanly is
//!   restored and the view goes back to a plain Gantt Chart. If it keeps
//!   panicking after that, a recovery copy is written and the panic is
//!   allowed to end the program.
//! * AutoRecover: unsaved work is copied to the recovery folder every few
//!   minutes (File ▸ Options), from a background thread. After a crash,
//!   power cut or kill, the next start offers the copies back.
//!
//! Crash reports go to `<state dir>/crash-reports/` and say what failed
//! and where, not what the plan contains.

use super::{MainView, ProjectFcgApp};
use crate::model::Project;
use crate::recovery::{self, Orphan, Recovery, RecoveryMeta};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// After this many panicking frames in a row, give up.
pub const GIVE_UP_AFTER: u32 = 4;

enum Job {
    Write(Box<Project>, RecoveryMeta),
    Remove,
}

/// AutoRecover's writer: one background thread per window that writes
/// copies, removes them, and keeps the heartbeat fresh, in order.
struct Writer {
    tx: mpsc::Sender<Job>,
    closed: Arc<AtomicBool>,
}

impl Writer {
    fn start(rec: Recovery, session: String) -> Option<Writer> {
        let (tx, rx) = mpsc::channel::<Job>();
        let closed = Arc::new(AtomicBool::new(false));
        let c2 = closed.clone();
        std::thread::Builder::new()
            .name("autorecover".into())
            .spawn(move || loop {
                match rx.recv_timeout(recovery::HEARTBEAT_EVERY) {
                    Ok(Job::Write(p, meta)) => {
                        if c2.load(Ordering::SeqCst) {
                            continue;
                        }
                        if let Ok(json) = crate::io::to_json(&p) {
                            let _ = rec.write(&session, &json, &meta);
                        }
                        // closed while writing: don't leave it behind
                        if c2.load(Ordering::SeqCst) {
                            rec.remove(&session);
                        }
                    }
                    Ok(Job::Remove) => rec.remove(&session),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if !c2.load(Ordering::SeqCst) {
                            let _ = rec.touch(&session);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            })
            .ok()?;
        Some(Writer { tx, closed })
    }
}

/// Shown after a caught panic.
#[derive(Clone, Debug)]
pub struct CrashNotice {
    pub message: String,
    pub report: Option<PathBuf>,
}

#[derive(Default)]
pub struct Safety {
    /// Where recovery copies go (None: AutoRecover off, as in tests).
    pub recovery: Option<Recovery>,
    /// Where crash reports go.
    pub state_dir: Option<PathBuf>,
    pub session: String,
    writer: Option<Writer>,
    /// History generation of the last copy written, or of the plan when
    /// it was last clean.
    saved_gen: Option<u64>,
    /// When the plan first changed after the last copy.
    changed_at: Option<Instant>,
    /// A copy is on disk (removed again once the plan is saved).
    has_copy: bool,
    /// The last plan that drew without a panic, and its generation.
    last_good: Option<(u64, Box<Project>)>,
    pub consecutive_panics: u32,
    pub notice: Option<CrashNotice>,
    /// Recovery copies from windows that didn't close normally.
    pub offers: Vec<Orphan>,
    /// Tests: make the next N frames panic.
    #[doc(hidden)]
    pub inject_frame_panics: u32,
    /// Tests: AutoRecover interval instead of the preference's minutes.
    #[doc(hidden)]
    pub interval_override: Option<Duration>,
}

impl Safety {
    /// A different plan is now open.
    pub fn reset_for_new_plan(&mut self) {
        self.saved_gen = None;
        self.changed_at = None;
        self.last_good = None;
        self.consecutive_panics = 0;
    }

    /// Turn on AutoRecover and crash reports for a real window.
    pub fn enable(&mut self, recovery: Recovery, state_dir: PathBuf) {
        self.session = recovery::new_session_id();
        self.offers = recovery.orphans(std::time::SystemTime::now());
        self.writer = Writer::start(recovery.clone(), self.session.clone());
        self.recovery = Some(recovery);
        self.state_dir = Some(state_dir);
    }
}

impl ProjectFcgApp {
    /// One frame, with panics caught (what the window runs).
    pub fn safe_frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if self.safety.inject_frame_panics > 0 {
                self.safety.inject_frame_panics -= 1;
                panic!("injected test panic");
            }
            self.frame(ui);
        }));
        match r {
            Ok(()) => {
                self.safety.consecutive_panics = 0;
                let g = self.history.generation();
                if self.safety.last_good.as_ref().is_none_or(|(lg, _)| *lg != g) {
                    self.safety.last_good = Some((g, Box::new(self.project.clone())));
                }
            }
            Err(payload) => self.after_frame_panic(payload, &ctx),
        }
        // the notices are drawn outside the frame, so they show even when
        // the frame itself keeps failing
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.safety_windows(&ctx)));
        self.autorecover_tick(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && (self.allow_close || !self.dirty) {
            self.session_closing();
        }
    }

    fn panic_context(&self, what: &str) -> String {
        format!(
            "while: {what}\nview: {}\nlast status: {}\ntasks: {}, resources: {}, assignments: {}\npanics in a row: {}",
            self.view.main.label(),
            self.status,
            self.project.tasks.len(),
            self.project.resources.len(),
            self.project.assignments.len(),
            self.safety.consecutive_panics
        )
    }

    /// Write a crash report for a caught panic. Returns its path.
    pub fn report_panic(&self, what: &str, payload: &(dyn std::any::Any + Send)) -> Option<PathBuf> {
        let text = recovery::take_panic_text(payload);
        let dir = self.safety.state_dir.as_ref()?;
        recovery::write_crash_report(dir, &self.panic_context(what), &text)
    }

    fn after_frame_panic(&mut self, payload: Box<dyn std::any::Any + Send>, ctx: &egui::Context) {
        self.safety.consecutive_panics += 1;
        let n = self.safety.consecutive_panics;
        let report = self.report_panic("drawing the window", &*payload);
        // first, keep the work as it is now
        self.write_recovery_now();
        // drop whatever was half-done
        self.edit = None;
        self.gantt_state.drag = None;
        self.dialogs = Default::default();
        self.file_browser = None;
        self.pending = None;
        self.deferred_action = None;
        let mut message = "ProjectFCG ran into an internal error and closed what was open. Your plan is unchanged.".to_string();
        if n >= 2 {
            // it happens again: go back to the last plan that drew, in a
            // plain view
            if let Some((_, good)) = self.safety.last_good.clone() {
                if self.project != *good {
                    self.project = *good;
                    self.history.clear();
                    self.dirty = true;
                    message = "ProjectFCG ran into the same internal error again and went back to the plan as it was just before. Undo history was cleared.".into();
                }
            }
            self.view.main = MainView::Gantt;
            self.filter = super::commands::Filter::AllTasks;
            self.view.highlight = super::commands::Filter::AllTasks;
            self.group = Default::default();
            self.scroll_y = 0.0;
            self.select_single(super::RowRef::Blank);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.recalc()));
        }
        if n >= GIVE_UP_AFTER {
            std::panic::resume_unwind(payload);
        }
        if self.safety.recovery.is_some() {
            message.push_str(" A recovery copy was saved too, just in case.");
        }
        self.error(message.clone());
        self.safety.notice = Some(CrashNotice { message, report });
        ctx.request_repaint();
    }

    fn recovery_meta(&self) -> RecoveryMeta {
        let title = match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None if !self.project.title.trim().is_empty() => self.project.title.trim().to_string(),
            None => "Unsaved project".into(),
        };
        RecoveryMeta { original: self.path.clone(), title, saved_at: chrono::Local::now().naive_local(), app_version: env!("CARGO_PKG_VERSION").into() }
    }

    /// Write a recovery copy now, on this thread (after a panic: the
    /// process may not live long enough for the background writer).
    pub fn write_recovery_now(&mut self) -> bool {
        let Some(rec) = self.safety.recovery.clone() else { return false };
        if !self.dirty {
            return false;
        }
        let meta = self.recovery_meta();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::io::to_json(&self.project)))
            .ok()
            .and_then(|r| r.ok())
            .is_some_and(|json| rec.write(&self.safety.session, &json, &meta).is_ok());
        if ok {
            self.safety.has_copy = true;
            self.safety.saved_gen = Some(self.history.generation());
            self.safety.changed_at = None;
        }
        ok
    }

    /// Every frame: copy unsaved work to the recovery folder once it has
    /// been unsaved for the AutoRecover interval; remove the copy once the
    /// plan is saved.
    pub fn autorecover_tick(&mut self, ctx: &egui::Context) {
        let Some(writer) = &self.safety.writer else { return };
        let g = self.history.generation();
        if !self.dirty {
            if self.safety.has_copy {
                let _ = writer.tx.send(Job::Remove);
                self.safety.has_copy = false;
            }
            self.safety.saved_gen = Some(g);
            self.safety.changed_at = None;
            return;
        }
        let minutes = self.prefs.autorecover_minutes;
        if (minutes == 0 && self.safety.interval_override.is_none()) || self.safety.saved_gen == Some(g) {
            return;
        }
        let interval = self.safety.interval_override.unwrap_or(Duration::from_secs(u64::from(minutes) * 60));
        let since = *self.safety.changed_at.get_or_insert_with(Instant::now);
        let waited = since.elapsed();
        if waited >= interval {
            let job = Job::Write(Box::new(self.project.clone()), self.recovery_meta());
            if writer.tx.send(job).is_ok() {
                self.safety.has_copy = true;
                self.safety.saved_gen = Some(g);
                self.safety.changed_at = None;
            }
        } else {
            // make sure a frame comes when it's due, even if idle
            ctx.request_repaint_after(interval - waited);
        }
    }

    /// The window is closing normally: nothing to recover.
    pub fn session_closing(&mut self) {
        if let Some(w) = &self.safety.writer {
            w.closed.store(true, Ordering::SeqCst);
        }
        if let Some(rec) = &self.safety.recovery {
            rec.remove(&self.safety.session);
        }
        self.safety.has_copy = false;
    }

    /// Open a recovery copy offered at start. It opens as a new, unsaved
    /// plan, so saving it never overwrites a file without asking.
    pub fn open_recovered(&mut self, i: usize) {
        if i >= self.safety.offers.len() {
            return;
        }
        let o = self.safety.offers.remove(i);
        let Some(rec) = self.safety.recovery.clone() else { return };
        match rec.load(&o) {
            Ok(p) => {
                self.replace_project(p, None);
                self.dirty = true;
                rec.remove(&o.session);
                let from = o.meta.original.as_ref().map(|p| format!(" of {}", p.display())).unwrap_or_default();
                self.set_status(&format!("Recovered “{}”{from}, autosaved {}. Save it to keep it.", o.meta.title, o.meta.saved_at.format("%b %-d %-I:%M %p")));
            }
            Err(e) => self.error(format!("Couldn't open the recovery copy: {e}")),
        }
    }

    pub fn discard_recovered(&mut self, i: usize) {
        if i >= self.safety.offers.len() {
            return;
        }
        let o = self.safety.offers.remove(i);
        if let Some(rec) = &self.safety.recovery {
            rec.remove(&o.session);
        }
    }

    fn safety_windows(&mut self, ctx: &egui::Context) {
        if let Some(n) = self.safety.notice.clone() {
            let mut open = true;
            let mut close = false;
            egui::Window::new("Something went wrong")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
                .show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(&n.message);
                    if let Some(r) = &n.report {
                        ui.add_space(6.0);
                        ui.label("A report of what failed (not your plan's contents) was saved to:");
                        ui.monospace(r.display().to_string());
                        ui.horizontal(|ui| {
                            if ui.button("Copy path").clicked() {
                                ui.ctx().copy_text(r.display().to_string());
                            }
                            if ui.button("Open folder").clicked() {
                                if let Some(d) = r.parent() {
                                    let _ = super::print::open_with_system(d);
                                }
                            }
                        });
                        ui.label("Attaching it to an issue on GitHub helps get it fixed.");
                    }
                    ui.add_space(6.0);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                });
            if !open || close {
                self.safety.notice = None;
            }
        }
        if !self.safety.offers.is_empty() {
            let mut action: Option<(usize, bool)> = None;
            let mut later = false;
            egui::Window::new("Recover unsaved work")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
                .show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    ui.label("ProjectFCG didn't close normally last time. AutoRecover kept a copy of the unsaved work:");
                    ui.add_space(6.0);
                    egui::Grid::new("recover-list").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                        for (i, o) in self.safety.offers.iter().enumerate() {
                            ui.vertical(|ui| {
                                ui.strong(&o.meta.title);
                                let from = o.meta.original.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "never saved".into());
                                ui.small(format!("{from} · autosaved {}", o.meta.saved_at.format("%a %b %-d, %-I:%M %p")));
                                if let Some(orig) = &o.meta.original {
                                    let newer = std::fs::metadata(orig)
                                        .and_then(|m| m.modified())
                                        .ok()
                                        .map(|t| chrono::DateTime::<chrono::Local>::from(t).naive_local() > o.meta.saved_at)
                                        .unwrap_or(false);
                                    if newer {
                                        ui.small("The file was saved after this copy was made.");
                                    }
                                }
                            });
                            if ui.button("Open").clicked() {
                                action = Some((i, true));
                            }
                            if ui.button("Delete").clicked() {
                                action = Some((i, false));
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    if ui.button("Decide later").on_hover_text("Keep the copies; you'll be asked again next time").clicked() {
                        later = true;
                    }
                });
            match action {
                Some((i, true)) => self.open_recovered(i),
                Some((i, false)) => self.discard_recovered(i),
                None if later => self.safety.offers.clear(),
                None => {}
            }
        }
    }
}
