//! Search the plan (Ctrl+E), as Outlook searches mail: tasks, tickets,
//! people and risks, by name, ID, key or anything written in them.
//! Commands have their own search (Alt+Q, tell_me.rs).
//!
//! * Each typed word has to appear somewhere in the item (name, notes,
//!   description, labels, assignee, owner…); names, IDs and keys rank first.
//!   "7" or "#7" is task 7, "APP-12" a ticket, "R-003" a risk.
//! * All shows the best few of each kind; the chips (or Tab) narrow it to
//!   one kind.
//! * Enter goes to the item (its view, selected and shown); Shift+Enter
//!   opens it (Task Information, the ticket, Resource Information).

use super::commands::{Command, Filter};
use super::{MainView, ProjectFcgApp};
use egui::{pos2, text::LayoutJob, vec2, Align2, FontId, Id, Key, Modifiers, Rect, Sense, TextFormat};

pub fn box_id() -> Id {
    Id::unique("item_search_box")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Tasks,
    Tickets,
    People,
    Risks,
}

impl Scope {
    pub const ALL: [Scope; 5] = [Scope::All, Scope::Tasks, Scope::Tickets, Scope::People, Scope::Risks];
    pub fn label(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Tasks => "Tasks",
            Scope::Tickets => "Tickets",
            Scope::People => "Resources",
            Scope::Risks => "Risks",
        }
    }
}

/// A found item: task row, ticket id, resource uid, risk id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Task(usize),
    Ticket(u32),
    Resource(u32),
    Risk(u32),
}

impl Item {
    fn scope(self) -> Scope {
        match self {
            Item::Task(_) => Scope::Tasks,
            Item::Ticket(_) => Scope::Tickets,
            Item::Resource(_) => Scope::People,
            Item::Risk(_) => Scope::Risks,
        }
    }
}

