//! A date-only type that serializes as "YYYY-MM-DD" in both YAML and JSON.

use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A date-only type (no time or timezone) that serializes as "YYYY-MM-DD".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(pub NaiveDate);

const FORMAT: &str = "%Y-%m-%d";

impl Date {
    /// Create a Date from year, month, day.
    ///
    /// # Panics
    /// Panics if the date is invalid (e.g. month 13 or day 32).
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Date(
            NaiveDate::from_ymd_opt(year, month, day)
                .expect("invalid date components"),
        )
    }

    /// Returns today's date (local time).
    pub fn today() -> Self {
        Date(chrono::Local::now().date_naive())
    }

    /// Parse a "YYYY-MM-DD" string into a Date.
    pub fn parse(s: &str) -> Result<Self, chrono::ParseError> {
        NaiveDate::parse_from_str(s, FORMAT).map(Date)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(FORMAT))
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Date::parse(&s).map_err(|e| de::Error::custom(format!("invalid date {s:?}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_display() {
        let d = Date::new(2024, 3, 15);
        assert_eq!(d.to_string(), "2024-03-15");
    }

    #[test]
    fn test_parse_valid() {
        let d = Date::parse("2024-03-15").unwrap();
        assert_eq!(d, Date::new(2024, 3, 15));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(Date::parse("not-a-date").is_err());
        assert!(Date::parse("2024-13-01").is_err());
    }

    #[test]
    fn test_today_does_not_panic() {
        let _ = Date::today();
    }

    #[test]
    fn test_ordering() {
        let a = Date::new(2024, 1, 1);
        let b = Date::new(2024, 6, 15);
        assert!(a < b);
    }

    #[test]
    fn test_serde_json_roundtrip() {
        let d = Date::new(2024, 3, 15);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, "\"2024-03-15\"");
        let parsed: Date = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, d);
    }

    #[test]
    fn test_serde_yml_roundtrip() {
        let d = Date::new(2024, 3, 15);
        let yaml = serde_yml::to_string(&d).unwrap();
        // serde_yml quotes date-like strings (e.g. '2024-03-15')
        let parsed: Date = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(parsed, d);
    }
}
