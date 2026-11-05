//! Risk register: threats, opportunities and issues, each tied to the
//! whole project or to tasks, resources or releases, and assessed either
//! freely or by one of two standards:
//!
//! * **DoD**: the *DoD Risk, Issue, and Opportunity (RIO) Management Guide
//!   for Defense Acquisition Programs* (September 2023). "If … then …"
//!   statements; likelihood 1–5 (Not Likely … Near Certainty, with the
//!   guide's probability bands); consequence 1–5 rated for cost, schedule
//!   and performance, the worst setting the level; the 5×5 risk reporting
//!   matrix (Low/Moderate/High); handling Accept, Avoid, Transfer or
//!   Control, with a burn-down plan of mitigation steps. Opportunities are
//!   handled Pursue now, Defer, Reevaluate or Reject; issues have no
//!   likelihood (they've happened) and are handled Ignore, Control, Avoid
//!   or Transfer.
//! * **PMI**: the *PMBOK® Guide* and PMI's *Standard for Risk Management*.
//!   Cause → risk → effect statements; probability Very Low … Very High
//!   (0.1–0.9); impact on cost, schedule, scope and quality (0.05–0.8);
//!   probability × impact scores with High ≥ 0.18 and Moderate ≥ 0.05;
//!   threat responses Escalate, Avoid, Transfer, Mitigate, Accept and
//!   opportunity responses Escalate, Exploit, Share, Enhance, Accept;
//!   triggers, contingency and fallback plans, residual and secondary
//!   risks, urgency, and expected monetary value.
//! * **None**: likelihood and impact 1–5, score = L × I (1–25), Low ≤ 4,
//!   Medium ≤ 12, High above; responses in your own words.
//!
//! Each risk follows the register's standard unless it names its own.

use crate::model::Project;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskStandard {
    #[default]
    None,
    Dod,
    Pmi,
}

impl RiskStandard {
    pub const ALL: [RiskStandard; 3] = [RiskStandard::None, RiskStandard::Dod, RiskStandard::Pmi];
    pub fn label(self) -> &'static str {
        match self {
            RiskStandard::None => "No standard",
            RiskStandard::Dod => "DoD (RIO Guide)",
            RiskStandard::Pmi => "PMI (PMBOK)",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            RiskStandard::None => "—",
            RiskStandard::Dod => "DoD",
            RiskStandard::Pmi => "PMI",
        }
    }

    /// Names of likelihood levels 1–5, and the probability each stands for.
    pub fn likelihood_levels(self) -> [(&'static str, &'static str); 5] {
        match self {
            RiskStandard::Dod => [
                ("Not Likely", ">1% to ≤20%"),
                ("Low Likelihood", ">20% to ≤40%"),
                ("Likely", ">40% to ≤60%"),
                ("Highly Likely", ">60% to ≤80%"),
                ("Near Certainty", ">80% to ≤99%"),
            ],
            RiskStandard::Pmi => [("Very Low", "1–10% (0.1)"), ("Low", "11–30% (0.3)"), ("Medium", "31–50% (0.5)"), ("High", "51–70% (0.7)"), ("Very High", ">70% (0.9)")],
            RiskStandard::None => [("Very low", ""), ("Low", ""), ("Medium", ""), ("High", ""), ("Very high", "")],
        }
    }

    /// Names of consequence (impact) levels 1–5.
    pub fn consequence_levels(self) -> [&'static str; 5] {
        match self {
            RiskStandard::Dod => ["Minimal Impact", "Minor Impact", "Moderate Impact", "Significant Impact", "Critical Impact"],
            RiskStandard::Pmi => ["Very Low (0.05)", "Low (0.10)", "Medium (0.20)", "High (0.40)", "Very High (0.80)"],
            RiskStandard::None => ["Very low", "Low", "Medium", "High", "Very high"],
        }
    }

    /// The dimensions consequence is rated in (the worst one counts).
    pub fn dimensions(self) -> &'static [Dimension] {
        match self {
            RiskStandard::Dod => &[Dimension::Cost, Dimension::Schedule, Dimension::Performance],
            RiskStandard::Pmi => &[Dimension::Cost, Dimension::Schedule, Dimension::Scope, Dimension::Quality],
            RiskStandard::None => &[Dimension::Overall],
        }
    }

    /// Categories offered (any text can be typed).
    pub fn categories(self) -> &'static [&'static str] {
        match self {
            RiskStandard::Dod => &["Technical", "Programmatic", "Business"],
            RiskStandard::Pmi => &["Technical", "Management", "Commercial", "External"],
            RiskStandard::None => &["Schedule", "Cost", "Scope", "Resources", "Quality", "External"],
        }
    }
}

