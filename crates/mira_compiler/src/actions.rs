//! Actions: typed requests an agent can make on a site's behalf, such as
//! booking a table or joining a waitlist.
//!
//! ```json
//! "actions": {
//!   "book_table": {
//!     "description": "Book a table for dinner.",
//!     "input": { "name": "string", "party_size": "number", "date": "date", "time": "time", "notes": "string?" },
//!     "endpoint": "https://api.example.com/bookings"
//!   }
//! }
//! ```
//!
//! A site stays static: each action sends its input as a JSON `POST` to an
//! endpoint the site owner runs or rents, such as a form service or a
//! serverless function. The build publishes every action at
//! `/_mira/actions.json` and lists them in llms.txt, and `mira mcp` offers
//! each as a tool. Input is checked against its types before anything is
//! sent, and an action only sends after the person using the agent has
//! seen exactly what will be sent, unless the site sets `confirm: false`.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::schema;

/// The most input an action accepts, in bytes of JSON.
pub const MAX_INPUT: usize = 16 * 1024;

/// Tool names `mira mcp` already uses.
const RESERVED: [&str; 7] = ["site", "pages", "search", "read", "items", "data", "media"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// What the action does, in a sentence an agent can act on.
    pub description: String,
    /// Fields and their types, as in collection schemas.
    #[serde(default)]
    pub input: BTreeMap<String, String>,
    /// Where the input is sent, as a JSON `POST`. `https://` only, or
    /// `http://localhost` while developing.
    pub endpoint: String,
    /// Ask the person using the agent before sending. On by default.
    #[serde(default = "yes")]
    pub confirm: bool,
}

fn yes() -> bool {
    true
}

/// Checks action names, input types, and endpoints when the config loads.
pub fn check(actions: &BTreeMap<String, Action>) -> Result<()> {
    for (name, action) in actions {
        let at = format!("mira.config.json: actions.{name}");
        let valid = name.len() <= 64
            && name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            bail!("{at}: name it with lowercase letters, digits, and _, starting with a letter, like book_table");
        }
        if RESERVED.contains(&name.as_str()) {
            bail!("{at}: {name} is a built-in MCP tool\nhint: choose another name, such as {name}_request");
        }
        if action.description.trim().is_empty() {
            bail!("{at}: add a description that says what the action does");
        }
        for (field, spec) in &action.input {
            if !schema::is_type(spec) {
                bail!(
                    "{at}.input.{field} has unknown type {spec:?}\nhint: use string, number, boolean, date, time, datetime, url, object, string[], number[], or object[], with ? for optional"
                );
            }
            if field == "confirm" {
                bail!("{at}.input.confirm: confirm is reserved for the confirmation step\nhint: rename the field");
            }
        }
        if !endpoint_allowed(&action.endpoint) {
            bail!("{at}.endpoint must be an https:// URL without credentials, or http://localhost while developing");
        }
    }
    Ok(())
}

/// `https://` URLs, and `http://` only on this machine, with no user name
/// or password in them.
pub fn endpoint_allowed(url: &str) -> bool {
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| url.strip_prefix(p).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    let authority = url.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    (url.starts_with("https://") || local) && !authority.is_empty() && !authority.contains('@') && !url.contains([' ', '\\', '\n'])
}

/// Whether `url` points at this machine or a private network: `localhost`,
/// loopback, private, link-local, and unspecified addresses. A deployed
/// site's actions must never reach these, or visiting a hostile site with
/// an agent could send requests to services on the agent's own network.
pub fn endpoint_is_local(url: &str) -> bool {
    use std::net::IpAddr;
    let authority = url.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.rsplit_once(':').map_or(host, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { host }),
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(is_local_ip)
}

/// Loopback, private, link-local, unspecified, and unique local addresses,
/// including IPv4 addresses mapped into IPv6. Checked again on the address
/// a request actually connects to, since a public name can resolve to any
/// of these.
pub fn is_local_ip(ip: std::net::IpAddr) -> bool {
    use std::net::{IpAddr, Ipv4Addr};
    let v4 = |ip: Ipv4Addr| ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.octets()[0] == 0;
    match ip {
        IpAddr::V4(ip) => v4(ip),
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(v4)
        }
    }
}

/// The published `/_mira/actions.json`: every action with its input types,
/// a JSON Schema for its input, its endpoint, and whether it confirms.
pub fn index(actions: &BTreeMap<String, Action>) -> Value {
    let list: Vec<Value> = actions
        .iter()
        .map(|(name, a)| {
            json!({
                "name": name,
                "description": a.description,
                "input": a.input,
                "input_schema": input_schema(&a.input),
                "endpoint": a.endpoint,
                "method": "POST",
                "confirm": a.confirm,
            })
        })
        .collect();
    json!({ "schema": 1, "actions": list })
}

