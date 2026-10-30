//! Jira CSV: import an export from Jira (Filters ▸ Export ▸ CSV) into the
//! board, and write the board as a CSV Jira's importer accepts
//! (System ▸ External System Import ▸ CSV).
//!
//! Jira's export repeats a header once per value for multi-value fields
//! ("Labels", "Labels", …; "Sprint", "Sprint"), names link columns
//! "Outward issue link (Blocks)" / "Inward issue link (Blocks)", and puts
//! the parent's numeric *Issue id* (not its key) in "Parent". Story points
//! are a custom field whose name depends on the project type: "Story
//! Points" (company-managed) or "Story point estimate" (team-managed).

use crate::board::{IssueType, LinkKind, Priority};
use crate::model::{Project, ResourceType};
use chrono::{NaiveDate, NaiveDateTime};
use std::collections::HashMap;

/// RFC 4180 CSV: quoted fields, doubled quotes, newlines inside quotes,
/// CRLF or LF, a leading BOM. Blank lines are skipped.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Excel in some locales writes ';'-separated files: sniff the header.
    let first = text.lines().next().unwrap_or("");
    let sep = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let mut rows = vec![];
    let mut row: Vec<String> = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == sep => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn esc(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Jira's default CSV date format ("dd/MMM/yy h:mm a") plus the ISO and
/// spreadsheet-ish forms people end up with.
pub fn parse_jira_date(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    const DT: [&str; 10] = [
        "%d/%b/%y %I:%M %p",
        "%d/%b/%Y %I:%M %p",
        "%d/%b/%y %H:%M",
        "%d/%b/%Y %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%m/%d/%Y %I:%M %p",
    ];
    for f in DT {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d);
        }
    }
    // ISO with an offset ("2026-10-27T15:45:00.000+0000")
    if let Ok(d) = chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z") {
        return Some(d.naive_local());
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.naive_local());
    }
    for f in ["%d/%b/%y", "%d/%b/%Y", "%Y-%m-%d", "%m/%d/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return d.and_hms_opt(0, 0, 0);
        }
    }
    None
}

pub fn fmt_jira_date(d: NaiveDateTime) -> String {
    d.format("%d/%b/%y %-I:%M %p").to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Col {
    Summary,
    Key,
    Id,
    Type,
    Status,
    Priority,
    Assignee,
    Labels,
    Description,
    Sprint,
    Points,
    Parent,
    ParentKey,
    EpicLink,
    EpicName,
    Created,
    Resolved,
    Updated,
    Outward(LinkKind),
    Inward(LinkKind),
    Ignore,
}

fn classify(h: &str) -> Col {
    let n = h.trim().to_lowercase();
    let link = |prefix: &str| -> Option<LinkKind> {
        let rest = n.strip_prefix(prefix)?.trim();
        let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
        LinkKind::from_jira_name(inner)
    };
    if let Some(k) = link("outward issue link") {
        return Col::Outward(k);
    }
    if let Some(k) = link("inward issue link") {
        return Col::Inward(k);
    }
    // our own export and hand-made files may use bare link-type headers
    if let Some(k) = LinkKind::from_jira_name(&n).filter(|_| n != "relates to") {
        return Col::Outward(k);
    }
    let n = n.strip_prefix("custom field (").and_then(|x| x.strip_suffix(')')).map(str::to_string).unwrap_or(n);
    match n.as_str() {
        "summary" | "title" => Col::Summary,
        "issue key" | "key" | "work item key" => Col::Key,
        "issue id" | "id" | "work item id" => Col::Id,
        "issue type" | "work type" | "type" => Col::Type,
        "status" => Col::Status,
        "priority" => Col::Priority,
        "assignee" => Col::Assignee,
        "labels" | "label" => Col::Labels,
        "description" => Col::Description,
        "sprint" => Col::Sprint,
        "story points" | "story point estimate" | "story point" | "points" | "estimate" => Col::Points,
        "parent" | "parent id" => Col::Parent,
        "parent key" => Col::ParentKey,
        "epic link" => Col::EpicLink,
        "epic name" => Col::EpicName,
        "created" => Col::Created,
        "resolved" | "resolution date" => Col::Resolved,
        "updated" => Col::Updated,
        _ => Col::Ignore,
    }
}

#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    /// Create a sprint task (after the last one) for each sprint name the
    /// project doesn't have yet; otherwise those issues go to the backlog.
    pub create_sprints: bool,
    /// Add assignees missing from the Resource Sheet as work resources;
    /// otherwise they're dropped.
    pub create_people: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    pub tickets: usize,
    pub epics: usize,
    pub links: usize,
    pub sprints_created: Vec<String>,
    pub people_created: Vec<String>,
    pub warnings: Vec<String>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        let mut s = format!("Imported {} issue{}", self.tickets, if self.tickets == 1 { "" } else { "s" });
        if self.epics > 0 {
            s += &format!(" ({} epic{})", self.epics, if self.epics == 1 { "" } else { "s" });
        }
        if self.links > 0 {
            s += &format!(", {} link{}", self.links, if self.links == 1 { "" } else { "s" });
        }
        if !self.sprints_created.is_empty() {
            s += &format!(", {} new sprint{}", self.sprints_created.len(), if self.sprints_created.len() == 1 { "" } else { "s" });
        }
        s
    }
}

