//! Format-tab dialogs: Bar Styles, Format Bar, Text Styles, Gridlines,
//! Layout, Timescale, and Set/Clear Baseline.

use super::bars::{c32, preview};
use super::dialogs::{modal, ok_cancel};
use super::theme::*;
use super::ProjectFcgApp;
use crate::format::{
    BarCategory, BarField, BarStyle, BarStyles, DateFormat, EndShape, Gridlines, LayoutOptions, LinkStyle,
    NonworkingDraw, Pattern, Rgb, TextStyle, TextStyles, TimescaleSpec, TsUnit, STYLE_PRESETS,
};
use egui::{vec2, RichText, Sense};

#[derive(Clone, Debug)]
pub struct BarStylesDlg {
    pub styles: BarStyles,
    pub selected: BarCategory,
}

#[derive(Clone, Copy, Debug)]
pub struct FormatBarDlg {
    pub color: Option<Rgb>,
}

#[derive(Clone, Copy, Debug)]
pub struct TextStylesDlg {
    pub styles: TextStyles,
    pub size: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BaselineDlg {
    pub selected_only: bool,
    pub clear: bool,
    /// Which baseline (0 = Baseline, 1–10).
    pub n: usize,
}

/// Standard colours offered by Project's colour drop-downs.
pub const PALETTE: [Rgb; 20] = [
    Rgb(0, 0, 0),
    Rgb(89, 89, 89),
    Rgb(166, 166, 166),
    Rgb(217, 217, 217),
    Rgb(255, 255, 255),
    Rgb(192, 0, 0),
    Rgb(255, 0, 0),
    Rgb(255, 192, 0),
    Rgb(255, 255, 0),
    Rgb(146, 208, 80),
    Rgb(0, 176, 80),
    Rgb(0, 176, 240),
    Rgb(0, 112, 192),
    Rgb(0, 32, 96),
    Rgb(112, 48, 160),
    Rgb(138, 180, 232),
    Rgb(234, 128, 128),
    Rgb(150, 210, 214),
    Rgb(244, 177, 131),
    Rgb(180, 160, 220),
];

/// Palette swatches plus a custom colour button. Returns true on change.
pub fn color_field(ui: &mut egui::Ui, id: &str, c: &mut Rgb) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut arr = [c.0, c.1, c.2];
        if ui.color_edit_button_srgb(&mut arr).changed() {
            *c = Rgb(arr[0], arr[1], arr[2]);
            changed = true;
        }
        ui.menu_button("▾", |ui| {
            ui.label(RichText::new("Standard colors").size(11.0).color(pal().text_dim));
            egui::Grid::new(("pal", id)).spacing([3.0, 3.0]).show(ui, |ui| {
                for (k, p) in PALETTE.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                    ui.painter().rect_filled(r, 2.0, c32(*p));
                    ui.painter().rect_stroke(
                        r,
                        2.0,
                        egui::Stroke::new(if *p == *c { 2.0 } else { 1.0 }, pal().text_dim),
                        egui::StrokeKind::Inside,
                    );
                    if resp.clicked() {
                        *c = *p;
                        changed = true;
                        ui.close();
                    }
                    if k % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
        });
    });
    changed
}

fn field_combo(ui: &mut egui::Ui, id: &str, f: &mut BarField) {
    egui::ComboBox::from_id_salt(id).selected_text(f.label()).width(130.0).show_ui(ui, |ui| {
        for x in BarField::ALL {
            ui.selectable_value(f, x, x.label());
        }
    });
}

fn shape_combo(ui: &mut egui::Ui, id: &str, s: &mut EndShape) {
    egui::ComboBox::from_id_salt(id).selected_text(s.label()).width(100.0).show_ui(ui, |ui| {
        for x in EndShape::ALL {
            ui.selectable_value(s, x, x.label());
        }
    });
}

pub fn show_all(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    bar_styles(app, ctx);
    format_bar(app, ctx);
    text_styles(app, ctx);
    gridlines(app, ctx);
    layout(app, ctx);
    timescale(app, ctx);
    baseline(app, ctx);
}

