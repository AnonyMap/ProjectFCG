//! Long operations run off the UI thread so the window never stops
//! responding (and the desktop never offers to kill it): resource
//! leveling of large plans. The job works on a copy; when it finishes, the
//! result is applied as one undoable edit. Cancel throws the copy away.

use super::ProjectFcgApp;
use crate::leveling::{Control, LevelResult};
use crate::model::Project;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use web_time::Instant;

/// Plans at least this big level in the background.
pub const BACKGROUND_TASKS: usize = 1500;
/// …or this big when they have ALAP tasks (which make leveling slower).
pub const BACKGROUND_TASKS_ALAP: usize = 300;

pub struct LevelJob {
    pub ctl: Arc<Control>,
    pub started: Instant,
    handle: Option<std::thread::JoinHandle<Option<(Project, LevelResult)>>>,
}

impl ProjectFcgApp {
    /// Should leveling this plan run in the background?
    pub fn level_in_background(&self) -> bool {
        let n = self.project.tasks.len();
        let alap = self.project.tasks.iter().any(|t| t.constraint == crate::model::ConstraintType::ALAP);
        n >= BACKGROUND_TASKS || (alap && n >= BACKGROUND_TASKS_ALAP)
    }

    /// Start leveling a copy of the plan on another thread.
    pub fn start_level_job(&mut self, rows: Option<Vec<usize>>) {
        let ctl = Arc::new(Control::default());
        let c2 = ctl.clone();
        let mut copy = self.project.clone();
        let handle = std::thread::Builder::new().name("leveling".into()).spawn(move || {
            let r = crate::leveling::level_controlled(&mut copy, rows.as_deref(), &c2)?;
            Some((copy, r))
        });
        match handle {
            Ok(h) => self.leveling_job = Some(LevelJob { ctl, started: Instant::now(), handle: Some(h) }),
            Err(e) => self.error(format!("Couldn't start leveling: {e}")),
        }
    }

    pub fn cancel_level_job(&mut self) {
        if let Some(j) = &self.leveling_job {
            j.ctl.cancel.store(true, Relaxed);
        }
    }

    /// Every frame while a job runs: show progress; apply the result when
    /// it's done.
    pub fn poll_jobs(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.leveling_job else { return };
        let finished = job.handle.as_ref().is_some_and(|h| h.is_finished());
        if !finished {
            let (done, total, pass) = (job.ctl.done.load(Relaxed), job.ctl.total.load(Relaxed), job.ctl.pass.load(Relaxed));
            let secs = job.started.elapsed().as_secs();
            let cancelling = job.ctl.cancel.load(Relaxed);
            let mut cancel = false;
            let keys = super::dialogs::modal(ctx, "leveling_job", "Leveling resources", 380.0, |ui| {
                let frac = if total == 0 { 0.0 } else { done as f32 / total as f32 };
                ui.label(format!("Pass {}: {done} of {total} tasks placed", pass + 1));
                ui.add(egui::ProgressBar::new(frac).show_percentage());
                ui.weak(format!("{secs} s"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = ui.add_enabled(!cancelling, egui::Button::new(if cancelling { "Cancelling…" } else { "Cancel" }));
                        cancel = b.clicked();
                    });
                });
            });
            if cancel || keys.esc {
                self.cancel_level_job();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let job = self.leveling_job.take().expect("checked above");
        let outcome = job.handle.and_then(|h| h.join().ok()).flatten();
        match outcome {
            None if job.ctl.cancel.load(Relaxed) => self.set_status("Leveling cancelled; nothing changed."),
            None => self.error("Leveling ran into an internal error; nothing changed."),
            Some((leveled, res)) => {
                if self.edit_project("Level Resources", move |p| {
                    *p = leveled;
                    Ok(())
                }).is_some() {
                    self.report_leveling(&res);
                }
            }
        }
    }

    pub fn report_leveling(&mut self, res: &LevelResult) {
        let mut msg = format!("Leveling delayed {} task(s).", res.delayed.len());
        if res.unresolved.is_empty() {
            self.set_status(msg);
        } else {
            msg.push_str("\n\nCould not resolve:\n• ");
            msg.push_str(&res.unresolved.join("\n• "));
            self.dialogs.message = Some(("Resource Leveling".into(), msg));
        }
    }
}