struct Row {
    summary: String,
    key: String,
    jira_id: String,
    kind: IssueType,
    status: String,
    priority: Option<Priority>,
    assignee: String,
    labels: Vec<String>,
    description: String,
    sprint: String,
    points: Option<f64>,
    parent: String,
    parent_key: String,
    epic_link: String,
    created: Option<NaiveDateTime>,
    resolved: Option<NaiveDateTime>,
    updated: Option<NaiveDateTime>,
    outward: Vec<(LinkKind, String)>,
    inward: Vec<(LinkKind, String)>,
}

/// Split a key "ABC-123" into ("ABC", 123).
fn split_key(k: &str) -> Option<(String, u32)> {
    let (p, n) = k.trim().rsplit_once('-')?;
    let n: u32 = n.parse().ok()?;
    (!p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| (p.to_string(), n))
}

/// Map a Jira status to a board column: same name, else by Jira's status
/// categories (done-like → last column, in-progress-like → the column whose
/// name looks like it, else the second), else the first.
fn status_column(p: &Project, status: &str) -> (usize, bool) {
    let s = status.trim().to_lowercase();
    let cols: Vec<String> = p.board.columns.iter().map(|c| c.to_lowercase()).collect();
    if let Some(i) = cols.iter().position(|c| *c == s) {
        return (i, true);
    }
    let done = p.board.done_column();
    if s.is_empty() {
        return (0, true);
    }
    let has = |w: &[&str]| w.iter().any(|x| s.contains(x));
    // negated statuses ("Not done", "Incomplete", "Unresolved", "Not
    // started") are open work, whatever words they contain
    let negated = s.starts_with("not ") || ["unresolved", "undone", "unstarted", "unfinished", "incomplete"].iter().any(|w| s.starts_with(w)) || s.contains(" not ") || s.contains("won't") || s.contains("reopen");
    if negated {
        return (0, true);
    }
    if has(&["done", "closed", "resolved", "complete", "released", "shipped", "fixed"]) {
        return (done, true);
    }
    if has(&["review", "qa", "test", "verif", "accept"]) {
        if let Some(i) = cols.iter().position(|c| c.contains("review") || c.contains("test") || c.contains("qa")) {
            return (i.min(done), true);
        }
    }
    if has(&["progress", "doing", "develop", "active", "started", "review", "qa", "test"]) {
        let i = cols.iter().position(|c| c.contains("progress") || c.contains("doing")).unwrap_or(1.min(done));
        return (i, true);
    }
    if has(&["to do", "todo", "open", "backlog", "new", "selected", "ready"]) {
        return (0, true);
    }
    (0, false)
}

