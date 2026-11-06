//! The web build's link to the browser: opening and saving files, the
//! tab title, the "leave with unsaved changes?" warning, browser storage
//! for preferences and AutoRecover.
//!
//! Files:
//! * In Chrome and Edge (the File System Access API), Open and Save As
//!   use the system's file dialogs, and Save writes back to the same file,
//!   as on the desktop. The browser asks once per file for permission to
//!   save changes to it. A write lands in a temporary file that replaces
//!   the original only when it's complete, so a failed save never leaves
//!   half a plan.
//! * Elsewhere, Open uploads a file and Save downloads a copy.
//!
//! The browser's answers come back asynchronously: they're queued here
//! and applied at the start of the next frame (`poll`).

use super::{PendingAction, ProjectFcgApp};
use std::cell::RefCell;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// The functions are globals defined by web/pfcg.js, which the page loads
// before the app (a plain script: the single-file build runs from file://,
// where browsers don't load JavaScript modules).
#[wasm_bindgen]
extern "C" {
    fn pfcg_can_save_in_place() -> bool;
    fn pfcg_has_handle() -> bool;
    fn pfcg_forget_handle();
    fn pfcg_set_title(t: &str);
    fn pfcg_set_dirty(d: bool);
    fn pfcg_open(kind: &str) -> js_sys::Promise;
    fn pfcg_save_as(name: &str, data: &[u8], kind: &str, mime: &str) -> js_sys::Promise;
    fn pfcg_save(data: &[u8]) -> js_sys::Promise;
    fn pfcg_open_pdf(data: &[u8], name: &str) -> bool;
    fn pfcg_store_get(k: &str) -> Option<String>;
    fn pfcg_store_set(k: &str, v: &str) -> bool;
    fn pfcg_store_remove(k: &str);
}

pub fn store_get(k: &str) -> Option<String> {
    pfcg_store_get(k)
}
pub fn store_set(k: &str, v: &str) -> bool {
    pfcg_store_set(k, v)
}

/// What a file dialog was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenKind {
    Project,
    JiraCsv,
}

enum Done {
    Opened(OpenKind, String, Vec<u8>),
    /// Saved the project (`as_new`: under a name just picked) at the
    /// edit generation `gen`.
    Saved { name: String, gen: u64, as_new: bool },
    /// Save found no file to write to: ask for one.
    NeedsName,
    Exported(String),
    /// A file dropped onto the page, read.
    Dropped(String, Vec<u8>),
    Cancelled,
    Failed(String),
}

thread_local! {
    static DONE: RefCell<Vec<Done>> = const { RefCell::new(Vec::new()) };
}

fn finish(ctx: &egui::Context, d: Done) {
    DONE.with(|q| q.borrow_mut().push(d));
    ctx.request_repaint();
}

