//! Earned value: Project's calculations, the choice of baseline, the
//! discrete techniques of DoD EVMS, PMI/DoD terms, the S-curve and the
//! period report. Numbers are worked by hand (Standard calendar, start Mon
//! 1/5/2026, one 10-day task costing $8,000).
use chrono::{NaiveDate, NaiveDateTime};
use egui::{vec2, Pos2, RawInput, Rect};
use projectfcg::app::columns::{ColumnId, Table};
use projectfcg::app::commands::Command;
use projectfcg::app::{MainView, ProjectFcgApp};
use projectfcg::earned_value::{EarnedValue, EvPeriod, Metric};
use projectfcg::model::{Accrue, EacMethod, EvMethod, EvTerms, Project, ResourceType};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(m: u32, d: u32, h: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
}

#[track_caller]
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.01, "{a} ≠ {b}");
}

/// One 10-day task with a $100/h developer ($8,000), baselined; status at
/// the end of day 5 (half the baseline due).
fn build(accrue: Accrue) -> (Project, usize) {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let a = p.append_task("Build");
    p.set_field_text(a, Field::Duration, "10d").unwrap();
    let r = p.add_resource("Dev", ResourceType::Work);
    p.resources[r].std_rate = 100.0;
    p.resources[r].accrue = accrue;
    p.set_field_text(a, Field::ResourceNames, "Dev").unwrap();
    schedule(&mut p);
    assert_eq!(p.tasks[a].calc.cost, 8000.0);
    p.set_baseline(None);
    p.status_date = Some(dt(1, 9, 17));
    (p, a)
}

#[test]
fn the_formulas() {
    let ev = EarnedValue { bac: 1000.0, bcws: 500.0, bcwp: 400.0, acwp: 500.0, cost: 1200.0, eac_method: EacMethod::Cpi };
    assert_eq!(ev.sv(), -100.0);
    assert_eq!(ev.cv(), -100.0);
    assert_eq!(ev.sv_percent(), Some(-20.0), "SV ÷ PV");
    assert_eq!(ev.cv_percent(), Some(-25.0), "CV ÷ EV");
    assert_eq!((ev.spi(), ev.cpi()), (Some(0.8), Some(0.8)));
    close(ev.eac(), 1250.0); // AC + (BAC − EV) ÷ CPI = 500 + 600 ÷ 0.8
    close(ev.etc(), 750.0);
    close(ev.vac(), -250.0);
    close(ev.tcpi().unwrap(), 1.2); // (BAC − EV) ÷ (BAC − AC)
    close(ev.tcpi_eac().unwrap(), 0.8); // (BAC − EV) ÷ (EAC − AC)
    assert_eq!((ev.percent_complete(), ev.percent_spent(), ev.percent_scheduled()), (Some(40.0), Some(50.0), Some(50.0)));
    let by = |m| EarnedValue { eac_method: m, ..ev }.eac();
    close(by(EacMethod::CpiSpi), 500.0 + 600.0 / 0.64);
    close(by(EacMethod::BudgetRate), 1100.0);
    close(by(EacMethod::Schedule), 1200.0);
    // nothing spent or planned yet: the budget stands, indices are unknown
    let fresh = EarnedValue { bac: 1000.0, ..Default::default() };
    for m in EacMethod::ALL {
        if m != EacMethod::Schedule {
            assert_eq!(EarnedValue { eac_method: m, ..fresh }.eac(), 1000.0, "{m:?}");
        }
    }
    assert_eq!((fresh.spi(), fresh.cpi(), fresh.sv_percent(), fresh.cv_percent()), (None, None, None, None));
    // a period's figures: the change, with the budget kept
    let earlier = EarnedValue { bcws: 300.0, bcwp: 250.0, acwp: 260.0, ..ev };
    let d = ev.since(&earlier);
    assert_eq!((d.bcws, d.bcwp, d.acwp, d.bac), (200.0, 150.0, 240.0, 1000.0));
}