/// What a consequence is rated on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    Overall,
    Cost,
    Schedule,
    Performance,
    Scope,
    Quality,
}

impl Dimension {
    pub fn label(self) -> &'static str {
        match self {
            Dimension::Overall => "Impact",
            Dimension::Cost => "Cost",
            Dimension::Schedule => "Schedule",
            Dimension::Performance => "Performance",
            Dimension::Scope => "Scope",
            Dimension::Quality => "Quality",
        }
    }

    /// The DoD guide's consequence criteria (Table 2-1), abbreviated, for
    /// levels 1–5.
    pub fn dod_criteria(self) -> [&'static str; 5] {
        match self {
            Dimension::Cost => [
                "Costs within approved funding levels",
                "Unit production cost increase <1%; manageable within the program",
                "Cost increase ≥1% to <5%; manageable with PEO or Service help",
                "Cost increase ≥5% to <10%, or exceeds the life-cycle ownership cost KSA",
                "Cost increase ≥10% over APB objectives, or breaches affordability caps",
            ],
            Dimension::Schedule => [
                "Minimal schedule impact",
                "Some slip, but APB dates are met",
                "Non-APB key events may slip",
                "Slip to within 2 months of an APB threshold date, or fielding delayed >6 months",
                "Slip needs a major rebaseline, or misses APB threshold dates",
            ],
            Dimension::Performance => [
                "Minimal consequence to technical performance",
                "Reduced performance, tolerable with little impact",
                "Can't meet lower-tier attributes, TPMs or CTPs",
                "Impairs a KSA; margins exhausted",
                "Precludes a KPP or key threshold, or prevents the mission",
            ],
            _ => [""; 5],
        }
    }
}

/// Threat, opportunity or issue (DoD's "RIO").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskKind {
    #[default]
    Threat,
    Opportunity,
    /// It has happened (or certainly will): no likelihood.
    Issue,
}

impl RiskKind {
    pub const ALL: [RiskKind; 3] = [RiskKind::Threat, RiskKind::Opportunity, RiskKind::Issue];
    pub fn label(self) -> &'static str {
        match self {
            RiskKind::Threat => "Risk",
            RiskKind::Opportunity => "Opportunity",
            RiskKind::Issue => "Issue",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskStatus {
    #[default]
    Open,
    /// A response is being carried out.
    Handling,
    /// Accepted and watched.
    Watch,
    /// It happened (a risk that became an issue).
    Realized,
    Closed,
}

impl RiskStatus {
    pub const ALL: [RiskStatus; 5] = [RiskStatus::Open, RiskStatus::Handling, RiskStatus::Watch, RiskStatus::Realized, RiskStatus::Closed];
    pub fn label(self) -> &'static str {
        match self {
            RiskStatus::Open => "Open",
            RiskStatus::Handling => "Handling",
            RiskStatus::Watch => "Watch",
            RiskStatus::Realized => "Realized",
            RiskStatus::Closed => "Closed",
        }
    }
    pub fn is_active(self) -> bool {
        !matches!(self, RiskStatus::Closed)
    }
}

/// The level a rating comes out at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Not assessed yet.
    None,
    Low,
    Moderate,
    High,
}

impl RiskLevel {
    pub fn label(self, std: RiskStandard) -> &'static str {
        match (self, std) {
            (RiskLevel::None, _) => "Not rated",
            (RiskLevel::Low, _) => "Low",
            (RiskLevel::Moderate, RiskStandard::None) => "Medium",
            (RiskLevel::Moderate, _) => "Moderate",
            (RiskLevel::High, _) => "High",
        }
    }
}

