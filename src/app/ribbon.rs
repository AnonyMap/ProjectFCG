//! A compact Office-style ribbon mirroring Project's Task / Project / View /
//! Format tabs.

use super::commands::{Command, Filter, Timescale};
use super::theme::*;
use super::{ProjectFcgApp, RibbonTab};
use egui::{pos2, vec2, Align2, FontId, Rect, RichText, Sense, Stroke, StrokeKind};

const GROUP_H: f32 = 72.0;

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    // ---- tab strip with quick-access toolbar ----
    let strip = ui.allocate_space(vec2(ui.available_width(), 28.0)).1;
    let th = pal();
    ui.painter().rect_filled(strip, 0.0, th.band);
    let mut x = strip.left() + 6.0;
    // Quick access: Save, Undo, Redo
    for (glyph, tip, cmd, enabled) in [
        ("💾", "Save (Ctrl+S)", Command::Save, true),
        ("⟲", "Undo (Ctrl+Z)", Command::Undo, app.history.can_undo()),
        ("⟳", "Redo (Ctrl+Y)", Command::Redo, app.history.can_redo()),
    ] {
        let r = Rect::from_min_size(pos2(x, strip.top() + 3.0), vec2(24.0, 22.0));
        let resp = ui.interact(r, ui.scope_id().with(("qat", glyph)), Sense::click());
        if resp.hovered() && enabled {
            ui.painter().rect_filled(r, 3.0, th.band_hover);
        }
        let col = if enabled { th.on_band } else { th.on_band.gamma_multiply(0.35) };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), col);
        let label = match cmd {
            Command::Undo => app.history.undo_label().map(|l| format!("Undo {l} (Ctrl+Z)")),
            Command::Redo => app.history.redo_label().map(|l| format!("Redo {l} (Ctrl+Y)")),
            _ => None,
        }
        .unwrap_or_else(|| tip.to_string());
        if resp.on_hover_text(label).clicked() && enabled {
            app.execute(cmd, &ctx);
        }
        x += 26.0;
    }
    x += 10.0;
    for (tab, name) in [
        (RibbonTab::File, "File"),
        (RibbonTab::Task, "Task"),
        (RibbonTab::Resource, "Resource"),
        (RibbonTab::Project, "Project"),
        (RibbonTab::View, "View"),
        (RibbonTab::Format, "Format"),
        (RibbonTab::Board, "Board"),
        (RibbonTab::Help, "Help"),
    ] {
        let galley = ui.painter().layout_no_wrap(name.into(), FontId::proportional(13.5), th.on_band);
        let w = galley.size().x + 22.0;
        let r = Rect::from_min_size(pos2(x, strip.top() + 4.0), vec2(w, strip.height() - 4.0));
        let resp = ui.interact(r, ui.scope_id().with(("tab", name)), Sense::click());
        let selected = app.tab == tab;
        if selected {
            ui.painter().rect_filled(r, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, th.ribbon);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 3.0, th.band_hover);
        }
        let col = if selected { th.heading } else { th.on_band };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, name, FontId::proportional(13.5), col);
        if resp.clicked() {
            app.tab = tab;
            // the Board tab is a workspace: it shows the board
            if tab == RibbonTab::Board && app.view.main != super::MainView::Board {
                app.execute(Command::ShowView(super::MainView::Board), &ctx);
            }
        }
        x += w + 2.0;
    }
    // document title on the right
    ui.painter().text(
        pos2(strip.right() - 10.0, strip.center().y),
        Align2::RIGHT_CENTER,
        format!("{}{} - ProjectFCG", if app.dirty { "*" } else { "" }, app.file_name()),
        FontId::proportional(12.5),
        th.on_band.gamma_multiply(0.86),
    );

    // ---- group panel ----
    egui::Frame::new()
        .inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 2 })
        .show(ui, |ui| {
            ui.horizontal_top(|ui| match app.tab {
                RibbonTab::File => file_tab(app, ui),
                RibbonTab::Task => task_tab(app, ui),
                RibbonTab::Resource => resource_tab(app, ui),
                RibbonTab::Project => project_tab(app, ui),
                RibbonTab::View => view_tab(app, ui),
                RibbonTab::Format => format_tab(app, ui),
                RibbonTab::Board => board_tab(app, ui),
                RibbonTab::Help => help_tab(app, ui),
            });
        });
    let r = ui.min_rect();
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, pal().header_border));
}

/// A ribbon group: contents above, caption below, separator after.
fn group(ui: &mut egui::Ui, caption: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        let r = ui
            .horizontal_top(|ui| {
                ui.set_min_height(GROUP_H - 6.0);
                ui.set_max_height(GROUP_H - 6.0);
                ui.spacing_mut().item_spacing.y = 1.0;
                add(ui);
            })
            .response
            .rect;
        let cap = ui.painter().layout_no_wrap(caption.to_string(), FontId::proportional(11.0), pal().text_dim);
        let w = r.width().max(cap.size().x + 8.0);
        let (cr, _) = ui.allocate_exact_size(vec2(w, 14.0), Sense::hover());
        ui.painter().galley(pos2(cr.center().x - cap.size().x / 2.0, cr.top()), cap, pal().text_dim);
    });
    let (r, _) = ui.allocate_exact_size(vec2(9.0, GROUP_H + 8.0), Sense::hover());
    ui.painter().line_segment(
        [pos2(r.center().x, r.top() + 4.0), pos2(r.center().x, r.bottom() - 4.0)],
        Stroke::new(1.0, pal().grid),
    );
}