#[test]
fn project_by_hand() {
    let (mut p, a) = build(Accrue::Prorated);
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    let ev = p.earned_value(a);
    assert_eq!((ev.bac, ev.bcws, ev.bcwp, ev.acwp), (8000.0, 4000.0, 2400.0, 2400.0));
    assert_eq!(ev.sv_percent(), Some(-40.0));
    assert_eq!(ev.cv_percent(), Some(0.0));
    assert_eq!(ev.etc(), 5600.0);
    // the same through the S-curve's function, at the status date
    let at = p.earned_value_at(p.status_or_now());
    assert_eq!((at[a].bcws, at[a].bcwp, at[a].acwp), (ev.bcws, ev.bcwp, ev.acwp));
}

#[test]
fn planned_value_follows_accrual() {
    // a cost that accrues at the start is planned from the baseline start
    let (mut p, a) = build(Accrue::Start);
    schedule(&mut p);
    assert_eq!(p.tasks[a].baseline.unwrap().cost_at_start, 8000.0);
    assert_eq!(p.earned_value(a).bcws, 8000.0);
    // one that accrues at the end, at the baseline finish
    let (mut q, b) = build(Accrue::End);
    schedule(&mut q);
    assert_eq!(q.earned_value(b).bcws, 0.0);
    q.status_date = Some(dt(1, 16, 17));
    schedule(&mut q);
    assert_eq!(q.earned_value(b).bcws, 8000.0);
    // a baseline from an older file has no split: prorated
    let (mut r, c) = build(Accrue::Start);
    let bl = r.tasks[c].baseline.as_mut().unwrap();
    bl.cost_at_start = 0.0;
    schedule(&mut r);
    assert_eq!(r.earned_value(c).bcws, 4000.0);
}

#[test]
fn baseline_for_earned_value() {
    let (mut p, a) = build(Accrue::Prorated);
    // re-planned at 15 days and saved as Baseline 1
    p.set_field_text(a, Field::Duration, "15d").unwrap();
    schedule(&mut p);
    p.set_baseline_n(1, None);
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    assert_eq!(p.earned_value(a).bac, 8000.0, "Baseline by default");
    p.settings.ev_baseline = 1;
    schedule(&mut p);
    let ev = p.earned_value(a);
    assert_eq!(ev.bac, 12000.0);
    close(ev.bcws, 4000.0); // 5 of 15 days
    close(ev.bcwp, 3600.0);
    // a baseline that was never saved: nothing to measure against
    p.settings.ev_baseline = 7;
    schedule(&mut p);
    assert_eq!(p.earned_value(a), EarnedValue { acwp: p.earned_value(a).acwp, cost: 12000.0, ..Default::default() });
    // and it travels in Project XML
    p.settings.ev_baseline = 4;
    let (q, _) = projectfcg::mspdi::from_xml(&projectfcg::mspdi::to_xml(&p)).unwrap();
    assert_eq!(q.settings.ev_baseline, 4);
}

#[test]
fn earned_value_methods() {
    let at = |m: EvMethod, pct: f64, phys: f64| {
        let (mut p, a) = build(Accrue::Prorated);
        p.tasks[a].ev_method = m;
        p.tasks[a].physical_percent = phys;
        if pct > 0.0 {
            p.set_percent_complete(a, pct).unwrap();
        }
        schedule(&mut p);
        let e = p.earned_value(a);
        (e.bcws, e.bcwp)
    };
    assert_eq!(at(EvMethod::PercentComplete, 30.0, 45.0), (4000.0, 2400.0));
    assert_eq!(at(EvMethod::PhysicalPercentComplete, 30.0, 45.0), (4000.0, 3600.0));
    // 0/100: neither planned nor earned until the finish
    assert_eq!(at(EvMethod::ZeroHundred, 60.0, 0.0), (0.0, 0.0));
    assert_eq!(at(EvMethod::ZeroHundred, 100.0, 0.0), (0.0, 8000.0));
    // 50/50: half at the start
    assert_eq!(at(EvMethod::FiftyFifty, 0.0, 0.0), (4000.0, 0.0));
    assert_eq!(at(EvMethod::FiftyFifty, 30.0, 0.0), (4000.0, 4000.0));
    // level of effort: earned as planned, never a schedule variance
    assert_eq!(at(EvMethod::LevelOfEffort, 0.0, 0.0), (4000.0, 4000.0));
    assert_eq!(at(EvMethod::LevelOfEffort, 90.0, 0.0), (4000.0, 4000.0));
    // typed into the column
    for (txt, m) in [("0/100", EvMethod::ZeroHundred), ("50/50", EvMethod::FiftyFifty), ("LOE", EvMethod::LevelOfEffort), ("physical", EvMethod::PhysicalPercentComplete)] {
        let (mut p, a) = build(Accrue::Prorated);
        projectfcg::app::columns::set_cell(&mut p, a, ColumnId::EarnedValueMethod, txt).unwrap();
        assert_eq!(p.tasks[a].ev_method, m, "{txt}");
    }
    let (mut p, a) = build(Accrue::Prorated);
    assert!(projectfcg::app::columns::set_cell(&mut p, a, ColumnId::EarnedValueMethod, "sometimes").is_err());
}