/// Import a Jira CSV export into `p`'s board (adds to what's there).
pub fn import_csv(p: &mut Project, text: &str, opts: &ImportOptions) -> Result<ImportReport, String> {
    let rows = parse_csv(text);
    let Some((header, body)) = rows.split_first() else {
        return Err("The file is empty.".into());
    };
    let cols: Vec<Col> = header.iter().map(|h| classify(h)).collect();
    if !cols.contains(&Col::Summary) {
        return Err("This doesn't look like a Jira CSV export: there's no \"Summary\" column.".into());
    }
    let mut rep = ImportReport::default();
    // ---- read rows (a key repeated on several rows adds links/labels) ----
    let mut parsed: Vec<Row> = vec![];
    let mut by_key_row: HashMap<String, usize> = HashMap::new();
    for (ln, r) in body.iter().enumerate() {
        let mut row = Row {
            summary: String::new(),
            key: String::new(),
            jira_id: String::new(),
            kind: IssueType::Story,
            status: String::new(),
            priority: None,
            assignee: String::new(),
            labels: vec![],
            description: String::new(),
            sprint: String::new(),
            points: None,
            parent: String::new(),
            parent_key: String::new(),
            epic_link: String::new(),
            created: None,
            resolved: None,
            updated: None,
            outward: vec![],
            inward: vec![],
        };
        let mut type_name = String::new();
        let mut epic_name = String::new();
        for (c, v) in cols.iter().zip(r.iter()) {
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            match c {
                Col::Summary => row.summary = v.to_string(),
                Col::Key => row.key = v.to_string(),
                Col::Id => row.jira_id = v.to_string(),
                Col::Type => type_name = v.to_string(),
                Col::Status => row.status = v.to_string(),
                Col::Priority => row.priority = Priority::from_name(v),
                Col::Assignee => row.assignee = v.to_string(),
                Col::Labels => row.labels.extend(v.split([' ', ',']).filter(|x| !x.is_empty()).map(str::to_string)),
                Col::Description => row.description = v.replace("\r\n", "\n"),
                // the last sprint listed is the one it's in now
                Col::Sprint => row.sprint = v.to_string(),
                Col::Points => row.points = v.replace(',', ".").parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0),
                Col::Parent => row.parent = v.to_string(),
                Col::ParentKey => row.parent_key = v.to_string(),
                Col::EpicLink => row.epic_link = v.to_string(),
                Col::EpicName => epic_name = v.to_string(),
                Col::Created => row.created = parse_jira_date(v),
                Col::Resolved => row.resolved = parse_jira_date(v),
                Col::Updated => row.updated = parse_jira_date(v),
                Col::Outward(k) => row.outward.extend(v.split([',', ' ']).filter(|x| !x.is_empty()).map(|x| (*k, x.to_string()))),
                Col::Inward(k) => row.inward.extend(v.split([',', ' ']).filter(|x| !x.is_empty()).map(|x| (*k, x.to_string()))),
                Col::Ignore => {}
            }
        }
        if !type_name.is_empty() {
            match IssueType::from_name(&type_name) {
                Some(k) => row.kind = k,
                None => {
                    let w = format!("Unknown issue type \"{type_name}\" imported as Story.");
                    if !rep.warnings.contains(&w) {
                        rep.warnings.push(w);
                    }
                }
            }
        }
        if row.summary.is_empty() {
            row.summary = epic_name;
        }
        if row.summary.is_empty() && row.key.is_empty() {
            continue;
        }
        if row.summary.is_empty() {
            row.summary = row.key.clone();
        }
        // a repeated key is a continuation row (Jira's "one row per link")
        if !row.key.is_empty() {
            if let Some(&i) = by_key_row.get(&row.key) {
                let prev = &mut parsed[i];
                prev.outward.extend(row.outward);
                prev.inward.extend(row.inward);
                for l in row.labels {
                    if !prev.labels.contains(&l) {
                        prev.labels.push(l);
                    }
                }
                continue;
            }
            by_key_row.insert(row.key.clone(), parsed.len());
        }
        let _ = ln;
        parsed.push(row);
    }
    if parsed.is_empty() {
        return Err("No issues found in the file.".into());
    }

    // ---- ticket numbers: keep Jira's when they're free ----
    let keys: Vec<Option<(String, u32)>> = parsed.iter().map(|r| split_key(&r.key)).collect();
    let prefixes: std::collections::HashSet<&str> = keys.iter().flatten().map(|k| k.0.as_str()).collect();
    let mut nums: Vec<u32> = keys.iter().flatten().map(|k| k.1).collect();
    nums.sort();
    let unique = nums.windows(2).all(|w| w[0] != w[1]) && nums.len() == parsed.len();
    let free = nums.iter().all(|n| p.ticket(*n).is_none());
    let same_prefix = prefixes.len() == 1 && {
        let pre = *prefixes.iter().next().unwrap();
        p.board.key_prefix.trim().is_empty() && p.tickets.is_empty() || p.board.key_prefix.trim().eq_ignore_ascii_case(pre)
    };
    let keep_numbers = unique && free && same_prefix;
    if keep_numbers {
        p.board.key_prefix = prefixes.iter().next().unwrap().to_string();
    } else if keys.iter().any(|k| k.is_some()) {
        rep.warnings.push("Issue keys were renumbered to fit this project's tickets.".into());
    }

    // ---- sprints ----
    let mut sprint_of: HashMap<String, Option<u32>> = HashMap::new();
    let mut wanted: Vec<String> = vec![];
    for r in &parsed {
        // sub-tasks count too: one whose parent doesn't fit becomes a task and keeps its sprint
        if !r.sprint.is_empty() && !wanted.contains(&r.sprint) && r.kind != IssueType::Epic {
            wanted.push(r.sprint.clone());
        }
    }
    // Numbered sprints in number order ("Sprint 2" before "Sprint 10"),
    // filling the places numbered names had in the file; others ("Hardening")
    // stay where they first appeared.
    let trailing = |s: &str| -> Option<u64> { s.rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()) };
    let mut numbered: Vec<String> = wanted.iter().filter(|n| trailing(n).is_some()).cloned().collect();
    numbered.sort_by_key(|n| (n.trim_end_matches(|c: char| c.is_ascii_digit()).to_lowercase(), trailing(n)));
    let mut it = numbered.into_iter();
    let ordered: Vec<String> = wanted.iter().map(|n| if trailing(n).is_some() { it.next().unwrap() } else { n.clone() }).collect();
    for name in ordered {
        let existing = p.tasks.iter().find(|t| t.is_sprint && t.name.trim().eq_ignore_ascii_case(name.trim())).map(|t| t.uid);
        let uid = match existing {
            Some(u) => Some(u),
            None if opts.create_sprints => {
                let row = p.new_sprint();
                p.tasks[row].name = name.clone();
                crate::schedule::schedule(p);
                rep.sprints_created.push(name.clone());
                Some(p.tasks[row].uid)
            }
            None => None,
        };
        if uid.is_none() {
            let w = format!("Sprint \"{name}\" isn't in this project; its issues went to the backlog.");
            rep.warnings.push(w);
        }
        sprint_of.insert(name, uid);
    }

    // ---- people ----
    let mut person: HashMap<String, Option<u32>> = HashMap::new();
    for r in &parsed {
        if r.assignee.is_empty() || person.contains_key(&r.assignee) {
            continue;
        }
        let found = p
            .resources
            .iter()
            .find(|x| x.name.trim().eq_ignore_ascii_case(r.assignee.trim()) || (!x.email.is_empty() && x.email.eq_ignore_ascii_case(r.assignee.trim())))
            .map(|x| x.uid);
        let uid = match found {
            Some(u) => Some(u),
            None if opts.create_people => {
                let at = p.resources.len();
                let i = p.insert_resource(at, &r.assignee, ResourceType::Work);
                rep.people_created.push(r.assignee.clone());
                Some(p.resources[i].uid)
            }
            None => None,
        };
        person.insert(r.assignee.clone(), uid);
    }

    // ---- create tickets (epics first, sub-tasks last, so parents exist) ----
    let mut order: Vec<usize> = (0..parsed.len()).collect();
    order.sort_by_key(|&i| match parsed[i].kind {
        IssueType::Epic => 0,
        IssueType::Subtask => 2,
        _ => 1,
    });
    let mut new_id: Vec<u32> = vec![0; parsed.len()];
    let mut unknown_status: Vec<String> = vec![];
    let now = chrono::Local::now().naive_local();
    for &i in &order {
        let r = &parsed[i];
        let id = match (keep_numbers, &keys[i]) {
            (true, Some((_, n))) => {
                p.next_ticket_id = p.next_ticket_id.max(*n);
                let tmp = p.add_ticket(&r.summary, None)?;
                let k = p.ticket_index(tmp).unwrap();
                p.tickets[k].id = *n;
                p.next_ticket_id = p.next_ticket_id.max(n + 1).max(tmp);
                *n
            }
            _ => p.add_ticket(&r.summary, None)?,
        };
        new_id[i] = id;
        let (col, known) = status_column(p, &r.status);
        if !known && !unknown_status.contains(&r.status) {
            unknown_status.push(r.status.clone());
        }
        let done = col >= p.board.done_column();
        let sprint = sprint_of.get(&r.sprint).copied().flatten();
        let k = p.ticket_index(id).unwrap();
        let t = &mut p.tickets[k];
        t.kind = r.kind;
        t.description = r.description.clone();
        t.points = r.points;
        t.priority = r.priority.unwrap_or_default();
        t.labels = r.labels.clone();
        t.assignee = person.get(&r.assignee).copied().flatten();
        t.column = col;
        t.sprint = if r.kind == IssueType::Epic { None } else { sprint };
        t.created = r.created.or(t.created);
        t.done_at = done.then(|| r.resolved.or(r.updated).unwrap_or(now));
        if r.kind == IssueType::Epic {
            rep.epics += 1;
        }
        rep.tickets += 1;
    }
    p.next_ticket_id = p.next_ticket_id.max(p.tickets.iter().map(|t| t.id + 1).max().unwrap_or(1));
    if !unknown_status.is_empty() {
        rep.warnings.push(format!("Statuses with no matching column went to \"{}\": {}.", p.board.columns[0], unknown_status.join(", ")));
    }

    // ---- hierarchy and links ----
    let by_jira_id: HashMap<&str, u32> = parsed.iter().zip(&new_id).filter(|(r, _)| !r.jira_id.is_empty()).map(|(r, &id)| (r.jira_id.as_str(), id)).collect();
    let by_key: HashMap<String, u32> = parsed.iter().zip(&new_id).filter(|(r, _)| !r.key.is_empty()).map(|(r, &id)| (r.key.to_uppercase(), id)).collect();
    let resolve = |s: &str| -> Option<u32> { by_jira_id.get(s.trim()).copied().or_else(|| by_key.get(&s.trim().to_uppercase()).copied()) };
    let mut bad_parents = 0;
    for &i in &order {
        let r = &parsed[i];
        let id = new_id[i];
        let parent = [&r.parent, &r.parent_key, &r.epic_link].into_iter().filter(|s| !s.is_empty()).find_map(|s| resolve(s));
        match parent {
            Some(par) => {
                if p.set_ticket_parent(id, Some(par)).is_err() {
                    bad_parents += 1;
                    if r.kind == IssueType::Subtask {
                        // a sub-task needs a story/task/bug parent: keep it as a task
                        let k = p.ticket_index(id).unwrap();
                        p.tickets[k].kind = IssueType::Task;
                    }
                }
            }
            None if r.kind == IssueType::Subtask => {
                // a sub-task without its parent in the file: keep it as a task
                let k = p.ticket_index(id).unwrap();
                p.tickets[k].kind = IssueType::Task;
                bad_parents += 1;
            }
            None => {}
        }
    }
    if bad_parents > 0 {
        rep.warnings.push(format!("{bad_parents} issue{} had a parent that isn't in the file or doesn't fit (sub-tasks without one became tasks).", if bad_parents == 1 { "" } else { "s" }));
    }
    let mut missing_links = 0;
    for (i, r) in parsed.iter().enumerate() {
        let id = new_id[i];
        for (kind, other) in &r.outward {
            match resolve(other) {
                Some(o) => {
                    if p.add_ticket_link(id, *kind, o).is_ok() {
                        rep.links += 1;
                    }
                }
                None => missing_links += 1,
            }
        }
        for (kind, other) in &r.inward {
            match resolve(other) {
                Some(o) => {
                    if p.add_ticket_link(o, *kind, id).is_ok() {
                        rep.links += 1;
                    }
                }
                None => missing_links += 1,
            }
        }
    }
    if missing_links > 0 {
        rep.warnings.push(if missing_links == 1 { "1 link pointed at an issue that isn't in the file and was skipped.".to_string() } else { format!("{missing_links} links pointed at issues that aren't in the file and were skipped.") });
    }
    Ok(rep)
}