fn bar_styles(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.bar_styles.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "bar_styles", "Bar Styles", 760.0, |ui| {
        // Gantt Chart Style gallery
        ui.label(RichText::new("Gantt Chart Style").strong());
        ui.horizontal_wrapped(|ui| {
            for p in STYLE_PRESETS {
                let (r, resp) = ui.allocate_exact_size(vec2(64.0, 30.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(r, 3.0, pal().bg);
                let stroke_col = if resp.hovered() { pal().accent } else { pal().grid };
                painter.rect_stroke(r, 3.0, egui::Stroke::new(1.0, stroke_col), egui::StrokeKind::Inside);
                let sum = egui::Rect::from_min_size(r.min + vec2(6.0, 5.0), vec2(48.0, 4.0));
                painter.rect_filled(sum, 0.0, ink(p.summary));
                let t1 = egui::Rect::from_min_size(r.min + vec2(8.0, 13.0), vec2(24.0, 6.0));
                let t2 = egui::Rect::from_min_size(r.min + vec2(30.0, 21.0), vec2(22.0, 6.0));
                painter.rect_filled(t1, 1.0, ink(p.task));
                painter.rect_filled(t2, 1.0, ink(p.task));
                let resp = resp.on_hover_text(p.name);
                if resp.clicked() {
                    st.styles.apply_preset(p);
                }
            }
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            // category list with previews
            ui.vertical(|ui| {
                ui.set_width(280.0);
                egui::Grid::new("bs_list").striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Appearance");
                    ui.strong("Show");
                    ui.end_row();
                    for cat in BarCategory::ALL {
                        let s = st.styles.get(cat);
                        if ui.selectable_label(st.selected == cat, cat.label()).clicked() {
                            st.selected = cat;
                        }
                        let (r, resp) = ui.allocate_exact_size(vec2(90.0, 18.0), Sense::click());
                        preview(ui.painter(), r, cat, &super::bars::adapt_style(s, cat));
                        if resp.clicked() {
                            st.selected = cat;
                        }
                        let mut show = s.show;
                        if ui.checkbox(&mut show, "").changed() {
                            let mut s2 = s;
                            s2.show = show;
                            st.styles.set(cat, s2);
                        }
                        ui.end_row();
                    }
                });
            });
            super::dialogs::vsep(ui, 420.0);
            // editor
            ui.vertical(|ui| {
                let cat = st.selected;
                let mut s: BarStyle = st.styles.get(cat);
                ui.label(RichText::new(cat.label()).strong().color(pal().heading));
                ui.label(RichText::new(cat.applies_to()).size(11.0).color(pal().text_dim));
                ui.add_space(4.0);
                ui.label(RichText::new("Bars").strong());
                egui::Grid::new("bs_bars").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Fill:");
                    color_field(ui, "fill", &mut s.fill);
                    ui.end_row();
                    ui.label("Border:");
                    color_field(ui, "border", &mut s.border);
                    ui.end_row();
                    ui.label("Pattern:");
                    egui::ComboBox::from_id_salt("bs_pat").selected_text(s.pattern.label()).show_ui(ui, |ui| {
                        for p in Pattern::ALL {
                            ui.selectable_value(&mut s.pattern, p, p.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Height:");
                    ui.add(egui::Slider::new(&mut s.height, 0.08..=0.95).show_value(false));
                    ui.end_row();
                    ui.label("Start shape:");
                    shape_combo(ui, "bs_ss", &mut s.start_shape);
                    ui.end_row();
                    ui.label("End shape:");
                    shape_combo(ui, "bs_es", &mut s.end_shape);
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Text").strong());
                egui::Grid::new("bs_text").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("Left:");
                    field_combo(ui, "tl", &mut s.text.left);
                    ui.end_row();
                    ui.label("Right:");
                    field_combo(ui, "tr", &mut s.text.right);
                    ui.end_row();
                    ui.label("Top:");
                    field_combo(ui, "tt", &mut s.text.top);
                    ui.end_row();
                    ui.label("Bottom:");
                    field_combo(ui, "tb", &mut s.text.bottom);
                    ui.end_row();
                    ui.label("Inside:");
                    field_combo(ui, "ti", &mut s.text.inside);
                    ui.end_row();
                });
                ui.add_space(4.0);
                let (r, _) = ui.allocate_exact_size(vec2(220.0, 28.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, pal().bg);
                preview(ui.painter(), r, cat, &super::bars::adapt_style(s, cat));
                if pal().dark {
                    ui.label(
                        RichText::new("Dark mode lightens colours that would be hard to see on the dark chart. The file keeps the colours you choose, so they print and look the same in light mode.")
                            .size(11.0)
                            .color(pal().text_dim),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Reset this style").clicked() {
                        s = cat.default_style();
                    }
                    if ui.button("Reset all styles").clicked() {
                        st.styles.reset();
                        s = cat.default_style();
                    }
                });
                st.styles.set(cat, s);
            });
        });
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.bar_styles = Some(st);
        return;
    }
    let styles = st.styles.clone();
    app.edit_project("Bar Styles", move |p| {
        p.format.styles = styles;
        Ok(())
    });
}

fn format_bar(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.format_bar.take() else { return };
    let rows = app.selected_rows();
    let names: Vec<String> = rows.iter().take(3).map(|&r| app.project.tasks[r].name.clone()).collect();
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "format_bar", "Format Bar", 380.0, |ui| {
        ui.label(format!(
            "{} task(s): {}{}",
            rows.len(),
            names.join(", "),
            if rows.len() > 3 { ", …" } else { "" }
        ));
        ui.add_space(4.0);
        let mut custom = st.color.is_some();
        ui.horizontal(|ui| {
            ui.radio_value(&mut custom, false, "Use the bar style");
            ui.radio_value(&mut custom, true, "Custom color");
        });
        if custom {
            let mut c = st.color.unwrap_or(Rgb(244, 177, 131));
            color_field(ui, "fb", &mut c);
            egui::Grid::new("fb_pal").spacing([3.0, 3.0]).show(ui, |ui| {
                for (k, p) in PALETTE.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
                    ui.painter().rect_filled(r, 2.0, c32(*p));
                    let w = if *p == c { 2.0 } else { 1.0 };
                    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(w, pal().text_dim), egui::StrokeKind::Inside);
                    if resp.clicked() {
                        c = *p;
                    }
                    if k % 10 == 9 {
                        ui.end_row();
                    }
                }
            });
            st.color = Some(c);
        } else {
            st.color = None;
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.format_bar = Some(st);
        return;
    }
    let ctx2 = ctx.clone();
    app.execute(super::commands::Command::SetBarColor(st.color), &ctx2);
}

fn text_style_row(ui: &mut egui::Ui, id: &str, label: &str, s: &mut TextStyle) {
    ui.label(label);
    let mut auto = s.color.is_none();
    if ui.checkbox(&mut auto, "Automatic").changed() {
        s.color = if auto { None } else { Some(Rgb(192, 0, 0)) };
    }
    if let Some(mut c) = s.color {
        color_field(ui, id, &mut c);
        s.color = Some(c);
    } else {
        ui.label("");
    }
    ui.checkbox(&mut s.bold, "Bold");
    ui.end_row();
}

fn text_styles(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.text_styles.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "text_styles", "Text Styles", 520.0, |ui| {
        ui.horizontal(|ui| {
            ui.label("Size (table and chart, all projects):");
            ui.add(egui::DragValue::new(&mut st.size).range(MIN_TEXT_SIZE..=MAX_TEXT_SIZE).speed(0.2).suffix(" pt"));
        });
        ui.separator();
        egui::Grid::new("ts").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.strong("Item to change");
            ui.strong("");
            ui.strong("Color");
            ui.strong("");
            ui.end_row();
            text_style_row(ui, "n", "All tasks", &mut st.styles.normal);
            text_style_row(ui, "c", "Critical tasks", &mut st.styles.critical);
            text_style_row(ui, "s", "Summary tasks", &mut st.styles.summary);
            text_style_row(ui, "m", "Milestone tasks", &mut st.styles.milestone);
            ui.label("Highlighted rows");
            ui.label("");
            color_field(ui, "hl", &mut st.styles.highlighted_fill);
            ui.label("");
            ui.end_row();
        });
        ui.add_space(4.0);
        if ui.button("Reset to defaults").clicked() {
            st.styles = TextStyles::default();
            st.size = DEFAULT_TEXT_SIZE;
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.text_styles = Some(st);
        return;
    }
    if (st.size - app.prefs.text_size).abs() > 0.01 {
        app.set_text_size(st.size);
    }
    let styles = st.styles;
    if styles != app.project.format.text {
        app.edit_project("Text Styles", move |p| {
            p.format.text = styles;
            Ok(())
        });
    }
}