/// Large ribbon button: glyph over caption.
fn big(ui: &mut egui::Ui, glyph: &str, caption: &str, tip: &str, enabled: bool) -> bool {
    let font = FontId::proportional(11.5);
    let lines: Vec<&str> = caption.split('\n').collect();
    let text_w = lines
        .iter()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), pal().text).size().x)
        .fold(0.0, f32::max);
    let w = (text_w + 12.0).max(46.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, GROUP_H - 6.0), Sense::click());
    let hovered = resp.hovered() && enabled;
    if hovered {
        ui.painter().rect_filled(r, 3.0, pal().tint);
        ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, pal().tint_border), StrokeKind::Inside);
    }
    let col = if enabled { pal().text } else { pal().text_disabled };
    let gcol = if enabled { pal().heading } else { pal().text_disabled };
    ui.painter().text(pos2(r.center().x, r.top() + 15.0), Align2::CENTER_CENTER, glyph, FontId::proportional(22.0), gcol);
    for (k, l) in lines.iter().enumerate() {
        ui.painter().text(
            pos2(r.center().x, r.top() + 34.0 + k as f32 * 12.5),
            Align2::CENTER_TOP,
            *l,
            font.clone(),
            col,
        );
    }
    let resp = resp.on_hover_text(tip);
    resp.clicked() && enabled
}

/// Small ribbon button: glyph + caption on one line.
fn small(ui: &mut egui::Ui, glyph: &str, caption: &str, tip: &str, enabled: bool) -> bool {
    let text = if glyph.is_empty() { caption.to_string() } else { format!("{glyph} {caption}") };
    let b = egui::Button::new(RichText::new(text).size(12.0)).frame(false).min_size(vec2(0.0, 18.0));
    ui.add_enabled(enabled, b).on_hover_text(tip).clicked()
}

fn file_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    group(ui, "Project", |ui| {
        if big(ui, "🗋", "New", "Create a new blank project (Ctrl+N)", true) {
            app.execute(Command::New, &ctx);
        }
        if big(ui, "📂", "Open", "Open a project (Ctrl+O)", true) {
            app.execute(Command::Open, &ctx);
        }
        if big(ui, "💾", "Save", "Save (Ctrl+S)", true) {
            app.execute(Command::Save, &ctx);
        }
        if big(ui, "💾", "Save As", "Save with a new name (F12)", true) {
            app.execute(Command::SaveAs, &ctx);
        }
    });
    group(ui, "Templates", |ui| {
        if big(ui, "⭐", "Sample\nProject", "Open a sample software-release schedule", true) {
            app.execute(Command::NewFromSample, &ctx);
        }
        if big(ui, "📋", "Agile\nSample", "Open a sample plan with sprints, tickets and a backlog", true) {
            app.execute(Command::NewFromAgileSample, &ctx);
        }
    });
    group(ui, "Share", |ui| {
        if big(ui, "📤", "Save as\nXML", "Save as Microsoft Project XML (opens in Project, ProjectLibre, …). Open reads .xml files too.", true) {
            app.execute(Command::ExportXml, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "📊", "Export CSV", "Export the task table as CSV", true) {
                app.execute(Command::ExportCsv, &ctx);
            }
            if small(ui, "📥", "Import Jira CSV", "Add issues from a Jira CSV export to the board", true) {
                app.execute(Command::ImportJiraCsv, &ctx);
            }
            if small(ui, "📤", "Export Jira CSV", "Write the board as a CSV for Jira's importer", !app.project.tickets.is_empty()) {
                app.execute(Command::ExportJiraCsv, &ctx);
            }
        });
    });
    group(ui, "Settings", |ui| {
        if big(ui, "⚙", "Options", "Scheduling options", true) {
            app.execute(Command::Options, &ctx);
        }
        if big(ui, "🚪", "Exit", "Close ProjectFCG (Ctrl+Q)", true) {
            app.execute(Command::Quit, &ctx);
        }
    });
    group(ui, "Theme", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode:").size(12.0));
                let cur = app.prefs.theme_mode;
                egui::ComboBox::from_id_salt("theme_mode")
                    .selected_text(cur.label())
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        for m in ThemeMode::ALL {
                            if ui.selectable_label(cur == m, m.label()).clicked() {
                                app.execute(Command::SetThemeMode(m), &ctx);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Colour:").size(12.0));
                for a in Accent::ALL {
                    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                    let pt = ui.painter();
                    let on = app.prefs.accent == a;
                    let sw = Palette::new(a, pal().dark).accent;
                    pt.rect_filled(r.shrink(2.0), 3.0, sw);
                    if on {
                        pt.rect_stroke(r, 4.0, Stroke::new(2.0, pal().text), StrokeKind::Inside);
                    } else if resp.hovered() {
                        pt.rect_stroke(r, 4.0, Stroke::new(1.0, pal().text_dim), StrokeKind::Inside);
                    }
                    if resp.on_hover_text(a.label()).clicked() {
                        app.execute(Command::SetAccent(a), &ctx);
                    }
                }
            });
            let dark = app.is_dark(&ctx);
            let next = if dark { ThemeMode::Light } else { ThemeMode::Dark };
            if small(ui, if dark { "☀" } else { "🌙" }, if dark { "Switch to Light" } else { "Switch to Dark" }, "Toggle light / dark (Ctrl+Shift+F11)", true) {
                app.execute(Command::SetThemeMode(next), &ctx);
            }
        });
    });
    if let Some(p) = &app.path {
        ui.label(RichText::new(format!("File: {}", p.display())).size(11.0).color(pal().text_dim));
    }
}

