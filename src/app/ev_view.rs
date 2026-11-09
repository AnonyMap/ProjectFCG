//! Earned Value view (View ▸ Earned Value): Project's Earned Value Over
//! Time report and a period report in the layout of DoD's IPMDAR / CPR
//! Format 1 (current period, cumulative to date, at completion), in the
//! project's earned value terms (PMI, DoD or both).

use super::item_search::Item;
use super::ProjectFcgApp;
use crate::earned_value::{EarnedValue, EvPeriod, EvSeries, Metric};
use crate::model::{EacMethod, EvTerms};
use chrono::NaiveDateTime;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

/// Which rows the period report lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowLevel {
    #[default]
    Level1,
    Level2,
    Level3,
    All,
}

impl RowLevel {
    pub const ALL: [RowLevel; 4] = [RowLevel::Level1, RowLevel::Level2, RowLevel::Level3, RowLevel::All];
    pub fn label(self) -> &'static str {
        match self {
            RowLevel::Level1 => "Outline level 1",
            RowLevel::Level2 => "Levels 1–2",
            RowLevel::Level3 => "Levels 1–3",
            RowLevel::All => "All tasks",
        }
    }
    fn max(self) -> u32 {
        match self {
            RowLevel::Level1 => 1,
            RowLevel::Level2 => 2,
            RowLevel::Level3 => 3,
            RowLevel::All => u32::MAX,
        }
    }
}

/// What the view computed, and for which version of the plan.
struct Cache {
    key: (u64, NaiveDateTime, EvPeriod, usize),
    series: EvSeries,
    /// Every task's figures at the start of the current period.
    period_start: Vec<EarnedValue>,
}

#[derive(Default)]
pub struct EvViewState {
    pub period: EvPeriod,
    pub rows: RowLevel,
    cache: Option<Cache>,
}

/// PV, EV and AC colours, readable in both themes.
pub fn colors() -> (Color32, Color32, Color32) {
    if super::theme::pal().dark {
        (Color32::from_rgb(110, 165, 235), Color32::from_rgb(110, 200, 125), Color32::from_rgb(240, 125, 105))
    } else {
        (Color32::from_rgb(40, 110, 200), Color32::from_rgb(40, 140, 70), Color32::from_rgb(200, 70, 50))
    }
}

fn money(app: &ProjectFcgApp, v: f64) -> String {
    crate::resources::fmt_money(v, &app.project.settings.currency_symbol)
}

/// Money in a few characters, for the axis: $1.2K, $3.4M.
fn short_money(sym: &str, v: f64) -> String {
    let a = v.abs();
    let (x, suffix) = if a >= 1e9 {
        (v / 1e9, "B")
    } else if a >= 1e6 {
        (v / 1e6, "M")
    } else if a >= 1e3 {
        (v / 1e3, "K")
    } else {
        (v, "")
    };
    let n = if suffix.is_empty() || x.abs() >= 100.0 { format!("{x:.0}") } else { format!("{:.1}", x).trim_end_matches(".0").to_string() };
    format!("{sym}{n}{suffix}")
}

fn axis_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if v <= m * mag {
            return m * mag;
        }
    }
    10.0 * mag
}

fn index(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "–".into())
}

fn percent(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.1}%")).unwrap_or_else(|| "–".into())
}

/// One sentence on where the plan stands.
pub fn verdict(ev: &EarnedValue, terms: EvTerms, money: impl Fn(f64) -> String) -> String {
    if ev.bac <= 0.0 {
        return "Set a baseline (Project ▸ Set Baseline) to measure earned value.".into();
    }
    let ev_name = terms.abbr(Metric::Ev);
    let sched = match ev.spi() {
        Some(v) if v < 0.95 => format!("behind schedule (SPI {v:.2}: {:.0}% of the work planned by now is done)", v * 100.0),
        Some(v) if v > 1.05 => format!("ahead of schedule (SPI {v:.2})"),
        Some(_) => "on schedule".into(),
        None => "not started per the baseline yet".into(),
    };
    let cost = match ev.cpi() {
        Some(v) if v < 0.95 => format!("over budget (every {} of {ev_name} is costing {})", money(1.0), money(1.0 / v)),
        Some(v) if v > 1.05 => format!("under budget (CPI {v:.2})"),
        Some(_) => "on budget".into(),
        None => "no actual cost yet".into(),
    };
    format!("The project is {sched}, and {cost}.")
}

