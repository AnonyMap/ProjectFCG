//! Custom fields (Project ▸ Custom Fields): Text1–10, Number1–10,
//! Flag1–10, Date1–10 and Cost1–10 on every task, each renamable. They
//! show as columns, can be filtered and grouped on, and travel through
//! Project XML as extended attributes with Project's own field IDs.

use crate::model::{parse_date, Project};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

pub const PER_KIND: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FieldKind {
    Text,
    Number,
    Flag,
    Date,
    Cost,
}

impl FieldKind {
    pub const ALL: [FieldKind; 5] = [FieldKind::Text, FieldKind::Number, FieldKind::Flag, FieldKind::Date, FieldKind::Cost];
    pub fn label(self) -> &'static str {
        match self {
            FieldKind::Text => "Text",
            FieldKind::Number => "Number",
            FieldKind::Flag => "Flag",
            FieldKind::Date => "Date",
            FieldKind::Cost => "Cost",
        }
    }
    /// Microsoft Project's task field indices (MPP / PjField − 0x0B400000).
    fn indices(self) -> [u32; 10] {
        match self {
            FieldKind::Text => [51, 54, 57, 60, 63, 66, 67, 68, 69, 70],
            FieldKind::Flag => [72, 73, 74, 75, 76, 77, 78, 79, 80, 81],
            FieldKind::Number => [87, 88, 89, 90, 91, 302, 303, 304, 305, 306],
            FieldKind::Cost => [106, 107, 108, 258, 259, 260, 261, 262, 263, 264],
            FieldKind::Date => [265, 266, 267, 268, 269, 270, 271, 272, 273, 274],
        }
    }
}

/// One custom field, e.g. Text3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomField {
    pub kind: FieldKind,
    /// 1–10.
    pub n: u8,
}

const TASK_FIELD_BASE: u32 = 0x0B40_0000;

impl CustomField {
    pub fn new(kind: FieldKind, n: u8) -> Self {
        CustomField { kind, n: n.clamp(1, PER_KIND) }
    }
    pub fn all() -> Vec<CustomField> {
        FieldKind::ALL.iter().flat_map(|&k| (1..=PER_KIND).map(move |n| CustomField { kind: k, n })).collect()
    }
    /// "Text3".
    pub fn name(self) -> String {
        format!("{}{}", self.kind.label(), self.n)
    }
    pub fn parse(s: &str) -> Option<CustomField> {
        let s = s.trim();
        FieldKind::ALL.iter().find_map(|&k| {
            let rest = s.strip_prefix(k.label())?;
            let n: u8 = rest.parse().ok()?;
            (1..=PER_KIND).contains(&n).then_some(CustomField { kind: k, n })
        })
    }
    /// Project's FieldID for it in MSPDI (pjTaskText1 = 188743731, …).
    pub fn field_id(self) -> u32 {
        TASK_FIELD_BASE + self.kind.indices()[(self.n - 1) as usize]
    }
    pub fn from_field_id(id: u32) -> Option<CustomField> {
        let idx = id.checked_sub(TASK_FIELD_BASE)?;
        FieldKind::ALL.iter().find_map(|&k| k.indices().iter().position(|&x| x == idx).map(|p| CustomField { kind: k, n: p as u8 + 1 }))
    }
}

/// A task's value for a custom field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CustomValue {
    Text(String),
    Number(f64),
    Flag(bool),
    Date(NaiveDateTime),
    Cost(f64),
}

impl CustomValue {
    fn kind(&self) -> FieldKind {
        match self {
            CustomValue::Text(_) => FieldKind::Text,
            CustomValue::Number(_) => FieldKind::Number,
            CustomValue::Flag(_) => FieldKind::Flag,
            CustomValue::Date(_) => FieldKind::Date,
            CustomValue::Cost(_) => FieldKind::Cost,
        }
    }
    fn finite(&self) -> bool {
        match self {
            CustomValue::Number(v) | CustomValue::Cost(v) => v.is_finite(),
            _ => true,
        }
    }
}

impl Project {
    pub fn custom_value(&self, row: usize, f: CustomField) -> Option<&CustomValue> {
        self.tasks.get(row)?.custom.get(&f.name())
    }

    /// The field's name as shown: its alias, or "Text1".
    pub fn custom_title(&self, f: CustomField) -> String {
        match self.custom_aliases.get(&f.name()).map(|a| a.trim()).filter(|a| !a.is_empty()) {
            Some(a) => a.to_string(),
            None => f.name(),
        }
    }

    /// Text shown in the cell. Flags show Yes/No; empty numbers and costs
    /// show 0 as Project does, empty dates NA.
    pub fn custom_text(&self, row: usize, f: CustomField) -> String {
        let df = self.format.layout.date_format;
        match (self.custom_value(row, f), f.kind) {
            (Some(CustomValue::Text(s)), _) => s.clone(),
            (Some(CustomValue::Number(v)), _) => crate::duration::fmt_num(*v),
            (Some(CustomValue::Cost(v)), _) => crate::resources::fmt_money(*v, &self.settings.currency_symbol),
            (Some(CustomValue::Flag(b)), _) => if *b { "Yes" } else { "No" }.into(),
            (Some(CustomValue::Date(d)), _) => df.format(*d),
            (None, FieldKind::Text) => String::new(),
            (None, FieldKind::Number) => "0".into(),
            (None, FieldKind::Cost) => crate::resources::fmt_money(0.0, &self.settings.currency_symbol),
            (None, FieldKind::Flag) => "No".into(),
            (None, FieldKind::Date) => "NA".into(),
        }
    }

