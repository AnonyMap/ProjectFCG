//! File ▸ Print: the Gantt view as a PDF, as many pages as the rows need,
//! with the sheet's columns on the left and the bars (and links) for the
//! whole plan, or a date range, across the page. What prints is what the
//! view shows: filters, groups and collapsed summaries apply.

use super::columns::{cell_text, ColumnId};
use super::commands::Command;
use super::dialogs::{date_field, modal};
use super::theme::*;
use super::{DisplayRow, ProjectFcgApp};
use crate::format::{BarCategory, Rgb as FRgb};
use crate::pdf::{fit_text, Font, Page, Paper, Rgb};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use egui::RichText;

#[derive(Clone, Debug, PartialEq)]
pub struct PrintOptions {
    pub paper: Paper,
    pub landscape: bool,
    /// Days to show; `None`: the whole plan.
    pub range: Option<(NaiveDate, NaiveDate)>,
    pub links: bool,
    /// Repeat the column headings and timescale on every page.
    pub header_every_page: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions { paper: Paper::Letter, landscape: true, range: None, links: true, header_every_page: true }
    }
}

#[derive(Clone, Debug)]
pub struct PrintDlg {
    pub opts: PrintOptions,
    pub whole: bool,
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}

const MARGIN: f32 = 28.0;
const ROW_H: f32 = 13.0;
const FONT: f32 = 7.5;
const HEAD_H: f32 = 26.0;
const TITLE_H: f32 = 30.0;
const FOOT_H: f32 = 18.0;

fn rgb(c: FRgb) -> Rgb {
    Rgb(c.0, c.1, c.2)
}

const GRID: Rgb = Rgb(220, 222, 228);
const HEAD_BG: Rgb = Rgb(240, 241, 244);
const DIM: Rgb = Rgb(110, 112, 120);
const GROUP_BG: Rgb = Rgb(232, 238, 248);
const LINK: Rgb = Rgb(70, 100, 160);

/// Width in points a column gets on paper.
fn col_width(c: ColumnId) -> f32 {
    match c {
        ColumnId::Name => 165.0,
        ColumnId::Predecessors | ColumnId::Successors | ColumnId::ResourceNames | ColumnId::Notes => 80.0,
        ColumnId::Start | ColumnId::Finish | ColumnId::EarlyStart | ColumnId::EarlyFinish | ColumnId::LateStart | ColumnId::LateFinish | ColumnId::ActualStart | ColumnId::ActualFinish | ColumnId::BaselineStart | ColumnId::BaselineFinish | ColumnId::ConstraintDate | ColumnId::Deadline => 58.0,
        _ => 46.0,
    }
}

/// The sheet columns that go on paper: the view's, in order, up to about
/// 45% of the width (the Name column is always kept).
pub fn print_columns(app: &ProjectFcgApp, width: f32) -> Vec<(ColumnId, f32)> {
    let limit = width * 0.5;
    let mut out = vec![];
    let mut w = 22.0; // ID
    for c in app.columns.iter().map(|c| c.id).filter(|c| !matches!(c, ColumnId::Indicators | ColumnId::Mode)) {
        let cw = col_width(c);
        if w + cw > limit && c != ColumnId::Name {
            continue;
        }
        w += cw;
        out.push((c, cw));
    }
    if !out.iter().any(|c| c.0 == ColumnId::Name) {
        out.insert(0, (ColumnId::Name, col_width(ColumnId::Name)));
    }
    out
}

struct Scale {
    x0: f32,
    start: NaiveDateTime,
    per_day: f32,
}

impl Scale {
    fn x(&self, d: NaiveDateTime) -> f32 {
        let days = (d - self.start).num_minutes() as f32 / 1440.0;
        self.x0 + days * self.per_day
    }
}

fn month_name(m: u32) -> &'static str {
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m as usize + 11) % 12]
}

