//! Collection schemas: typed frontmatter checked at build time.
//!
//! ```json
//! "collections": {
//!   "posts": {
//!     "fields": { "title": "string", "date": "date", "tags": "string[]?" }
//!   }
//! }
//! ```
//!
//! Types are `string`, `number`, `boolean`, `date` (`YYYY-MM-DD`), `time`
//! (`HH:MM`), `datetime` (`YYYY-MM-DDTHH:MM`, with optional seconds and
//! offset), `url`, `object`, and the lists `string[]`, `number[]`, and
//! `object[]`. A trailing `?` makes a field optional. Fields not in the
//! schema fail the build unless `"strict": false`.
//!
//! Typed fields are what agents query: dates, times, and numbers compare in
//! order, so an agent can ask for events after a date or products under a
//! price without reading every entry.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value};

/// Keys Mira itself reads or adds, allowed in every collection.
const BUILT_IN: [&str; 14] = [
    "slug",
    "draft",
    "layout",
    "transition",
    "reading_time",
    "toc",
    "order",
    "robots",
    "canonical",
    "updated",
    "faq",
    "image",
    "image_alt",
    "author",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub strict: bool,
    /// Entries fetched from a CMS or JSON API at build time, in addition to
    /// any files in `content/<name>/`.
    pub source: Option<crate::sources::Source>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    String,
    Number,
    Boolean,
    Date,
    Url,
    Time,
    DateTime,
    Object,
    StringList,
    NumberList,
    ObjectList,
}

fn parse_type(spec: &str) -> Option<(Kind, bool)> {
    let (name, optional) = match spec.strip_suffix('?') {
        Some(name) => (name, true),
        None => (spec, false),
    };
    let kind = match name {
        "string" => Kind::String,
        "number" => Kind::Number,
        "boolean" => Kind::Boolean,
        "date" => Kind::Date,
        "url" => Kind::Url,
        "time" => Kind::Time,
        "datetime" => Kind::DateTime,
        "object" => Kind::Object,
        "string[]" => Kind::StringList,
        "number[]" => Kind::NumberList,
        "object[]" => Kind::ObjectList,
        _ => return None,
    };
    Some((kind, optional))
}

impl Schema {
    pub fn check_types(&self, collection: &str) -> Result<()> {
        if let Some(source) = &self.source {
            source.check(collection)?;
        }
        for (field, spec) in &self.fields {
            if parse_type(spec).is_none() {
                bail!(
                    "mira.config.json: collections.{collection}.fields.{field} has unknown type {spec:?}\nhint: use string, number, boolean, date, time, datetime, url, object, string[], number[], or object[], with ? for optional"
                );
            }
        }
        Ok(())
    }

    /// Validates `data`, reporting the frontmatter line of the first problem.
    pub fn validate(&self, data: &Map<String, Value>, source: &str, path: &str) -> Result<()> {
        let line_of = |key: &str| {
            source.lines().position(|l| l.strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with(':'))).map_or(1, |i| i + 1)
        };
        for (field, spec) in &self.fields {
            let (kind, optional) = parse_type(spec).expect("checked when config loads");
            match data.get(field) {
                None | Some(Value::Null) if optional => {}
                None | Some(Value::Null) => {
                    bail!("{path}:1: missing required field `{field}` ({spec})\nhint: add `{field}:` to the frontmatter");
                }
                Some(value) => {
                    if let Some(problem) = mismatch(kind, value) {
                        bail!(
                            "{path}:{}: `{field}` {problem}\nhint: the schema in mira.config.json declares {field}: {spec}",
                            line_of(field)
                        );
                    }
                }
            }
        }
        // A schema that only names a source takes every field as it comes.
        if self.strict && !self.fields.is_empty() {
            for key in data.keys() {
                if !self.fields.contains_key(key) && !BUILT_IN.contains(&key.as_str()) {
                    let known: Vec<&str> = self.fields.keys().map(String::as_str).collect();
                    bail!(
                        "{path}:{}: unknown field `{key}`\nhint: add it to the schema, or use one of: {}",
                        line_of(key),
                        known.join(", ")
                    );
                }
            }
        }
        Ok(())
    }
}

/// Whether `spec` is a known type, such as `number` or `date?`.
pub fn is_type(spec: &str) -> bool {
    parse_type(spec).is_some()
}

/// Whether a field of type `spec` may be left out.
pub fn is_optional(spec: &str) -> bool {
    spec.ends_with('?')
}

/// Checks one value against a type such as `number` or `time`, returning
/// what is wrong, as in `must be a number, got "4"`.
pub fn check(spec: &str, value: &Value) -> Option<String> {
    let (kind, _) = parse_type(spec)?;
    mismatch(kind, value)
}

/// The JSON Schema for a type, for tools that describe their input.
pub fn json_schema(spec: &str) -> Value {
    let Some((kind, _)) = parse_type(spec) else { return serde_json::json!({}) };
    let string = |format: &str| serde_json::json!({ "type": "string", "format": format });
    match kind {
        Kind::String => serde_json::json!({ "type": "string" }),
        Kind::Number => serde_json::json!({ "type": "number" }),
        Kind::Boolean => serde_json::json!({ "type": "boolean" }),
        Kind::Date => string("date"),
        Kind::Time => serde_json::json!({ "type": "string", "pattern": "^[0-2][0-9]:[0-5][0-9](:[0-5][0-9])?$" }),
        Kind::DateTime => string("date-time"),
        Kind::Url => string("uri"),
        Kind::Object => serde_json::json!({ "type": "object" }),
        Kind::StringList => serde_json::json!({ "type": "array", "items": { "type": "string" } }),
        Kind::NumberList => serde_json::json!({ "type": "array", "items": { "type": "number" } }),
        Kind::ObjectList => serde_json::json!({ "type": "array", "items": { "type": "object" } }),
    }
}