impl ProjectFcgApp {
    fn ev_cache(&mut self) -> &Cache {
        let status = self.project.status_or_now();
        // "now" moves: recompute at most once a minute
        let at = status.with_second(0).unwrap_or(status).with_nanosecond(0).unwrap_or(status);
        let key = (self.history.generation(), at, self.ev_view.period, self.project.tasks.len());
        if self.ev_view.cache.as_ref().is_none_or(|c| c.key != key) {
            let series = self.project.earned_value_series(self.ev_view.period);
            let period_start = self.project.earned_value_at(self.ev_view.period.back(status));
            self.ev_view.cache = Some(Cache { key, series, period_start });
        }
        self.ev_view.cache.as_ref().unwrap()
    }
}

use chrono::Timelike;

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui, rect: Rect) {
    let pal = super::theme::pal();
    ui.painter().rect_filled(rect, 0.0, pal.bg);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 38.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 4.0))), |ui| toolbar(app, ui));
    ui.painter().hline(rect.x_range(), bar.bottom(), Stroke::new(1.0, pal.grid));
    let body = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
    let has_baseline = (0..app.project.tasks.len()).any(|i| app.project.ev_baseline_of(i).is_some());
    if !has_baseline {
        ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink(24.0)), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                let n = app.project.settings.ev_baseline;
                let which = if n == 0 { "the baseline".to_string() } else { format!("Baseline {n}") };
                ui.label(RichText::new(format!("Earned value is measured against {which}, which hasn't been set.")).size(super::theme::text_size() + 2.0));
                ui.add_space(8.0);
                ui.label("Save a baseline of the plan, then record progress (% complete, actual dates) as the work happens.");
                ui.add_space(12.0);
                if ui.button("Set Baseline…").clicked() {
                    app.execute(super::commands::Command::SetBaseline, &ui.ctx().clone());
                }
                ui.add_space(4.0);
                if ui.button("Earned value options…").clicked() {
                    app.execute(super::commands::Command::Options, &ui.ctx().clone());
                }
            });
        });
        return;
    }
    let top_h = (body.height() * 0.52).clamp(280.0, 440.0);
    let top = Rect::from_min_size(body.min, vec2(body.width(), top_h));
    let tiles_w = (top.width() * 0.34).clamp(300.0, 430.0);
    let chart_r = Rect::from_min_max(top.min, pos2(top.right() - tiles_w, top.bottom()));
    let tiles_r = Rect::from_min_max(pos2(chart_r.right(), top.top()), top.max);
    chart(app, ui, chart_r.shrink2(vec2(12.0, 10.0)));
    ui.painter().vline(tiles_r.left(), tiles_r.y_range(), Stroke::new(1.0, pal.grid));
    ui.scope_builder(egui::UiBuilder::new().max_rect(tiles_r.shrink2(vec2(12.0, 8.0))), |ui| tiles(app, ui));
    ui.painter().hline(body.x_range(), top.bottom(), Stroke::new(1.0, pal.grid));
    let table_r = Rect::from_min_max(pos2(body.left(), top.bottom()), body.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(table_r.shrink2(vec2(10.0, 6.0))), |ui| period_table(app, ui));
}