fn task_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let has = app.active_index().is_some();
    let multi = app.selected.len() > 1;
    group(ui, "Clipboard", |ui| {
        if big(ui, "📋", "Paste", "Paste tasks (Ctrl+V)", !app.clipboard.is_empty()) {
            app.execute(Command::Paste, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "✂", "Cut", "Cut tasks (Ctrl+X)", has) {
                app.execute(Command::Cut, &ctx);
            }
            if small(ui, "🗐", "Copy", "Copy tasks (Ctrl+C)", has) {
                app.execute(Command::Copy, &ctx);
            }
            if small(ui, "📷", "Copy Picture", "Save the view as it looks on screen as a PNG image", true) {
                app.execute(Command::CopyPicture, &ctx);
            }
        });
    });
    group(ui, "Schedule", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for v in [0u8, 25, 50, 75, 100] {
                    let b = egui::Button::new(RichText::new(format!("{v}%")).size(11.0)).min_size(vec2(30.0, 18.0));
                    if ui.add_enabled(has, b).on_hover_text(format!("Mark {v}% complete")).clicked() {
                        app.execute(Command::SetPercent(v), &ctx);
                    }
                }
            });
            ui.horizontal(|ui| {
                if small(ui, "⬅", "Outdent", "Outdent task (Alt+Shift+Left)", has) {
                    app.execute(Command::Outdent, &ctx);
                }
                if small(ui, "➡", "Indent", "Indent task (Alt+Shift+Right)", has) {
                    app.execute(Command::Indent, &ctx);
                }
            });
            ui.horizontal(|ui| {
                if small(ui, "🔗", "Link", "Link the selected tasks (Ctrl+F2)", multi) {
                    app.execute(Command::LinkTasks, &ctx);
                }
                if small(ui, "✖", "Unlink", "Unlink tasks (Ctrl+Shift+F2)", has) {
                    app.execute(Command::UnlinkTasks, &ctx);
                }
            });
        });
    });
    group(ui, "Tasks", |ui| {
        if big(ui, "📌", "Manually\nSchedule", "Manually schedule selected tasks (Ctrl+Shift+M)", has) {
            app.execute(Command::SetManual, &ctx);
        }
        if big(ui, "🔄", "Auto\nSchedule", "Auto schedule selected tasks (Ctrl+Alt+Shift+A)", has) {
            app.execute(Command::SetAuto, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "⬆", "Move Up", "Move task up (Alt+Shift+Up)", has) {
                app.execute(Command::MoveUp, &ctx);
            }
            if small(ui, "⬇", "Move Down", "Move task down (Alt+Shift+Down)", has) {
                app.execute(Command::MoveDown, &ctx);
            }
            let active = app.active_index().is_none_or(|r| app.project.tasks[r].active);
            if small(ui, "⊘", if active { "Inactivate" } else { "Activate" }, "Keep the task in the plan without it affecting the schedule, rollups or resources (what-if)", has) {
                app.execute(Command::ToggleActive, &ctx);
            }
        });
    });
    group(ui, "Insert", |ui| {
        if big(ui, "➕", "Task", "Insert a task above the selection (Insert)", true) {
            app.execute(Command::InsertTask, &ctx);
        }
        if big(ui, "📑", "Summary", "Insert a summary task over the selection", true) {
            app.execute(Command::InsertSummary, &ctx);
        }
        ui.vertical(|ui| {
            if big(ui, "🔶", "Milestone", "Insert a milestone", true) {
                app.execute(Command::InsertMilestone, &ctx);
            }
        });
        ui.vertical(|ui| {
            if small(ui, "🔁", "Recurring…", "Insert a recurring task (a weekly meeting, a monthly report)", true) {
                app.execute(Command::InsertRecurring, &ctx);
            }
        });
    });
    group(ui, "Properties", |ui| {
        if big(ui, "ℹ", "Information", "Task Information (Shift+F2)", has) {
            app.execute(Command::TaskInfo, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "📝", "Notes", "Task notes", has) {
                app.execute(Command::TaskNotes, &ctx);
            }
            if small(ui, "📅", "Add to Timeline", "Show the selected tasks on the Timeline", has) {
                app.execute(Command::AddToTimeline, &ctx);
            }
            if small(ui, "🔶", "Mark Milestone", "Toggle milestone flag", has) {
                app.execute(Command::MarkMilestone, &ctx);
            }
        });
    });
    group(ui, "Inspect", |ui| {
        let on = app.view.inspector;
        let cap = if on { "▸Inspect" } else { "Inspect" };
        if big(ui, "🔍", cap, "Task Inspector: what drives the selected task's dates", true) {
            app.execute(Command::ToggleInspector, &ctx);
        }
    });
    group(ui, "Tracking", |ui| {
        let leaf = app.active_index().is_some_and(|r| !app.project.tasks[r].calc.is_summary);
        if big(ui, "✔", "Mark on\nTrack", "Set % complete to where the schedule says it should be at the status date", leaf) {
            app.execute(Command::MarkOnTrack, &ctx);
        }
        if big(ui, "📝", "Update\nTasks", "Enter % complete, actual and remaining duration, actual start and finish", leaf) {
            app.execute(Command::UpdateTasks, &ctx);
        }
    });
    group(ui, "Editing", |ui| {
        if big(ui, "🎯", "Scroll\nto Task", "Scroll the chart to the selected task (Ctrl+Shift+F5)", has) {
            app.execute(Command::ScrollToTask, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "🔍", "Find", "Find (Ctrl+F)", true) {
                app.execute(Command::Find, &ctx);
            }
            if small(ui, "👉", "Go To", "Go to task ID or date (F5)", true) {
                app.execute(Command::GoTo, &ctx);
            }
        });
    });
}