/// A JSON Schema object for fields typed as in collection schemas.
pub fn input_schema(input: &BTreeMap<String, String>) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (field, spec) in input {
        properties.insert(field.clone(), schema::json_schema(spec));
        if !schema::is_optional(spec) {
            required.push(field.clone());
        }
    }
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// Checks `input` against the action's fields: every required field is
/// present, every value has its type, and nothing else is sent. Returns the
/// input to send, without empty optional fields.
pub fn validate(fields: &Map<String, Value>, input: &Value) -> Result<Map<String, Value>> {
    let Some(given) = input.as_object() else { bail!("input must be an object of fields") };
    if serde_json::to_string(given)?.len() > MAX_INPUT {
        bail!("input is larger than {} KB", MAX_INPUT / 1024);
    }
    let mut out = Map::new();
    for (field, spec) in fields {
        let spec = spec.as_str().unwrap_or("string");
        match given.get(field) {
            None | Some(Value::Null) if schema::is_optional(spec) => {}
            None | Some(Value::Null) => bail!("missing required field {field} ({spec})"),
            Some(value) => {
                if let Some(problem) = schema::check(spec, value) {
                    bail!("{field} {problem}");
                }
                out.insert(field.clone(), value.clone());
            }
        }
    }
    if let Some(extra) = given.keys().find(|k| !fields.contains_key(*k)) {
        let known: Vec<&str> = fields.keys().map(String::as_str).collect();
        bail!("unknown field {extra}; this action takes {}", known.join(", "));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(json: Value) -> BTreeMap<String, Action> {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn checks_names_types_and_endpoints() {
        let good =
            action(json!({"book_table": {"description": "Book.", "input": {"party_size": "number"}, "endpoint": "https://x.dev/b"}}));
        check(&good).unwrap();
        for bad in [
            json!({"Book": {"description": "Book.", "endpoint": "https://x.dev"}}),
            json!({"search": {"description": "Book.", "endpoint": "https://x.dev"}}),
            json!({"book": {"description": "Book.", "input": {"n": "int"}, "endpoint": "https://x.dev"}}),
            json!({"book": {"description": "Book.", "endpoint": "http://x.dev"}}),
            json!({"book": {"description": "Book.", "endpoint": "https://u:p@x.dev"}}),
            json!({"book": {"description": " ", "endpoint": "https://x.dev"}}),
        ] {
            assert!(check(&action(bad.clone())).is_err(), "{bad}");
        }
        assert!(endpoint_allowed("http://localhost:8787/book"));
        assert!(!endpoint_allowed("http://localhost.evil.dev/"));
        for local in [
            "http://localhost:8787/book",
            "https://localhost/x",
            "https://api.localhost/x",
            "https://127.0.0.1/x",
            "https://10.0.0.5:8443/x",
            "https://192.168.1.1/x",
            "https://172.20.0.1/x",
            "https://169.254.169.254/latest",
            "https://[::1]/x",
            "https://[fd00::1]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://0.0.0.0/x",
        ] {
            assert!(endpoint_is_local(local), "{local}");
        }
        for public in ["https://api.example.com/b", "https://8.8.8.8/x", "https://[2606:4700::1111]/x", "https://localhost.evil.dev/"] {
            assert!(!endpoint_is_local(public), "{public}");
        }
    }

    #[test]
    fn validates_input() {
        let fields = json!({"name": "string", "party_size": "number", "time": "time", "notes": "string?"});
        let fields = fields.as_object().unwrap();
        let ok = validate(fields, &json!({"name": "Ada", "party_size": 2, "time": "19:30"})).unwrap();
        assert_eq!(ok.len(), 3);
        let err = |input: Value| validate(fields, &input).unwrap_err().to_string();
        assert!(err(json!({"name": "Ada", "time": "19:30"})).contains("missing required field party_size"));
        assert!(err(json!({"name": "Ada", "party_size": "2", "time": "19:30"})).contains("party_size must be a number"));
        assert!(err(json!({"name": "Ada", "party_size": 2, "time": "7pm"})).contains("time must be a time"));
        assert!(err(json!({"name": "Ada", "party_size": 2, "time": "19:30", "card": "4242"})).contains("unknown field card"));
    }

    #[test]
    fn publishes_a_schema() {
        let index = index(&action(
            json!({"book": {"description": "Book.", "input": {"date": "date", "notes": "string?"}, "endpoint": "https://x.dev"}}),
        ));
        let schema = &index["actions"][0]["input_schema"];
        assert_eq!(schema["required"], json!(["date"]));
        assert_eq!(schema["properties"]["date"]["format"], "date");
        assert_eq!(index["actions"][0]["confirm"], true);
    }
}