fn toolbar(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        let s = app.project.settings.clone();
        ui.label(RichText::new("Earned Value").strong().size(super::theme::text_size() + 2.0));
        ui.separator();
        ui.label("Status date:");
        super::ribbon::status_date_field(app, ui, 90.0);
        if app.project.status_date.is_none() {
            ui.weak("(today)");
        }
        ui.separator();
        ui.label("Against:");
        let bl = |n: u8| if n == 0 { "Baseline".to_string() } else { format!("Baseline {n}") };
        let mut pick = s.ev_baseline;
        egui::ComboBox::from_id_salt("ev_bl").selected_text(bl(pick)).width(100.0).show_ui(ui, |ui| {
            for n in 0..=10u8 {
                let saved = app.project.baseline_saved[n as usize].is_some();
                let label = if saved { bl(n) } else { format!("{} (not set)", bl(n)) };
                ui.selectable_value(&mut pick, n, label);
            }
        });
        if pick != s.ev_baseline {
            app.edit_project("Baseline for Earned Value", move |p| {
                p.settings.ev_baseline = pick;
                Ok(())
            });
        }
        ui.label("Terms:");
        let mut terms = s.ev_terms;
        egui::ComboBox::from_id_salt("ev_terms").selected_text(match terms {
            EvTerms::Pmi => "PMI",
            EvTerms::Dod => "DoD",
            EvTerms::Both => "Both",
        })
        .width(70.0)
        .show_ui(ui, |ui| {
            for t in EvTerms::ALL {
                ui.selectable_value(&mut terms, t, t.label());
            }
        })
        .response
        .on_hover_text("PMI: PV, EV, AC.  DoD EVMS: BCWS, BCWP, ACWP.  Both, as Project shows them.");
        if terms != s.ev_terms {
            app.edit_project("Earned Value Terms", move |p| {
                p.settings.ev_terms = terms;
                Ok(())
            });
        }
        ui.label("EAC:");
        let mut eac = s.eac_method;
        egui::ComboBox::from_id_salt("ev_eac").selected_text(eac.label()).width(200.0).show_ui(ui, |ui| {
            for m in EacMethod::ALL {
                ui.selectable_value(&mut eac, m, m.label());
            }
        });
        if eac != s.eac_method {
            app.edit_project("Estimate at Completion", move |p| {
                p.settings.eac_method = eac;
                Ok(())
            });
        }
        ui.label("Period:");
        egui::ComboBox::from_id_salt("ev_period").selected_text(app.ev_view.period.label()).width(70.0).show_ui(ui, |ui| {
            for p in [EvPeriod::Week, EvPeriod::Month] {
                ui.selectable_value(&mut app.ev_view.period, p, p.label());
            }
        });
    });
}