fn js_error(e: JsValue) -> String {
    js_sys::Reflect::get(&e, &"message".into()).ok().and_then(|m| m.as_string()).or_else(|| e.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

#[derive(Default)]
pub struct WebState {
    /// An open or save dialog, or a save, is under way.
    pub busy: bool,
    /// What to do once the save under way succeeds (the "save changes?"
    /// prompt's New / Open).
    after_save: Option<PendingAction>,
    title: String,
    dirty_shown: Option<bool>,
    /// AutoRecover: the generation last copied to browser storage, and
    /// when the plan first changed after it.
    recovered_gen: Option<u64>,
    changed_at: Option<web_time::Instant>,
    /// A copy found at startup: (file name, when, JSON).
    pub offer: Option<(String, String, String)>,
}

impl WebState {
    /// A different plan is open: AutoRecover starts over, and Save writes
    /// to the browser's file only when the plan came from one.
    pub fn reset_for_new_plan(&mut self, from_file: bool) {
        self.recovered_gen = None;
        self.changed_at = None;
        if !from_file {
            pfcg_forget_handle();
        }
    }
}

const RECOVER_KEY: &str = "projectfcg.autorecover";
const PREFS_KEY: &str = "projectfcg.prefs";

impl ProjectFcgApp {
    pub fn web_ctx(&self) -> Option<egui::Context> {
        self.web_ctx.clone()
    }

    /// File ▸ Open (or Import Jira CSV): the browser's file picker.
    pub fn web_open(&mut self, kind: OpenKind) {
        let Some(ctx) = self.web_ctx() else { return };
        if self.web.busy {
            return;
        }
        self.web.busy = true;
        let p = pfcg_open(if kind == OpenKind::Project { "project" } else { "csv" });
        wasm_bindgen_futures::spawn_local(async move {
            let d = match JsFuture::from(p).await {
                Ok(v) if v.is_null() || v.is_undefined() => Done::Cancelled,
                Ok(v) => {
                    let name = js_sys::Reflect::get(&v, &"name".into()).ok().and_then(|n| n.as_string()).unwrap_or_default();
                    let bytes = js_sys::Reflect::get(&v, &"bytes".into()).map(|b| js_sys::Uint8Array::new(&b).to_vec()).unwrap_or_default();
                    Done::Opened(kind, name, bytes)
                }
                Err(e) => Done::Failed(format!("Couldn't open the file: {}", js_error(e))),
            };
            finish(&ctx, d);
        });
    }

    fn project_bytes(&mut self) -> Option<Vec<u8>> {
        match crate::io::to_json(&self.project) {
            Ok(s) => Some(s.into_bytes()),
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    /// File ▸ Save: back to the open file, or Save As when there's none.
    pub fn web_save(&mut self) {
        let Some(ctx) = self.web_ctx() else { return };
        if self.web.busy {
            return;
        }
        if !pfcg_has_handle() {
            return self.web_save_as();
        }
        let Some(bytes) = self.project_bytes() else { return };
        self.web.busy = true;
        self.web.after_save = self.pending.take();
        let gen = self.history.generation();
        let p = pfcg_save(&bytes);
        self.set_status("Saving…");
        wasm_bindgen_futures::spawn_local(async move {
            let d = match JsFuture::from(p).await {
                Ok(v) => match v.as_string() {
                    Some(name) => Done::Saved { name, gen, as_new: false },
                    None => Done::NeedsName,
                },
                Err(e) => Done::Failed(format!("Couldn't save: {}", js_error(e))),
            };
            finish(&ctx, d);
        });
    }

    /// File ▸ Save As: the browser's save dialog (Chrome, Edge), or a
    /// download.
    pub fn web_save_as(&mut self) {
        let Some(ctx) = self.web_ctx() else { return };
        if self.web.busy {
            return;
        }
        let Some(bytes) = self.project_bytes() else { return };
        self.web.busy = true;
        self.web.after_save = self.pending.take();
        let gen = self.history.generation();
        let name = format!("{}.{}", self.file_stem(), crate::io::EXTENSION);
        let p = pfcg_save_as(&name, &bytes, "pfcg", "application/octet-stream");
        wasm_bindgen_futures::spawn_local(async move {
            let d = match JsFuture::from(p).await {
                Ok(v) => match v.as_string() {
                    Some(name) => Done::Saved { name, gen, as_new: true },
                    None => Done::Cancelled,
                },
                Err(e) => Done::Failed(format!("Couldn't save: {}", js_error(e))),
            };
            finish(&ctx, d);
        });
    }

    fn file_stem(&self) -> String {
        let t = self.project.title.trim();
        let t = if t.is_empty() { "Project1" } else { t };
        t.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect()
    }

    /// An export (CSV, XML, PDF, picture): Save As in its own format.
    pub fn web_export(&mut self, purpose: super::filebrowser::Mode) {
        use super::filebrowser::Mode;
        let Some(ctx) = self.web_ctx() else { return };
        let stem = self.file_stem();
        let (bytes, name, kind, mime) = match purpose {
            Mode::ExportCsv => (crate::io::to_csv(&self.project).into_bytes(), format!("{stem}.csv"), "csv", "text/csv"),
            Mode::ExportRisksCsv => (self.project.risks_csv().into_bytes(), format!("{stem} - Risks.csv"), "csv", "text/csv"),
            Mode::ExportJiraCsv => (crate::jira::export_csv(&self.project).into_bytes(), format!("{stem} - Jira.csv"), "csv", "text/csv"),
            Mode::ExportXml => (crate::mspdi::to_xml(&self.project).into_bytes(), format!("{stem}.xml"), "xml", "application/xml"),
            Mode::ExportPdf => (super::print::print_pdf(self, &self.print_options), format!("{stem}.pdf"), "pdf", "application/pdf"),
            Mode::ExportPng => match std::mem::take(&mut self.picture) {
                super::Picture::Ready(b) => (b, format!("{stem} - {}.png", self.view.main.label()), "png", "image/png"),
                _ => return,
            },
            _ => return,
        };
        let p = pfcg_save_as(&name, &bytes, kind, mime);
        wasm_bindgen_futures::spawn_local(async move {
            let d = match JsFuture::from(p).await {
                Ok(v) => v.as_string().map_or(Done::Cancelled, Done::Exported),
                Err(e) => Done::Failed(format!("Couldn't export: {}", js_error(e))),
            };
            finish(&ctx, d);
        });
    }

    /// Print: the PDF opens in a new tab, where the browser prints it.
    pub fn web_print(&mut self) {
        let pdf = super::print::print_pdf(self, &self.print_options);
        let name = format!("{} - print.pdf", self.file_stem());
        if pfcg_open_pdf(&pdf, &name) {
            self.set_status("The PDF opened in a new tab: print it from there");
        } else {
            self.set_status("The browser blocked the new tab, so the PDF was downloaded instead");
        }
    }

    /// Files dropped onto the page: read the first (browsers read files
    /// asynchronously), then open it as File ▸ Open would.
    pub fn web_read_dropped(&mut self, ctx: &egui::Context) {
        let Some(f) = ctx.input(|i| i.raw.dropped_files.first().cloned()) else { return };
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let name = f.path().to_string_lossy().to_string();
            let d = match f.bytes_async().await {
                Ok(b) => Done::Dropped(name, b),
                Err(e) => Done::Failed(format!("Couldn't read {name}: {e}")),
            };
            finish(&ctx, d);
        });
    }

    /// A dropped file's contents.
    pub fn web_dropped(&mut self, name: String, bytes: Vec<u8>) {
        pfcg_forget_handle();
        self.web_open_bytes(name, bytes);
    }

    fn web_open_bytes(&mut self, name: String, bytes: Vec<u8>) {
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => return self.error(format!("{name} isn't a ProjectFCG or Project XML file.")),
        };
        if name.to_lowercase().ends_with(".xml") {
            pfcg_forget_handle();
            return self.open_xml_text(&text, &name);
        }
        match crate::io::from_json(&text) {
            Ok(p) => {
                // the name shows in the title; Save writes back through the
                // browser's handle (when it gave one), never to this path
                let keep = pfcg_has_handle();
                self.replace_project(p, Some(PathBuf::from(&name)));
                if keep {
                    self.set_status(format!("Opened {name}"));
                } else {
                    self.path = None;
                    self.set_status(format!("Opened {name}. Save will ask where to save it."));
                }
            }
            Err(e) => self.error(e),
        }
    }

    /// Every frame: apply the browser's answers, keep the tab's title and
    /// unsaved-changes warning up to date, and copy unsaved work to browser
    /// storage.
    pub fn web_poll(&mut self, ctx: &egui::Context) {
        if self.web_ctx.is_none() {
            self.web_ctx = Some(ctx.clone());
        }
        let done: Vec<Done> = DONE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for d in done {
            match d {
                Done::Opened(OpenKind::Project, name, bytes) => {
                    self.web.busy = false;
                    self.web_open_bytes(name, bytes);
                }
                Done::Opened(OpenKind::JiraCsv, name, bytes) => {
                    self.web.busy = false;
                    self.start_jira_import_bytes(bytes, &name);
                }
                Done::Saved { name, gen, as_new } => {
                    self.web.busy = false;
                    if as_new && (self.project.title == "Project1" || self.project.title.is_empty()) {
                        if let Some(stem) = std::path::Path::new(&name).file_stem() {
                            self.project.title = stem.to_string_lossy().to_string();
                        }
                    }
                    let can_reopen = pfcg_has_handle();
                    self.path = can_reopen.then(|| PathBuf::from(&name));
                    // edits made while the browser was writing are still unsaved
                    self.dirty = self.history.generation() != gen;
                    if !self.dirty {
                        pfcg_store_remove(RECOVER_KEY);
                        self.web.recovered_gen = None;
                    }
                    self.set_status(if can_reopen { format!("Saved {name}") } else { format!("Downloaded {name}") });
                    if let Some(a) = self.web.after_save.take() {
                        if !self.dirty {
                            self.deferred_action = Some(a);
                        }
                    }
                }
                Done::NeedsName => {
                    self.web.busy = false;
                    let after = self.web.after_save.take();
                    self.pending = after;
                    self.web_save_as();
                }
                Done::Exported(name) => self.set_status(format!("Saved {name}")),
                Done::Dropped(name, bytes) => self.guarded(PendingAction::OpenBytes(name, bytes), ctx),
                Done::Cancelled => {
                    self.web.busy = false;
                    self.web.after_save = None;
                }
                Done::Failed(e) => {
                    self.web.busy = false;
                    self.web.after_save = None;
                    self.error(e);
                }
            }
        }
        let title = format!("{}{} - {}", if self.dirty { "*" } else { "" }, self.file_name(), super::APP_NAME);
        if title != self.web.title {
            pfcg_set_title(&title);
            self.web.title = title;
        }
        if self.web.dirty_shown != Some(self.dirty) {
            pfcg_set_dirty(self.dirty);
            self.web.dirty_shown = Some(self.dirty);
        }
        self.web_autorecover();
    }

    /// AutoRecover in the browser: unsaved work is copied to the browser's
    /// storage 10 s after the last change, and offered back next time.
    fn web_autorecover(&mut self) {
        if !self.dirty {
            self.web.changed_at = None;
            return;
        }
        let g = self.history.generation();
        if self.web.recovered_gen == Some(g) {
            return;
        }
        let since = *self.web.changed_at.get_or_insert_with(web_time::Instant::now);
        if since.elapsed() < std::time::Duration::from_secs(10) {
            if let Some(ctx) = &self.web_ctx {
                ctx.request_repaint_after(std::time::Duration::from_secs(10));
            }
            return;
        }
        self.web.changed_at = None;
        self.web.recovered_gen = Some(g);
        let Ok(json) = crate::io::to_json(&self.project) else { return };
        let when = chrono::Local::now().format("%a %-m/%-d %-I:%M %p").to_string();
        let rec = serde_json::json!({ "name": self.file_name(), "when": when, "plan": json }).to_string();
        if !pfcg_store_set(RECOVER_KEY, &rec) {
            self.set_status("AutoRecover: the browser's storage is full, so unsaved work isn't being kept. Save to a file.");
        }
    }

    /// At startup: a copy left by a tab that closed without saving.
    pub fn web_find_recovery(&mut self) {
        let Some(s) = pfcg_store_get(RECOVER_KEY) else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else { return pfcg_store_remove(RECOVER_KEY) };
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        self.web.offer = Some((get("name"), get("when"), get("plan")));
    }

    pub fn web_recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some((name, when, plan)) = self.web.offer.clone() else { return };
        let (mut restore, mut discard) = (false, false);
        super::dialogs::modal(ctx, "web_recover", "Recover unsaved work", 420.0, |ui| {
            ui.label(format!("'{name}' had changes that weren't saved when its tab closed ({when})."));
            ui.label("Recover them? Save them to a file afterwards to keep them.");
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                discard = ui.button("Discard").clicked();
                restore = ui.button(egui::RichText::new("Recover").strong()).clicked();
            });
        });
        if restore {
            self.web.offer = None;
            match crate::io::from_json(&plan) {
                Ok(p) => {
                    pfcg_forget_handle();
                    self.replace_project(p, None);
                    self.dirty = true;
                    self.set_status(format!("Recovered '{name}': save it to keep it"));
                }
                Err(e) => self.error(format!("The recovered copy couldn't be read: {e}")),
            }
        } else if discard {
            self.web.offer = None;
            pfcg_store_remove(RECOVER_KEY);
        }
    }
}