fn board_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    use super::MainView;
    let ctx = ui.ctx().clone();
    let on_board = app.view.main == MainView::Board;
    group(ui, "View", |ui| {
        if big(ui, "📋", if on_board { "▸Board" } else { "Board" }, "Show the Kanban board", true) && !on_board {
            app.execute(Command::ShowView(MainView::Board), &ctx);
        }
        if big(ui, "📊", "Gantt\nChart", "Back to the Gantt chart", true) {
            app.execute(Command::ShowView(MainView::Gantt), &ctx);
        }
    });
    group(ui, "Sprints", |ui| {
        if big(ui, "📆", "New\nSprint", "Add a sprint to the schedule, after the last one", true) {
            app.execute(Command::NewSprint, &ctx);
        }
        let row = app.active_index().filter(|_| !on_board);
        let is = row.is_some_and(|r| app.project.tasks[r].is_sprint);
        if big(ui, "🏁", if is { "Unmark\nSprint" } else { "Mark as\nSprint" }, "Make the selected Gantt task a sprint (or stop it being one)", row.is_some()) {
            app.execute(Command::ToggleSprint, &ctx);
        }
        ui.vertical(|ui| {
            let s = app.board_sprint().filter(|_| on_board);
            let m = s.and_then(|u| app.project.board.sprint_meta(u));
            let started = m.is_some_and(|m| m.started.is_some());
            let done = m.is_some_and(|m| m.completed.is_some());
            if !started {
                if small(ui, "▶", "Start Sprint", "Record the shown sprint's goal and committed scope", s.is_some()) {
                    app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Start);
                }
            } else if small(ui, "✔", "Complete Sprint", "Freeze the shown sprint's velocity and move unfinished tickets on", s.is_some() && !done) {
                app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Complete);
            }
            if small(ui, "✏", "Sprint Goal…", "Edit the shown sprint's goal", s.is_some()) {
                app.open_sprint_dialog(super::sprint_dialogs::SprintAction::Goal);
            }
            if small(ui, "▥", "Columns…", "Rename, add, remove or reorder board columns; set WIP limits", true) {
                app.open_columns_dialog();
            }
        });
        ui.vertical(|ui| {
            ui.label(RichText::new("New sprints:").size(12.0));
            let mut d = app.project.board.sprint_days;
            if ui.add(egui::DragValue::new(&mut d).range(1..=60).suffix(" days")).on_hover_text("Length of a new sprint, in working days").changed() {
                app.project.board.sprint_days = d;
                app.dirty = true;
            }
        });
    });
    group(ui, "Tickets", |ui| {
        let sel = app.board.selected.is_some() && on_board;
        if big(ui, "✚", "New\nTicket", "New ticket (Insert)", true) {
            app.execute(Command::NewTicket, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "🏷", "New Epic", "An epic groups stories across sprints; it can drive a Gantt task's % complete", true) {
                app.execute(Command::NewEpic, &ctx);
            }
            if small(ui, "✏", "Edit", "Edit the selected ticket (Enter)", sel) {
                app.execute(Command::EditTicket, &ctx);
            }
            if small(ui, "📥", "To Backlog", "Send the selected ticket back to the backlog", sel) {
                app.execute(Command::TicketToBacklog, &ctx);
            }
            if small(ui, "✖", "Delete", "Delete the selected ticket (Delete)", sel) {
                app.execute(Command::DeleteTicket, &ctx);
            }
        });
        ui.vertical(|ui| {
            if small(ui, "⚑", "New Release", "A release (fix version) groups stories that ship together; it can stand behind a Gantt milestone", true) {
                app.view.main = super::MainView::Board;
                app.board.side = super::board::SidePanel::Releases;
                app.board.backlog_open = true;
                app.open_release_dialog(None);
            }
        });
    });
    group(ui, "Estimate", |ui| {
        ui.vertical(|ui| {
            for e in crate::board::Estimate::ALL {
                if ui.radio(app.project.board.estimate == e, RichText::new(e.label()).size(12.0)).on_hover_text("What sprint progress, burndown and velocity count").clicked() {
                    app.execute(Command::SetEstimate(e), &ctx);
                }
            }
        });
    });
    group(ui, "Jira", |ui| {
        ui.vertical(|ui| {
            if small(ui, "📥", "Import CSV", "Add issues from a Jira CSV export (Filters ▸ Export ▸ CSV)", true) {
                app.execute(Command::ImportJiraCsv, &ctx);
            }
            if small(ui, "📤", "Export CSV", "Write the board as a CSV for Jira's importer", !app.project.tickets.is_empty()) {
                app.execute(Command::ExportJiraCsv, &ctx);
            }
        });
    });
    group(ui, "Charts", |ui| {
        let mut c = app.board.show_charts;
        if ui.checkbox(&mut c, RichText::new("Burndown &\nvelocity").size(12.0)).changed() {
            app.execute(Command::ToggleBoardCharts, &ctx);
        }
    });
    // stats
    ui.vertical(|ui| {
        let est = app.project.board.estimate;
        if let Some(s) = app.board_sprint() {
            let st = app.project.sprint_stats(Some(s));
            let name = app.project.task_by_uid(s).map(|t| t.name.clone()).unwrap_or_default();
            ui.label(RichText::new(format!("{name}: {} of {} done", crate::duration::fmt_num(st.done), est.unit(st.total))).size(12.0));
        }
        let bl = app.project.sprint_stats(None);
        ui.label(RichText::new(format!("Backlog: {}", est.unit(bl.total))).size(12.0).color(pal().text_dim));
        if let Some(v) = app.project.average_velocity(chrono::Local::now().naive_local()) {
            ui.label(RichText::new(format!("Velocity: {} / sprint", est.unit((v * 10.0).round() / 10.0))).size(12.0).color(pal().text_dim));
        }
    });
}