    /// Type a value (empty, or the field's default, clears it).
    pub fn set_custom(&mut self, row: usize, f: CustomField, text: &str) -> Result<(), String> {
        let t = text.trim();
        if row >= self.tasks.len() {
            return Err("No such task.".into());
        }
        let name = self.custom_title(f);
        let v = match f.kind {
            FieldKind::Text => (!t.is_empty()).then(|| CustomValue::Text(text.to_string())),
            FieldKind::Number => {
                if t.is_empty() {
                    None
                } else {
                    let v = crate::duration::parse_number(&t.replace(',', "")).ok_or_else(|| format!("{name}: \"{t}\" isn't a number."))?;
                    (v != 0.0).then_some(CustomValue::Number(v))
                }
            }
            FieldKind::Cost => {
                if t.is_empty() {
                    None
                } else {
                    let v = crate::resources::parse_money(t, &self.settings.currency_symbol).map_err(|_| format!("{name}: \"{t}\" isn't an amount."))?;
                    (v != 0.0).then_some(CustomValue::Cost(v))
                }
            }
            FieldKind::Flag => match t.to_ascii_lowercase().as_str() {
                "" | "no" | "n" | "false" | "0" => None,
                "yes" | "y" | "true" | "1" => Some(CustomValue::Flag(true)),
                _ => return Err(format!("{name}: type Yes or No.")),
            },
            FieldKind::Date => {
                if t.is_empty() || t.eq_ignore_ascii_case("na") {
                    None
                } else {
                    Some(CustomValue::Date(parse_date(t, self.default_start_time()).map_err(|_| format!("{name}: \"{t}\" isn't a date."))?))
                }
            }
        };
        let map = &mut self.tasks[row].custom;
        match v {
            Some(v) => {
                map.insert(f.name(), v);
            }
            None => {
                map.remove(&f.name());
            }
        }
        Ok(())
    }

    /// Rename a field (empty: back to its own name).
    pub fn set_custom_alias(&mut self, f: CustomField, alias: &str) -> Result<(), String> {
        let a = alias.trim();
        if a.is_empty() {
            self.custom_aliases.remove(&f.name());
            return Ok(());
        }
        if self.custom_aliases.iter().any(|(k, v)| *k != f.name() && v.trim().eq_ignore_ascii_case(a)) {
            return Err(format!("Another custom field is already called \"{a}\"."));
        }
        self.custom_aliases.insert(f.name(), a.to_string());
        Ok(())
    }

    /// Fields that hold a value somewhere, or have an alias (what's worth
    /// writing to Project XML).
    pub fn custom_fields_in_use(&self) -> Vec<CustomField> {
        let mut v: Vec<CustomField> = CustomField::all()
            .into_iter()
            .filter(|f| self.custom_aliases.contains_key(&f.name()) || self.tasks.iter().any(|t| t.custom.contains_key(&f.name())))
            .collect();
        v.sort();
        v
    }

    /// Files from elsewhere: drop values for unknown fields, of the wrong
    /// kind, or not finite; and aliases of unknown fields.
    pub fn repair_custom_fields(&mut self) -> usize {
        let mut fixed = 0;
        for t in self.tasks.iter_mut() {
            let n = t.custom.len();
            t.custom.retain(|k, v| CustomField::parse(k).is_some_and(|f| f.kind == v.kind() && f.name() == *k) && v.finite());
            fixed += n - t.custom.len();
        }
        let n = self.custom_aliases.len();
        self.custom_aliases.retain(|k, _| CustomField::parse(k).is_some_and(|f| f.name() == *k));
        fixed + n - self.custom_aliases.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_project_field_ids() {
        let t1 = CustomField::new(FieldKind::Text, 1);
        assert_eq!(t1.name(), "Text1");
        assert_eq!(t1.field_id(), 188_743_731);
        assert_eq!(CustomField::new(FieldKind::Text, 10).field_id(), 188_743_750);
        assert_eq!(CustomField::new(FieldKind::Cost, 1).field_id(), 188_743_786);
        assert_eq!(CustomField::new(FieldKind::Cost, 4).field_id(), 188_743_938);
        assert_eq!(CustomField::new(FieldKind::Date, 10).field_id(), 188_743_954);
        for f in CustomField::all() {
            assert_eq!(CustomField::parse(&f.name()), Some(f));
            assert_eq!(CustomField::from_field_id(f.field_id()), Some(f));
        }
        assert_eq!(CustomField::all().len(), 50);
        assert_eq!(CustomField::parse("Text11"), None);
        assert_eq!(CustomField::parse("Texts1"), None);
    }
}