#[derive(Default)]
pub struct ItemSearch {
    pub open: bool,
    pub query: String,
    pub sel: usize,
    pub scope: Scope,
    list: Option<Rect>,
    /// Scroll the selected row into view next frame.
    scroll_to_sel: bool,
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// How well the item's name (`name`) and other text (`rest`) match the
/// words: every word must appear somewhere; name hits count most.
fn text_score(name: &str, id_words: &[String], rest: &[&str], words: &[String], phrase: &str) -> Option<u32> {
    let n = lower(name);
    let rest: Vec<String> = rest.iter().map(|s| lower(s)).collect();
    let mut total = 0;
    for w in words {
        let s = if id_words.iter().any(|x| x == w) {
            60
        } else if n.split(|c: char| !c.is_alphanumeric()).any(|x| x.starts_with(w.as_str())) {
            12
        } else if n.contains(w.as_str()) {
            8
        } else if rest.iter().any(|r| r.contains(w.as_str())) {
            3
        } else {
            return None;
        };
        total += s;
    }
    if n == phrase {
        total += 40;
    } else if n.starts_with(phrase) {
        total += 20;
    } else if n.contains(phrase) {
        total += 10;
    }
    Some(total)
}

/// Everything matching `q`, best first, per kind.
pub fn search(p: &crate::model::Project, q: &str) -> Vec<(Item, u32)> {
    let phrase = lower(q.trim());
    let words: Vec<String> = phrase.split_whitespace().map(|w| w.trim_start_matches('#').to_string()).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    for (i, t) in p.tasks.iter().enumerate() {
        let ids = [format!("{}", i + 1)];
        let res = p.resource_names_text(i);
        if let Some(s) = text_score(&t.name, &ids, &[&t.notes, &res], &words, &phrase) {
            out.push((Item::Task(i), s));
        }
    }
    for t in &p.tickets {
        let key = lower(&p.ticket_key(t.id));
        let who = t.assignee.and_then(|u| p.resource_index(u)).map(|r| p.resources[r].name.clone()).unwrap_or_default();
        let col = p.board.columns.get(t.column).cloned().unwrap_or_default();
        let labels = t.labels.join(" ");
        if let Some(s) = text_score(&t.title, &[key, format!("{}", t.id)], &[&t.description, &labels, &who, t.kind.label(), &col], &words, &phrase) {
            out.push((Item::Ticket(t.id), s));
        }
    }
    for r in &p.resources {
        if let Some(s) = text_score(&r.name, &[], &[&r.group, &r.notes, r.rtype.label()], &words, &phrase) {
            out.push((Item::Resource(r.uid), s));
        }
    }
    for r in &p.risks {
        let key = lower(&r.key());
        let std = p.risk_standard(r);
        let stmt = r.statement(std);
        if let Some(s) = text_score(&r.title, &[key.clone(), key.replace('-', ""), format!("{}", r.id)], &[&stmt, &r.description, &r.owner, &r.category, &r.response_plan], &words, &phrase) {
            out.push((Item::Risk(r.id), s));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then(order(a.0).cmp(&order(b.0))));
    out
}

fn order(i: Item) -> (u8, u64) {
    match i {
        Item::Task(r) => (0, r as u64),
        Item::Ticket(id) => (1, id as u64),
        Item::Resource(u) => (2, u as u64),
        Item::Risk(id) => (3, id as u64),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Header(Scope, usize),
    Item(Item),
    More(Scope, usize),
}

const PER_KIND: usize = 5;
const MAX_ONE_KIND: usize = 80;

impl ProjectFcgApp {
    /// Ctrl+E: put the cursor in the plan search (with `query`, if any).
    pub fn focus_item_search(&mut self, ctx: &egui::Context, query: Option<String>) {
        let st = &mut self.item_search;
        st.open = true;
        st.sel = 0;
        if let Some(q) = query {
            st.query = q;
        }
        ctx.memory_mut(|m| m.request_focus(box_id()));
    }

    fn item_rows(&self, hits: &[(Item, u32)]) -> Vec<Row> {
        let st = &self.item_search;
        let mut rows = vec![];
        // the kind with the best match first ("marco" is a person before
        // the tasks he's on)
        let mut kinds = [Scope::Tasks, Scope::Tickets, Scope::People, Scope::Risks];
        let best = |k: Scope| hits.iter().filter(|h| h.0.scope() == k).map(|h| h.1).max().unwrap_or(0);
        kinds.sort_by_key(|&k| std::cmp::Reverse(best(k)));
        for kind in kinds {
            if st.scope != Scope::All && st.scope != kind {
                continue;
            }
            let of: Vec<Item> = hits.iter().map(|h| h.0).filter(|i| i.scope() == kind).collect();
            if of.is_empty() {
                continue;
            }
            rows.push(Row::Header(kind, of.len()));
            let cap = if st.scope == Scope::All { PER_KIND } else { MAX_ONE_KIND };
            rows.extend(of.iter().take(cap).map(|&i| Row::Item(i)));
            if of.len() > cap {
                rows.push(Row::More(kind, of.len()));
            }
        }
        rows
    }

    /// Go to an item (Enter), or open it (`open`: Shift+Enter).
    pub fn go_to_item(&mut self, item: Item, open: bool, ctx: &egui::Context) {
        match item {
            Item::Task(i) => {
                if i >= self.project.tasks.len() {
                    return;
                }
                if !matches!(self.view.main, MainView::Gantt | MainView::TaskUsage | MainView::NetworkDiagram | MainView::Calendar) {
                    self.execute(Command::ShowView(MainView::Gantt), ctx);
                }
                // a filter that hides it goes
                if self.filter != Filter::AllTasks && !super::filters::FilterEval::new(&self.project, self.filter).matches(i) {
                    self.execute(Command::SetFilter(Filter::AllTasks), ctx);
                }
                for a in self.project.ancestors_of(i) {
                    self.project.tasks[a].collapsed = false;
                }
                self.select_index(i);
                self.execute(Command::ScrollToTask, ctx);
                if open {
                    self.execute(Command::TaskInfo, ctx);
                }
            }
            Item::Ticket(id) => {
                let Some(t) = self.project.ticket(id).cloned() else { return };
                self.execute(Command::ShowView(MainView::Board), ctx);
                let b = &mut self.board;
                match (t.kind, t.sprint) {
                    (crate::board::IssueType::Epic, _) => {
                        b.side = super::board::SidePanel::Epics;
                        b.backlog_open = true;
                    }
                    (_, Some(s)) => b.sprint = Some(s),
                    (_, None) => {
                        b.side = super::board::SidePanel::Backlog;
                        b.backlog_open = true;
                    }
                }
                b.selected = Some(id);
                if !self.board.filter.matches(&self.project, &t) {
                    self.board.filter = Default::default();
                    let key = self.project.ticket_key(id);
                    self.set_status(format!("Cleared the board filter to show {key}"));
                }
                if open {
                    self.open_ticket_dialog(Some(id));
                }
            }
            Item::Resource(uid) => {
                self.execute(Command::ShowView(MainView::ResourceSheet), ctx);
                self.sheet.active = Some(uid);
                self.sheet.selected = vec![uid];
                if open {
                    self.execute(Command::ResourceInfo, ctx);
                }
            }
            Item::Risk(id) => {
                self.execute(Command::ShowView(MainView::Risks), ctx);
                let closed = self.project.risk(id).is_some_and(|r| !r.status.is_active());
                let rv = &mut self.risk_view;
                rv.kind = None;
                rv.cell = None;
                rv.search.clear();
                if closed {
                    rv.status = super::risk_view::StatusFilter::All;
                }
                self.select_risk(Some(id));
            }
        }
    }

    /// One line about an item, for its row: (icon, title, details).
    pub(crate) fn describe(&self, item: Item) -> (String, String, String) {
        let p = &self.project;
        let d = |x: Option<chrono::NaiveDateTime>| x.map(|d| d.format("%a %-m/%-d/%y").to_string()).unwrap_or_default();
        match item {
            Item::Task(i) => {
                let t = &p.tasks[i];
                let (icon, what) = if t.is_sprint {
                    ("📆", "Sprint")
                } else if p.is_summary(i) {
                    ("▼", "Summary")
                } else if t.is_milestone() {
                    ("◆", "Milestone")
                } else {
                    ("▸", "Task")
                };
                let mut sub = format!("{what} · {} → {}", d(t.calc.start), d(t.calc.finish));
                if t.percent_complete > 0.0 {
                    sub += &format!(" · {:.0}%", t.percent_complete);
                }
                let who = p.resource_names_text(i);
                if !who.is_empty() {
                    sub += &format!(" · {who}");
                }
                (icon.into(), format!("{}  {}", i + 1, t.name), sub)
            }
            Item::Ticket(id) => {
                let Some(t) = p.ticket(id) else { return ("?".into(), String::new(), String::new()) };
                let col = p.board.columns.get(t.column).cloned().unwrap_or_default();
                let sprint = t.sprint.and_then(|s| p.index_of_uid(s)).map(|i| p.tasks[i].name.clone()).unwrap_or_else(|| "Backlog".into());
                let mut sub = format!("{} · {col} · {sprint}", t.kind.label());
                if let Some(who) = t.assignee.and_then(|u| p.resource_index(u)).map(|r| p.resources[r].name.clone()) {
                    sub += &format!(" · {who}");
                }
                if let Some(pts) = t.points {
                    sub += &format!(" · {} pts", crate::duration::fmt_num(pts));
                }
                ("🎫".into(), format!("{}  {}", p.ticket_key(id), t.title), sub)
            }
            Item::Resource(uid) => {
                let Some(r) = p.resource_index(uid).map(|i| &p.resources[i]) else { return ("?".into(), String::new(), String::new()) };
                let icon = match r.rtype {
                    crate::model::ResourceType::Work => "👷",
                    crate::model::ResourceType::Material => "📦",
                    crate::model::ResourceType::Cost => "💲",
                };
                let mut sub = format!("{} resource", r.rtype.label());
                if !r.group.is_empty() {
                    sub += &format!(" · {}", r.group);
                }
                let n = p.tasks.iter().enumerate().filter(|(i, _)| p.resource_names_text(*i).split(", ").any(|x| x.split('[').next() == Some(r.name.as_str()))).count();
                sub += &format!(" · {n} task(s)");
                (icon.into(), r.name.clone(), sub)
            }
            Item::Risk(id) => {
                let Some(r) = p.risk(id) else { return ("?".into(), String::new(), String::new()) };
                let std = p.risk_standard(r);
                let mut sub = format!("{} · {} · {}", r.kind.label(), p.risk_level(r).label(std), r.status.label());
                if !r.owner.is_empty() {
                    sub += &format!(" · {}", r.owner);
                }
                ("⚠".into(), format!("{}  {}", r.key(), r.title), sub)
            }
        }
    }
}

/// The plan search box (`r`) and its results.
pub fn search_box(app: &mut ProjectFcgApp, ui: &mut egui::Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let pal = super::theme::pal();
    let id = box_id();
    let focused = ui.memory(|m| m.has_focus(id));
    let (mut down, mut up, mut enter, mut shift_enter, mut esc, mut tab, mut back_tab) = (false, false, false, false, false, false, false);
    if focused {
        ui.input_mut(|i| {
            down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
            up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
            shift_enter = i.consume_key(Modifiers::SHIFT, Key::Enter);
            enter = i.consume_key(Modifiers::NONE, Key::Enter);
            esc = i.consume_key(Modifiers::NONE, Key::Escape);
            back_tab = i.consume_key(Modifiers::SHIFT, Key::Tab);
            tab = i.consume_key(Modifiers::NONE, Key::Tab);
        });
    }
    let before = app.item_search.query.clone();
    // the hint is painted in the theme's weak text colour: on the dark
    // title bar it must be a light one while the box is unfocused
    let hint = if focused { pal.text_dim } else { pal.on_band.gamma_multiply(0.8) };
    let saved_weak = ui.visuals().weak_text_color;
    ui.visuals_mut().weak_text_color = Some(hint);
    let resp = ui.put(
        r,
        egui::TextEdit::singleline(&mut app.item_search.query)
            .id(id)
            // Tab picks the kind of item, not the next widget
            .lock_focus(true)
            .hint_text(egui::RichText::new("🔍  Search tasks, tickets, people…  (Ctrl+E)").color(pal.on_band.gamma_multiply(0.75)))
            .text_color(if focused { pal.text } else { pal.on_band })
            .background_color(if focused { pal.bg } else { pal.band_hover })
            .margin(egui::Margin::symmetric(8, 3)),
    );
    ui.visuals_mut().weak_text_color = saved_weak;
    resp.clone().on_hover_text("Search the plan: task names and IDs, ticket keys, people, risks, notes (as Outlook searches mail)");
    let st = &mut app.item_search;
    if resp.gained_focus() || (focused && !st.open) {
        st.open = true;
    }
    if st.query != before {
        st.sel = 0;
    }
    if tab || back_tab {
        let k = Scope::ALL.iter().position(|s| *s == st.scope).unwrap_or(0);
        st.scope = Scope::ALL[(k + if tab { 1 } else { Scope::ALL.len() - 1 }) % Scope::ALL.len()];
        st.sel = 0;
        // keep the cursor here: egui moved its focus on Tab
        ctx.memory_mut(|m| m.request_focus(id));
    }
    let over_list = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())).is_some_and(|p| st.list.is_some_and(|l| l.contains(p)));
    if esc || (st.open && ui.input(|i| i.key_pressed(Key::Escape))) || (resp.lost_focus() && !over_list && !tab && !back_tab) {
        st.open = false;
        st.query.clear();
        st.list = None;
        st.scope = Scope::All;
        ctx.memory_mut(|m| m.surrender_focus(id));
        return;
    }
    if !st.open {
        return;
    }
    let q = st.query.trim().to_string();
    let hits = search(&app.project, &q);
    let rows = app.item_rows(&hits);
    let pickable: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| !matches!(r, Row::Header(..))).map(|(i, _)| i).collect();
    let n = pickable.len();
    let st = &mut app.item_search;
    if n > 0 {
        st.sel = st.sel.min(n - 1);
        if down {
            st.sel = (st.sel + 1) % n;
            st.scroll_to_sel = true;
        }
        if up {
            st.sel = (st.sel + n - 1) % n;
            st.scroll_to_sel = true;
        }
    }
    let run = |app: &mut ProjectFcgApp, row: Row, open: bool, ctx: &egui::Context| match row {
        Row::Item(item) => {
            let st = &mut app.item_search;
            st.open = false;
            st.query.clear();
            st.scope = Scope::All;
            st.list = None;
            ctx.memory_mut(|m| m.surrender_focus(box_id()));
            app.go_to_item(item, open, ctx);
        }
        Row::More(kind, _) => {
            app.item_search.scope = kind;
            app.item_search.sel = 0;
            ctx.memory_mut(|m| m.request_focus(box_id()));
        }
        Row::Header(..) => {}
    };
    if (enter || shift_enter) && n > 0 {
        let row = rows[pickable[app.item_search.sel]];
        run(app, row, shift_enter, &ctx);
        return;
    }