fn resource_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    use crate::model::ResourceType;
    use super::MainView;
    let ctx = ui.ctx().clone();
    let has_task = app.active_index().is_some();
    group(ui, "View", |ui| {
        for (v, glyph, cap) in [
            (MainView::ResourceSheet, "👥", "Resource\nSheet"),
            (MainView::ResourceUsage, "📈", "Resource\nUsage"),
        ] {
            let shown = if app.view.main == v { format!("▸{cap}") } else { cap.to_string() };
            if big(ui, glyph, &shown, v.label(), true) {
                app.execute(Command::ShowView(v), &ctx);
            }
        }
    });
    group(ui, "Assignments", |ui| {
        let open = app.dialogs.assign.is_some();
        let cap = if open { "▸Assign\nResources" } else { "Assign\nResources" };
        if big(ui, "👤", cap, "Assign resources to the selected tasks (Alt+F10)", true) {
            app.execute(Command::AssignResources, &ctx);
        }
    });
    group(ui, "Add Resources", |ui| {
        ui.vertical(|ui| {
            if small(ui, "👷", "Work Resource", "People and equipment that do work", true) {
                app.execute(Command::AddResource(ResourceType::Work), &ctx);
            }
            if small(ui, "📦", "Material Resource", "Consumables: concrete, licences…", true) {
                app.execute(Command::AddResource(ResourceType::Material), &ctx);
            }
            if small(ui, "💲", "Cost Resource", "Fixed costs: travel, fees…", true) {
                app.execute(Command::AddResource(ResourceType::Cost), &ctx);
            }
        });
    });
    group(ui, "Properties", |ui| {
        if big(ui, "ℹ", "Information", "Resource Information (Shift+F2 in the Resource Sheet)", app.res_active_index().is_some()) {
            app.execute(Command::ResourceInfo, &ctx);
        }
    });
    group(ui, "Level", |ui| {
        if big(ui, "⚖", "Level\nSelection", "Level the selected tasks", has_task) {
            app.execute(Command::LevelSelection, &ctx);
        }
        if big(ui, "⚖", "Level\nAll", "Resolve every overallocation by delaying tasks", !app.project.resources.is_empty()) {
            app.execute(Command::LevelAll, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "⚙", "Leveling Options", "Leveling order and slack", true) {
                app.execute(Command::LevelingOptions, &ctx);
            }
            let any_delay = app.project.tasks.iter().any(|t| t.leveling_delay > 0);
            if small(ui, "✖", "Clear Leveling", "Remove all leveling delays", any_delay) {
                app.execute(Command::ClearLeveling, &ctx);
            }
            let any_over = app.project.calc.resources.iter().any(|r| r.overallocated);
            if small(ui, "➡", "Next Overallocation", "Go to the next task with an overallocated resource", any_over) {
                app.execute(Command::NextOverallocation, &ctx);
            }
        });
    });
    let n_over = app.project.calc.resources.iter().filter(|r| r.overallocated).count();
    ui.vertical(|ui| {
        ui.label(RichText::new(format!("Resources: {}", app.project.resources.len())).size(12.0));
        if n_over > 0 {
            ui.label(RichText::new(format!("{n_over} overallocated")).size(12.0).color(pal().warn));
        }
        ui.label(
            RichText::new(format!(
                "Project cost: {}",
                crate::resources::fmt_money(app.project.calc.total_cost, &app.project.settings.currency_symbol)
            ))
            .size(12.0),
        );
    });
}

fn project_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    group(ui, "Properties", |ui| {
        if big(ui, "ℹ", "Project\nInformation", "Project start date, calendar and statistics", true) {
            app.execute(Command::ProjectInfo, &ctx);
        }
        if big(ui, "📅", "Change\nWorking Time", "Edit the project calendar", true) {
            app.execute(Command::WorkingTime, &ctx);
        }
    });
    group(ui, "Schedule", |ui| {
        if big(ui, "🔃", "Calculate\nProject", "Recalculate the schedule (F9)", true) {
            app.execute(Command::Calculate, &ctx);
        }
        ui.vertical(|ui| {
            let mut auto = app.auto_calc;
            if ui.checkbox(&mut auto, RichText::new("Automatic calculation").size(12.0)).on_hover_text("Ctrl+F9").changed() {
                app.execute(Command::ToggleAutoCalc, &ctx);
            }
            let mut honor = app.project.settings.honor_constraints;
            if ui
                .checkbox(&mut honor, RichText::new("Honor constraint dates").size(12.0))
                .on_hover_text("Tasks will always honor their constraint dates")
                .changed()
            {
                app.edit_project("Honor Constraints", |p| {
                    p.settings.honor_constraints = honor;
                    Ok(())
                });
            }
        });
    });
    group(ui, "Baseline", |ui| {
        if big(ui, "📌", "Set\nBaseline", "Save the current plan as the baseline", true) {
            app.execute(Command::SetBaseline, &ctx);
        }
        if big(ui, "🗑", "Clear\nBaseline", "Remove baseline data", (0..=10).any(|n| app.project.has_baseline_n(n))) {
            app.execute(Command::ClearBaseline, &ctx);
        }
    });
    group(ui, "Status", |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("Status date:").size(12.0));
            let id = ui.make_persistent_id("status_date_text");
            let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| app.project.status_date.map(crate::model::fmt_date).unwrap_or_else(|| "NA".into()));
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(96.0).background_color(pal().header_bg)).on_hover_text("Progress, Late tasks, progress lines and earned value are measured at this date (NA = today)");
            if r.lost_focus() {
                let t = text.trim().to_string();
                let parsed = if t.is_empty() || t.eq_ignore_ascii_case("na") {
                    Ok(None)
                } else {
                    crate::model::parse_date(&t, app.project.default_finish_time()).map(Some)
                };
                match parsed {
                    Ok(v) if v != app.project.status_date => {
                        app.edit_project("Status Date", move |p| {
                            p.status_date = v;
                            Ok(())
                        });
                    }
                    Ok(_) => {}
                    Err(e) => app.error(e),
                }
                ui.data_mut(|d| d.remove::<String>(id));
            } else if r.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, text));
            }
        });
        if big(ui, "🔄", "Update\nProject", "Mark work complete through a date, or reschedule unstarted work", true) {
            app.execute(Command::UpdateProject, &ctx);
        }
        if big(ui, "📊", "Statistics", "Current, baseline and actual dates, work and cost; earned value", true) {
            app.execute(Command::ProjectStatistics, &ctx);
        }
    });
    group(ui, "Critical Path", |ui| {
        ui.vertical(|ui| {
            if check(ui, app.project.settings.multiple_critical_paths, "Multiple critical paths", "Each independent chain gets its own critical path") {
                app.execute(Command::ToggleMultipleCriticalPaths, &ctx);
            }
            let crit = crate::analysis::critical_tasks(&app.project).len();
            ui.label(RichText::new(format!("{crit} critical task(s)")).size(12.0).color(pal().text_dim));
            if small(ui, "🔥", "Show Critical Path", "Detail Gantt view with the Critical filter", true) {
                app.execute(Command::SetGanttView(crate::format::GanttViewKind::Detail), &ctx);
                app.execute(Command::SetFilter(Filter::Critical), &ctx);
            }
        });
    });
    group(ui, "Options", |ui| {
        if big(ui, "⚙", "Options", "Hours per day, default times, scheduling options", true) {
            app.execute(Command::Options, &ctx);
        }
    });
    if let Some(f) = app.project.calc.finish {
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("Start:  {}", crate::model::fmt_date(app.project.start))).size(12.0));
            ui.label(RichText::new(format!("Finish: {}", crate::model::fmt_date(f))).size(12.0));
            ui.label(RichText::new(format!("Tasks:  {}", app.project.tasks.len())).size(12.0));
        });
    }
}

