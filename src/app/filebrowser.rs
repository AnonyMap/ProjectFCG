//! Built-in file dialog, used when no XDG desktop portal answers (e.g. a
//! bare X session) so Open/Save always work from the AppImage.

use super::theme::*;
use egui::RichText;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    SaveAs,
    ExportCsv,
    ImportJiraCsv,
    ExportJiraCsv,
    ExportXml,
}

impl Mode {
    /// Picks an existing file (rather than naming a new one).
    pub fn is_open(self) -> bool {
        matches!(self, Mode::Open | Mode::ImportJiraCsv)
    }
}

/// The user's home folder (`USERPROFILE` on Windows).
fn home_dir() -> Option<std::ffi::OsString> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
}

pub enum Outcome {
    Open,
    Cancel,
    Chosen(PathBuf),
}

pub struct FileBrowser {
    pub mode: Mode,
    pub dir: PathBuf,
    pub file_name: String,
    pub show_all: bool,
    pub error: Option<String>,
    focused: bool,
    frames: u32,
    entries: Vec<(String, bool)>,
    loaded_for: Option<PathBuf>,
}

impl FileBrowser {
    pub fn new(mode: Mode, current: Option<PathBuf>) -> Self {
        let dir = current
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .filter(|d| d.is_dir())
            .or_else(|| home_dir().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        FileBrowser {
            mode,
            dir,
            file_name: String::new(),
            show_all: false,
            error: None,
            focused: false,
            frames: 0,
            entries: vec![],
            loaded_for: None,
        }
    }

    fn ext(&self) -> &'static str {
        match self.mode {
            Mode::ExportCsv | Mode::ImportJiraCsv | Mode::ExportJiraCsv => "csv",
            Mode::ExportXml => "xml",
            _ => crate::io::EXTENSION,
        }
    }

    fn refresh(&mut self) {
        if self.loaded_for.as_ref() == Some(&self.dir) {
            return;
        }
        self.entries.clear();
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    self.entries.push((name, is_dir));
                }
                self.entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Can't read {}: {e}", self.dir.display())),
        }
        self.loaded_for = Some(self.dir.clone());
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        self.refresh();
        self.frames += 1;
        let title = match self.mode {
            Mode::Open => "Open",
            Mode::SaveAs => "Save As",
            Mode::ExportCsv => "Export as CSV",
            Mode::ImportJiraCsv => "Import Jira CSV",
            Mode::ExportJiraCsv => "Export Board as Jira CSV",
            Mode::ExportXml => "Save as Project XML",
        };
        let mut outcome = Outcome::Open;
        let mut go_to: Option<PathBuf> = None;
        let ext = self.ext();
        let resp = egui::Modal::new(egui::Id::unique("file_browser")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new(title).size(15.0).strong().color(pal().heading));
            ui.horizontal(|ui| {
                if ui.button("⬆ Up").clicked() {
                    if let Some(p) = self.dir.parent() {
                        go_to = Some(p.to_path_buf());
                    }
                }
                if ui.button("🏠 Home").clicked() {
                    if let Some(h) = home_dir() {
                        go_to = Some(PathBuf::from(h));
                    }
                }
                let mut d = self.dir.display().to_string();
                let r = ui.add(egui::TextEdit::singleline(&mut d).desired_width(380.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go_to = Some(PathBuf::from(d));
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, false]).show(ui, |ui| {
                for (name, is_dir) in self.entries.clone() {
                    let matches = is_dir
                        || self.show_all
                        || if ext == crate::io::EXTENSION {
                            crate::io::is_project_file(&name) || name.to_lowercase().ends_with(".xml")
                        } else {
                            name.to_lowercase().ends_with(&format!(".{ext}"))
                        };
                    if !matches {
                        continue;
                    }
                    let label = if is_dir { format!("📁 {name}") } else { format!("📄 {name}") };
                    let r = ui.selectable_label(self.file_name == name, label);
                    if r.clicked() {
                        if is_dir {
                            go_to = Some(self.dir.join(&name));
                        } else {
                            self.file_name = name.clone();
                        }
                    }
                    if r.double_clicked() && !is_dir {
                        outcome = Outcome::Chosen(self.dir.join(&name));
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("File name:");
                let id = egui::Id::unique("fb_file_name");
                let r = ui.add(egui::TextEdit::singleline(&mut self.file_name).id(id).desired_width(320.0));
                if !self.focused {
                    // Focus the name with everything selected, so typing replaces it.
                    r.request_focus();
                    if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
                        let n = self.file_name.chars().count();
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(n),
                        )));
                        st.store(ui.ctx(), id);
                        self.focused = true;
                    }
                }
                // Enter = Save/Open (ignored on the frame the dialog opened).
                if self.frames > 1
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !self.file_name.trim().is_empty()
                {
                    outcome = Outcome::Chosen(self.dir.join(self.file_name.trim()));
                }
                ui.checkbox(&mut self.show_all, "All files");
            });
            if let Some(e) = &self.error {
                ui.colored_label(pal().warn, e);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("  Cancel  ").clicked() {
                    outcome = Outcome::Cancel;
                }
                let action = if self.mode.is_open() { "Open" } else { "Save" };
                if ui
                    .add_enabled(!self.file_name.trim().is_empty(), egui::Button::new(RichText::new(format!("  {action}  ")).strong()))
                    .clicked()
                {
                    outcome = Outcome::Chosen(self.dir.join(self.file_name.trim()));
                }
            });
        });
        let _ = resp;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            outcome = Outcome::Cancel;
        }
        if let Some(d) = go_to {
            if d.is_dir() {
                self.dir = d;
            } else {
                self.error = Some(format!("{} is not a folder", d.display()));
            }
        }
        if let Outcome::Chosen(p) = &outcome {
            if self.mode.is_open() && !p.is_file() {
                self.error = Some(format!("{} does not exist", p.display()));
                return Outcome::Open;
            }
        }
        outcome
    }
}
