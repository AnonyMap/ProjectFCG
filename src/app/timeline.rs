//! The Timeline pane above the Gantt chart (View ▸ Timeline). Shows the
//! whole project span, tasks flagged "Display on Timeline", today, and the
//! portion currently visible in the Gantt chart (drag it to pan).

use super::theme::*;
use super::{from_days, to_days, ProjectFcgApp};
use crate::model::fmt_date;
use chrono::Datelike;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Shape, Stroke, StrokeKind};

pub fn show(app: &mut ProjectFcgApp, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, pal().bg);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, pal().header_border));

    let (s, f) = app.project_span();
    let label_w = 96.0;
    let band = Rect::from_min_max(
        pos2(rect.left() + label_w, rect.top() + 22.0),
        pos2(rect.right() - label_w, rect.bottom() - 12.0),
    );
    let sd = to_days(s);
    let fd = to_days(f).max(sd + 1.0);
    let x_of = |d: f64| band.left() + ((d - sd) / (fd - sd)) as f32 * band.width();

    let font = FontId::proportional(11.5);
    p.text(pos2(rect.left() + 8.0, band.center().y - 7.0), Align2::LEFT_CENTER, "Start", font.clone(), pal().text_dim);
    p.text(pos2(rect.left() + 8.0, band.center().y + 7.0), Align2::LEFT_CENTER, fmt_date(s), font.clone(), pal().text);
    p.text(pos2(rect.right() - 8.0, band.center().y - 7.0), Align2::RIGHT_CENTER, "Finish", font.clone(), pal().text_dim);
    p.text(pos2(rect.right() - 8.0, band.center().y + 7.0), Align2::RIGHT_CENTER, fmt_date(f), font.clone(), pal().text);

    p.rect_filled(band, 2.0, pal().timeline_band);
    p.rect_stroke(band, 2.0, Stroke::new(1.0, pal().timeline_border), StrokeKind::Inside);

    // month ticks along the top
    let mut d = s.date().with_day(1).unwrap();
    let mut guard = 0;
    while to_days(d.and_hms_opt(0, 0, 0).unwrap()) <= fd && guard < 400 {
        let x = x_of(to_days(d.and_hms_opt(0, 0, 0).unwrap()));
        if x >= band.left() {
            p.line_segment([pos2(x, rect.top() + 4.0), pos2(x, band.top())], Stroke::new(1.0, pal().grid));
            p.text(pos2(x + 3.0, rect.top() + 11.0), Align2::LEFT_CENTER, d.format("%b").to_string(), FontId::proportional(10.5), pal().text_dim);
        }
        d = super::datepicker::shift_month(d, 1);
        guard += 1;
    }

    // tasks on the timeline
    let mut any = false;
    for t in app.project.tasks.iter().filter(|t| t.on_timeline) {
        let (Some(ts), Some(tf)) = (t.calc.start, t.calc.finish) else { continue };
        any = true;
        let x0 = x_of(to_days(ts));
        let x1 = x_of(to_days(tf));
        if t.is_milestone() {
            let c = pos2(x0, band.top() - 1.0);
            p.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -5.0), c + vec2(5.0, 0.0), c + vec2(0.0, 5.0), c + vec2(-5.0, 0.0)],
                pal().milestone,
                Stroke::NONE,
            ));
            p.text(c + vec2(7.0, -2.0), Align2::LEFT_BOTTOM, &t.name, FontId::proportional(10.5), pal().text);
        } else {
            let r = Rect::from_min_max(pos2(x0, band.top() + 3.0), pos2(x1.max(x0 + 2.0), band.bottom() - 3.0));
            let fill = if t.calc.is_summary { pal().timeline_summary } else { pal().bar_fill };
            p.rect_filled(r, 1.0, fill);
            p.rect_stroke(r, 1.0, Stroke::new(1.0, pal().bar_border), StrokeKind::Inside);
            let clip = p.with_clip_rect(r.shrink(2.0));
            clip.text(pos2(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, &t.name, font.clone(), on_fill(fill));
        }
    }
    if !any {
        p.text(
            band.center(),
            Align2::CENTER_CENTER,
            "Add tasks with dates to the timeline (Task ▸ Add to Timeline)",
            font.clone(),
            pal().text_dim,
        );
    }

    // today
    let now = to_days(chrono::Local::now().naive_local());
    if now >= sd && now <= fd {
        let x = x_of(now);
        p.line_segment([pos2(x, band.top() - 4.0), pos2(x, band.bottom() + 4.0)], Stroke::new(1.5, pal().today));
        p.text(pos2(x, band.bottom() + 2.0), Align2::CENTER_TOP, "Today", FontId::proportional(9.5), pal().today);
    }

    // visible window of the Gantt chart
    let vl = app.gantt.left_days;
    let vr = vl + (app.gantt_view_w / app.gantt.ppd) as f64;
    let wx0 = x_of(vl).max(band.left() - 2.0);
    let wx1 = x_of(vr).min(band.right() + 2.0);
    if wx1 > wx0 {
        let w = Rect::from_min_max(pos2(wx0, band.top() - 2.0), pos2(wx1, band.bottom() + 2.0));
        p.rect_filled(w, 0.0, {
            let a = pal().accent;
            Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 28)
        });
        p.rect_stroke(w, 0.0, Stroke::new(1.5, pal().accent), StrokeKind::Inside);
    }

    let resp = ui.interact(band.expand(4.0), ui.scope_id().with("timeline_band"), Sense::click_and_drag());
    if resp.clicked() || resp.dragged() {
        if let Some(pt) = resp.interact_pointer_pos() {
            let frac = ((pt.x - band.left()) / band.width()) as f64;
            let center = sd + frac * (fd - sd);
            let half = (vr - vl) / 2.0;
            app.gantt.left_days = center - half;
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let _ = from_days;
}