/// The DoD 5×5 risk reporting matrix: `DOD_MATRIX[likelihood-1][consequence-1]`
/// (0 Low/green, 1 Moderate/yellow, 2 High/red), as the guide draws it.
pub const DOD_MATRIX: [[u8; 5]; 5] = [
    [0, 0, 0, 0, 1], // 1 Not Likely
    [0, 0, 1, 1, 1], // 2 Low Likelihood
    [0, 1, 1, 1, 2], // 3 Likely
    [0, 1, 1, 2, 2], // 4 Highly Likely
    [0, 1, 2, 2, 2], // 5 Near Certainty
];

/// PMI's probability and impact scale values for levels 1–5.
pub const PMI_PROBABILITY: [f64; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];
pub const PMI_IMPACT: [f64; 5] = [0.05, 0.10, 0.20, 0.40, 0.80];
/// PMI priority thresholds on probability × impact.
pub const PMI_HIGH: f64 = 0.18;
pub const PMI_MODERATE: f64 = 0.05;

/// A rating: likelihood and the consequence in each dimension, 1–5
/// (0: not rated).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub likelihood: u8,
    /// (dimension, level 1–5); the worst counts.
    pub consequence: Vec<(Dimension, u8)>,
}

impl Rating {
    pub fn new(likelihood: u8, consequence: u8) -> Self {
        Rating { likelihood, consequence: vec![(Dimension::Overall, consequence)] }
    }
    /// The consequence that counts: the worst dimension.
    pub fn worst(&self) -> u8 {
        self.consequence.iter().map(|(_, c)| *c).max().unwrap_or(0).min(5)
    }
    pub fn get(&self, d: Dimension) -> u8 {
        self.consequence.iter().find(|(x, _)| *x == d).map(|(_, c)| *c).unwrap_or(0)
    }
    pub fn set(&mut self, d: Dimension, level: u8) {
        let level = level.min(5);
        match self.consequence.iter_mut().find(|(x, _)| *x == d) {
            Some(x) => x.1 = level,
            None => self.consequence.push((d, level)),
        }
        self.consequence.retain(|(_, c)| *c > 0);
    }
    pub fn is_rated(&self, kind: RiskKind) -> bool {
        self.worst() > 0 && (kind == RiskKind::Issue || self.likelihood > 0)
    }
}

/// A planned step of a response (DoD's burn-down activities; PMI's
/// response actions): what's done by when, and the rating expected after.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RiskStep {
    pub activity: String,
    pub date: Option<NaiveDate>,
    pub done: bool,
    /// The rating once this step is done.
    pub after: Rating,
}

/// What a risk is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskLink {
    /// A task (by unique ID): WBS/IMS linkage.
    Task(u32),
    Resource(u32),
    /// A board release (a delivery).
    Release(u32),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Risk {
    pub id: u32,
    pub title: String,
    pub kind: RiskKind,
    /// None: the register's standard.
    pub standard: Option<RiskStandard>,
    pub category: String,
    /// DoD "if" / PMI risk event.
    pub event: String,
    /// DoD "then" / PMI effect.
    pub consequence: String,
    /// Root cause (DoD, when known) / PMI cause.
    pub cause: String,
    pub description: String,
    /// Tied to these; empty: the whole project.
    pub links: Vec<RiskLink>,
    pub owner: String,
    pub status: RiskStatus,
    pub identified: Option<NaiveDate>,
    /// Expected closure (DoD) / response due.
    pub due: Option<NaiveDate>,
    pub closed: Option<NaiveDate>,
    /// The current rating.
    pub rating: Rating,
    /// Where handling should bring it (DoD target rating / PMI residual).
    pub target: Rating,
    /// One of the standard's responses ("Mitigate", "Control"…), or any text.
    pub response: String,
    pub response_plan: String,
    pub steps: Vec<RiskStep>,
    // ---- PMI ----
    pub triggers: String,
    pub contingency: String,
    pub fallback: String,
    pub secondary: String,
    /// How soon it could happen ("this sprint", "Q3"…).
    pub urgency: String,
    /// Money it would cost (threat) or save (opportunity), for EMV.
    pub cost_impact: f64,
    /// Working days it would add (threat) or save.
    pub schedule_days: f64,
    /// For "no standard": a probability typed directly (0–100), if wanted.
    pub probability_pct: Option<f64>,
}

