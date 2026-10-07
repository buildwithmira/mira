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
//! Types are `string`, `number`, `boolean`, `date` (`YYYY-MM-DD`), `url`,
//! and `string[]`. A trailing `?` makes a field optional. Fields not in the
//! schema fail the build unless `"strict": false`.

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
    pub fields: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub strict: bool,
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
    StringList,
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
        "string[]" => Kind::StringList,
        _ => return None,
    };
    Some((kind, optional))
}

impl Schema {
    pub fn check_types(&self, collection: &str) -> Result<()> {
        for (field, spec) in &self.fields {
            if parse_type(spec).is_none() {
                bail!(
                    "mira.config.json: collections.{collection}.fields.{field} has unknown type {spec:?}\nhint: use string, number, boolean, date, url, or string[], with ? for optional"
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
        if self.strict {
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

fn mismatch(kind: Kind, value: &Value) -> Option<String> {
    let ok = match kind {
        Kind::String => value.is_string(),
        Kind::Number => value.is_number(),
        Kind::Boolean => value.is_boolean(),
        Kind::Date => value.as_str().is_some_and(is_date),
        Kind::Url => value.as_str().is_some_and(|s| s.starts_with('/') || s.starts_with("https://") || s.starts_with("http://")),
        Kind::StringList => value.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
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
        Kind::StringList => "a list of strings",
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
    fn rejects_unknown_and_missing_fields() {
        let data = json!({"title": "Hi", "date": "2026-10-07", "auther": "x"});
        let err = schema().validate(data.as_object().unwrap(), "", "a.md").unwrap_err().to_string();
        assert!(err.contains("unknown field `auther`"), "{err}");
        let data = json!({"date": "2026-10-07"});
        let err = schema().validate(data.as_object().unwrap(), "", "a.md").unwrap_err().to_string();
        assert!(err.contains("missing required field `title`"), "{err}");
    }
}