#[test]
fn terms_pmi_dod_or_both() {
    let (mut p, _) = build(Accrue::Prorated);
    let titles = |p: &Project| [ColumnId::Bcws, ColumnId::Bcwp, ColumnId::Acwp, ColumnId::Tcpi].map(|c| c.title_in(p));
    assert_eq!(titles(&p), ["PV", "EV", "AC", "TCPI"], "PMI by default");
    p.settings.ev_terms = EvTerms::Dod;
    assert_eq!(titles(&p), ["BCWS", "BCWP", "ACWP", "TCPI (BAC)"]);
    p.settings.ev_terms = EvTerms::Both;
    assert_eq!(titles(&p), ["PV (BCWS)", "EV (BCWP)", "AC (ACWP)", "TCPI (BAC)"]);
    assert_eq!(EvTerms::Dod.name(Metric::Pv), "Budgeted Cost of Work Scheduled");
    assert_eq!(EvTerms::Both.long(Metric::Ac), "Actual Cost (AC, ACWP)");
    // other columns keep their names
    assert_eq!(ColumnId::Duration.title_in(&p), "Duration");
    // a file without these settings opens with the defaults
    let json = projectfcg::io::to_json(&p).unwrap();
    assert_eq!(projectfcg::io::from_json(&json).unwrap().settings.ev_terms, EvTerms::Both);
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let s = v["project"]["settings"].as_object_mut().unwrap();
    for k in ["ev_terms", "ev_baseline", "eac_method"] {
        assert!(s.remove(k).is_some(), "{k} is saved");
    }
    let q = projectfcg::io::from_json(&serde_json::to_string(&v).unwrap()).unwrap();
    assert_eq!((q.settings.ev_terms, q.settings.ev_baseline, q.settings.eac_method), (EvTerms::Pmi, 0, EacMethod::Cpi));
}

#[test]
fn projects_earned_value_tables() {
    use ColumnId::*;
    assert_eq!(Table::EarnedValueCost.columns(), vec![Indicators, Name, Bcws, Bcwp, Cv, CvPercent, Cpi, Bac, Eac, Vac, Tcpi]);
    assert_eq!(Table::EarnedValueSchedule.columns(), vec![Indicators, Name, Bcws, Bcwp, Sv, SvPercent, Spi]);
    let (mut p, a) = build(Accrue::Prorated);
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    let cell = |c| projectfcg::app::columns::cell_text(&p, a, c);
    assert_eq!(cell(SvPercent), "-40%");
    assert_eq!(cell(CvPercent), "0%");
    assert_eq!(cell(Etc), "$5,600.00");
    assert_eq!(cell(TcpiEac), "1.00");
}

#[test]
fn earned_value_over_time() {
    let (mut p, a) = build(Accrue::Prorated);
    p.set_percent_complete(a, 30.0).unwrap();
    schedule(&mut p);
    // 30% done by the status date (end of day 5), from the actual start
    // (Mon 8:00): spread evenly, two fifths of it by the end of day 2
    close(p.progress_share_at(a, dt(1, 6, 17)), 0.4);
    let e = p.earned_value_at(dt(1, 6, 17));
    close(e[a].bcws, 1600.0);
    close(e[a].bcwp, 960.0);
    close(e[a].acwp, 960.0);
    assert_eq!(p.earned_value_at(dt(1, 5, 8))[a].bcwp, 0.0);
    // weekly: Mon 1/5, 1/12, 1/19
    let s = p.earned_value_series(EvPeriod::Week);
    let dates: Vec<NaiveDateTime> = s.points.iter().map(|x| x.0).collect();
    assert_eq!(dates, vec![dt(1, 5, 0), dt(1, 12, 0), dt(1, 19, 0)]);
    close(s.points[1].1.bcws, 4000.0);
    assert_eq!(s.points.last().unwrap().1.bcws, 8000.0, "planned value reaches the budget");
    assert_eq!(s.now, p.project_earned_value());
    assert!(s.points.windows(2).all(|w| w[0].1.bcws <= w[1].1.bcws), "planned value only rises");
    // the current period: what changed in the last week
    p.status_date = Some(dt(1, 14, 17));
    p.set_percent_complete(a, 80.0).unwrap();
    schedule(&mut p);
    let now = p.earned_value(a);
    let week_ago = p.earned_value_at(EvPeriod::Week.back(p.status_or_now()));
    let d = now.since(&week_ago[a]);
    close(d.bcws, 8000.0 * 5.0 / 10.0); // the end of day 3 to the end of day 8
    close(d.bcwp, 6400.0 * 5.0 / 8.0); // 80% over 8 days: 5 of them this week
}