fn view_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    group(ui, "Task Views", |ui| {
        use crate::format::GanttViewKind;
        for (v, glyph, cap, tip) in [
            (GanttViewKind::Gantt, "📊", "Gantt\nChart", "Gantt Chart view"),
            (GanttViewKind::Tracking, "📉", "Tracking\nGantt", "Actual bars over baseline bars; critical path shown"),
            (GanttViewKind::Detail, "📐", "Detail\nGantt", "Critical path with total slack bars"),
            (GanttViewKind::Leveling, "📶", "Leveling\nGantt", "Leveling delays shown before each task"),
        ] {
            let selected = app.view.main == super::MainView::Gantt && app.project.format.view == v;
            let shown_cap = if selected { format!("▸{cap}") } else { cap.to_string() };
            if big(ui, glyph, &shown_cap, tip, true) && !selected {
                app.execute(Command::SetGanttView(v), &ctx);
            }
        }
    });
    group(ui, "Other Views", |ui| {
        use super::MainView;
        for (v, glyph, cap, tip) in [
            (MainView::Board, "📋", "Board", "Kanban board: sprints, tickets, backlog, burndown"),
            (MainView::TaskUsage, "📆", "Task\nUsage", "Work per period for each task and its resources"),
            (MainView::ResourceSheet, "👥", "Resource\nSheet", "List of resources: rates, max units, calendars"),
            (MainView::ResourceUsage, "📈", "Resource\nUsage", "Work per period for each resource; overallocations in red"),
        ] {
            let selected = app.view.main == v;
            let shown = if selected { format!("▸{cap}") } else { cap.to_string() };
            if big(ui, glyph, &shown, tip, true) && !selected {
                app.execute(Command::ShowView(v), &ctx);
            }
        }
    });
    group(ui, "Data", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Table:").size(12.0));
                egui::ComboBox::from_id_salt("table_preset")
                    .selected_text(app.table.label())
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for t in super::columns::Table::ALL {
                            if ui.selectable_label(app.table == t, t.label()).clicked() {
                                app.execute(Command::SetTable(t), &ctx);
                            }
                        }
                    })
                    .response
                    .on_hover_text("Which columns the sheet shows (Project's built-in tables)");
                ui.label(RichText::new("Outline:").size(12.0));
                egui::ComboBox::from_id_salt("outline_lvl")
                    .selected_text("Show")
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(false, "All Subtasks").clicked() {
                            app.execute(Command::OutlineLevel(None), &ctx);
                        }
                        for l in 1..=9u32 {
                            if ui.selectable_label(false, format!("Level {l}")).clicked() {
                                app.execute(Command::OutlineLevel(Some(l)), &ctx);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter:").size(12.0));
                let customs: Vec<String> = app.project.views.filters.iter().map(|f| f.name.clone()).collect();
                egui::ComboBox::from_id_salt("filter")
                    .selected_text(app.filter_label(app.filter))
                    .width(130.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        for f in Filter::ALL {
                            if ui.selectable_label(app.filter == f, f.label()).clicked() {
                                app.execute(Command::SetFilter(f), &ctx);
                            }
                        }
                        if ui.selectable_label(matches!(app.filter, Filter::DateRange(..)), "Date Range…").clicked() {
                            app.execute(Command::DateRangeFilterDlg, &ctx);
                        }
                        if ui.selectable_label(matches!(app.filter, Filter::UsingResource(_)), "Using Resource…").clicked() {
                            app.execute(Command::UsingResourceFilterDlg, &ctx);
                        }
                        if !customs.is_empty() {
                            ui.separator();
                            for (k, name) in customs.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(app.filter == Filter::Custom(k), name).clicked() {
                                        app.execute(Command::SetFilter(Filter::Custom(k)), &ctx);
                                    }
                                    if ui.small_button("Edit").on_hover_text("Edit this filter").clicked() {
                                        app.execute(Command::EditFilter(k), &ctx);
                                    }
                                });
                            }
                        }
                        ui.separator();
                        if ui.selectable_label(false, "New Filter…").clicked() {
                            app.execute(Command::NewFilter, &ctx);
                        }
                        let mut rel = app.view.related_summaries;
                        if ui.checkbox(&mut rel, "Show Related Summary Rows").changed() {
                            app.execute(Command::ToggleRelatedSummaries, &ctx);
                        }
                    })
                    .response
                    .on_hover_text("Show only the tasks that match (F3 shows all tasks)");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Highlight:").size(12.0));
                let cur = app.view.highlight;
                let customs: Vec<String> = app.project.views.filters.iter().map(|f| f.name.clone()).collect();
                egui::ComboBox::from_id_salt("highlight")
                    .selected_text(if cur == Filter::AllTasks { "No Highlight".to_string() } else { app.filter_label(cur) })
                    .width(117.0)
                    .show_ui(ui, |ui| {
                        for f in Filter::ALL {
                            let label = if f == Filter::AllTasks { "No Highlight" } else { f.label() };
                            if ui.selectable_label(cur == f, label).clicked() {
                                app.execute(Command::SetHighlight(f), &ctx);
                            }
                        }
                        for (k, name) in customs.iter().enumerate() {
                            if ui.selectable_label(cur == Filter::Custom(k), name).clicked() {
                                app.execute(Command::SetHighlight(Filter::Custom(k)), &ctx);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Group by:").size(12.0));
                let cur = app.group;
                egui::ComboBox::from_id_salt("group_by")
                    .selected_text(cur.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for g in super::filters::GroupBy::BUILT_IN {
                            if ui.selectable_label(cur == g, g.label()).clicked() {
                                app.execute(Command::SetGroup(g), &ctx);
                            }
                        }
                        ui.separator();
                        if ui.selectable_label(matches!(cur, super::filters::GroupBy::Field(_)), "By Field…").clicked() {
                            app.execute(Command::CustomGroupDlg, &ctx);
                        }
                    })
                    .response
                    .on_hover_text("Group tasks under headers that total their dates, work and cost");
            });
        });
    });
    group(ui, "Zoom", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Timescale:").size(12.0));
                let cur = Timescale::for_ppd(app.gantt.ppd);
                egui::ComboBox::from_id_salt("timescale")
                    .selected_text(cur.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for t in Timescale::ALL {
                            if ui.selectable_label(cur == t, t.label()).clicked() {
                                app.execute(Command::SetTimescale(t), &ctx);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                if small(ui, "🔍", "Zoom In", "Zoom in (Ctrl+/)", true) {
                    app.execute(Command::ZoomIn, &ctx);
                }
                if small(ui, "🔎", "Zoom Out", "Zoom out (Ctrl+*)", true) {
                    app.execute(Command::ZoomOut, &ctx);
                }
            });
            if small(ui, "📏", "Timescale…", "Customize timescale tiers", true) {
                app.execute(Command::TimescaleDlg, &ctx);
            }
        });
        if big(ui, "🔭", "Entire\nProject", "Fit the whole project in the chart", true) {
            app.execute(Command::ZoomEntireProject, &ctx);
        }
        if big(ui, "🔬", "Selected\nTasks", "Fit the selected tasks in the chart", app.active_index().is_some()) {
            app.execute(Command::ZoomSelectedTasks, &ctx);
        }
    });
    group(ui, "Split View", |ui| {
        let mut tl = app.view.timeline;
        if ui.checkbox(&mut tl, RichText::new("Timeline").size(12.0)).changed() {
            app.execute(Command::ToggleTimeline, &ctx);
        }
    });
    group(ui, "Text Size", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Text:").size(12.0));
                if ui.add_enabled(app.prefs.text_size > MIN_TEXT_SIZE, egui::Button::new(RichText::new("A−").size(11.0)).small()).on_hover_text("Smaller table and chart text (Ctrl+Shift+<)").clicked() {
                    app.execute(Command::TextSmaller, &ctx);
                }
                let mut ts = app.prefs.text_size;
                let r = ui.add(egui::DragValue::new(&mut ts).range(MIN_TEXT_SIZE..=MAX_TEXT_SIZE).speed(0.2).suffix(" pt"));
                if r.changed() {
                    app.set_text_size(ts);
                }
                if ui.add_enabled(app.prefs.text_size < MAX_TEXT_SIZE, egui::Button::new(RichText::new("A+").size(13.0)).small()).on_hover_text("Larger table and chart text (Ctrl+Shift+>)").clicked() {
                    app.execute(Command::TextBigger, &ctx);
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Interface:").size(12.0));
                let cur = app.prefs.ui_zoom;
                egui::ComboBox::from_id_salt("ui_zoom")
                    .selected_text(format!("{}%", (cur * 100.0).round()))
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for z in super::prefs::ZOOM_STEPS {
                            if ui.selectable_label((cur - z).abs() < 0.001, format!("{}%", (z * 100.0).round())).clicked() {
                                app.execute(Command::SetUiZoom(z), &ctx);
                            }
                        }
                    });
            });
            if small(ui, "↺", "Reset", "Default text size and 100% interface (Ctrl+0)", true) {
                app.execute(Command::TextReset, &ctx);
            }
        });
    });
}

