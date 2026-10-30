//! Durations, lags, and the text formats Microsoft Project uses for them.
//!
//! Working durations are stored as working minutes and converted using the
//! project's "hours per day / hours per week / days per month" options, just
//! as Project does (so "1 wk" = 40h by default regardless of the calendar).
//! Elapsed durations ("ed", "ew"…) are stored as wall-clock minutes.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DurUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

/// Conversion factors (Project → File → Options → Schedule).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DurationSettings {
    pub minutes_per_day: i64,
    pub minutes_per_week: i64,
    pub days_per_month: i64,
}

impl Default for DurationSettings {
    fn default() -> Self {
        DurationSettings {
            minutes_per_day: 480,
            minutes_per_week: 2400,
            days_per_month: 20,
        }
    }
}

impl DurationSettings {
    pub fn unit_minutes(&self, unit: DurUnit, elapsed: bool) -> f64 {
        if elapsed {
            match unit {
                DurUnit::Minute => 1.0,
                DurUnit::Hour => 60.0,
                DurUnit::Day => 1440.0,
                DurUnit::Week => 10080.0,
                DurUnit::Month => 43200.0,
            }
        } else {
            match unit {
                DurUnit::Minute => 1.0,
                DurUnit::Hour => 60.0,
                DurUnit::Day => self.minutes_per_day as f64,
                DurUnit::Week => self.minutes_per_week as f64,
                DurUnit::Month => (self.minutes_per_day * self.days_per_month) as f64,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Duration {
    /// Working minutes, or wall-clock minutes if `elapsed`.
    pub minutes: i64,
    /// Unit used for display (what the user typed).
    pub unit: DurUnit,
    pub elapsed: bool,
    /// Project's "?" estimated flag.
    pub estimated: bool,
}

impl Default for Duration {
    fn default() -> Self {
        Duration::days(1.0, &DurationSettings::default())
    }
}

impl Duration {
    pub fn days(d: f64, s: &DurationSettings) -> Self {
        Duration {
            minutes: (d * s.minutes_per_day as f64).round() as i64,
            unit: DurUnit::Day,
            elapsed: false,
            estimated: false,
        }
    }
    pub fn zero() -> Self {
        Duration {
            minutes: 0,
            unit: DurUnit::Day,
            elapsed: false,
            estimated: false,
        }
    }
    pub fn from_minutes(minutes: i64, unit: DurUnit) -> Self {
        Duration {
            minutes,
            unit,
            elapsed: false,
            estimated: false,
        }
    }
    pub fn is_zero(&self) -> bool {
        self.minutes == 0
    }

    pub fn value_in_unit(&self, s: &DurationSettings) -> f64 {
        self.minutes as f64 / s.unit_minutes(self.unit, self.elapsed)
    }

    /// Format the way Project's Duration column does: "5 days", "1 wk",
    /// "3 hrs", "2 edays", "1 day?".
    pub fn format(&self, s: &DurationSettings) -> String {
        let v = self.value_in_unit(s);
        let plural = (v - 1.0).abs() > 1e-9;
        let e = if self.elapsed { "e" } else { "" };
        let word = match (self.unit, plural) {
            (DurUnit::Minute, false) => "min",
            (DurUnit::Minute, true) => "mins",
            (DurUnit::Hour, false) => "hr",
            (DurUnit::Hour, true) => "hrs",
            (DurUnit::Day, false) => "day",
            (DurUnit::Day, true) => "days",
            (DurUnit::Week, false) => "wk",
            (DurUnit::Week, true) => "wks",
            (DurUnit::Month, false) => "mon",
            (DurUnit::Month, true) => "mons",
        };
        format!(
            "{} {}{}{}",
            fmt_num(v),
            e,
            word,
            if self.estimated { "?" } else { "" }
        )
    }

    /// Parse a duration as typed in Project. A bare number uses `default_unit`
    /// (days unless changed in options).
    pub fn parse(text: &str, s: &DurationSettings, default_unit: DurUnit) -> Result<Self, String> {
        let t = text.trim();
        if t.is_empty() {
            return Err("Duration is empty".into());
        }
        let (t, estimated) = match t.strip_suffix('?') {
            Some(rest) => (rest.trim_end(), true),
            None => (t, false),
        };
        let num_end = t
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == ','))
            .map(|(i, _)| i)
            .unwrap_or(t.len());
        let num_str = t[..num_end].replace(',', "");
        if num_str.is_empty() {
            return Err(format!("\"{text}\" is not a valid duration"));
        }
        let value: f64 = num_str
            .parse()
            .map_err(|_| format!("\"{text}\" is not a valid duration"))?;
        if value < 0.0 {
            return Err("Duration can't be negative".into());
        }
        let unit_str = t[num_end..].trim().to_ascii_lowercase();
        let (unit, elapsed) = parse_unit(&unit_str, default_unit)
            .ok_or_else(|| format!("\"{text}\" is not a valid duration unit"))?;
        let minutes = (value * s.unit_minutes(unit, elapsed)).round() as i64;
        Ok(Duration {
            minutes,
            unit,
            elapsed,
            estimated,
        })
    }
}

fn parse_unit(u: &str, default_unit: DurUnit) -> Option<(DurUnit, bool)> {
    if u.is_empty() {
        return Some((default_unit, false));
    }
    let (elapsed, base) = match u.strip_prefix('e') {
        // "e" alone is not a unit; "ed", "eday", "ew" … are elapsed
        Some(rest) if !rest.is_empty() => (true, rest),
        _ => (false, u),
    };
    let unit = match base {
        "m" | "min" | "mins" | "minute" | "minutes" => DurUnit::Minute,
        "h" | "hr" | "hrs" | "hour" | "hours" => DurUnit::Hour,
        "d" | "dy" | "day" | "days" => DurUnit::Day,
        "w" | "wk" | "wks" | "week" | "weeks" => DurUnit::Week,
        "mo" | "mon" | "mons" | "month" | "months" => DurUnit::Month,
        _ => return None,
    };
    Some((unit, elapsed))
}

pub fn fmt_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        let s = format!("{:.2}", r);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Lag/lead on a dependency: either a duration (possibly negative = lead)
/// or a percentage of the predecessor's duration.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Lag {
    Time {
        minutes: i64,
        unit: DurUnit,
        elapsed: bool,
    },
    Percent(f64),
}

impl Default for Lag {
    fn default() -> Self {
        Lag::Time {
            minutes: 0,
            unit: DurUnit::Day,
            elapsed: false,
        }
    }
}

impl Lag {
    pub fn is_zero(&self) -> bool {
        match self {
            Lag::Time { minutes, .. } => *minutes == 0,
            Lag::Percent(p) => p.abs() < 1e-9,
        }
    }