fn gridlines(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut g) = app.dialogs.gridlines.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "gridlines", "Gridlines", 420.0, |ui| {
        egui::Grid::new("gl").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.checkbox(&mut g.gantt_rows, "Gantt rows");
            ui.checkbox(&mut g.sheet_rows, "Sheet rows");
            ui.end_row();
            ui.checkbox(&mut g.major_columns, "Top tier columns");
            ui.checkbox(&mut g.sheet_columns, "Sheet columns");
            ui.end_row();
            ui.checkbox(&mut g.minor_columns, "Bottom tier columns");
            ui.label("");
            ui.end_row();
            ui.checkbox(&mut g.current_date, "Current date");
            color_field(ui, "cd", &mut g.current_date_color);
            ui.end_row();
            ui.checkbox(&mut g.status_date, "Status date");
            color_field(ui, "sd", &mut g.status_date_color);
            ui.end_row();
            ui.checkbox(&mut g.project_start, "Project start");
            ui.checkbox(&mut g.project_finish, "Project finish");
            ui.end_row();
            ui.label("Line color:");
            color_field(ui, "lc", &mut g.line_color);
            ui.end_row();
        });
        if ui.button("Reset to defaults").clicked() {
            g = Gridlines::default();
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.gridlines = Some(g);
        return;
    }
    app.edit_project("Gridlines", move |p| {
        p.format.gridlines = g;
        Ok(())
    });
}