/// Preferences live in the browser's storage.
pub fn load_prefs() -> Option<super::prefs::Prefs> {
    pfcg_store_get(PREFS_KEY).and_then(|s| serde_json::from_str::<super::prefs::Prefs>(&s).ok()).map(|p| p.sanitized())
}

pub fn save_prefs(p: &super::prefs::Prefs) {
    if let Ok(s) = serde_json::to_string(p) {
        pfcg_store_set(PREFS_KEY, &s);
    }
}

/// Is saving back to the same file possible here (Chrome, Edge)?
pub fn can_save_in_place() -> bool {
    pfcg_can_save_in_place()
}

/// Start the app on the page's canvas.
pub fn start() {
    use wasm_bindgen::JsCast;
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let Some(canvas) = document.get_element_by_id("projectfcg").and_then(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            web_sys::console::error_1(&"ProjectFCG: no <canvas id=\"projectfcg\"> on the page".into());
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    let mut app = if web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|s| s.contains("sample")) {
                        ProjectFcgApp::new(super::sample::software_release())
                    } else {
                        ProjectFcgApp::default()
                    };
                    app.web_ctx = Some(cc.egui_ctx.clone());
                    if let Some(p) = load_prefs() {
                        let size = p.text_size;
                        app.prefs = p;
                        app.set_text_size(size);
                    }
                    app.prefs_path = Some(PathBuf::from("browser storage"));
                    app.web_find_recovery();
                    Ok(Box::new(app))
                }),
            )
            .await;
        if let (Err(e), Some(el)) = (started, document.get_element_by_id("loading")) {
            el.set_inner_html(&format!("<p>ProjectFCG couldn't start: {e:?}</p><p>It needs WebGL 2; try Chrome or Edge.</p>"));
            return;
        }
        if let Some(el) = document.get_element_by_id("loading") {
            el.remove();
        }
    });
}