/// Register-wide settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskSettings {
    /// The standard risks follow unless they name their own.
    pub standard: RiskStandard,
    /// Matrix cell levels if your risk management plan differs from the
    /// standard's (`[likelihood-1][consequence-1]`, 0/1/2).
    pub matrix: Option<[[u8; 5]; 5]>,
    pub next_id: u32,
}

impl Risk {
    /// "R-007" / "O-003" / "I-002".
    pub fn key(&self) -> String {
        let p = match self.kind {
            RiskKind::Threat => "R",
            RiskKind::Opportunity => "O",
            RiskKind::Issue => "I",
        };
        format!("{p}-{:03}", self.id)
    }

    /// The standard's statement of the risk.
    /// Levels into 0–5 and amounts finite; returns how many were fixed.
    pub fn clamp(&mut self) -> usize {
        let mut fixed = 0;
        for rating in std::iter::once(&mut self.rating).chain(std::iter::once(&mut self.target)).chain(self.steps.iter_mut().map(|s| &mut s.after)) {
            if rating.likelihood > 5 {
                rating.likelihood = 5;
                fixed += 1;
            }
            for (_, c) in rating.consequence.iter_mut() {
                if *c > 5 {
                    *c = 5;
                    fixed += 1;
                }
            }
        }
        for v in [&mut self.cost_impact, &mut self.schedule_days] {
            if !v.is_finite() {
                *v = 0.0;
                fixed += 1;
            }
        }
        if let Some(p) = self.probability_pct {
            if !p.is_finite() || !(0.0..=100.0).contains(&p) {
                self.probability_pct = p.is_finite().then(|| p.clamp(0.0, 100.0));
                fixed += 1;
            }
        }
        fixed
    }

    pub fn statement(&self, std: RiskStandard) -> String {
        let (e, c, cause) = (self.event.trim(), self.consequence.trim(), self.cause.trim());
        match std {
            RiskStandard::Dod if !e.is_empty() || !c.is_empty() => match self.kind {
                RiskKind::Issue => format!("{e}; consequence: {c}"),
                _ => format!("If {e}, then {c}."),
            },
            RiskStandard::Pmi if !e.is_empty() || !c.is_empty() => {
                let because = if cause.is_empty() { String::new() } else { format!("Because of {cause}, ") };
                let may = if self.kind == RiskKind::Issue { "has occurred" } else { "may occur" };
                format!("{because}{e} {may}, which would lead to {c}.")
            }
            _ => self.description.trim().to_string(),
        }
    }
}

/// Responses the standard offers for a kind.
pub fn responses(std: RiskStandard, kind: RiskKind) -> &'static [&'static str] {
    match (std, kind) {
        (RiskStandard::Dod, RiskKind::Threat) => &["Accept", "Avoid", "Transfer", "Control"],
        (RiskStandard::Dod, RiskKind::Opportunity) => &["Pursue now", "Defer", "Reevaluate", "Reject"],
        (RiskStandard::Dod, RiskKind::Issue) => &["Ignore", "Control", "Avoid", "Transfer"],
        (RiskStandard::Pmi, RiskKind::Opportunity) => &["Escalate", "Exploit", "Share", "Enhance", "Accept"],
        (RiskStandard::Pmi, _) => &["Escalate", "Avoid", "Transfer", "Mitigate", "Accept"],
        (RiskStandard::None, RiskKind::Opportunity) => &["Pursue", "Watch", "Reject"],
        (RiskStandard::None, _) => &["Reduce", "Avoid", "Transfer", "Accept"],
    }
}

/// One-line definitions of the standard responses.
pub fn response_help(r: &str) -> &'static str {
    match r {
        "Accept" => "Acknowledge it and prepare for the consequences; keep watching it",
        "Avoid" => "Remove the source: change the plan so it can't happen",
        "Transfer" => "Move responsibility to another party (contract, insurance, MOA)",
        "Control" => "Actively reduce likelihood or consequence to an acceptable level",
        "Mitigate" => "Reduce the probability or impact",
        "Escalate" => "Outside the project's authority: pass it to the program or organization",
        "Exploit" => "Make sure the opportunity happens",
        "Share" => "Give ownership to a party better able to capture it",
        "Enhance" => "Increase the probability or impact of the opportunity",
        "Pursue now" => "Fund and carry out a plan to realize it",
        "Defer" => "Pursue it later (future funding, maturing the idea)",
        "Reevaluate" => "Keep evaluating it as things change",
        "Reject" => "Set it aside deliberately",
        "Ignore" => "Accept the consequences without action",
        _ => "",
    }
}