/// Two timescale tiers sized to the scale: (top, bottom) boundaries with labels.
fn tiers(sc: &Scale, end: NaiveDateTime) -> (Vec<(NaiveDate, String)>, Vec<(NaiveDate, String)>) {
    let first = sc.start.date();
    let last = end.date();
    let mut days = vec![];
    let mut d = first;
    while d <= last {
        days.push(d);
        d = d.succ_opt().unwrap_or(d);
        if days.len() > 20_000 {
            break;
        }
    }
    let pd = sc.per_day;
    let month_start = |d: &NaiveDate| d.day() == 1 || *d == first;
    let week_start = |d: &NaiveDate| d.weekday() == chrono::Weekday::Mon || *d == first;
    let quarter_start = |d: &NaiveDate| (d.day() == 1 && (d.month() - 1) % 3 == 0) || *d == first;
    let year_start = |d: &NaiveDate| d.ordinal() == 1 || *d == first;
    let pick = |f: &dyn Fn(&NaiveDate) -> bool, label: &dyn Fn(&NaiveDate) -> String| -> Vec<(NaiveDate, String)> { days.iter().filter(|d| f(d)).map(|d| (*d, label(d))).collect() };
    if pd >= 14.0 {
        (pick(&week_start, &|d| format!("{} {}, '{:02}", month_name(d.month()), d.day(), d.year() % 100)), pick(&|_| true, &|d| ["M", "T", "W", "T", "F", "S", "S"][d.weekday().num_days_from_monday() as usize].to_string()))
    } else if pd >= 2.2 {
        (pick(&month_start, &|d| format!("{} {}", month_name(d.month()), d.year())), pick(&week_start, &|d| format!("{}/{}", d.month(), d.day())))
    } else if pd >= 0.5 {
        (pick(&quarter_start, &|d| format!("Q{} {}", (d.month() - 1) / 3 + 1, d.year())), pick(&month_start, &|d| month_name(d.month()).to_string()))
    } else {
        (pick(&year_start, &|d| d.year().to_string()), pick(&quarter_start, &|d| format!("Q{}", (d.month() - 1) / 3 + 1)))
    }
}

fn diamond(pg: &mut Page, cx: f32, cy: f32, r: f32, fill: Rgb) {
    pg.polygon(&[(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)], fill);
}