fn chart(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect) {
    let th = super::theme::pal();
    let terms = app.project.settings.ev_terms;
    let sym = app.project.settings.currency_symbol.clone();
    let (cpv, cev, cac) = colors();
    let period = app.ev_view.period;
    let cache = app.ev_cache();
    let s = &cache.series;
    let p = ui.painter_at(r);
    let sf = FontId::proportional(super::theme::small_size() - 1.0);
    p.text(r.left_top(), Align2::LEFT_TOP, "Earned value over time", FontId::proportional(super::theme::text_size() + 1.0), th.heading);
    if s.points.len() < 2 {
        p.text(r.center(), Align2::CENTER_CENTER, "Nothing to chart yet", sf, th.text_dim);
        return;
    }
    let plot = Rect::from_min_max(r.min + vec2(52.0, 26.0), r.max - vec2(8.0, 40.0));
    let (t0, t1) = (s.points[0].0, s.points.last().unwrap().0);
    let eac = s.now.eac();
    let finish = s.finish.unwrap_or(t1).max(s.status);
    let t1 = t1.max(finish);
    let span = (t1 - t0).num_minutes().max(1) as f32;
    let x = |t: NaiveDateTime| plot.left() + plot.width() * ((t - t0).num_minutes() as f32 / span).clamp(0.0, 1.0);
    let top = s.points.iter().map(|(_, e)| e.bcws.max(e.bcwp).max(e.acwp)).fold(s.now.bac.max(eac), f64::max);
    let ymax = axis_max(top * 1.04);
    let y = |v: f64| plot.bottom() - plot.height() * (v / ymax).clamp(-0.05, 1.05) as f32;
    for k in 0..=4 {
        let v = ymax * k as f64 / 4.0;
        let yy = y(v);
        p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], Stroke::new(1.0, th.grid));
        p.text(pos2(plot.left() - 5.0, yy), Align2::RIGHT_CENTER, short_money(&sym, v), sf.clone(), th.text_dim);
    }
    // date labels: as many as fit
    let n = s.points.len();
    let step = ((n as f32 * 56.0 / plot.width()).ceil() as usize).max(1);
    for (k, (t, _)) in s.points.iter().enumerate() {
        if k % step == 0 {
            let label = match period {
                EvPeriod::Month => t.format("%b %y").to_string(),
                EvPeriod::Week => t.format("%-m/%-d").to_string(),
            };
            let lx = x(*t);
            let anchor = if lx > plot.right() - 24.0 { Align2::RIGHT_TOP } else { Align2::CENTER_TOP };
            p.text(pos2(lx, plot.bottom() + 4.0), anchor, label, sf.clone(), th.text_dim);
        }
    }
    // the budget, and the status date
    p.add(egui::Shape::dashed_line(&[pos2(plot.left(), y(s.now.bac)), pos2(plot.right(), y(s.now.bac))], Stroke::new(1.0, th.text_dim), 6.0, 4.0));
    p.text(pos2(plot.left() + 4.0, y(s.now.bac) - 2.0), Align2::LEFT_BOTTOM, terms.abbr(Metric::Bac), sf.clone(), th.text_dim);
    let sx = x(s.status);
    p.line_segment([pos2(sx, plot.top()), pos2(sx, plot.bottom())], Stroke::new(1.5, th.today));
    // planned value all the way; earned value and actual cost to the status date
    let pv: Vec<egui::Pos2> = s.points.iter().map(|(t, e)| pos2(x(*t), y(e.bcws))).collect();
    p.add(egui::Shape::line(pv, Stroke::new(2.0, cpv)));
    let upto = |f: fn(&EarnedValue) -> f64| -> Vec<egui::Pos2> {
        let mut v: Vec<egui::Pos2> = s.points.iter().filter(|(t, _)| *t < s.status).map(|(t, e)| pos2(x(*t), y(f(e)))).collect();
        v.push(pos2(sx, y(f(&s.now))));
        v
    };
    p.add(egui::Shape::line(upto(|e| e.acwp), Stroke::new(2.0, cac)));
    p.add(egui::Shape::line(upto(|e| e.bcwp), Stroke::new(2.5, cev)));
    // the estimate at completion: from today's actual cost to the forecast finish
    if s.now.acwp > 0.0 || s.now.bcwp > 0.0 {
        let fx = x(finish);
        p.add(egui::Shape::dashed_line(&[pos2(sx, y(s.now.acwp)), pos2(fx.max(sx + 1.0), y(eac))], Stroke::new(1.5, cac), 5.0, 4.0));
        p.circle_filled(pos2(fx.max(sx + 1.0), y(eac)), 3.5, cac);
        p.text(pos2(fx.max(sx + 1.0) - 4.0, y(eac) - 4.0), Align2::RIGHT_BOTTOM, format!("{} {}", terms.abbr(Metric::Eac), short_money(&sym, eac)), sf.clone(), cac);
    }
    // legend
    let ly = r.bottom() - 6.0;
    let mut lx = plot.left();
    for (c, name, dashed) in [(cpv, terms.abbr(Metric::Pv), false), (cev, terms.abbr(Metric::Ev), false), (cac, terms.abbr(Metric::Ac), false), (cac, terms.abbr(Metric::Eac), true), (th.today, "Status date", false)] {
        if dashed {
            p.add(egui::Shape::dashed_line(&[pos2(lx, ly), pos2(lx + 18.0, ly)], Stroke::new(1.5, c), 4.0, 3.0));
        } else {
            p.line_segment([pos2(lx, ly), pos2(lx + 18.0, ly)], Stroke::new(2.5, c));
        }
        let g = p.layout_no_wrap(name.to_string(), sf.clone(), th.text_dim);
        let w = g.size().x;
        p.galley(pos2(lx + 22.0, ly - g.size().y / 2.0), g, th.text_dim);
        lx += 22.0 + w + 16.0;
    }
    // hover: the figures at that date
    let resp = ui.interact(plot, ui.scope_id().with("ev_chart"), Sense::hover());
    if let Some(h) = resp.hover_pos() {
        let (k, (t, e)) = s.points.iter().enumerate().min_by_key(|(_, (t, _))| (x(*t) - h.x).abs() as i64).unwrap();
        let _ = k;
        let px = x(*t);
        p.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], Stroke::new(1.0, th.text_dim));
        let past = *t <= s.status;
        let mut text = format!("{}\n{} {}", crate::model::fmt_date(*t), terms.abbr(Metric::Pv), crate::resources::fmt_money(e.bcws, &sym));
        if past {
            text += &format!("\n{} {}\n{} {}", terms.abbr(Metric::Ev), crate::resources::fmt_money(e.bcwp, &sym), terms.abbr(Metric::Ac), crate::resources::fmt_money(e.acwp, &sym));
        }
        resp.on_hover_text_at_pointer(text);
    }
}

