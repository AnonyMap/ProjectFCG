//! A month-grid date picker like the drop-down calendar on Project's date
//! cells. (egui_extras' picker uses `jiff`; we use chrono throughout.)

use chrono::{Datelike, Duration as ChronoDur, NaiveDate};
use egui::RichText;

/// Show a calendar; returns the clicked date. The displayed month is kept
/// in egui's temp memory under `id`.
pub fn calendar(ui: &mut egui::Ui, id: egui::Id, selected: NaiveDate) -> Option<NaiveDate> {
    let mut month: NaiveDate = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(selected.year(), selected.month(), 1).unwrap());
    let mut picked = None;
    ui.horizontal(|ui| {
        if ui.small_button("◀").clicked() {
            month = shift_month(month, -1);
        }
        ui.add_sized(
            [130.0, 18.0],
            egui::Label::new(RichText::new(month.format("%B %Y").to_string()).strong()),
        );
        if ui.small_button("▶").clicked() {
            month = shift_month(month, 1);
        }
    });
    let first_cell = month - ChronoDur::days(month.weekday().num_days_from_sunday() as i64);
    let today = chrono::Local::now().date_naive();
    egui::Grid::new(id.with("grid")).spacing([2.0, 2.0]).show(ui, |ui| {
        for w in ["S", "M", "T", "W", "T", "F", "S"] {
            ui.label(RichText::new(w).size(11.0).color(super::theme::pal().text_dim));
        }
        ui.end_row();
        for week in 0..6 {
            for dow in 0..7 {
                let d = first_cell + ChronoDur::days(week * 7 + dow);
                let in_month = d.month() == month.month();
                let mut text = RichText::new(d.day().to_string()).size(12.0);
                if !in_month {
                    text = text.color(super::theme::pal().text_disabled);
                }
                if d == today {
                    text = text.underline();
                }
                let btn = egui::Button::new(text)
                    .min_size(egui::vec2(24.0, 20.0))
                    .selected(d == selected);
                if ui.add(btn).clicked() {
                    picked = Some(d);
                }
            }
            ui.end_row();
        }
    });
    if ui.small_button("Today").clicked() {
        picked = Some(today);
    }
    ui.data_mut(|d| d.insert_temp(id, month));
    picked
}

pub fn shift_month(d: NaiveDate, delta: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + delta;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn month_shift_wraps_years() {
        let d = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(shift_month(d, 1), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(shift_month(d, -12), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
        let j = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(shift_month(j, -1), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
    }
}