fn layout(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut l) = app.dialogs.layout.take() else { return };
    let sample = app.project.start;
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "layout", "Layout", 420.0, |ui| {
        ui.label(RichText::new("Links").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut l.links, LinkStyle::Elbow, "Elbow ┗━▶");
            ui.radio_value(&mut l.links, LinkStyle::Straight, "Straight ╲");
            ui.radio_value(&mut l.links, LinkStyle::Hidden, "Hidden");
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Bar options").strong());
        egui::Grid::new("lay").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Date format:");
            egui::ComboBox::from_id_salt("df").selected_text(l.date_format.format(sample)).width(170.0).show_ui(ui, |ui| {
                for f in DateFormat::ALL {
                    ui.selectable_value(&mut l.date_format, f, f.format(sample));
                }
            });
            ui.end_row();
            ui.label("Bar height:");
            ui.add(egui::Slider::new(&mut l.bar_scale, 0.5..=1.8).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut l.show_bar_text, "Show bar text");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut l.short_bar_dates, "Short dates in bar text (m/d)");
            ui.end_row();
        });
        if ui.button("Reset to defaults").clicked() {
            l = LayoutOptions::default();
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.layout = Some(l);
        return;
    }
    app.edit_project("Layout", move |p| {
        p.format.layout = l;
        Ok(())
    });
}

fn tier_editor(ui: &mut egui::Ui, id: &str, label: &str, t: &mut crate::format::Tier) {
    ui.label(label);
    egui::ComboBox::from_id_salt(id).selected_text(t.unit.label()).width(130.0).show_ui(ui, |ui| {
        for u in TsUnit::ALL {
            ui.selectable_value(&mut t.unit, u, u.label());
        }
    });
    ui.add(egui::DragValue::new(&mut t.count).range(1..=12).prefix("Count: "));
    ui.end_row();
}