#[test]
fn summaries_roll_up_with_methods_mixed() {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    let s = p.append_task("Phase");
    let a = p.append_task("A");
    let b = p.append_task("B");
    p.set_field_text(a, Field::Duration, "2d").unwrap();
    p.set_field_text(b, Field::Duration, "2d").unwrap();
    p.indent(&[a, b]).unwrap();
    p.tasks[a].fixed_cost = 100.0;
    p.tasks[b].fixed_cost = 300.0;
    p.tasks[b].ev_method = EvMethod::ZeroHundred;
    schedule(&mut p);
    p.set_baseline(None);
    p.status_date = Some(dt(1, 5, 17));
    p.set_percent_complete(a, 50.0).unwrap();
    p.set_percent_complete(b, 50.0).unwrap();
    schedule(&mut p);
    let ev = p.earned_value(s);
    // A: half planned, half earned; B: 0/100 — nothing yet
    assert_eq!((ev.bac, ev.bcws, ev.bcwp, ev.acwp), (400.0, 50.0, 50.0, 200.0));
    assert_eq!(p.project_earned_value().bcwp, 50.0);
}

// ------------------------------------------------------------------ UI

fn frame(ctx: &egui::Context, app: &mut ProjectFcgApp, t: f64) {
    let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), time: Some(t), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
    out.textures_delta.clear();
}

#[test]
fn the_view_draws_in_every_state() {
    let ctx = egui::Context::default();
    // no baseline: the view says so
    let mut app = ProjectFcgApp::new(projectfcg::app::sample::software_release());
    app.prefs_path = None;
    app.execute(Command::ShowView(MainView::EarnedValue), &ctx);
    frame(&ctx, &mut app, 0.1);
    frame(&ctx, &mut app, 0.2);
    assert_eq!(app.view.main, MainView::EarnedValue);
    // baselined and under way, in each set of terms and each period
    app.edit_project("Set Baseline", |p| {
        p.set_baseline(None);
        p.status_date = Some(NaiveDate::from_ymd_opt(2026, 11, 20).unwrap().and_hms_opt(17, 0, 0).unwrap());
        Ok(())
    });
    let mut t = 0.3;
    for terms in EvTerms::ALL {
        for period in [EvPeriod::Week, EvPeriod::Month] {
            app.edit_project("Terms", move |p| {
                p.settings.ev_terms = terms;
                Ok(())
            });
            app.ev_view.period = period;
            frame(&ctx, &mut app, t);
            frame(&ctx, &mut app, t + 0.05);
            t += 0.1;
        }
    }
    let ev = app.project.project_earned_value();
    assert!(ev.bac > 0.0 && ev.bcws > 0.0 && ev.bcwp > 0.0, "{ev:?}");
    // the terms are one undo step each
    app.execute(Command::Undo, &ctx);
    assert_eq!(app.project.settings.ev_terms, EvTerms::Dod);
    // every table, every view of earned value
    for tbl in [Table::EarnedValue, Table::EarnedValueCost, Table::EarnedValueSchedule] {
        app.execute(Command::ShowView(MainView::Gantt), &ctx);
        app.execute(Command::SetTable(tbl), &ctx);
        frame(&ctx, &mut app, t);
        t += 0.1;
        assert_eq!(app.columns.len(), tbl.columns().len());
    }
}