/// The board as a CSV Jira's importer reads: parents above children,
/// "Parent" holding the parent's Issue id, repeated Labels and link columns.
pub fn export_csv(p: &Project) -> String {
    let mut order: Vec<usize> = (0..p.tickets.len()).collect();
    order.sort_by_key(|&k| match p.tickets[k].kind {
        IssueType::Epic => 0,
        IssueType::Subtask => 2,
        _ => 1,
    });
    let max_labels = p.tickets.iter().map(|t| t.labels.len()).max().unwrap_or(0).max(1);
    let mut link_cols: Vec<(LinkKind, usize)> = vec![];
    for kind in LinkKind::ALL {
        let n = p.tickets.iter().map(|t| t.links.iter().filter(|l| l.kind == kind).count()).max().unwrap_or(0);
        if n > 0 {
            link_cols.push((kind, n));
        }
    }
    let mut head: Vec<String> = ["Issue key", "Issue id", "Summary", "Issue Type", "Status", "Priority", "Assignee", "Created", "Resolved", "Sprint", "Custom field (Story Points)", "Parent", "Parent summary", "Description"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    head.extend(std::iter::repeat_n("Labels".to_string(), max_labels));
    for (k, n) in &link_cols {
        head.extend(std::iter::repeat_n(format!("Outward issue link ({})", k.jira_name()), *n));
    }
    let mut out = head.iter().map(|h| esc(h)).collect::<Vec<_>>().join(",");
    out.push('\n');
    for k in order {
        let t = &p.tickets[k];
        let sprint = t.sprint.and_then(|u| p.task_by_uid(u)).map(|s| s.name.clone()).unwrap_or_default();
        let assignee = t.assignee.and_then(|u| p.resources.iter().find(|r| r.uid == u)).map(|r| r.name.clone()).unwrap_or_default();
        let parent = t.parent.and_then(|x| p.ticket(x));
        let mut cells: Vec<String> = vec![
            p.ticket_key(t.id).trim_start_matches('#').to_string(),
            t.id.to_string(),
            t.title.clone(),
            t.kind.label().to_string(),
            p.board.columns.get(t.column).cloned().unwrap_or_default(),
            t.priority.label().to_string(),
            assignee,
            t.created.map(fmt_jira_date).unwrap_or_default(),
            if p.is_done(t) { t.done_at.map(fmt_jira_date).unwrap_or_default() } else { String::new() },
            sprint,
            t.points.map(crate::duration::fmt_num).unwrap_or_default(),
            parent.map(|x| x.id.to_string()).unwrap_or_default(),
            parent.map(|x| x.title.clone()).unwrap_or_default(),
            t.description.clone(),
        ];
        for i in 0..max_labels {
            cells.push(t.labels.get(i).cloned().unwrap_or_default());
        }
        for (kind, n) in &link_cols {
            let targets: Vec<String> = t.links.iter().filter(|l| l.kind == *kind).map(|l| p.ticket_key(l.to).trim_start_matches('#').to_string()).collect();
            for i in 0..*n {
                cells.push(targets.get(i).cloned().unwrap_or_default());
            }
        }
        out.push_str(&cells.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_handles_quotes_and_newlines() {
        let rows = parse_csv("\u{feff}a,b,c\r\n\"x, y\",\"he said \"\"hi\"\"\",\"line1\nline2\"\n\n1,,3");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["x, y", "he said \"hi\"", "line1\nline2"]);
        assert_eq!(rows[2], vec!["1", "", "3"]);
    }

    #[test]
    fn semicolon_files() {
        let rows = parse_csv("Summary;Issue key\nA;X-1\n");
        assert_eq!(rows[1], vec!["A", "X-1"]);
    }

    #[test]
    fn dates() {
        let d = parse_jira_date("27/Oct/26 3:45 PM").unwrap();
        assert_eq!(d.to_string(), "2026-10-27 15:45:00");
        assert_eq!(parse_jira_date(&fmt_jira_date(d)), Some(d));
        assert!(parse_jira_date("2026-10-27T15:45:00.000+0000").is_some());
        assert!(parse_jira_date("2026-10-27").is_some());
        assert!(parse_jira_date("soon").is_none());
    }

    #[test]
    fn header_classification() {
        assert_eq!(classify("Outward issue link (Blocks)"), Col::Outward(LinkKind::Blocks));
        assert_eq!(classify("Inward issue link (Cloners)"), Col::Inward(LinkKind::Clones));
        assert_eq!(classify("Custom field (Story point estimate)"), Col::Points);
        assert_eq!(classify("Custom field (Epic Link)"), Col::EpicLink);
        assert_eq!(classify("Work type"), Col::Type);
        assert_eq!(classify("Watchers"), Col::Ignore);
    }
}