/// Lay the view out on pages and return the PDF.
pub fn print_pdf(app: &ProjectFcgApp, o: &PrintOptions) -> Vec<u8> {
    let (pw, ph) = o.paper.size();
    let (w, h) = if o.landscape { (ph, pw) } else { (pw, ph) };
    let p = &app.project;
    let rows = app.display_rows();
    let groups = app.groups.borrow().clone();
    let cols = print_columns(app, w - 2.0 * MARGIN);
    let table_w = 22.0 + cols.iter().map(|c| c.1).sum::<f32>();
    let tx0 = MARGIN;
    let cx0 = tx0 + table_w + 4.0;
    let cx1 = w - MARGIN;
    // time span
    let (s, f) = match o.range {
        Some((a, b)) => (a.and_time(chrono::NaiveTime::MIN), b.succ_opt().unwrap_or(b).and_time(chrono::NaiveTime::MIN)),
        None => {
            let (s, f) = app.project_span();
            (s - chrono::Duration::days(1), f + chrono::Duration::days(2))
        }
    };
    let span_days = ((f - s).num_minutes() as f32 / 1440.0).max(1.0);
    let sc = Scale { x0: cx0, start: s, per_day: (cx1 - cx0) / span_days };
    let (top, bottom) = tiers(&sc, f);

    let body_top_first = MARGIN + TITLE_H + HEAD_H;
    let body_top = if o.header_every_page { body_top_first } else { MARGIN + 8.0 };
    let body_bottom = h - MARGIN - FOOT_H;
    let per_first = (((body_bottom - body_top_first) / ROW_H).floor() as usize).max(1);
    let per_page = (((body_bottom - if o.header_every_page { body_top_first } else { body_top }) / ROW_H).floor() as usize).max(1);
    // rows → pages
    let mut chunks: Vec<std::ops::Range<usize>> = vec![];
    let mut k = 0;
    while k < rows.len() || chunks.is_empty() {
        let n = if chunks.is_empty() { per_first } else { per_page };
        chunks.push(k..(k + n).min(rows.len()));
        k += n;
    }
    let npages = chunks.len();
    let printed = chrono::Local::now().naive_local();
    let fmt = &p.format;
    let grouped = app.group != super::filters::GroupBy::None;
    let mut pages = vec![];
    for (pi, chunk) in chunks.iter().enumerate() {
        let mut pg = Page::new(w, h);
        let first = pi == 0;
        // title block
        if first || o.header_every_page {
            pg.text(MARGIN, MARGIN + 12.0, &p.title, Font::Bold, 12.0, Rgb::BLACK);
            let span = format!("{} – {}", crate::model::fmt_date(app.project_span().0), p.calc.finish.map(crate::model::fmt_date).unwrap_or_default());
            pg.text_right(w - MARGIN, MARGIN + 12.0, &span, Font::Regular, 9.0, DIM);
            let mut sub = vec![];
            if !p.manager.is_empty() {
                sub.push(p.manager.clone());
            }
            if app.filter != super::commands::Filter::AllTasks {
                sub.push(format!("Filter: {}", app.filter_label(app.filter)));
            }
            if grouped {
                sub.push(format!("Grouped by {}", app.group.label()));
            }
            if let Some(sd) = p.status_date {
                sub.push(format!("Status date {}", crate::model::fmt_date(sd)));
            }
            pg.text(MARGIN, MARGIN + 24.0, &sub.join(" · "), Font::Regular, 8.0, DIM);
        }
        let btop = if first { body_top_first } else { body_top };
        // headings and timescale
        if first || o.header_every_page {
            let hy = btop - HEAD_H;
            pg.rect(tx0, hy, cx1 - tx0, HEAD_H, HEAD_BG);
            pg.text(tx0 + 3.0, btop - 8.0, "ID", Font::Bold, FONT, Rgb::BLACK);
            let mut x = tx0 + 22.0;
            for (c, cw) in &cols {
                pg.text(x + 3.0, btop - 8.0, &fit_text(&c.title_in(p), Font::Bold, FONT, cw - 6.0), Font::Bold, FONT, Rgb::BLACK);
                x += cw;
            }
            pg.clip(cx0, hy, cx1 - cx0, HEAD_H);
            let mid = hy + HEAD_H / 2.0;
            for (tier, ty0, ty1) in [(&top, hy, mid), (&bottom, mid, btop)] {
                for (k, (d, label)) in tier.iter().enumerate() {
                    let xa = sc.x(d.and_time(chrono::NaiveTime::MIN)).max(cx0);
                    let xb = tier.get(k + 1).map(|n| sc.x(n.0.and_time(chrono::NaiveTime::MIN))).unwrap_or(cx1);
                    pg.line(xa, ty0, xa, ty1, GRID, 0.5);
                    let lw = xb - xa - 3.0;
                    if lw > 6.0 {
                        pg.text(xa + 2.0, ty1 - 3.5, &fit_text(label, Font::Regular, FONT - 0.5, lw), Font::Regular, FONT - 0.5, DIM);
                    }
                }
            }
            pg.line(cx0, mid, cx1, mid, GRID, 0.5);
            pg.unclip();
        }
        // vertical guides: bottom tier boundaries, and the status date
        let bbot = btop + chunk.len() as f32 * ROW_H;
        pg.clip(cx0, btop, cx1 - cx0, bbot - btop);
        for (d, _) in &bottom {
            let x = sc.x(d.and_time(chrono::NaiveTime::MIN));
            pg.line(x, btop, x, bbot, Rgb(238, 239, 243), 0.4);
        }
        if let Some(sd) = p.status_date {
            let x = sc.x(sd);
            pg.line(x, btop, x, bbot, Rgb(220, 120, 40), 0.8);
        }
        pg.unclip();
        // where each task's row is on this page, for links
        let mut ypos: std::collections::HashMap<usize, f32> = Default::default();
        for (k, d) in chunk.clone().enumerate() {
            let y = btop + k as f32 * ROW_H;
            match rows[d] {
                DisplayRow::Task(i) => {
                    ypos.insert(i, y + ROW_H / 2.0);
                }
                DisplayRow::Group(_) => pg.rect(tx0, y, cx1 - tx0, ROW_H, GROUP_BG),
                DisplayRow::ProjectSummary => {}
            }
        }
        // links first, under the bars
        if o.links {
            for (&i, &ys) in &ypos {
                for l in &p.tasks[i].predecessors {
                    let Some(pr) = p.index_of_uid(l.pred) else { continue };
                    let Some(&yp) = ypos.get(&pr) else { continue };
                    let (Some(ps), Some(pf), Some(ss), Some(sf)) = (p.tasks[pr].calc.start, p.tasks[pr].calc.finish, p.tasks[i].calc.start, p.tasks[i].calc.finish) else { continue };
                    use crate::model::LinkType::*;
                    let (xa, xb) = match l.kind {
                        FS => (sc.x(pf), sc.x(ss)),
                        SS => (sc.x(ps), sc.x(ss)),
                        FF => (sc.x(pf), sc.x(sf)),
                        SF => (sc.x(ps), sc.x(sf)),
                    };
                    pg.clip(cx0, btop, cx1 - cx0, bbot - btop);
                    if xb >= xa + 4.0 && matches!(l.kind, FS | SS) {
                        pg.path(&[(xa, yp), (xb, yp), (xb, ys - 3.5)], LINK, 0.6);
                    } else {
                        let out = xa + 4.0;
                        pg.path(&[(xa, yp), (out, yp), (out, (yp + ys) / 2.0), (xb - 4.0, (yp + ys) / 2.0), (xb - 4.0, ys), (xb, ys)], LINK, 0.6);
                    }
                    pg.unclip();
                }
            }
        }
        for (k, d) in chunk.clone().enumerate() {
            let y = btop + k as f32 * ROW_H;
            let base = y + ROW_H - 3.8;
            let mid = y + ROW_H / 2.0;
            pg.line(tx0, y + ROW_H, cx1, y + ROW_H, GRID, 0.3);
            match rows[d] {
                DisplayRow::ProjectSummary => {
                    pg.text(tx0 + 3.0, base, "0", Font::Regular, FONT, DIM);
                    let x = tx0 + 22.0;
                    let name_w = cols.iter().find(|c| c.0 == ColumnId::Name).map_or(150.0, |c| c.1);
                    pg.text(x + 3.0, base, &fit_text(&p.title, Font::Bold, FONT, name_w), Font::Bold, FONT, Rgb::BLACK);
                }
                DisplayRow::Group(g) => {
                    let Some(gr) = groups.get(g) else { continue };
                    let mut x = tx0 + 22.0;
                    for (c, cw) in &cols {
                        let text = match c {
                            ColumnId::Name => format!("{} ({})", gr.label, gr.count),
                            ColumnId::Start => gr.start.map(|d| fmt.layout.date_format.format(d)).unwrap_or_default(),
                            ColumnId::Finish => gr.finish.map(|d| fmt.layout.date_format.format(d)).unwrap_or_default(),
                            ColumnId::Work => p.format_duration_minutes(gr.work, crate::duration::DurUnit::Hour),
                            ColumnId::Cost => crate::resources::fmt_money(gr.cost, &p.settings.currency_symbol),
                            _ => String::new(),
                        };
                        pg.text(x + 3.0, base, &fit_text(&text, Font::Bold, FONT, cw - 6.0), Font::Bold, FONT, Rgb::BLACK);
                        x += cw;
                    }
                    if let (Some(a), Some(b)) = (gr.start, gr.finish) {
                        pg.clip(cx0, btop, cx1 - cx0, bbot - btop);
                        pg.rect(sc.x(a), mid - 1.5, (sc.x(b) - sc.x(a)).max(1.0), 3.0, Rgb(60, 60, 70));
                        pg.unclip();
                    }
                }
                DisplayRow::Task(i) => {
                    let t = &p.tasks[i];
                    let bold = t.calc.is_summary;
                    let font = if bold { Font::Bold } else { Font::Regular };
                    pg.text_right(tx0 + 19.0, base, &(i + 1).to_string(), Font::Regular, FONT, DIM);
                    let mut x = tx0 + 22.0;
                    let inactive = app.inactive_rows.get(i).copied().unwrap_or(false);
                    let color = if inactive { DIM } else { Rgb::BLACK };
                    for (c, cw) in &cols {
                        let (text, indent) = if *c == ColumnId::Name {
                            let lvl = if grouped { 1 } else { t.outline_level.max(1) as usize };
                            (t.name.clone(), (lvl - 1) as f32 * 9.0)
                        } else {
                            (cell_text(p, i, *c), 0.0)
                        };
                        let avail = cw - 6.0 - indent;
                        let shown = fit_text(&text, font, FONT, avail);
                        if c.align() == egui::Align::RIGHT {
                            pg.text_right(x + cw - 3.0, base, &shown, font, FONT, color);
                        } else {
                            pg.text(x + 3.0 + indent, base, &shown, font, FONT, color);
                        }
                        x += cw;
                    }
                    // the bar
                    let (Some(a), Some(b)) = (t.calc.start, t.calc.finish) else { continue };
                    let (cat, st) = super::bars::file_style_for(app, i, None);
                    if !st.show {
                        continue;
                    }
                    pg.clip(cx0, btop, cx1 - cx0, bbot - btop);
                    let (xa, xb) = (sc.x(a), sc.x(b).max(sc.x(a) + 1.0));
                    if fmt.baseline_on() {
                        if let Some(bl) = t.baseline_slot(fmt.baseline_index as usize) {
                            let bs = fmt.styles.get(BarCategory::Baseline);
                            pg.rect(sc.x(bl.start), mid + 2.0, (sc.x(bl.finish) - sc.x(bl.start)).max(1.0), 2.0, rgb(bs.fill));
                        }
                    }
                    let mut right_x = xb;
                    match cat {
                        BarCategory::Milestone => {
                            diamond(&mut pg, xa, mid, 4.0, rgb(st.fill));
                            right_x = xa + 4.0;
                        }
                        BarCategory::Summary => {
                            pg.rect(xa, mid - 2.0, xb - xa, 3.0, rgb(st.fill));
                            for xe in [xa, xb] {
                                pg.polygon(&[(xe - 2.5, mid + 1.0), (xe + 2.5, mid + 1.0), (xe, mid + 4.0)], rgb(st.fill));
                            }
                        }
                        _ => {
                            let hh = (ROW_H * st.height * fmt.layout.bar_scale / 2.0).clamp(1.0, ROW_H * 0.4);
                            if inactive {
                                pg.rect_stroke(xa, mid - hh, xb - xa, hh * 2.0, rgb(st.border), 0.6);
                            } else {
                                pg.rect(xa, mid - hh, xb - xa, hh * 2.0, rgb(st.fill));
                                pg.rect_stroke(xa, mid - hh, xb - xa, hh * 2.0, rgb(st.border), 0.4);
                            }
                            let pct = (t.calc.percent_complete / 100.0).clamp(0.0, 1.0) as f32;
                            if pct > 0.0 {
                                let ps = fmt.styles.get(BarCategory::Progress);
                                pg.rect(xa, mid - hh * 0.4, (xb - xa) * pct, hh * 0.8, rgb(ps.fill));
                            }
                        }
                    }
                    pg.unclip();
                    if fmt.layout.show_bar_text {
                        let label = super::bars::bar_text(app, i, st.text.right);
                        if !label.is_empty() && right_x >= cx0 && right_x + 3.0 < cx1 {
                            pg.text(right_x + 3.0, base, &fit_text(&label, Font::Regular, FONT - 0.5, cx1 - right_x - 4.0), Font::Regular, FONT - 0.5, Rgb(60, 62, 70));
                        }
                    }
                }
            }
        }
        // frame and column rules
        pg.rect_stroke(tx0, btop - if first || o.header_every_page { HEAD_H } else { 0.0 }, cx1 - tx0, bbot - btop + if first || o.header_every_page { HEAD_H } else { 0.0 }, Rgb(150, 152, 160), 0.6);
        let mut x = tx0 + 22.0;
        for (_, cw) in &cols {
            pg.line(x, btop - HEAD_H.min(btop), x, bbot, GRID, 0.4);
            x += cw;
        }
        pg.line(cx0 - 2.0, btop - if first || o.header_every_page { HEAD_H } else { 0.0 }, cx0 - 2.0, bbot, Rgb(150, 152, 160), 0.6);
        // footer
        let foot = format!("Page {} of {npages}", pi + 1);
        pg.text_right(w - MARGIN, h - MARGIN, &foot, Font::Regular, 7.5, DIM);
        pg.text(MARGIN, h - MARGIN, &format!("Printed {} · ProjectFCG {}", crate::model::fmt_datetime(printed), env!("CARGO_PKG_VERSION")), Font::Regular, 7.5, DIM);
        pages.push(pg);
    }
    crate::pdf::write(&pages, &p.title)
}