    /// Parse "+2d", "-1 day", "50%", "-25%", "3ed".
    pub fn parse(text: &str, s: &DurationSettings) -> Result<Self, String> {
        let t = text.trim();
        if t.is_empty() {
            return Ok(Lag::default());
        }
        let (neg, body) = if let Some(r) = t.strip_prefix('-') {
            (true, r.trim())
        } else if let Some(r) = t.strip_prefix('+') {
            (false, r.trim())
        } else {
            (false, t)
        };
        if let Some(p) = body.strip_suffix('%') {
            let v: f64 = p
                .trim()
                .parse()
                .map_err(|_| format!("\"{text}\" is not a valid lag"))?;
            return Ok(Lag::Percent(if neg { -v } else { v }));
        }
        let d = Duration::parse(body, s, DurUnit::Day)?;
        Ok(Lag::Time {
            minutes: if neg { -d.minutes } else { d.minutes },
            unit: d.unit,
            elapsed: d.elapsed,
        })
    }

    /// "+2 days", "-1 day", "+50%" — empty for zero lag.
    pub fn format(&self, s: &DurationSettings) -> String {
        match self {
            Lag::Percent(p) => {
                if p.abs() < 1e-9 {
                    String::new()
                } else {
                    format!("{}{}%", if *p < 0.0 { "-" } else { "+" }, fmt_num(p.abs()))
                }
            }
            Lag::Time {
                minutes,
                unit,
                elapsed,
            } => {
                if *minutes == 0 {
                    return String::new();
                }
                let d = Duration {
                    minutes: minutes.abs(),
                    unit: *unit,
                    elapsed: *elapsed,
                    estimated: false,
                };
                format!("{}{}", if *minutes < 0 { "-" } else { "+" }, d.format(s))
            }
        }
    }
}

impl fmt::Display for DurUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DurUnit::Minute => "Minutes",
            DurUnit::Hour => "Hours",
            DurUnit::Day => "Days",
            DurUnit::Week => "Weeks",
            DurUnit::Month => "Months",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> DurationSettings {
        DurationSettings::default()
    }