fn mismatch(kind: Kind, value: &Value) -> Option<String> {
    let ok = match kind {
        Kind::String => value.is_string(),
        Kind::Number => value.is_number(),
        Kind::Boolean => value.is_boolean(),
        Kind::Date => value.as_str().is_some_and(is_date),
        Kind::Url => value.as_str().is_some_and(|s| s.starts_with('/') || s.starts_with("https://") || s.starts_with("http://")),
        Kind::Time => value.as_str().is_some_and(is_time),
        Kind::DateTime => value.as_str().is_some_and(is_datetime),
        Kind::Object => value.is_object(),
        Kind::StringList => value.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
        Kind::NumberList => value.as_array().is_some_and(|a| a.iter().all(Value::is_number)),
        Kind::ObjectList => value.as_array().is_some_and(|a| a.iter().all(Value::is_object)),
    };
    if ok {
        return None;
    }
    let expected = match kind {
        Kind::String => "a string",
        Kind::Number => "a number",
        Kind::Boolean => "true or false",
        Kind::Date => "a date like 2026-10-07",
        Kind::Url => "a URL starting with / or https://",
        Kind::Time => "a time like 18:30",
        Kind::DateTime => "a date and time like 2026-10-07T18:30",
        Kind::Object => "a mapping of keys to values",
        Kind::StringList => "a list of strings",
        Kind::NumberList => "a list of numbers",
        Kind::ObjectList => "a list of mappings",
    };
    Some(format!("must be {expected}, got {value}"))
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && (1..=12).contains(&s[5..7].parse::<u8>().unwrap_or(0))
        && (1..=31).contains(&s[8..10].parse::<u8>().unwrap_or(0))
}

/// `18:30` or `18:30:15`, on a 24 hour clock.
fn is_time(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_digit()))
        && parts[0] < "24"
        && parts[1..].iter().all(|p| *p < "60")
}

/// `2026-10-07T18:30`, with optional seconds, fraction, and `Z` or `+05:30`.
fn is_datetime(s: &str) -> bool {
    let Some((date, rest)) = s.split_once(['T', ' ']) else { return false };
    let clock = rest.trim_end_matches('Z');
    let clock = match clock.rfind(['+', '-']) {
        Some(i) if is_time(&clock[i + 1..]) => &clock[..i],
        _ => clock,
    };
    let clock = clock.split('.').next().unwrap_or("");
    is_date(date) && is_time(clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Schema {
        serde_json::from_value(json!({"fields": {"title": "string", "date": "date", "tags": "string[]?"}})).unwrap()
    }

    #[test]
    fn accepts_valid_data() {
        let data = json!({"title": "Hi", "date": "2026-10-07", "draft": true});
        schema().validate(data.as_object().unwrap(), "", "a.md").unwrap();
    }

    #[test]
    fn reports_type_errors_with_line() {
        let src = "---\ntitle: Hi\ndate: soon\n---\n";
        let data = json!({"title": "Hi", "date": "soon"});
        let err = schema().validate(data.as_object().unwrap(), src, "a.md").unwrap_err().to_string();
        assert!(err.starts_with("a.md:3: `date` must be a date"), "{err}");
    }

    #[test]
    fn checks_times_and_structures() {
        let schema: Schema = serde_json::from_value(json!({"fields": {
            "opens": "time", "starts": "datetime", "hours": "object", "prices": "number[]", "talks": "object[]"
        }}))
        .unwrap();
        let good = json!({"opens": "09:00", "starts": "2026-10-07T18:30:00+05:30", "hours": {"mon": "9-5"}, "prices": [4, 4.5], "talks": [{"at": "10:00"}]});
        schema.validate(good.as_object().unwrap(), "", "a.md").unwrap();
        for (field, bad) in [("opens", json!("25:00")), ("starts", json!("2026-10-07")), ("prices", json!(["4"])), ("talks", json!(["x"]))]
        {
            let mut data = good.clone();
            data[field] = bad;
            assert!(schema.validate(data.as_object().unwrap(), "", "a.md").is_err(), "{field}");
        }
        assert!(is_datetime("2026-10-07T18:30Z") && is_datetime("2026-10-07 18:30:05.5"));
    }

    #[test]
    fn rejects_unknown_and_missing_fields() {
        let data = json!({"title": "Hi", "date": "2026-10-07", "auther": "x"});
        let err = schema().validate(data.as_object().unwrap(), "", "a.md").unwrap_err().to_string();
        assert!(err.contains("unknown field `auther`"), "{err}");
        let data = json!({"date": "2026-10-07"});
        let err = schema().validate(data.as_object().unwrap(), "", "a.md").unwrap_err().to_string();
        assert!(err.contains("missing required field `title`"), "{err}");
    }
}