impl RiskSettings {
    /// The level of a matrix cell (likelihood, consequence 1–5) under a
    /// standard, or this register's own matrix.
    pub fn cell_level(&self, std: RiskStandard, l: u8, c: u8) -> RiskLevel {
        if !(1..=5).contains(&l) || !(1..=5).contains(&c) {
            return RiskLevel::None;
        }
        let (li, ci) = (l as usize - 1, c as usize - 1);
        let v = match (self.matrix, std) {
            // custom colours are for the register's own standard
            (Some(m), s) if s == self.standard => m[li][ci],
            (_, RiskStandard::Dod) => DOD_MATRIX[li][ci],
            (_, RiskStandard::Pmi) => {
                let s = PMI_PROBABILITY[li] * PMI_IMPACT[ci];
                if s >= PMI_HIGH - 1e-9 {
                    2
                } else if s >= PMI_MODERATE - 1e-9 {
                    1
                } else {
                    0
                }
            }
            (_, RiskStandard::None) => match (l as u32) * (c as u32) {
                0..=4 => 0,
                5..=12 => 1,
                _ => 2,
            },
        };
        [RiskLevel::Low, RiskLevel::Moderate, RiskLevel::High][v.min(2) as usize]
    }
}

impl Project {
    pub fn risk_standard(&self, r: &Risk) -> RiskStandard {
        r.standard.unwrap_or(self.risk_settings.standard)
    }

    /// The level of a rating of `r` (current or target).
    pub fn risk_level_of(&self, r: &Risk, rating: &Rating) -> RiskLevel {
        let std = self.risk_standard(r);
        if !rating.is_rated(r.kind) {
            return RiskLevel::None;
        }
        let c = rating.worst();
        if r.kind == RiskKind::Issue {
            // no likelihood: the consequence alone (DoD's issue matrix)
            return match c {
                0 => RiskLevel::None,
                1 | 2 => RiskLevel::Low,
                3 => RiskLevel::Moderate,
                _ => RiskLevel::High,
            };
        }
        self.risk_settings.cell_level(std, rating.likelihood, c)
    }

    pub fn risk_level(&self, r: &Risk) -> RiskLevel {
        self.risk_level_of(r, &r.rating)
    }

    /// The number the register sorts on: P × I for PMI (0–0.72), L × C
    /// otherwise (1–25); issues count as certain.
    pub fn risk_score(&self, r: &Risk) -> f64 {
        let rating = &r.rating;
        if !rating.is_rated(r.kind) {
            return 0.0;
        }
        let c = rating.worst();
        let l = if r.kind == RiskKind::Issue { 5 } else { rating.likelihood.min(5) };
        match self.risk_standard(r) {
            RiskStandard::Pmi => {
                let p = if r.kind == RiskKind::Issue { 1.0 } else { PMI_PROBABILITY[l as usize - 1] };
                p * PMI_IMPACT[c as usize - 1]
            }
            _ => f64::from(l) * f64::from(c),
        }
    }

    /// The probability a risk stands for (0–1): typed, or the middle of its
    /// likelihood level.
    pub fn risk_probability(&self, r: &Risk) -> f64 {
        if r.kind == RiskKind::Issue {
            return 1.0;
        }
        if let Some(p) = r.probability_pct.filter(|p| p.is_finite()) {
            return (p / 100.0).clamp(0.0, 1.0);
        }
        match (self.risk_standard(r), r.rating.likelihood) {
            (_, 0) => 0.0,
            (RiskStandard::Pmi, l) => PMI_PROBABILITY[l as usize - 1],
            (_, l) => [0.10, 0.30, 0.50, 0.70, 0.90][l.min(5) as usize - 1],
        }
    }

    /// Expected monetary value: probability × cost impact (negative for
    /// threats, positive for opportunities).
    pub fn risk_emv(&self, r: &Risk) -> f64 {
        let sign = if r.kind == RiskKind::Opportunity { 1.0 } else { -1.0 };
        sign * self.risk_probability(r) * r.cost_impact
    }