/// Open a file with the desktop's default app (a PDF viewer, to print).
pub fn open_with_system(path: &std::path::Path) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(path).spawn().map(|_| ())
}

impl ProjectFcgApp {
    pub fn open_print_dialog(&mut self) {
        let opts = self.print_options.clone();
        let (s, f) = self.project_span();
        let (a, b) = opts.range.unwrap_or((s.date(), f.date()));
        let day = |d: NaiveDate| crate::model::fmt_date(d.and_time(chrono::NaiveTime::MIN));
        self.dialogs.print = Some(PrintDlg { whole: opts.range.is_none(), from: day(a), to: day(b), opts, error: None });
    }

    /// Write the PDF and open it in the system viewer, from where it prints.
    pub fn print_now(&mut self) {
        let pdf = print_pdf(self, &self.print_options);
        let name: String = self.project.title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = std::env::temp_dir().join(format!("{} - print.pdf", name.trim()));
        match std::fs::write(&path, pdf).and_then(|_| open_with_system(&path)) {
            Ok(()) => self.set_status(format!("Opened {} for printing", path.display())),
            Err(e) => self.error(format!("Couldn't open the PDF for printing ({e}). Use Save as PDF instead.")),
        }
    }
}

pub fn show_dialog(app: &mut ProjectFcgApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.print.take() else { return };
    let fallback = app.project.start.date();
    let (mut print, mut save, mut cancel) = (false, false, false);
    let pages = {
        let (pw, ph) = st.opts.paper.size();
        let h = if st.opts.landscape { pw } else { ph };
        let per = ((h - 2.0 * MARGIN - FOOT_H - TITLE_H - HEAD_H) / ROW_H).floor().max(1.0) as usize;
        app.display_rows().len().div_ceil(per).max(1)
    };
    let keys = modal(ctx, "print", "Print", 440.0, |ui| {
        egui::Grid::new("print_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Paper:");
            egui::ComboBox::from_id_salt("print_paper").selected_text(st.opts.paper.label()).show_ui(ui, |ui| {
                for p in Paper::ALL {
                    ui.selectable_value(&mut st.opts.paper, p, p.label());
                }
            });
            ui.end_row();
            ui.label("Orientation:");
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.opts.landscape, true, "Landscape");
                ui.radio_value(&mut st.opts.landscape, false, "Portrait");
            });
            ui.end_row();
            ui.label("Dates:");
            ui.vertical(|ui| {
                ui.radio_value(&mut st.whole, true, "The whole project");
                ui.radio_value(&mut st.whole, false, "From … to:");
                if !st.whole {
                    date_field(ui, &mut st.from, "print_from", fallback);
                    date_field(ui, &mut st.to, "print_to", fallback);
                }
            });
            ui.end_row();
            ui.label("");
            ui.vertical(|ui| {
                ui.checkbox(&mut st.opts.links, "Draw links");
                ui.checkbox(&mut st.opts.header_every_page, "Headings on every page");
            });
            ui.end_row();
        });
        ui.label(RichText::new(format!("The view as it is (filter, groups, collapsed rows) on about {pages} page{}. The sheet's first columns print beside the chart.", if pages == 1 { "" } else { "s" })).size(11.0).color(pal().text_dim));
        if let Some(e) = &st.error {
            ui.colored_label(pal().warn, e);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if ui.button("Save as PDF…").clicked() {
                    save = true;
                }
                if ui.button(RichText::new("Print…").strong()).on_hover_text("Open the PDF in your viewer to print it").clicked() {
                    print = true;
                }
            });
        });
    });
    if cancel || keys.esc {
        return;
    }
    print |= keys.enter;
    if !(print || save) {
        app.dialogs.print = Some(st);
        return;
    }
    st.opts.range = if st.whole {
        None
    } else {
        let t0 = chrono::NaiveTime::MIN;
        match (crate::model::parse_date(&st.from, t0), crate::model::parse_date(&st.to, t0)) {
            (Ok(a), Ok(b)) if a.date() <= b.date() => Some((a.date(), b.date())),
            _ => {
                st.error = Some("Enter two dates, the first not after the second.".into());
                app.dialogs.print = Some(st);
                return;
            }
        }
    };
    app.print_options = st.opts.clone();
    if print {
        app.print_now();
    } else {
        app.execute(Command::SavePdf, ctx);
    }
}