fn tile(ui: &mut egui::Ui, abbr: &str, name: &str, value: String, tone: Option<bool>) {
    let th = super::theme::pal();
    let color = match tone {
        Some(true) => colors().1,
        Some(false) => th.warn,
        None => th.text,
    };
    egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.grid)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(7, 3)).show(ui, |ui| {
        ui.set_width(98.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.vertical(|ui| {
            ui.label(RichText::new(abbr).size(super::theme::small_size() - 1.0).color(th.text_dim));
            ui.label(RichText::new(value).strong().color(color));
        });
    })
    .response
    .on_hover_text(name);
}

fn tiles(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let ev = app.ev_cache().series.now;
    let t = app.project.settings.ev_terms;
    let good = |v: Option<f64>| v.map(|x| x >= 0.995);
    let sign = |v: f64| (v.abs() >= 0.005).then_some(v >= 0.0);
    let m = |v: f64| money(app, v);
    let rows: Vec<(Metric, String, Option<bool>)> = vec![
        (Metric::Pv, m(ev.bcws), None),
        (Metric::Ev, m(ev.bcwp), None),
        (Metric::Ac, m(ev.acwp), None),
        (Metric::Sv, m(ev.sv()), sign(ev.sv())),
        (Metric::Cv, m(ev.cv()), sign(ev.cv())),
        (Metric::Spi, index(ev.spi()), good(ev.spi())),
        (Metric::SvPercent, percent(ev.sv_percent()), ev.sv_percent().and_then(sign)),
        (Metric::CvPercent, percent(ev.cv_percent()), ev.cv_percent().and_then(sign)),
        (Metric::Cpi, index(ev.cpi()), good(ev.cpi())),
        (Metric::Bac, m(ev.bac), None),
        (Metric::Eac, m(ev.eac()), None),
        (Metric::Vac, m(ev.vac()), sign(ev.vac())),
        (Metric::Etc, m(ev.etc()), None),
        (Metric::Tcpi, index(ev.tcpi()), ev.tcpi().map(|x| x <= 1.005)),
        (Metric::TcpiEac, index(ev.tcpi_eac()), None),
    ];
    egui::ScrollArea::vertical().id_salt("ev_tiles").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        egui::Grid::new("ev_tiles_grid").num_columns(3).spacing([5.0, 5.0]).show(ui, |ui| {
            for (k, (metric, value, tone)) in rows.into_iter().enumerate() {
                tile(ui, t.abbr(metric), &t.long(metric), value, tone);
                if k % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} complete, {} spent, {} planned by now (of {}).",
                percent(ev.percent_complete()),
                percent(ev.percent_spent()),
                percent(ev.percent_scheduled()),
                t.abbr(Metric::Bac)
            ))
            .color(super::theme::pal().text_dim),
        );
        ui.label(verdict(&ev, t, |v| money(app, v)));
    });
}

/// A left-aligned cell `w` wide.
fn name_cell(ui: &mut egui::Ui, w: f32, label: egui::Label) -> egui::Response {
    ui.allocate_ui_with_layout(vec2(w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(w);
        ui.add(label)
    })
    .inner
}