    pub fn risk(&self, id: u32) -> Option<&Risk> {
        self.risks.iter().find(|r| r.id == id)
    }

    /// Add a risk; returns its ID.
    pub fn add_risk(&mut self, mut r: Risk) -> u32 {
        r.clamp();
        let used = self.risks.iter().map(|r| r.id).max().unwrap_or(0);
        let id = self.risk_settings.next_id.max(used + 1).max(1);
        self.risk_settings.next_id = id + 1;
        r.id = id;
        self.risks.push(r);
        id
    }

    pub fn delete_risk(&mut self, id: u32) -> Result<(), String> {
        let n = self.risks.len();
        self.risks.retain(|r| r.id != id);
        if self.risks.len() == n {
            return Err("No such risk.".into());
        }
        Ok(())
    }

    /// Replace a risk with an edited copy (same ID).
    pub fn update_risk(&mut self, mut r: Risk) -> Result<(), String> {
        r.clamp();
        let slot = self.risks.iter_mut().find(|x| x.id == r.id).ok_or("No such risk.")?;
        *slot = r;
        Ok(())
    }

    /// Open risks tied to a task.
    pub fn risks_of_task(&self, uid: u32) -> Vec<&Risk> {
        self.risks.iter().filter(|r| r.status.is_active() && r.links.contains(&RiskLink::Task(uid))).collect()
    }

    /// What a link shows as: "Task 4: Build", "Resource: Ann", "Release: 2.0".
    pub fn risk_link_label(&self, l: RiskLink) -> String {
        match l {
            RiskLink::Task(u) => match self.index_of_uid(u) {
                Some(i) => format!("Task {}: {}", i + 1, self.tasks[i].name),
                None => "Task (deleted)".into(),
            },
            RiskLink::Resource(u) => match self.resources.iter().find(|r| r.uid == u) {
                Some(r) => format!("Resource: {}", r.name),
                None => "Resource (deleted)".into(),
            },
            RiskLink::Release(id) => match self.board.releases.iter().find(|r| r.id == id) {
                Some(r) => format!("Release: {}", r.name),
                None => "Release (deleted)".into(),
            },
        }
    }

    /// "Project" when tied to nothing, else the links.
    pub fn risk_links_text(&self, r: &Risk) -> String {
        if r.links.is_empty() {
            return "Project".into();
        }
        r.links.iter().map(|&l| self.risk_link_label(l)).collect::<Vec<_>>().join("; ")
    }

    /// Files from elsewhere: links to things that are gone, levels out of
    /// range, unique IDs.
    pub fn repair_risks(&mut self) -> usize {
        let tasks: std::collections::HashSet<u32> = self.tasks.iter().map(|t| t.uid).collect();
        let res: std::collections::HashSet<u32> = self.resources.iter().map(|r| r.uid).collect();
        let rel: std::collections::HashSet<u32> = self.board.releases.iter().map(|r| r.id).collect();
        let mut fixed = 0;
        let mut seen = std::collections::HashSet::new();
        let mut next = self.risks.iter().map(|r| r.id).max().unwrap_or(0).max(self.risk_settings.next_id.saturating_sub(1));
        for r in self.risks.iter_mut() {
            if r.id == 0 || !seen.insert(r.id) {
                next += 1;
                r.id = next;
                seen.insert(next);
                fixed += 1;
            }
            let n = r.links.len();
            r.links.retain(|l| match l {
                RiskLink::Task(u) => tasks.contains(u),
                RiskLink::Resource(u) => res.contains(u),
                RiskLink::Release(u) => rel.contains(u),
            });
            fixed += n - r.links.len();
            fixed += r.clamp();
        }
        if let Some(m) = self.risk_settings.matrix.as_mut() {
            for v in m.iter_mut().flatten() {
                *v = (*v).min(2);
            }
        }
        if !self.risks.is_empty() {
            self.risk_settings.next_id = self.risk_settings.next_id.max(next + 1);
        }
        fixed
    }