    // ---- the list
    let width = 520.0f32.min(ctx.content_rect().width() - 16.0);
    let x = r.left().min(ctx.content_rect().right() - width - 8.0).max(8.0);
    let counts = |s: Scope| hits.iter().filter(|h| s == Scope::All || h.0.scope() == s).count();
    let mut clicked: Option<(Row, bool)> = None;
    let mut chip: Option<Scope> = None;
    let mut hovered_any = false;
    let words: Vec<String> = q.to_lowercase().split_whitespace().map(|w| w.trim_start_matches('#').to_string()).collect();
    let area = egui::Area::new(Id::unique("item_search_list")).order(egui::Order::Foreground).fixed_pos(pos2(x, r.bottom() + 3.0)).show(&ctx, |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
            ui.set_width(width);
            if q.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Search the plan by name, ID or anything written in it:").size(12.0).color(pal.text_dim));
                for tip in ["7 or #7: task 7", "APP-12: a ticket's key", "R-003: a risk", "a person's name: their tasks, tickets and risks too", "a word from notes, descriptions or labels"] {
                    ui.label(egui::RichText::new(format!("   {tip}")).size(11.5).color(pal.text_dim));
                }
                ui.label(egui::RichText::new("Commands have their own search: Alt+Q.").size(11.0).color(pal.text_dim));
                return;
            }
            // kind chips: All · Tasks · Tickets · Resources · Risks
            ui.horizontal(|ui| {
                for s in Scope::ALL {
                    let c = counts(s);
                    let on = app.item_search.scope == s;
                    let t = egui::RichText::new(format!("{} {c}", s.label())).size(11.5);
                    if ui.add_enabled(c > 0 || s == Scope::All, egui::Button::selectable(on, t)).clicked() {
                        chip = Some(s);
                    }
                }
                ui.weak(egui::RichText::new("Tab: next").size(10.5));
            });
            ui.separator();
            if rows.is_empty() {
                ui.label(egui::RichText::new(format!("Nothing in the plan matches \u{201c}{q}\u{201d}.")).color(pal.text_dim));
                ui.label(egui::RichText::new("Looking for a command? Alt+Q searches those.").size(11.0).color(pal.text_dim));
                return;
            }
            egui::ScrollArea::vertical().max_height(460.0).auto_shrink([false, true]).show(ui, |ui| {
                let mut k = 0usize;
                for row in &rows {
                    match *row {
                        Row::Header(s, c) => {
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(format!("{}  ({c})", s.label())).size(11.0).strong().color(pal.text_dim));
                        }
                        _ => {
                            let sel = k == app.item_search.sel;
                            let h = if matches!(row, Row::More(..)) { 24.0 } else { 34.0 };
                            let (rect, resp) = ui.allocate_exact_size(vec2(width - 12.0, h), Sense::click());
                            if sel && app.item_search.scroll_to_sel {
                                ui.scroll_to_rect(rect, None);
                            }
                            if resp.hovered() {
                                hovered_any = true;
                                if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                                    app.item_search.sel = k;
                                }
                            }
                            let p = ui.painter();
                            if sel || resp.hovered() {
                                p.rect_filled(rect, 4.0, pal.accent.gamma_multiply(if sel { 0.22 } else { 0.12 }));
                            }
                            match *row {
                                Row::Item(item) => {
                                    let (icon, title, sub) = app.describe(item);
                                    p.text(pos2(rect.left() + 18.0, rect.center().y), Align2::CENTER_CENTER, &icon, FontId::proportional(14.0), pal.text);
                                    let mut job = LayoutJob::default();
                                    let base = TextFormat { font_id: FontId::proportional(13.0), color: pal.text, ..Default::default() };
                                    let strong = TextFormat { font_id: FontId::proportional(13.0), color: pal.accent, ..Default::default() };
                                    for (piece, hit) in mark(&title, &words) {
                                        job.append(&piece, 0.0, if hit { strong.clone() } else { base.clone() });
                                    }
                                    job.wrap.max_width = rect.width() - 110.0;
                                    job.wrap.max_rows = 1;
                                    let g = ui.fonts_mut(|f| f.layout_job(job));
                                    p.galley(pos2(rect.left() + 36.0, rect.top() + 3.0), g, pal.text);
                                    let sub = if sub.chars().count() > 90 { format!("{}…", sub.chars().take(89).collect::<String>()) } else { sub };
                                    p.text(pos2(rect.left() + 36.0, rect.bottom() - 4.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(10.5), pal.text_dim);
                                    let where_ = match item {
                                        Item::Task(_) => "Gantt",
                                        Item::Ticket(_) => "Board",
                                        Item::Resource(_) => "Resource Sheet",
                                        Item::Risk(_) => "Risk Register",
                                    };
                                    p.text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, where_, FontId::proportional(10.5), pal.text_dim);
                                }
                                Row::More(s, c) => {
                                    p.text(pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, format!("Show all {c} {}…", s.label().to_lowercase()), FontId::proportional(12.0), pal.accent);
                                }
                                Row::Header(..) => {}
                            }
                            if resp.clicked() {
                                clicked = Some((*row, ui.input(|i| i.modifiers.shift)));
                            }
                            k += 1;
                        }
                    }
                }
            });
            ui.separator();
            ui.label(egui::RichText::new("Enter: go to it · Shift+Enter: open it · Tab: next kind · Esc: close").size(10.5).color(pal.text_dim));
        });
    });
    app.item_search.scroll_to_sel = false;
    app.item_search.list = Some(area.response.rect);
    if let Some(s) = chip {
        app.item_search.scope = s;
        app.item_search.sel = 0;
        ctx.memory_mut(|m| m.request_focus(id));
    }
    if let Some((row, open)) = clicked {
        run(app, row, open, &ctx);
        return;
    }
    if !ui.memory(|m| m.has_focus(id)) && !hovered_any && chip.is_none() && ui.input(|i| i.pointer.any_pressed()) {
        app.item_search.open = false;
        app.item_search.list = None;
    }
}

/// `text` in pieces, marking where a typed word occurs.
fn mark(text: &str, words: &[String]) -> Vec<(String, bool)> {
    let low = text.to_lowercase();
    if low.len() != text.len() {
        return vec![(text.to_string(), false)];
    }
    let mut hit = vec![false; text.len()];
    for w in words.iter().filter(|w| !w.is_empty()) {
        let mut from = 0;
        while let Some(k) = low[from..].find(w.as_str()) {
            let s = from + k;
            for h in hit.iter_mut().skip(s).take(w.len()) {
                *h = true;
            }
            from = s + w.len();
        }
    }
    let mut out: Vec<(String, bool)> = vec![];
    for (i, c) in text.char_indices() {
        let h = hit[i];
        match out.last_mut() {
            Some((s, hh)) if *hh == h => s.push(c),
            _ => out.push((c.to_string(), h)),
        }
    }
    out
}