fn period_table(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let th = super::theme::pal();
    let t = app.project.settings.ev_terms;
    let ctx = ui.ctx().clone();
    let status = app.project.status_or_now();
    let back = app.ev_view.period.back(status);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Period report").strong());
        ui.weak(format!(
            "current period ({} to {}), cumulative to date, at completion{}",
            crate::model::fmt_date(back),
            crate::model::fmt_date(status),
            if t == EvTerms::Pmi { "" } else { " — the layout of IPMDAR / CPR Format 1" }
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("ev_rows").selected_text(app.ev_view.rows.label()).width(120.0).show_ui(ui, |ui| {
                for r in RowLevel::ALL {
                    ui.selectable_value(&mut app.ev_view.rows, r, r.label());
                }
            });
            ui.label("Show:");
        });
    });
    let start = app.ev_cache().period_start.clone();
    let p = &app.project;
    let max_level = app.ev_view.rows.max();
    let name_w = (ui.available_width() * 0.16).clamp(130.0, 220.0);
    let rows: Vec<usize> = (0..p.tasks.len()).filter(|&i| p.tasks[i].outline_level <= max_level).collect();
    let cur = |i: usize| p.earned_value(i);
    let mut go: Option<usize> = None;
    let mut total_cum = p.project_earned_value();
    total_cum.eac_method = p.settings.eac_method;
    let total_start = p.total_earned_value(&start);
    let money = |v: f64| crate::resources::fmt_money(v, &p.settings.currency_symbol);
    let heads = [
        t.abbr(Metric::Pv),
        t.abbr(Metric::Ev),
        t.abbr(Metric::Ac),
        t.abbr(Metric::Sv),
        t.abbr(Metric::Cv),
        t.abbr(Metric::Pv),
        t.abbr(Metric::Ev),
        t.abbr(Metric::Ac),
        t.abbr(Metric::Sv),
        t.abbr(Metric::Cv),
        t.abbr(Metric::Bac),
        t.abbr(Metric::Eac),
        t.abbr(Metric::Vac),
    ];
    let cells = |e: &EarnedValue, since: &EarnedValue| -> Vec<f64> {
        let d = e.since(since);
        vec![d.bcws, d.bcwp, d.acwp, d.sv(), d.cv(), e.bcws, e.bcwp, e.acwp, e.sv(), e.cv(), e.bac, e.eac(), e.vac()]
    };
    egui::ScrollArea::both().id_salt("ev_table").auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("ev_period").striped(true).spacing([12.0, 4.0]).min_col_width(64.0).show(ui, |ui| {
            // a row for the group headings, painted across their columns below
            ui.label("");
            ui.end_row();
            name_cell(ui, name_w, egui::Label::new(RichText::new("Task").strong()));
            let mut cols: Vec<Rect> = vec![];
            for h in heads {
                cols.push(ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(h).strong())).response.rect);
            }
            ui.end_row();
            if cols.len() == 13 {
                let painter = ui.painter();
                for (a, b, g) in [(0, 4, "Current period"), (5, 9, "Cumulative to date"), (10, 12, "At completion")] {
                    let (l, r) = (cols[a].left(), cols[b].right());
                    let y = cols[a].top() - 6.0;
                    painter.text(pos2((l + r) / 2.0, y - 2.0), Align2::CENTER_BOTTOM, g, FontId::proportional(super::theme::text_size()), th.heading);
                    painter.hline(l..=r, y, Stroke::new(1.0, th.grid));
                }
            }
            for &i in &rows {
                let task = &p.tasks[i];
                let indent = "   ".repeat(task.outline_level.saturating_sub(1) as usize);
                let name = format!("{indent}{}", task.name);
                let txt = if task.calc.is_summary { RichText::new(name).strong() } else { RichText::new(name) };
                if name_cell(ui, name_w, egui::Label::new(txt).sense(Sense::click()).truncate()).on_hover_text("Double-click: go to the task").double_clicked() {
                    go = Some(i);
                }
                for v in cells(&cur(i), &start[i]) {
                    let color = if v < -0.005 { th.warn } else { th.text };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(money(v)).color(color)));
                }
                ui.end_row();
            }
            ui.label(RichText::new("Total").strong());
            for v in cells(&total_cum, &total_start) {
                let color = if v < -0.005 { th.warn } else { th.text };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(money(v)).strong().color(color)));
            }
            ui.end_row();
        });
    });
    if let Some(i) = go {
        app.go_to_item(Item::Task(i), false, &ctx);
    }
}