fn timescale(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut t) = app.dialogs.timescale.take() else { return };
    let (mut ok, mut cancel) = (false, false);
    let keys = modal(ctx, "timescale", "Timescale", 460.0, |ui| {
        ui.checkbox(&mut t.auto, "Automatic tiers (follow the zoom level)");
        ui.add_enabled_ui(!t.auto, |ui| {
            ui.horizontal(|ui| {
                ui.label("Show:");
                ui.radio_value(&mut t.tiers, 1, "One tier");
                ui.radio_value(&mut t.tiers, 2, "Two tiers");
                ui.radio_value(&mut t.tiers, 3, "Three tiers");
            });
            egui::Grid::new("tiers").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                if t.tiers >= 3 {
                    tier_editor(ui, "t_top", "Top tier:", &mut t.top);
                }
                if t.tiers >= 2 {
                    tier_editor(ui, "t_mid", "Middle tier:", &mut t.middle);
                }
                tier_editor(ui, "t_bot", "Bottom tier:", &mut t.bottom);
            });
        });
        ui.separator();
        ui.label(RichText::new("Nonworking time").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut t.nonworking, NonworkingDraw::Behind, "Behind task bars");
            ui.radio_value(&mut t.nonworking, NonworkingDraw::InFront, "In front of task bars");
            ui.radio_value(&mut t.nonworking, NonworkingDraw::Hidden, "Do not draw");
        });
        ui.horizontal(|ui| {
            ui.label("Color:");
            color_field(ui, "nw", &mut t.nonworking_color);
        });
        if ui.button("Reset to defaults").clicked() {
            t = TimescaleSpec::default();
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.timescale = Some(t);
        return;
    }
    app.edit_project("Timescale", move |p| {
        p.format.timescale = t;
        Ok(())
    });
}

fn baseline(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut b) = app.dialogs.baseline.take() else { return };
    let n_sel = app.selected_rows().len();
    let (mut ok, mut cancel) = (false, false);
    let title = if b.clear { "Clear Baseline" } else { "Set Baseline" };
    let keys = modal(ctx, "baseline", title, 380.0, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut b.clear, false, "Set baseline");
            ui.radio_value(&mut b.clear, true, "Clear baseline");
        });
        let label = |n: usize, app: &ProjectFcgApp| match app.project.baseline_saved[n] {
            Some(d) => format!("{} (last saved {})", super::baseline_name(n), crate::model::fmt_datetime(d)),
            None => super::baseline_name(n),
        };
        egui::ComboBox::from_id_salt("baseline_n").width(300.0).selected_text(label(b.n, app)).show_ui(ui, |ui| {
            for n in 0..=10 {
                ui.selectable_value(&mut b.n, n, label(n, app));
            }
        });
        ui.label("For:");
        ui.radio_value(&mut b.selected_only, false, "Entire project");
        ui.add_enabled(
            n_sel > 0,
            egui::RadioButton::new(b.selected_only, format!("Selected tasks ({n_sel})")),
        )
        .clicked()
        .then(|| b.selected_only = true);
        if !b.clear && app.project.has_baseline_n(b.n) && !b.selected_only {
            ui.colored_label(pal().warn, format!("This replaces the existing {}.", super::baseline_name(b.n).to_lowercase()));
        }
        if b.n == 0 && !b.clear {
            ui.label(RichText::new("Baseline (0) is what variance, the Tracking Gantt and earned value compare against.").size(11.0).color(pal().text_dim));
        }
        (ok, cancel) = ok_cancel(ui);
    });
    if cancel || keys.esc {
        return;
    }
    if !(ok || keys.enter) {
        app.dialogs.baseline = Some(b);
        return;
    }
    let rows = app.selected_rows_sorted();
    let label = if b.clear { "Clear Baseline" } else { "Set Baseline" };
    app.edit_project(label, move |p| {
        let sel = if b.selected_only { Some(rows.as_slice()) } else { None };
        if b.clear {
            p.clear_baseline_n(b.n, sel);
        } else {
            p.set_baseline_n(b.n, sel);
        }
        Ok(())
    });
}

impl super::dialogs::Dialogs {
    pub fn open_bar_styles(&mut self, p: &crate::model::Project) {
        self.bar_styles = Some(BarStylesDlg {
            styles: p.format.styles.clone(),
            selected: BarCategory::Task,
        });
    }
}