fn check(ui: &mut egui::Ui, on: bool, label: &str, tip: &str) -> bool {
    let mut v = on;
    ui.checkbox(&mut v, RichText::new(label).size(12.0)).on_hover_text(tip).changed()
}

fn format_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let has = app.active_index().is_some();
    group(ui, "Format", |ui| {
        if big(ui, "🔤", "Text\nStyles", "Text size, colors and bold for task categories", true) {
            app.execute(Command::TextStylesDlg, &ctx);
        }
        ui.vertical(|ui| {
            if small(ui, "▦", "Gridlines", "Row and column lines, date lines", true) {
                app.execute(Command::GridlinesDlg, &ctx);
            }
            if small(ui, "⛓", "Layout", "Link style, date format, bar height", true) {
                app.execute(Command::LayoutDlg, &ctx);
            }
            if small(ui, "📏", "Timescale", "Timescale tiers and nonworking time", true) {
                app.execute(Command::TimescaleDlg, &ctx);
            }
        });
    });
    group(ui, "Columns", |ui| {
        ui.vertical(|ui| {
            ui.menu_button(RichText::new("➕ Insert Column").size(12.0), |ui| {
                let present: Vec<_> = app.columns.iter().map(|s| s.id).collect();
                if let Some(c) = super::columns::insert_column_menu(ui, &present) {
                    let at = (app.active_col + 1).min(app.columns.len());
                    let w = (c.default_width() * app.prefs.text_size / DEFAULT_TEXT_SIZE).round();
                    app.columns.insert(at, super::ColumnState { id: c, width: w });
                    ui.close();
                }
            });
            if ui
                .add_enabled(
                    app.columns.get(app.active_col).is_some_and(|c| c.id != super::columns::ColumnId::Name),
                    egui::Button::new(RichText::new("✖ Hide Column").size(12.0)).frame(false),
                )
                .clicked()
            {
                app.columns.remove(app.active_col);
                app.active_col = app.active_col.min(app.columns.len() - 1);
            }
            if small(ui, "↺", "Reset Columns", "Restore the Entry table's default columns", true) {
                let k = app.prefs.text_size / DEFAULT_TEXT_SIZE;
                app.columns = super::columns::ColumnId::default_set()
                    .into_iter()
                    .map(|(id, width)| super::ColumnState { id, width: (width * k).round() })
                    .collect();
                app.active_col = 2;
            }
        });
    });
    group(ui, "Bar Styles", |ui| {
        if big(ui, "🎨", "Bar\nStyles", "Edit every bar style (colors, shapes, bar text)", true) {
            app.execute(Command::BarStylesDlg, &ctx);
        }
        if big(ui, "🖊", "Format\nBar", "Color the selected tasks' bars", has) {
            app.execute(Command::FormatBarDlg, &ctx);
        }
        ui.vertical(|ui| {
            let f = app.project.format.clone();
            if check(ui, f.show_critical, "Critical Tasks", "Show the critical path in red") {
                app.execute(Command::ToggleCritical, &ctx);
            }
            if check(ui, f.show_slack, "Slack", "Show total slack bars after tasks") {
                app.execute(Command::ToggleSlack, &ctx);
            }
            if check(ui, f.show_late, "Late Tasks", "Show tasks behind schedule at the status date") {
                app.execute(Command::ToggleLate, &ctx);
            }
        });
        ui.vertical(|ui| {
            let f = app.project.format.clone();
            if check(ui, f.show_baseline, "Baseline", "Show baseline bars under task bars") {
                app.execute(Command::ToggleBaselineBars, &ctx);
            }
            if check(ui, f.show_slippage, "Slippage", "Show how far each task's start has moved from its baseline start") {
                app.execute(Command::ToggleSlippage, &ctx);
            }
            ui.menu_button(RichText::new("🚦 Task Path").size(12.0), |ui| {
                use super::bars::PathRole::*;
                let v = app.view.clone();
                for (role, on) in [
                    (Predecessor, v.path_preds),
                    (DrivingPredecessor, v.path_driving),
                    (Successor, v.path_succs),
                    (DrivenSuccessor, v.path_driven),
                ] {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, ink(role.color()));
                        if ui.selectable_label(on, role.label()).clicked() {
                            app.execute(Command::TaskPath(role), &ctx);
                        }
                    });
                }
                ui.separator();
                if ui.button("Remove Highlighting").clicked() {
                    app.execute(Command::ClearTaskPath, &ctx);
                    ui.close();
                }
            })
            .response
            .on_hover_text("Highlight the selected task's predecessors and successors");
            if check(ui, f.layout.show_bar_text, "Bar Text", "Show text next to bars") {
                app.execute(Command::ToggleBarText, &ctx);
            }
        });
    });
    group(ui, "Tracking", |ui| {
        ui.vertical(|ui| {
            let f = app.project.format.clone();
            if check(ui, f.gridlines.progress_lines, "Progress Lines", "Draw a progress line at the status date: peaks to the left are behind schedule") {
                app.execute(Command::ToggleProgressLines, &ctx);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Show:").size(12.0));
                egui::ComboBox::from_id_salt("shown_baseline")
                    .selected_text(super::baseline_name(f.baseline_index as usize))
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for n in 0..=10u8 {
                            let has = app.project.has_baseline_n(n as usize);
                            let label = if has { super::baseline_name(n as usize) } else { format!("{} (not set)", super::baseline_name(n as usize)) };
                            if ui.selectable_label(f.baseline_index == n, label).clicked() {
                                app.execute(Command::SetShownBaseline(n), &ctx);
                            }
                        }
                    })
                    .response
                    .on_hover_text("Which baseline the baseline bars show");
            });
        });
    });
    group(ui, "Gantt Chart Style", |ui| {
        egui::Grid::new("style_gallery").spacing([3.0, 3.0]).show(ui, |ui| {
            for (k, p) in crate::format::STYLE_PRESETS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(34.0, 25.0), Sense::click());
                let pt = ui.painter();
                pt.rect_filled(r, 2.0, pal().bg);
                let col = if resp.hovered() { pal().accent } else { pal().grid };
                pt.rect_stroke(r, 2.0, Stroke::new(1.0, col), StrokeKind::Inside);
                pt.rect_filled(Rect::from_min_size(r.min + vec2(3.0, 4.0), vec2(28.0, 3.0)), 0.0, ink(p.summary));
                pt.rect_filled(Rect::from_min_size(r.min + vec2(4.0, 10.0), vec2(14.0, 5.0)), 1.0, ink(p.task));
                pt.rect_filled(Rect::from_min_size(r.min + vec2(16.0, 17.0), vec2(14.0, 5.0)), 1.0, ink(p.task));
                if resp.on_hover_text(p.name).clicked() {
                    app.execute(Command::ApplyStylePreset(k), &ctx);
                }
                if k % 5 == 4 {
                    ui.end_row();
                }
            }
        });
    });
    group(ui, "Show/Hide", |ui| {
        ui.vertical(|ui| {
            if check(ui, app.view.outline_number, "Outline Number", "Show WBS numbers before names") {
                app.execute(Command::ToggleOutlineNumber, &ctx);
            }
            if check(ui, app.view.project_summary, "Project Summary Task", "Show the project summary row") {
                app.execute(Command::ToggleProjectSummary, &ctx);
            }
            if check(ui, app.view.summary_rows, "Summary Tasks", "Show summary task rows") {
                app.execute(Command::ToggleSummaryRows, &ctx);
            }
        });
    });
}

fn help_tab(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    group(ui, "Help", |ui| {
        if big(ui, "⌨", "Keyboard\nShortcuts", "List of keyboard shortcuts (F1)", true) {
            app.execute(Command::Shortcuts, &ctx);
        }
        if big(ui, "ℹ", "About", "About ProjectFCG", true) {
            app.execute(Command::About, &ctx);
        }
    });
}