    #[test]
    fn parse_basic_units() {
        let st = s();
        let p = |t: &str| Duration::parse(t, &st, DurUnit::Day).unwrap();
        assert_eq!(p("5").minutes, 2400);
        assert_eq!(p("5d").minutes, 2400);
        assert_eq!(p("5 days").minutes, 2400);
        assert_eq!(p("2w").minutes, 4800);
        assert_eq!(p("2 wks").minutes, 4800);
        assert_eq!(p("3h").minutes, 180);
        assert_eq!(p("30m").minutes, 30);
        assert_eq!(p("1mo").minutes, 9600);
        assert_eq!(p("0.5d").minutes, 240);
        assert_eq!(p("1.5 days").minutes, 720);
        assert_eq!(p("0").minutes, 0);
    }

    #[test]
    fn parse_elapsed_and_estimated() {
        let st = s();
        let d = Duration::parse("3ed", &st, DurUnit::Day).unwrap();
        assert!(d.elapsed);
        assert_eq!(d.minutes, 3 * 1440);
        let d = Duration::parse("1 day?", &st, DurUnit::Day).unwrap();
        assert!(d.estimated);
        assert_eq!(d.minutes, 480);
        let d = Duration::parse("2 ewks", &st, DurUnit::Day).unwrap();
        assert!(d.elapsed && d.unit == DurUnit::Week);
    }

    #[test]
    fn parse_rejects_garbage() {
        let st = s();
        assert!(Duration::parse("abc", &st, DurUnit::Day).is_err());
        assert!(Duration::parse("5 parsecs", &st, DurUnit::Day).is_err());
        assert!(Duration::parse("", &st, DurUnit::Day).is_err());
    }

    #[test]
    fn format_like_project() {
        let st = s();
        let f = |t: &str| Duration::parse(t, &st, DurUnit::Day).unwrap().format(&st);
        assert_eq!(f("5d"), "5 days");
        assert_eq!(f("1d"), "1 day");
        assert_eq!(f("0d"), "0 days");
        assert_eq!(f("1w"), "1 wk");
        assert_eq!(f("3h"), "3 hrs");
        assert_eq!(f("0.5d"), "0.5 days");
        assert_eq!(f("2ed"), "2 edays");
        assert_eq!(f("1d?"), "1 day?");
        assert_eq!(f("2mo"), "2 mons");
    }

    #[test]
    fn lag_parse_format() {
        let st = s();
        let l = Lag::parse("+2d", &st).unwrap();
        assert_eq!(l.format(&st), "+2 days");
        let l = Lag::parse("-1 day", &st).unwrap();
        assert_eq!(l, Lag::Time { minutes: -480, unit: DurUnit::Day, elapsed: false });
        assert_eq!(l.format(&st), "-1 day");
        assert_eq!(Lag::parse("50%", &st).unwrap(), Lag::Percent(50.0));
        assert_eq!(Lag::parse("-25%", &st).unwrap().format(&st), "-25%");
        assert!(Lag::parse("", &st).unwrap().is_zero());
    }
}
