//! Saved view definitions: custom filters (View ▸ Filter ▸ New Filter…),
//! kept in the project file as Project keeps them in the .mpp.
//!
//! A filter is a list of conditions on task fields. Fields are named by
//! the sheet column they appear in (`ColumnId`'s name); the app evaluates
//! them, since it owns the column definitions.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Views {
    pub filters: Vec<CustomFilter>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomFilter {
    pub name: String,
    pub conditions: Vec<Condition>,
    /// Also show the summary tasks of matching tasks (Project's "Show
    /// related summary rows").
    pub related_summaries: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Condition {
    /// How this condition joins the ones before it. "And" binds tighter
    /// than "Or": `a And b Or c` is `(a And b) Or c`.
    pub or: bool,
    /// Column name (`ColumnId`'s `Debug` form, e.g. "Duration").
    pub field: String,
    pub test: Test,
    pub value: String,
    /// Upper bound for "is within" / "is not within".
    pub value2: String,
}

impl Default for Condition {
    fn default() -> Self {
        Condition { or: false, field: "Name".into(), test: Test::Contains, value: String::new(), value2: String::new() }
    }
}

/// Project's filter tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Test {
    #[default]
    Equals,
    NotEquals,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Within,
    NotWithin,
    Contains,
    NotContains,
}

impl Test {
    pub const ALL: [Test; 10] = [
        Test::Equals,
        Test::NotEquals,
        Test::Greater,
        Test::GreaterOrEqual,
        Test::Less,
        Test::LessOrEqual,
        Test::Within,
        Test::NotWithin,
        Test::Contains,
        Test::NotContains,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Test::Equals => "equals",
            Test::NotEquals => "does not equal",
            Test::Greater => "is greater than",
            Test::GreaterOrEqual => "is greater than or equal to",
            Test::Less => "is less than",
            Test::LessOrEqual => "is less than or equal to",
            Test::Within => "is within",
            Test::NotWithin => "is not within",
            Test::Contains => "contains",
            Test::NotContains => "does not contain",
        }
    }
    pub fn two_values(self) -> bool {
        matches!(self, Test::Within | Test::NotWithin)
    }
}

/// Evaluate a condition list with "And" binding tighter than "Or".
pub fn combine(conds: &[Condition], mut eval: impl FnMut(&Condition) -> bool) -> bool {
    if conds.is_empty() {
        return true;
    }
    let mut any = false;
    let mut group = true;
    for (k, c) in conds.iter().enumerate() {
        if k > 0 && c.or {
            any |= group;
            group = true;
        }
        // skip work once this And-group has failed
        if group {
            group = eval(c);
        }
    }
    any || group
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(or: bool, v: &str) -> Condition {
        Condition { or, value: v.into(), ..Default::default() }
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let truth = |c: &Condition| c.value == "t";
        // f And t Or t = (f And t) Or t = true
        assert!(combine(&[c(false, "f"), c(false, "t"), c(true, "t")], truth));
        // t Or f And f = t Or (f And f) = true
        assert!(combine(&[c(false, "t"), c(true, "f"), c(false, "f")], truth));
        // f Or t And f = f Or (t And f) = false
        assert!(!combine(&[c(false, "f"), c(true, "t"), c(false, "f")], truth));
        assert!(combine(&[], truth));
    }
}