    /// The register as CSV (one row per risk), for a spreadsheet or report.
    pub fn risks_csv(&self) -> String {
        let q = |s: &str| {
            if s.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        };
        let d = |d: Option<NaiveDate>| d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
        let mut out = String::from("ID,Type,Title,Standard,Category,Statement,Linked to,Owner,Status,Likelihood,Consequence,Level,Score,Target likelihood,Target consequence,Target level,Response,Response plan,Steps,Triggers,Contingency plan,Fallback plan,Secondary risks,Urgency,Cost impact,Schedule impact (days),EMV,Identified,Due,Closed\n");
        for r in &self.risks {
            let std = self.risk_standard(r);
            let steps = r
                .steps
                .iter()
                .map(|s| format!("{}{} {}", if s.done { "[x] " } else { "[ ] " }, d(s.date), s.activity))
                .collect::<Vec<_>>()
                .join("; ");
            let row = [
                r.key(),
                r.kind.label().into(),
                r.title.clone(),
                std.short().into(),
                r.category.clone(),
                r.statement(std),
                self.risk_links_text(r),
                r.owner.clone(),
                r.status.label().into(),
                if r.kind == RiskKind::Issue { "n/a".into() } else { r.rating.likelihood.to_string() },
                r.rating.worst().to_string(),
                self.risk_level(r).label(std).into(),
                crate::duration::fmt_num((self.risk_score(r) * 100.0).round() / 100.0),
                r.target.likelihood.to_string(),
                r.target.worst().to_string(),
                self.risk_level_of(r, &r.target).label(std).into(),
                r.response.clone(),
                r.response_plan.clone(),
                steps,
                r.triggers.clone(),
                r.contingency.clone(),
                r.fallback.clone(),
                r.secondary.clone(),
                r.urgency.clone(),
                format!("{:.2}", r.cost_impact),
                crate::duration::fmt_num(r.schedule_days),
                format!("{:.2}", self.risk_emv(r)),
                d(r.identified),
                d(r.due),
                d(r.closed),
            ];
            out.push_str(&row.iter().map(|s| q(s)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_follow_their_standards() {
        let s = RiskSettings::default();
        // DoD: consequence 1 is always low; 5/5 high; 1/5 moderate
        for l in 1..=5 {
            assert_eq!(s.cell_level(RiskStandard::Dod, l, 1), RiskLevel::Low);
        }
        assert_eq!(s.cell_level(RiskStandard::Dod, 5, 5), RiskLevel::High);
        assert_eq!(s.cell_level(RiskStandard::Dod, 1, 5), RiskLevel::Moderate);
        assert_eq!(s.cell_level(RiskStandard::Dod, 3, 3), RiskLevel::Moderate);
        // PMI: 0.9 × 0.20 = 0.18 is high; 0.1 × 0.80 = 0.08 moderate; 0.9 × 0.05 low
        assert_eq!(s.cell_level(RiskStandard::Pmi, 5, 3), RiskLevel::High);
        assert_eq!(s.cell_level(RiskStandard::Pmi, 1, 5), RiskLevel::Moderate);
        assert_eq!(s.cell_level(RiskStandard::Pmi, 5, 1), RiskLevel::Low);
        // none: L × I
        assert_eq!(s.cell_level(RiskStandard::None, 2, 2), RiskLevel::Low);
        assert_eq!(s.cell_level(RiskStandard::None, 3, 4), RiskLevel::Moderate);
        assert_eq!(s.cell_level(RiskStandard::None, 3, 5), RiskLevel::High);
        // a program's own matrix wins
        let own = RiskSettings { standard: RiskStandard::Dod, matrix: Some([[2; 5]; 5]), ..Default::default() };
        assert_eq!(own.cell_level(RiskStandard::Dod, 1, 1), RiskLevel::High);
        // …for the register's own standard only
        assert_eq!(own.cell_level(RiskStandard::Pmi, 1, 1), RiskLevel::Low);
        // the matrix only gets worse up and to the right
        for std in RiskStandard::ALL {
            for l in 1..=5u8 {
                for c in 1..=5u8 {
                    let v = s.cell_level(std, l, c);
                    if l < 5 {
                        assert!(s.cell_level(std, l + 1, c) >= v, "{std:?} {l} {c}");
                    }
                    if c < 5 {
                        assert!(s.cell_level(std, l, c + 1) >= v, "{std:?} {l} {c}");
                    }
                }
            }
        }
    }
}
